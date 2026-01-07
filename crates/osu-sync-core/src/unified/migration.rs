//! Migration logic for converting separate installations to unified storage.
//!
//! This module provides functionality to migrate existing osu! installations
//! (stable and lazer) to use unified storage with symlinks or junctions.
//!
//! # Migration Process
//!
//! 1. Check prerequisites (disk space, permissions, game not running)
//! 2. Create backup manifest for rollback capability
//! 3. Copy/move files to the master location
//! 4. Create symbolic links or junctions
//! 5. Verify integrity of the migration
//! 6. Clean up temporary files
//!
//! # Example
//!
//! ```rust,ignore
//! use osu_sync_core::unified::{UnifiedMigration, UnifiedStorageConfig};
//! use std::path::PathBuf;
//!
//! let config = UnifiedStorageConfig::stable_master();
//! let migration = UnifiedMigration::new(
//!     config,
//!     PathBuf::from("/path/to/stable"),
//!     PathBuf::from("/path/to/lazer"),
//! );
//!
//! // Plan the migration first
//! let plan = migration.plan()?;
//! println!("Space required: {} bytes", plan.space_required);
//!
//! // Execute with progress reporting
//! let result = migration.execute(|progress| {
//!     println!("Step {}/{}: {}", progress.current_step, progress.total_steps, progress.step_name);
//! })?;
//!
//! if !result.success {
//!     migration.rollback()?;
//! }
//! ```

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

use super::config::{SharedResourceType, UnifiedStorageConfig, UnifiedStorageMode};

/// A single step in the migration process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationStep {
    /// Check system prerequisites (disk space, permissions, etc.)
    CheckPrerequisites,
    /// Create the shared folder for unified storage
    CreateSharedFolder { path: PathBuf },
    /// Backup the original installation
    BackupOriginal { path: PathBuf },
    /// Copy beatmap files to the master location
    CopyBeatmaps { count: usize, size: u64 },
    /// Copy skin files to the master location
    CopySkins { count: usize, size: u64 },
    /// Copy replay files to the master location
    CopyReplays { count: usize },
    /// Copy screenshot files to the master location
    CopyScreenshots { count: usize },
    /// Create NTFS junctions (Windows)
    CreateJunctions { count: usize },
    /// Create symbolic links (Unix/Windows with privileges)
    CreateSymlinks { count: usize },
    /// Update the unified manifest file
    UpdateManifest,
    /// Verify the integrity of migrated files
    VerifyIntegrity,
    /// Clean up backup files after successful migration
    CleanupBackups,
}

impl MigrationStep {
    /// Returns a human-readable description of this step.
    pub fn description(&self) -> String {
        match self {
            Self::CheckPrerequisites => "Checking prerequisites".to_string(),
            Self::CreateSharedFolder { path } => {
                format!("Creating shared folder: {}", path.display())
            }
            Self::BackupOriginal { path } => {
                format!("Backing up: {}", path.display())
            }
            Self::CopyBeatmaps { count, size } => {
                format!(
                    "Copying {} beatmaps ({:.2} GB)",
                    count,
                    *size as f64 / 1_073_741_824.0
                )
            }
            Self::CopySkins { count, size } => {
                format!(
                    "Copying {} skins ({:.2} MB)",
                    count,
                    *size as f64 / 1_048_576.0
                )
            }
            Self::CopyReplays { count } => format!("Copying {} replays", count),
            Self::CopyScreenshots { count } => format!("Copying {} screenshots", count),
            Self::CreateJunctions { count } => format!("Creating {} junctions", count),
            Self::CreateSymlinks { count } => format!("Creating {} symlinks", count),
            Self::UpdateManifest => "Updating manifest".to_string(),
            Self::VerifyIntegrity => "Verifying integrity".to_string(),
            Self::CleanupBackups => "Cleaning up backups".to_string(),
        }
    }

    /// Returns the estimated duration for this step in seconds.
    pub fn estimated_duration_secs(&self) -> u64 {
        match self {
            Self::CheckPrerequisites => 2,
            Self::CreateSharedFolder { .. } => 1,
            Self::BackupOriginal { .. } => 5,
            Self::CopyBeatmaps { size, .. } => {
                // Estimate ~50 MB/s copy speed
                (*size / 52_428_800).max(1)
            }
            Self::CopySkins { size, .. } => (*size / 52_428_800).max(1),
            Self::CopyReplays { count } => (*count as u64 / 100).max(1),
            Self::CopyScreenshots { count } => (*count as u64 / 200).max(1),
            Self::CreateJunctions { count } | Self::CreateSymlinks { count } => {
                (*count as u64 / 50).max(1)
            }
            Self::UpdateManifest => 1,
            Self::VerifyIntegrity => 30,
            Self::CleanupBackups => 5,
        }
    }
}

/// A complete migration plan describing all steps and requirements.
#[derive(Debug, Clone)]
pub struct MigrationPlan {
    /// The unified storage mode being migrated to.
    pub mode: UnifiedStorageMode,
    /// Ordered list of migration steps.
    pub steps: Vec<MigrationStep>,
    /// Total disk space required for the migration (in bytes).
    pub space_required: u64,
    /// Disk space that will be freed after migration (in bytes).
    pub space_freed: u64,
    /// Estimated total duration for the migration.
    pub estimated_duration: Duration,
    /// Warnings that don't prevent migration but should be noted.
    pub warnings: Vec<String>,
    /// Whether the migration requires elevated privileges.
    pub requires_elevation: bool,
}

impl MigrationPlan {
    /// Creates a new empty migration plan.
    pub fn new(mode: UnifiedStorageMode) -> Self {
        Self {
            mode,
            steps: Vec::new(),
            space_required: 0,
            space_freed: 0,
            estimated_duration: Duration::ZERO,
            warnings: Vec::new(),
            requires_elevation: false,
        }
    }

    /// Adds a step to the migration plan.
    pub fn add_step(&mut self, step: MigrationStep) {
        let step_duration = Duration::from_secs(step.estimated_duration_secs());
        self.estimated_duration += step_duration;
        self.steps.push(step);
    }

    /// Adds a warning to the migration plan.
    pub fn add_warning(&mut self, warning: impl Into<String>) {
        self.warnings.push(warning.into());
    }

    /// Returns true if the plan has any warnings.
    pub fn has_warnings(&self) -> bool {
        !self.warnings.is_empty()
    }

    /// Returns the total number of steps.
    pub fn step_count(&self) -> usize {
        self.steps.len()
    }

    /// Returns human-readable estimated duration.
    pub fn estimated_duration_display(&self) -> String {
        let secs = self.estimated_duration.as_secs();
        if secs < 60 {
            format!("{} seconds", secs)
        } else if secs < 3600 {
            format!("{} minutes", secs / 60)
        } else {
            format!("{} hours {} minutes", secs / 3600, (secs % 3600) / 60)
        }
    }

    /// Returns human-readable space required.
    pub fn space_required_display(&self) -> String {
        format_bytes(self.space_required)
    }

    /// Returns human-readable space freed.
    pub fn space_freed_display(&self) -> String {
        format_bytes(self.space_freed)
    }
}

/// Progress information during migration.
#[derive(Debug, Clone)]
pub struct MigrationProgress {
    /// Current step index (0-based).
    pub current_step: usize,
    /// Total number of steps.
    pub total_steps: usize,
    /// Human-readable name of the current step.
    pub step_name: String,
    /// Progress within the current step (0.0 to 1.0).
    pub step_progress: f32,
    /// Bytes processed so far in the current step.
    pub bytes_processed: u64,
    /// Total bytes to process in the current step.
    pub total_bytes: u64,
}

impl MigrationProgress {
    /// Creates a new progress report.
    pub fn new(current_step: usize, total_steps: usize, step_name: impl Into<String>) -> Self {
        Self {
            current_step,
            total_steps,
            step_name: step_name.into(),
            step_progress: 0.0,
            bytes_processed: 0,
            total_bytes: 0,
        }
    }

    /// Returns the overall progress as a percentage (0.0 to 100.0).
    pub fn overall_progress(&self) -> f32 {
        if self.total_steps == 0 {
            return 0.0;
        }
        let completed_steps = self.current_step as f32;
        let step_contribution = self.step_progress / self.total_steps as f32;
        ((completed_steps + step_contribution) / self.total_steps as f32) * 100.0
    }

    /// Returns the step progress as a percentage (0.0 to 100.0).
    pub fn step_progress_percent(&self) -> f32 {
        self.step_progress * 100.0
    }
}

impl Default for MigrationProgress {
    fn default() -> Self {
        Self {
            current_step: 0,
            total_steps: 0,
            step_name: String::new(),
            step_progress: 0.0,
            bytes_processed: 0,
            total_bytes: 0,
        }
    }
}

/// Result of a completed migration.
#[derive(Debug, Clone, Default)]
pub struct MigrationResult {
    /// Whether the migration completed successfully.
    pub success: bool,
    /// Number of links (junctions/symlinks) created.
    pub links_created: usize,
    /// Total disk space saved by deduplication (in bytes).
    pub space_saved: u64,
    /// Warnings encountered during migration.
    pub warnings: Vec<String>,
    /// Errors encountered during migration.
    pub errors: Vec<String>,
}

impl MigrationResult {
    /// Creates a new successful migration result.
    pub fn success(links_created: usize, space_saved: u64) -> Self {
        Self {
            success: true,
            links_created,
            space_saved,
            warnings: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// Creates a new failed migration result.
    pub fn failure(errors: Vec<String>) -> Self {
        Self {
            success: false,
            links_created: 0,
            space_saved: 0,
            warnings: Vec::new(),
            errors,
        }
    }

    /// Adds a warning to the result.
    pub fn add_warning(&mut self, warning: impl Into<String>) {
        self.warnings.push(warning.into());
    }

    /// Adds an error to the result.
    pub fn add_error(&mut self, error: impl Into<String>) {
        self.errors.push(error.into());
        self.success = false;
    }

    /// Returns human-readable space saved.
    pub fn space_saved_display(&self) -> String {
        format_bytes(self.space_saved)
    }
}

/// Backup manifest for rollback capability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    /// When the backup was created.
    pub created_at: u64,
    /// Original stable path.
    pub stable_path: PathBuf,
    /// Original lazer path.
    pub lazer_path: PathBuf,
    /// Migration mode.
    pub mode: UnifiedStorageMode,
    /// Paths that were moved (original -> backup location).
    pub moved_paths: HashMap<PathBuf, PathBuf>,
    /// Links that were created.
    pub created_links: Vec<PathBuf>,
    /// Whether the migration completed.
    pub completed: bool,
}

impl BackupManifest {
    /// Creates a new backup manifest.
    pub fn new(stable_path: PathBuf, lazer_path: PathBuf, mode: UnifiedStorageMode) -> Self {
        let created_at = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            created_at,
            stable_path,
            lazer_path,
            mode,
            moved_paths: HashMap::new(),
            created_links: Vec::new(),
            completed: false,
        }
    }

    /// Saves the manifest to a file.
    pub fn save(&self, path: &Path) -> Result<()> {
        let file = File::create(path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, self)
            .map_err(|e| Error::Other(format!("Failed to save manifest: {}", e)))?;
        Ok(())
    }

    /// Loads a manifest from a file.
    pub fn load(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        serde_json::from_reader(reader)
            .map_err(|e| Error::Other(format!("Failed to load manifest: {}", e)))
    }

    /// Records a moved path.
    pub fn record_move(&mut self, original: PathBuf, backup: PathBuf) {
        self.moved_paths.insert(original, backup);
    }

    /// Records a created link.
    pub fn record_link(&mut self, link_path: PathBuf) {
        self.created_links.push(link_path);
    }

    /// Marks the migration as completed.
    pub fn mark_completed(&mut self) {
        self.completed = true;
    }
}

/// Main migration engine for unified storage.
pub struct UnifiedMigration {
    config: UnifiedStorageConfig,
    stable_path: PathBuf,
    lazer_path: PathBuf,
    manifest: Option<BackupManifest>,
    manifest_path: Option<PathBuf>,
}

impl UnifiedMigration {
    /// Creates a new migration engine.
    pub fn new(config: UnifiedStorageConfig, stable: PathBuf, lazer: PathBuf) -> Self {
        Self {
            config,
            stable_path: stable,
            lazer_path: lazer,
            manifest: None,
            manifest_path: None,
        }
    }

    /// Plans the migration and returns a detailed plan.
    pub fn plan(&self) -> Result<MigrationPlan> {
        let mut plan = MigrationPlan::new(self.config.mode);

        // Validate configuration
        self.config.validate().map_err(Error::Config)?;

        // Check prerequisites first
        plan.add_step(MigrationStep::CheckPrerequisites);

        // Determine master and slave paths based on mode
        let (master_path, slave_path) = match self.config.mode {
            UnifiedStorageMode::Disabled => {
                return Err(Error::Config(
                    "Cannot plan migration when unified storage is disabled".to_string(),
                ));
            }
            UnifiedStorageMode::StableMaster => (&self.stable_path, &self.lazer_path),
            UnifiedStorageMode::LazerMaster => (&self.lazer_path, &self.stable_path),
            UnifiedStorageMode::TrueUnified => {
                let shared_path = self.config.shared_path.as_ref().ok_or_else(|| {
                    Error::Config("TrueUnified mode requires a shared path".to_string())
                })?;
                plan.add_step(MigrationStep::CreateSharedFolder {
                    path: shared_path.clone(),
                });
                (shared_path, &self.stable_path)
            }
        };

        // Backup original
        plan.add_step(MigrationStep::BackupOriginal {
            path: slave_path.clone(),
        });

        // Calculate space requirements and add copy steps
        let (space_required, space_freed) = self.calculate_space_requirements()?;
        plan.space_required = space_required;
        plan.space_freed = space_freed;

        // Add steps for each shared resource type
        for resource in self.config.shared_resources.iter() {
            match resource {
                SharedResourceType::Beatmaps => {
                    let (count, size) = self.count_resource(master_path, "Songs")?;
                    if count > 0 {
                        plan.add_step(MigrationStep::CopyBeatmaps { count, size });
                    }
                }
                SharedResourceType::Skins => {
                    let (count, size) = self.count_resource(master_path, "Skins")?;
                    if count > 0 {
                        plan.add_step(MigrationStep::CopySkins { count, size });
                    }
                }
                SharedResourceType::Replays => {
                    let (count, _) = self.count_resource(master_path, "Replays")?;
                    if count > 0 {
                        plan.add_step(MigrationStep::CopyReplays { count });
                    }
                }
                SharedResourceType::Screenshots => {
                    let (count, _) = self.count_resource(master_path, "Screenshots")?;
                    if count > 0 {
                        plan.add_step(MigrationStep::CopyScreenshots { count });
                    }
                }
                _ => {}
            }
        }

        // Add link creation step
        let link_count = self.config.shared_resources.len();
        if self.config.should_use_junctions() {
            plan.add_step(MigrationStep::CreateJunctions { count: link_count });
        } else {
            plan.add_step(MigrationStep::CreateSymlinks { count: link_count });
            // Symlinks require elevation on Windows
            if cfg!(windows) {
                plan.requires_elevation = true;
                plan.add_warning(
                    "Creating symbolic links on Windows requires administrator privileges",
                );
            }
        }

        plan.add_step(MigrationStep::UpdateManifest);
        plan.add_step(MigrationStep::VerifyIntegrity);
        plan.add_step(MigrationStep::CleanupBackups);

        // Add prerequisites warnings
        let prereq_warnings = self.check_prerequisites()?;
        for warning in prereq_warnings {
            plan.add_warning(warning);
        }

        Ok(plan)
    }

    /// Executes the migration with progress reporting.
    pub fn execute<F>(&mut self, progress_callback: F) -> Result<MigrationResult>
    where
        F: Fn(MigrationProgress) + Send + Sync,
    {
        // Create the plan
        let plan = self.plan()?;
        let total_steps = plan.steps.len();

        // Initialize the backup manifest
        let manifest_path = self.stable_path.join(".osu-sync-migration.json");
        let mut manifest = BackupManifest::new(
            self.stable_path.clone(),
            self.lazer_path.clone(),
            self.config.mode,
        );

        let mut result = MigrationResult::default();
        let mut links_created = 0usize;
        let mut space_saved = 0u64;

        // Execute each step
        for (step_idx, step) in plan.steps.iter().enumerate() {
            let progress = MigrationProgress {
                current_step: step_idx,
                total_steps,
                step_name: step.description(),
                step_progress: 0.0,
                bytes_processed: 0,
                total_bytes: 0,
            };
            progress_callback(progress);

            let step_result = self.execute_step(step, &mut manifest, |step_progress| {
                let progress = MigrationProgress {
                    current_step: step_idx,
                    total_steps,
                    step_name: step.description(),
                    step_progress,
                    bytes_processed: 0,
                    total_bytes: 0,
                };
                progress_callback(progress);
            });

            match step_result {
                Ok(step_stats) => {
                    links_created += step_stats.links_created;
                    space_saved += step_stats.space_saved;
                    for warning in step_stats.warnings {
                        result.add_warning(warning);
                    }
                }
                Err(e) => {
                    result.add_error(format!("Step '{}' failed: {}", step.description(), e));

                    // Save manifest for potential rollback
                    self.manifest = Some(manifest.clone());
                    self.manifest_path = Some(manifest_path.clone());

                    if let Err(save_err) = manifest.save(&manifest_path) {
                        result.add_error(format!("Failed to save rollback manifest: {}", save_err));
                    }

                    return Ok(result);
                }
            }

            // Update progress to complete
            let progress = MigrationProgress {
                current_step: step_idx,
                total_steps,
                step_name: step.description(),
                step_progress: 1.0,
                bytes_processed: 0,
                total_bytes: 0,
            };
            progress_callback(progress);
        }

        // Mark manifest as completed
        manifest.mark_completed();
        manifest.save(&manifest_path)?;

        // Store manifest for reference
        self.manifest = Some(manifest);
        self.manifest_path = Some(manifest_path);

        result.success = true;
        result.links_created = links_created;
        result.space_saved = space_saved;

        Ok(result)
    }

    /// Rolls back a failed or incomplete migration.
    pub fn rollback(&self) -> Result<()> {
        let manifest = self.manifest.as_ref().ok_or_else(|| {
            Error::Other("No migration manifest available for rollback".to_string())
        })?;

        // Remove created links
        for link_path in manifest.created_links.iter().rev() {
            if link_path.exists() {
                // Check if it's a symlink/junction before removing
                if link_path.symlink_metadata().is_ok() {
                    if link_path.is_dir() {
                        // On Windows, junctions are removed with remove_dir
                        fs::remove_dir(link_path).ok();
                    } else {
                        fs::remove_file(link_path).ok();
                    }
                }
            }
        }

        // Restore moved paths
        for (original, backup) in manifest.moved_paths.iter() {
            if backup.exists() && !original.exists() {
                if backup.is_dir() {
                    Self::move_directory(backup, original)?;
                } else {
                    fs::rename(backup, original)?;
                }
            }
        }

        // Remove the manifest file
        if let Some(ref manifest_path) = self.manifest_path {
            fs::remove_file(manifest_path).ok();
        }

        Ok(())
    }

    /// Validates the current migration state and returns any issues.
    pub fn validate(&self) -> Result<Vec<String>> {
        let mut warnings = Vec::new();

        // Check that both paths exist
        if !self.stable_path.exists() {
            warnings.push(format!(
                "osu!stable path does not exist: {}",
                self.stable_path.display()
            ));
        }

        if !self.lazer_path.exists() {
            warnings.push(format!(
                "osu!lazer path does not exist: {}",
                self.lazer_path.display()
            ));
        }

        // Check for existing links
        for resource in self.config.shared_resources.iter() {
            let folder_name = resource.folder_name();

            let stable_resource = self.stable_path.join(folder_name);
            if stable_resource.exists() {
                if let Ok(metadata) = stable_resource.symlink_metadata() {
                    if metadata.file_type().is_symlink() {
                        warnings.push(format!(
                            "Existing symlink found at: {}",
                            stable_resource.display()
                        ));
                    }
                }
            }

            let lazer_resource = self.lazer_path.join(folder_name);
            if lazer_resource.exists() {
                if let Ok(metadata) = lazer_resource.symlink_metadata() {
                    if metadata.file_type().is_symlink() {
                        warnings.push(format!(
                            "Existing symlink found at: {}",
                            lazer_resource.display()
                        ));
                    }
                }
            }
        }

        // Check if games are running
        if Self::is_game_running() {
            warnings
                .push("osu! appears to be running. Close the game before migrating.".to_string());
        }

        Ok(warnings)
    }

    /// Calculates the disk space requirements for the migration.
    fn calculate_space_requirements(&self) -> Result<(u64, u64)> {
        let space_required;
        let space_freed;

        let (master_path, slave_path) = match self.config.mode {
            UnifiedStorageMode::StableMaster => (&self.stable_path, &self.lazer_path),
            UnifiedStorageMode::LazerMaster => (&self.lazer_path, &self.stable_path),
            UnifiedStorageMode::TrueUnified => {
                // For true unified, we need space in the shared location
                // and will free space in both installations
                let stable_size = self.calculate_resources_size(&self.stable_path)?;
                let lazer_size = self.calculate_resources_size(&self.lazer_path)?;

                // Need space for the larger of the two (assuming some overlap)
                space_required = stable_size.max(lazer_size);
                // Will free the smaller of the two (after dedup)
                space_freed = stable_size.min(lazer_size);

                return Ok((space_required, space_freed));
            }
            UnifiedStorageMode::Disabled => {
                return Ok((0, 0));
            }
        };

        // Calculate size of resources in slave that will be removed
        let slave_size = self.calculate_resources_size(slave_path)?;

        // For non-true-unified modes, we may need temporary space during copy
        // but will free the slave's space afterward
        let master_size = self.calculate_resources_size(master_path)?;

        // Worst case: need to copy all from slave to master
        space_required = if master_size == 0 { slave_size } else { 0 };
        space_freed = slave_size;

        Ok((space_required, space_freed))
    }

    /// Calculates the total size of shared resources in a path.
    fn calculate_resources_size(&self, base_path: &Path) -> Result<u64> {
        let mut total_size = 0u64;

        for resource in self.config.shared_resources.iter() {
            let folder_name = resource.folder_name();
            let resource_path = base_path.join(folder_name);

            if resource_path.exists() {
                total_size += Self::calculate_directory_size(&resource_path)?;
            }
        }

        Ok(total_size)
    }

    /// Recursively calculates the size of a directory.
    fn calculate_directory_size(path: &Path) -> Result<u64> {
        let mut total_size = 0u64;

        if path.is_dir() {
            for entry in fs::read_dir(path)? {
                let entry = entry?;
                let entry_path = entry.path();

                if entry_path.is_dir() {
                    total_size += Self::calculate_directory_size(&entry_path)?;
                } else if entry_path.is_file() {
                    total_size += entry.metadata()?.len();
                }
            }
        } else if path.is_file() {
            total_size = fs::metadata(path)?.len();
        }

        Ok(total_size)
    }

    /// Checks prerequisites and returns warnings.
    fn check_prerequisites(&self) -> Result<Vec<String>> {
        let mut warnings = Vec::new();

        // Check if games are running
        if Self::is_game_running() {
            warnings
                .push("osu! appears to be running. Please close it before migrating.".to_string());
        }

        // Check disk space
        let (space_required, _) = self.calculate_space_requirements()?;
        let available_space = Self::get_available_disk_space(&self.stable_path)?;

        if available_space < space_required {
            warnings.push(format!(
                "Insufficient disk space. Required: {}, Available: {}",
                format_bytes(space_required),
                format_bytes(available_space)
            ));
        }

        // Check write permissions
        if !Self::can_write_to(&self.stable_path) {
            warnings.push(format!(
                "No write permission to: {}",
                self.stable_path.display()
            ));
        }

        if !Self::can_write_to(&self.lazer_path) {
            warnings.push(format!(
                "No write permission to: {}",
                self.lazer_path.display()
            ));
        }

        // Check for true unified path
        if let Some(ref shared_path) = self.config.shared_path {
            if !shared_path.exists() {
                // This is fine, we'll create it
            } else if !Self::can_write_to(shared_path) {
                warnings.push(format!(
                    "No write permission to shared path: {}",
                    shared_path.display()
                ));
            }
        }

        Ok(warnings)
    }

    /// Creates the backup manifest file.
    #[allow(dead_code)]
    fn create_backup_manifest(&self) -> Result<PathBuf> {
        let manifest = BackupManifest::new(
            self.stable_path.clone(),
            self.lazer_path.clone(),
            self.config.mode,
        );

        let manifest_path = self.stable_path.join(".osu-sync-migration.json");
        manifest.save(&manifest_path)?;

        Ok(manifest_path)
    }

    /// Counts the number of items and total size in a resource folder.
    fn count_resource(&self, base_path: &Path, folder_name: &str) -> Result<(usize, u64)> {
        let resource_path = base_path.join(folder_name);

        if !resource_path.exists() {
            return Ok((0, 0));
        }

        let mut count = 0usize;
        let mut size = 0u64;

        for entry in fs::read_dir(&resource_path)? {
            let entry = entry?;
            count += 1;

            if entry.file_type()?.is_dir() {
                size += Self::calculate_directory_size(&entry.path())?;
            } else {
                size += entry.metadata()?.len();
            }
        }

        Ok((count, size))
    }

    /// Executes a single migration step.
    fn execute_step<F>(
        &self,
        step: &MigrationStep,
        manifest: &mut BackupManifest,
        progress_callback: F,
    ) -> Result<StepStats>
    where
        F: Fn(f32),
    {
        let mut stats = StepStats::default();

        match step {
            MigrationStep::CheckPrerequisites => {
                let warnings = self.check_prerequisites()?;
                for warning in warnings {
                    if warning.contains("running") || warning.contains("space") {
                        return Err(Error::Other(warning));
                    }
                    stats.warnings.push(warning);
                }
            }

            MigrationStep::CreateSharedFolder { path } => {
                if !path.exists() {
                    fs::create_dir_all(path)?;
                }
            }

            MigrationStep::BackupOriginal { path } => {
                let backup_path = path.with_extension("backup");
                if path.exists() && !backup_path.exists() {
                    // Just record the path for potential rollback, don't actually move yet
                    manifest.record_move(path.clone(), backup_path);
                }
            }

            MigrationStep::CopyBeatmaps { count: _, size }
            | MigrationStep::CopySkins { count: _, size } => {
                let folder_name = if matches!(step, MigrationStep::CopyBeatmaps { .. }) {
                    "Songs"
                } else {
                    "Skins"
                };

                self.copy_resource_folder(folder_name, manifest, *size, |progress| {
                    progress_callback(progress);
                })?;

                stats.space_saved = *size;
            }

            MigrationStep::CopyReplays { .. } => {
                self.copy_resource_folder("Replays", manifest, 0, |progress| {
                    progress_callback(progress);
                })?;
            }

            MigrationStep::CopyScreenshots { .. } => {
                self.copy_resource_folder("Screenshots", manifest, 0, |progress| {
                    progress_callback(progress);
                })?;
            }

            MigrationStep::CreateJunctions { count: _count } => {
                stats.links_created = self.create_links(manifest, true)?;
            }

            MigrationStep::CreateSymlinks { count: _count } => {
                stats.links_created = self.create_links(manifest, false)?;
            }

            MigrationStep::UpdateManifest => {
                // Manifest is updated continuously, just save it here
                if let Some(ref path) = self.manifest_path {
                    manifest.save(path)?;
                }
            }

            MigrationStep::VerifyIntegrity => {
                let issues = self.verify_migration()?;
                for issue in issues {
                    stats.warnings.push(issue);
                }
            }

            MigrationStep::CleanupBackups => {
                // Only cleanup if migration was successful
                if manifest.completed {
                    self.cleanup_backups(manifest)?;
                }
            }
        }

        Ok(stats)
    }

    /// Copies a resource folder from slave to master.
    fn copy_resource_folder<F>(
        &self,
        folder_name: &str,
        manifest: &mut BackupManifest,
        total_size: u64,
        progress_callback: F,
    ) -> Result<()>
    where
        F: Fn(f32),
    {
        let (master_path, slave_path) = match self.config.mode {
            UnifiedStorageMode::StableMaster => (&self.stable_path, &self.lazer_path),
            UnifiedStorageMode::LazerMaster => (&self.lazer_path, &self.stable_path),
            UnifiedStorageMode::TrueUnified => {
                let shared = self
                    .config
                    .shared_path
                    .as_ref()
                    .ok_or_else(|| Error::Config("Missing shared path".to_string()))?;
                (shared, &self.stable_path)
            }
            UnifiedStorageMode::Disabled => return Ok(()),
        };

        let source = slave_path.join(folder_name);
        let dest = master_path.join(folder_name);

        if !source.exists() {
            return Ok(());
        }

        // Create destination if needed
        if !dest.exists() {
            fs::create_dir_all(&dest)?;
        }

        // Copy files that don't exist in destination
        let mut bytes_copied = 0u64;
        Self::copy_directory_merge(
            &source,
            &dest,
            total_size,
            &mut bytes_copied,
            &progress_callback,
        )?;

        // Record the move for rollback
        let backup_path = source.with_extension("pre-migration");
        if source.exists() {
            manifest.record_move(source.clone(), backup_path.clone());
            // Rename source to backup
            if !backup_path.exists() {
                fs::rename(&source, &backup_path)?;
            }
        }

        Ok(())
    }

    /// Copies a directory, merging with existing contents.
    fn copy_directory_merge<F>(
        source: &Path,
        dest: &Path,
        total_size: u64,
        bytes_copied: &mut u64,
        progress_callback: &F,
    ) -> Result<()>
    where
        F: Fn(f32),
    {
        if !dest.exists() {
            fs::create_dir_all(dest)?;
        }

        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let source_path = entry.path();
            let file_name = entry.file_name();
            let dest_path = dest.join(&file_name);

            if source_path.is_dir() {
                Self::copy_directory_merge(
                    &source_path,
                    &dest_path,
                    total_size,
                    bytes_copied,
                    progress_callback,
                )?;
            } else if source_path.is_file() {
                // Only copy if destination doesn't exist or is older
                let should_copy = if dest_path.exists() {
                    let source_modified = source_path.metadata()?.modified()?;
                    let dest_modified = dest_path.metadata()?.modified()?;
                    source_modified > dest_modified
                } else {
                    true
                };

                if should_copy {
                    let file_size = entry.metadata()?.len();
                    fs::copy(&source_path, &dest_path)?;
                    *bytes_copied += file_size;

                    if total_size > 0 {
                        let progress = *bytes_copied as f32 / total_size as f32;
                        progress_callback(progress.min(1.0));
                    }
                }
            }
        }

        Ok(())
    }

    /// Creates links (junctions or symlinks) for shared resources.
    fn create_links(&self, manifest: &mut BackupManifest, use_junctions: bool) -> Result<usize> {
        let (master_path, slave_path) = match self.config.mode {
            UnifiedStorageMode::StableMaster => (&self.stable_path, &self.lazer_path),
            UnifiedStorageMode::LazerMaster => (&self.lazer_path, &self.stable_path),
            UnifiedStorageMode::TrueUnified => {
                let shared = self
                    .config
                    .shared_path
                    .as_ref()
                    .ok_or_else(|| Error::Config("Missing shared path".to_string()))?;
                // In TrueUnified, both installations link to shared
                return self.create_links_to_shared(shared, manifest, use_junctions);
            }
            UnifiedStorageMode::Disabled => return Ok(0),
        };

        let mut links_created = 0;

        for resource in self.config.shared_resources.iter() {
            let folder_name = resource.folder_name();
            let link_path = slave_path.join(folder_name);
            let target_path = master_path.join(folder_name);

            // Remove existing directory/link
            if link_path.exists() {
                if link_path.symlink_metadata()?.file_type().is_symlink() {
                    fs::remove_file(&link_path)?;
                } else {
                    // Should have been moved during copy step
                }
            }

            // Create the link
            if target_path.exists() {
                self.create_link(&link_path, &target_path, use_junctions)?;
                manifest.record_link(link_path);
                links_created += 1;
            }
        }

        Ok(links_created)
    }

    /// Creates links from both installations to a shared location.
    fn create_links_to_shared(
        &self,
        shared_path: &Path,
        manifest: &mut BackupManifest,
        use_junctions: bool,
    ) -> Result<usize> {
        let mut links_created = 0;

        for resource in self.config.shared_resources.iter() {
            let folder_name = resource.folder_name();
            let target_path = shared_path.join(folder_name);

            if !target_path.exists() {
                fs::create_dir_all(&target_path)?;
            }

            // Create link from stable
            let stable_link = self.stable_path.join(folder_name);
            if stable_link.exists() && !stable_link.symlink_metadata()?.file_type().is_symlink() {
                // Move to shared, then create link
                Self::move_directory(&stable_link, &target_path)?;
            }
            if !stable_link.exists() || stable_link.symlink_metadata()?.file_type().is_symlink() {
                if stable_link.exists() {
                    fs::remove_file(&stable_link)?;
                }
                self.create_link(&stable_link, &target_path, use_junctions)?;
                manifest.record_link(stable_link);
                links_created += 1;
            }

            // Create link from lazer
            let lazer_link = self.lazer_path.join(folder_name);
            if lazer_link.exists() && !lazer_link.symlink_metadata()?.file_type().is_symlink() {
                // Merge to shared
                Self::merge_directory(&lazer_link, &target_path)?;
                fs::remove_dir_all(&lazer_link)?;
            }
            if !lazer_link.exists() {
                self.create_link(&lazer_link, &target_path, use_junctions)?;
                manifest.record_link(lazer_link);
                links_created += 1;
            }
        }

        Ok(links_created)
    }

    /// Creates a single link (junction or symlink).
    fn create_link(&self, link: &Path, target: &Path, use_junction: bool) -> Result<()> {
        #[cfg(windows)]
        {
            if use_junction {
                // Use junction_rs or similar
                Self::create_junction(link, target)?;
            } else {
                std::os::windows::fs::symlink_dir(target, link)?;
            }
        }

        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(target, link)?;
        }

        Ok(())
    }

    /// Creates an NTFS junction (Windows only).
    #[cfg(windows)]
    fn create_junction(link: &Path, target: &Path) -> Result<()> {
        use std::process::Command;

        // Use mklink /J via cmd to create junction
        let output = Command::new("cmd")
            .args([
                "/C",
                "mklink",
                "/J",
                &link.to_string_lossy(),
                &target.to_string_lossy(),
            ])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(Error::Other(format!(
                "Failed to create junction: {}",
                stderr
            )));
        }

        Ok(())
    }

    #[cfg(not(windows))]
    fn create_junction(_link: &Path, _target: &Path) -> Result<()> {
        Err(Error::Other(
            "Junctions are only supported on Windows".to_string(),
        ))
    }

    /// Moves a directory to a new location.
    fn move_directory(source: &Path, dest: &Path) -> Result<()> {
        if !dest.exists() {
            fs::create_dir_all(dest)?;
        }

        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let source_path = entry.path();
            let dest_path = dest.join(entry.file_name());

            if source_path.is_dir() {
                Self::move_directory(&source_path, &dest_path)?;
            } else {
                fs::rename(&source_path, &dest_path)?;
            }
        }

        fs::remove_dir(source)?;
        Ok(())
    }

    /// Merges a directory into another, skipping existing files.
    fn merge_directory(source: &Path, dest: &Path) -> Result<()> {
        if !dest.exists() {
            fs::create_dir_all(dest)?;
        }

        for entry in fs::read_dir(source)? {
            let entry = entry?;
            let source_path = entry.path();
            let dest_path = dest.join(entry.file_name());

            if source_path.is_dir() {
                Self::merge_directory(&source_path, &dest_path)?;
            } else if !dest_path.exists() {
                fs::copy(&source_path, &dest_path)?;
            }
        }

        Ok(())
    }

    /// Verifies the migration was successful.
    fn verify_migration(&self) -> Result<Vec<String>> {
        let mut issues = Vec::new();

        for resource in self.config.shared_resources.iter() {
            let folder_name = resource.folder_name();

            // Check stable link
            let stable_resource = self.stable_path.join(folder_name);
            if stable_resource.exists() {
                if let Err(e) = fs::read_dir(&stable_resource) {
                    issues.push(format!("Cannot access {} in stable: {}", folder_name, e));
                }
            }

            // Check lazer link
            let lazer_resource = self.lazer_path.join(folder_name);
            if lazer_resource.exists() {
                if let Err(e) = fs::read_dir(&lazer_resource) {
                    issues.push(format!("Cannot access {} in lazer: {}", folder_name, e));
                }
            }
        }

        Ok(issues)
    }

    /// Cleans up backup files after successful migration.
    fn cleanup_backups(&self, manifest: &BackupManifest) -> Result<()> {
        for (_, backup_path) in manifest.moved_paths.iter() {
            if backup_path.exists() {
                if backup_path.is_dir() {
                    fs::remove_dir_all(backup_path)?;
                } else {
                    fs::remove_file(backup_path)?;
                }
            }
        }

        Ok(())
    }

    /// Checks if osu! is currently running.
    fn is_game_running() -> bool {
        // Platform-specific check
        #[cfg(windows)]
        {
            use std::process::Command;
            if let Ok(output) = Command::new("tasklist").output() {
                let output_str = String::from_utf8_lossy(&output.stdout).to_lowercase();
                return output_str.contains("osu!.exe") || output_str.contains("osu.exe");
            }
        }

        #[cfg(unix)]
        {
            use std::process::Command;
            if let Ok(output) = Command::new("pgrep").args(["-i", "osu"]).output() {
                return output.status.success();
            }
        }

        false
    }

    /// Gets the available disk space at a path.
    fn get_available_disk_space(path: &Path) -> Result<u64> {
        #[cfg(windows)]
        {
            // Get the root of the path
            let _root = path.ancestors().last().unwrap_or(path);

            // For simplicity, return a large value if we can't determine
            // In production, use winapi GetDiskFreeSpaceExW
            Ok(u64::MAX)
        }

        #[cfg(unix)]
        {
            use std::process::Command;

            let output = Command::new("df")
                .args(["-B1", &path.to_string_lossy()])
                .output()?;

            let output_str = String::from_utf8_lossy(&output.stdout);
            let lines: Vec<&str> = output_str.lines().collect();

            if lines.len() >= 2 {
                let parts: Vec<&str> = lines[1].split_whitespace().collect();
                if parts.len() >= 4 {
                    if let Ok(available) = parts[3].parse::<u64>() {
                        return Ok(available);
                    }
                }
            }

            Ok(u64::MAX)
        }
    }

    /// Checks if we can write to a path.
    fn can_write_to(path: &Path) -> bool {
        if !path.exists() {
            // Check parent
            if let Some(parent) = path.parent() {
                return Self::can_write_to(parent);
            }
            return false;
        }

        // Try to create a temp file
        let test_path = path.join(".osu-sync-write-test");
        if File::create(&test_path).is_ok() {
            fs::remove_file(&test_path).ok();
            return true;
        }

        false
    }
}

/// Statistics from executing a single step.
#[derive(Debug, Default)]
struct StepStats {
    links_created: usize,
    space_saved: u64,
    warnings: Vec<String>,
}

/// Formats bytes into a human-readable string.
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_migration_step_description() {
        let step = MigrationStep::CheckPrerequisites;
        assert_eq!(step.description(), "Checking prerequisites");

        let step = MigrationStep::CopyBeatmaps {
            count: 100,
            size: 10 * 1024 * 1024 * 1024, // 10 GB
        };
        assert!(step.description().contains("100 beatmaps"));
    }

    #[test]
    fn test_migration_plan() {
        let mut plan = MigrationPlan::new(UnifiedStorageMode::StableMaster);
        assert_eq!(plan.step_count(), 0);

        plan.add_step(MigrationStep::CheckPrerequisites);
        plan.add_step(MigrationStep::VerifyIntegrity);
        assert_eq!(plan.step_count(), 2);

        plan.add_warning("Test warning");
        assert!(plan.has_warnings());
    }

    #[test]
    fn test_migration_progress() {
        let progress = MigrationProgress::new(2, 10, "Test step");
        assert!(progress.overall_progress() > 0.0);
        assert!(progress.overall_progress() < 100.0);
    }

    #[test]
    fn test_migration_result() {
        let result = MigrationResult::success(5, 1024 * 1024 * 1024);
        assert!(result.success);
        assert_eq!(result.links_created, 5);
        assert_eq!(result.space_saved_display(), "1.00 GB");

        let result = MigrationResult::failure(vec!["Test error".to_string()]);
        assert!(!result.success);
        assert_eq!(result.errors.len(), 1);
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.00 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
        assert_eq!(format_bytes(1024 * 1024 * 1024 * 1024), "1.00 TB");
    }

    #[test]
    fn test_backup_manifest_serialization() {
        let manifest = BackupManifest::new(
            PathBuf::from("/stable"),
            PathBuf::from("/lazer"),
            UnifiedStorageMode::StableMaster,
        );

        let json = serde_json::to_string(&manifest).unwrap();
        assert!(json.contains("stable"));
        assert!(json.contains("lazer"));

        let parsed: BackupManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.stable_path, PathBuf::from("/stable"));
    }

    // ============================================================================
    // Additional Migration Step Tests
    // ============================================================================

    #[test]
    fn test_all_migration_step_descriptions() {
        let steps = vec![
            MigrationStep::CheckPrerequisites,
            MigrationStep::BackupOriginal {
                path: PathBuf::from("/backup"),
            },
            MigrationStep::CreateSharedFolder {
                path: PathBuf::from("/shared"),
            },
            MigrationStep::CopyBeatmaps {
                count: 50,
                size: 1024 * 1024 * 1024,
            },
            MigrationStep::CreateJunctions { count: 10 },
            MigrationStep::CreateSymlinks { count: 5 },
            MigrationStep::VerifyIntegrity,
            MigrationStep::CleanupBackups,
            MigrationStep::UpdateManifest,
        ];

        for step in steps {
            let desc = step.description();
            assert!(
                !desc.is_empty(),
                "Step {:?} should have a description",
                step
            );
        }
    }

    #[test]
    fn test_migration_step_copy_beatmaps_size_format() {
        // Test different size formats
        let step = MigrationStep::CopyBeatmaps {
            count: 10,
            size: 500, // 500 bytes
        };
        assert!(step.description().contains("10 beatmaps"));

        let step = MigrationStep::CopyBeatmaps {
            count: 100,
            size: 1024 * 1024, // 1 MB
        };
        assert!(step.description().contains("100 beatmaps"));
    }

    // ============================================================================
    // Migration Plan Tests
    // ============================================================================

    #[test]
    fn test_migration_plan_all_modes() {
        for mode in [
            UnifiedStorageMode::Disabled,
            UnifiedStorageMode::StableMaster,
            UnifiedStorageMode::LazerMaster,
            UnifiedStorageMode::TrueUnified,
        ] {
            let plan = MigrationPlan::new(mode);
            assert_eq!(plan.mode, mode);
            assert_eq!(plan.step_count(), 0);
            assert!(!plan.has_warnings());
        }
    }

    #[test]
    fn test_migration_plan_multiple_warnings() {
        let mut plan = MigrationPlan::new(UnifiedStorageMode::StableMaster);

        plan.add_warning("Warning 1");
        plan.add_warning("Warning 2");
        plan.add_warning("Warning 3");

        assert!(plan.has_warnings());
        assert_eq!(plan.warnings.len(), 3);
    }

    #[test]
    fn test_migration_plan_step_iteration() {
        let mut plan = MigrationPlan::new(UnifiedStorageMode::StableMaster);

        plan.add_step(MigrationStep::CheckPrerequisites);
        plan.add_step(MigrationStep::BackupOriginal {
            path: PathBuf::from("/backup"),
        });
        plan.add_step(MigrationStep::VerifyIntegrity);

        assert_eq!(plan.step_count(), 3);

        // Verify steps are in order
        let steps: Vec<_> = plan.steps.iter().collect();
        assert!(matches!(steps[0], MigrationStep::CheckPrerequisites));
        assert!(matches!(steps[1], MigrationStep::BackupOriginal { .. }));
        assert!(matches!(steps[2], MigrationStep::VerifyIntegrity));
    }

    // ============================================================================
    // Migration Progress Tests
    // ============================================================================

    #[test]
    fn test_migration_progress_boundaries() {
        // At start
        let progress = MigrationProgress::new(0, 10, "Starting");
        assert!(progress.overall_progress() >= 0.0);

        // In middle
        let progress = MigrationProgress::new(1, 10, "Middle");
        let pct = progress.overall_progress();
        assert!(pct > 0.0 && pct < 100.0);

        // Near end
        let progress = MigrationProgress::new(9, 10, "Almost done");
        assert!(progress.overall_progress() > 50.0);
    }

    #[test]
    fn test_migration_progress_step_progress() {
        let mut progress = MigrationProgress::new(0, 5, "Step 1");

        // Update step progress
        progress.step_progress = 0.5;
        let overall = progress.overall_progress();

        // Verify step progress contributes to overall
        assert!(overall > 0.0);
    }

    #[test]
    fn test_migration_progress_bytes_tracking() {
        let mut progress = MigrationProgress::new(2, 5, "Copying files");
        progress.bytes_processed = 500 * 1024 * 1024; // 500 MB processed
        progress.total_bytes = 1024 * 1024 * 1024; // 1 GB total

        assert_eq!(progress.bytes_processed, 500 * 1024 * 1024);
        assert_eq!(progress.total_bytes, 1024 * 1024 * 1024);
    }

    // ============================================================================
    // Migration Result Tests
    // ============================================================================

    #[test]
    fn test_migration_result_size_display() {
        // Test various sizes
        let result = MigrationResult::success(1, 512); // 512 bytes
        assert_eq!(result.space_saved_display(), "512 B");

        let result = MigrationResult::success(1, 2048); // 2 KB
        assert_eq!(result.space_saved_display(), "2.00 KB");

        let result = MigrationResult::success(1, 5 * 1024 * 1024); // 5 MB
        assert_eq!(result.space_saved_display(), "5.00 MB");

        let result = MigrationResult::success(1, 2 * 1024 * 1024 * 1024); // 2 GB
        assert_eq!(result.space_saved_display(), "2.00 GB");
    }

    #[test]
    fn test_migration_result_multiple_errors() {
        let errors = vec![
            "Error 1: File not found".to_string(),
            "Error 2: Permission denied".to_string(),
            "Error 3: Disk full".to_string(),
        ];

        let result = MigrationResult::failure(errors.clone());

        assert!(!result.success);
        assert_eq!(result.errors.len(), 3);
        assert_eq!(result.links_created, 0);
        assert_eq!(result.space_saved, 0);
    }

    #[test]
    fn test_migration_result_warnings() {
        let mut result = MigrationResult::success(10, 1024 * 1024 * 1024);
        result.warnings.push("Some files were skipped".to_string());

        assert!(result.success);
        assert_eq!(result.warnings.len(), 1);
    }

    // ============================================================================
    // Backup Manifest Tests
    // ============================================================================

    #[test]
    fn test_backup_manifest_with_moved_paths() {
        let mut manifest = BackupManifest::new(
            PathBuf::from("/stable"),
            PathBuf::from("/lazer"),
            UnifiedStorageMode::TrueUnified,
        );

        // Add some moved paths (HashMap insert)
        manifest.moved_paths.insert(
            PathBuf::from("/stable/Songs"),
            PathBuf::from("/backup/Songs"),
        );
        manifest.moved_paths.insert(
            PathBuf::from("/lazer/Songs"),
            PathBuf::from("/backup/LazerSongs"),
        );

        assert_eq!(manifest.moved_paths.len(), 2);

        // Verify serialization works with paths
        let json = serde_json::to_string(&manifest).unwrap();
        assert!(json.contains("Songs"));

        let parsed: BackupManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.moved_paths.len(), 2);
    }

    #[test]
    fn test_backup_manifest_all_modes() {
        for mode in [
            UnifiedStorageMode::StableMaster,
            UnifiedStorageMode::LazerMaster,
            UnifiedStorageMode::TrueUnified,
        ] {
            let manifest =
                BackupManifest::new(PathBuf::from("/stable"), PathBuf::from("/lazer"), mode);

            assert_eq!(manifest.mode, mode);
            assert!(manifest.moved_paths.is_empty());
        }
    }

    // ============================================================================
    // Format Bytes Edge Cases
    // ============================================================================

    #[test]
    fn test_format_bytes_edge_cases() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1), "1 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1536), "1.50 KB"); // 1.5 KB
    }

    #[test]
    fn test_format_bytes_large_values() {
        // Test very large values
        let petabyte = 1024u64 * 1024 * 1024 * 1024 * 1024;
        let result = format_bytes(petabyte);
        assert!(result.contains("PB") || result.contains("TB")); // Either is acceptable
    }

    // ============================================================================
    // SharedResourceType Tests
    // ============================================================================

    #[test]
    fn test_shared_resource_type_folder_names() {
        assert_eq!(SharedResourceType::Beatmaps.folder_name(), "Songs");
        assert_eq!(SharedResourceType::Skins.folder_name(), "Skins");
        assert_eq!(SharedResourceType::Replays.folder_name(), "Replays");
        assert_eq!(SharedResourceType::Screenshots.folder_name(), "Screenshots");
        assert_eq!(SharedResourceType::Exports.folder_name(), "Exports");
        assert_eq!(SharedResourceType::Backgrounds.folder_name(), "Backgrounds");
    }

    #[test]
    fn test_shared_resource_type_display_names() {
        for resource in SharedResourceType::all() {
            let display = resource.display_name();
            assert!(!display.is_empty());
            // Display name should be human-readable
            assert!(display.chars().next().unwrap().is_uppercase());
        }
    }

    #[test]
    fn test_shared_resource_type_all_count() {
        assert_eq!(SharedResourceType::all().len(), 6);
    }
}
