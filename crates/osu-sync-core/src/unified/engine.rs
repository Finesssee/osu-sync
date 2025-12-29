//! Main Unified Storage Engine
//!
//! This module provides the orchestration layer that coordinates all unified
//! storage operations between osu! stable and lazer installations.
//!
//! The engine manages:
//! - Initial setup of unified storage (creating links)
//! - Synchronization between installations
//! - Verification of link integrity
//! - Repair of broken links
//! - Teardown and cleanup

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

use crate::error::{Error, Result};

use super::config::{SharedResourceType, UnifiedStorageConfig, UnifiedStorageMode};
use super::link::{copy_dir_recursive, LinkManager};
use super::manifest::{LinkedResource, LinkStatus, UnifiedManifest};

/// Result of a setup operation.
///
/// Contains statistics about the initial unified storage setup,
/// including the number of links created and any warnings encountered.
#[derive(Debug, Clone, Default)]
pub struct SetupResult {
    /// Number of filesystem links (symlinks/junctions) created.
    pub links_created: usize,
    /// Number of resources (beatmaps, skins, etc.) successfully linked.
    pub resources_linked: usize,
    /// Non-fatal warnings encountered during setup.
    pub warnings: Vec<String>,
}

impl SetupResult {
    /// Creates a new empty setup result.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if setup completed without warnings.
    pub fn is_clean(&self) -> bool {
        self.warnings.is_empty()
    }

    /// Adds a warning message to the result.
    pub fn add_warning(&mut self, warning: impl Into<String>) {
        self.warnings.push(warning.into());
    }
}

/// Result of a sync operation.
///
/// Contains statistics about what changed during synchronization.
#[derive(Debug, Clone, Default)]
pub struct SyncResult {
    /// Number of new links created.
    pub new_links: usize,
    /// Number of existing links updated.
    pub updated: usize,
    /// Number of stale links removed.
    pub removed: usize,
    /// Errors encountered during sync (non-fatal).
    pub errors: Vec<String>,
}

impl SyncResult {
    /// Creates a new empty sync result.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if sync completed without errors.
    pub fn is_success(&self) -> bool {
        self.errors.is_empty()
    }

    /// Returns the total number of changes made.
    pub fn total_changes(&self) -> usize {
        self.new_links + self.updated + self.removed
    }

    /// Adds an error message to the result.
    pub fn add_error(&mut self, error: impl Into<String>) {
        self.errors.push(error.into());
    }

    /// Merges another sync result into this one.
    pub fn merge(&mut self, other: SyncResult) {
        self.new_links += other.new_links;
        self.updated += other.updated;
        self.removed += other.removed;
        self.errors.extend(other.errors);
    }
}

/// Result of a verification operation.
///
/// Contains statistics about the current state of all links.
#[derive(Debug, Clone, Default)]
pub struct VerificationResult {
    /// Total number of links tracked in the manifest.
    pub total_links: usize,
    /// Number of links that are active and valid.
    pub active: usize,
    /// Number of links that are broken (target missing).
    pub broken: usize,
    /// Number of links that are stale (no longer needed).
    pub stale: usize,
}

impl VerificationResult {
    /// Creates a new empty verification result.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if all links are healthy.
    pub fn is_healthy(&self) -> bool {
        self.broken == 0 && self.stale == 0
    }

    /// Returns the percentage of healthy links (0.0 to 100.0).
    pub fn health_percentage(&self) -> f64 {
        if self.total_links == 0 {
            return 100.0;
        }
        (self.active as f64 / self.total_links as f64) * 100.0
    }
}

/// Result of a repair operation.
///
/// Contains statistics about what was fixed during repair.
#[derive(Debug, Clone, Default)]
pub struct RepairResult {
    /// Number of links successfully repaired.
    pub repaired: usize,
    /// Number of links that could not be repaired.
    pub failed: usize,
    /// Number of stale links that were removed.
    pub removed: usize,
}

impl RepairResult {
    /// Creates a new empty repair result.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if all repairs were successful.
    pub fn is_success(&self) -> bool {
        self.failed == 0
    }

    /// Returns the total number of actions taken.
    pub fn total_actions(&self) -> usize {
        self.repaired + self.failed + self.removed
    }
}

/// Main orchestration engine for unified storage operations.
///
/// The `UnifiedStorageEngine` coordinates all unified storage operations,
/// including setup, synchronization, verification, and repair of links
/// between osu! stable and lazer installations.
///
/// # Example
///
/// ```rust,ignore
/// use osu_sync_core::unified::{UnifiedStorageConfig, UnifiedStorageEngine};
/// use std::path::PathBuf;
///
/// let config = UnifiedStorageConfig::stable_master();
/// let stable = PathBuf::from("/path/to/osu-stable");
/// let lazer = PathBuf::from("/path/to/osu-lazer");
///
/// let mut engine = UnifiedStorageEngine::new(config, stable, lazer)?;
///
/// // Initial setup
/// let setup_result = engine.setup()?;
/// println!("Created {} links", setup_result.links_created);
///
/// // Verify integrity
/// let verify_result = engine.verify()?;
/// if !verify_result.is_healthy() {
///     let repair_result = engine.repair()?;
///     println!("Repaired {} links", repair_result.repaired);
/// }
/// ```
pub struct UnifiedStorageEngine {
    /// Configuration for unified storage behavior.
    config: UnifiedStorageConfig,
    /// Path to the osu! stable installation.
    stable_path: PathBuf,
    /// Path to the osu! lazer installation.
    lazer_path: PathBuf,
    /// Manifest tracking all linked resources.
    manifest: UnifiedManifest,
    /// Manager for filesystem link operations.
    link_manager: LinkManager,
}

impl UnifiedStorageEngine {
    /// Creates a new unified storage engine.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration for unified storage behavior
    /// * `stable` - Path to the osu! stable installation
    /// * `lazer` - Path to the osu! lazer installation
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The configuration is invalid
    /// - The stable or lazer paths don't exist
    /// - Failed to initialize the link manager
    pub fn new(config: UnifiedStorageConfig, stable: PathBuf, lazer: PathBuf) -> Result<Self> {
        // Validate configuration
        config.validate().map_err(Error::Config)?;

        // Validate paths exist
        if !stable.exists() {
            return Err(Error::OsuNotFound(stable));
        }
        if !lazer.exists() {
            return Err(Error::OsuNotFound(lazer));
        }

        // Initialize manifest with the configured mode
        let manifest = UnifiedManifest::new(config.mode);

        // Initialize link manager with configuration options
        let link_manager = LinkManager::new(config.should_use_junctions());

        Ok(Self {
            config,
            stable_path: stable,
            lazer_path: lazer,
            manifest,
            link_manager,
        })
    }

    /// Returns a reference to the current configuration.
    pub fn config(&self) -> &UnifiedStorageConfig {
        &self.config
    }

    /// Returns the path to the osu! stable installation.
    pub fn stable_path(&self) -> &PathBuf {
        &self.stable_path
    }

    /// Returns the path to the osu! lazer installation.
    pub fn lazer_path(&self) -> &PathBuf {
        &self.lazer_path
    }

    /// Returns a reference to the manifest.
    pub fn manifest(&self) -> &UnifiedManifest {
        &self.manifest
    }

    /// Performs initial setup of unified storage.
    ///
    /// This operation:
    /// 1. Analyzes both installations for shared resources
    /// 2. Backs up existing data if necessary
    /// 3. Creates symbolic links or junctions as configured
    /// 4. Updates the manifest with tracked links
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Unified storage is not enabled in the configuration
    /// - Failed to create required links
    /// - Insufficient permissions for link creation
    pub fn setup(&mut self) -> Result<SetupResult> {
        if !self.config.is_enabled() {
            return Err(Error::Config(
                "Unified storage is not enabled in configuration".to_string(),
            ));
        }

        tracing::info!(
            "Setting up unified storage in {:?} mode",
            self.config.mode
        );

        let mut result = SetupResult::new();

        // TODO: Implement setup logic
        // 1. Scan shared resources in both installations
        // 2. Determine which installation owns each resource
        // 3. Create links from non-master to master
        // 4. Update manifest

        match self.config.mode {
            UnifiedStorageMode::Disabled => {
                // Should not reach here due to earlier check
                unreachable!("Setup called with disabled mode");
            }
            UnifiedStorageMode::StableMaster => {
                self.setup_stable_master(&mut result)?;
            }
            UnifiedStorageMode::LazerMaster => {
                self.setup_lazer_master(&mut result)?;
            }
            UnifiedStorageMode::TrueUnified => {
                self.setup_true_unified(&mut result)?;
            }
        }

        tracing::info!(
            "Setup complete: {} links created, {} resources linked",
            result.links_created,
            result.resources_linked
        );

        Ok(result)
    }

    /// Synchronizes changes between installations.
    ///
    /// This operation:
    /// 1. Detects new, modified, or deleted resources
    /// 2. Creates or updates links as needed
    /// 3. Removes stale links
    /// 4. Updates the manifest
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Unified storage is not enabled
    /// - Critical sync operation fails
    pub fn sync(&mut self) -> Result<SyncResult> {
        if !self.config.is_enabled() {
            return Err(Error::Config(
                "Unified storage is not enabled in configuration".to_string(),
            ));
        }

        tracing::info!("Starting unified storage sync");

        let result = match self.config.mode {
            UnifiedStorageMode::Disabled => {
                unreachable!("Sync called with disabled mode");
            }
            UnifiedStorageMode::StableMaster => self.sync_stable_master()?,
            UnifiedStorageMode::LazerMaster => self.sync_lazer_master()?,
            UnifiedStorageMode::TrueUnified => self.sync_true_unified()?,
        };

        tracing::info!(
            "Sync complete: {} new, {} updated, {} removed",
            result.new_links,
            result.updated,
            result.removed
        );

        Ok(result)
    }

    /// Verifies the integrity of all links.
    ///
    /// This operation checks each tracked link to determine if it:
    /// - Is still valid and accessible
    /// - Points to a valid target
    /// - Is still needed based on current configuration
    ///
    /// # Errors
    ///
    /// Returns an error if verification cannot be performed.
    pub fn verify(&self) -> Result<VerificationResult> {
        tracing::info!("Verifying unified storage integrity");

        let mut result = VerificationResult::new();

        for resource in self.manifest.iter() {
            // Check if this resource is marked as stale in the manifest
            let is_stale = resource.status == LinkStatus::Stale;

            // Count and verify each link for this resource
            for link_path in &resource.link_paths {
                result.total_links += 1;

                // If the resource is marked as stale, count all its links as stale
                if is_stale {
                    result.stale += 1;
                    continue;
                }

                // Otherwise, verify the link's validity
                match self.link_manager.check_link(link_path) {
                    Ok(info) => {
                        if info.is_valid {
                            result.active += 1;
                        } else {
                            result.broken += 1;
                        }
                    }
                    Err(_) => {
                        result.broken += 1;
                    }
                }
            }
        }

        tracing::info!(
            "Verification complete: {} total, {} active, {} broken, {} stale",
            result.total_links,
            result.active,
            result.broken,
            result.stale
        );

        Ok(result)
    }

    /// Repairs broken or stale links.
    ///
    /// This operation:
    /// 1. Attempts to recreate broken links
    /// 2. Removes stale links that are no longer needed
    /// 3. Updates the manifest with current state
    ///
    /// # Errors
    ///
    /// Returns an error if repair cannot be performed.
    pub fn repair(&mut self) -> Result<RepairResult> {
        tracing::info!("Repairing unified storage links");

        let mut result = RepairResult::new();

        // Run verification to ensure manifest status is up to date
        let _verification = self.verify()?;

        // Collect resources needing attention to avoid borrow issues
        // This includes Broken, Stale, and Pending resources
        let resources_to_repair: Vec<_> = self
            .manifest
            .find_needing_attention()
            .into_iter()
            .map(|r| (r.source_path.clone(), r.link_paths.clone(), r.status))
            .collect();

        // Track which resources were successfully repaired so we can update their status
        let mut repaired_sources: Vec<PathBuf> = Vec::new();

        for (source_path, link_paths, status) in resources_to_repair {
            // Skip stale resources here - they're handled separately below
            if status == LinkStatus::Stale {
                continue;
            }

            let mut all_links_ok = true;

            for link_path in &link_paths {
                match self.link_manager.check_link(link_path) {
                    Ok(info) => {
                        if !info.is_valid {
                            // Attempt to repair by recreating link
                            match self.link_manager.create_link(link_path, &source_path) {
                                Ok(_) => {
                                    result.repaired += 1;
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "Failed to repair link {}: {}",
                                        link_path.display(),
                                        e
                                    );
                                    result.failed += 1;
                                    all_links_ok = false;
                                }
                            }
                        }
                    }
                    Err(_) => {
                        // Link doesn't exist, try to create it
                        match self.link_manager.create_link(link_path, &source_path) {
                            Ok(_) => {
                                result.repaired += 1;
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to create link {}: {}",
                                    link_path.display(),
                                    e
                                );
                                result.failed += 1;
                                all_links_ok = false;
                            }
                        }
                    }
                }
            }

            // If all links for this resource were successfully repaired, mark for status update
            if all_links_ok && !link_paths.is_empty() {
                repaired_sources.push(source_path);
            }
        }

        // Update status to Active for successfully repaired resources
        for source_path in repaired_sources {
            self.manifest.update_status(&source_path, LinkStatus::Active);
        }

        // Handle stale links - remove them from the manifest
        let stale_resources: Vec<_> = self
            .manifest
            .find_by_status(LinkStatus::Stale)
            .into_iter()
            .map(|r| r.source_path.clone())
            .collect();

        for source_path in stale_resources {
            if self.manifest.remove_resource(&source_path) {
                result.removed += 1;
            }
        }

        tracing::info!(
            "Repair complete: {} repaired, {} failed, {} removed",
            result.repaired,
            result.failed,
            result.removed
        );

        Ok(result)
    }

    /// Tears down unified storage, restoring independent installations.
    ///
    /// This operation:
    /// 1. Removes all symbolic links and junctions
    /// 2. Clears the manifest
    ///
    /// Note: Restoring original data from backups is not currently implemented.
    /// If backup restoration is needed in the future, it would require:
    /// - A backup system that preserves original data before link creation
    /// - Logic to copy data back from the master location to the link locations
    /// - Handling of conflicts when data has changed in the master location
    ///
    /// # Errors
    ///
    /// Returns an error if teardown cannot be completed.
    pub fn teardown(&mut self) -> Result<()> {
        tracing::info!("Tearing down unified storage");

        // Collect all link paths to avoid borrow issues
        let all_link_paths: Vec<PathBuf> = self
            .manifest
            .iter()
            .flat_map(|r| r.link_paths.clone())
            .collect();

        let total_links = all_link_paths.len();
        let mut removed_count = 0;

        for link_path in all_link_paths {
            if let Err(e) = LinkManager::remove_link(&link_path) {
                tracing::warn!(
                    "Failed to remove link {}: {}",
                    link_path.display(),
                    e
                );
            } else {
                removed_count += 1;
            }
        }

        self.manifest.clear();

        tracing::info!(
            "Teardown complete: removed {}/{} links",
            removed_count,
            total_links
        );

        Ok(())
    }

    // -------------------------------------------------------------------------
    // Mode-specific setup implementations
    // -------------------------------------------------------------------------

    /// Sets up unified storage with stable as the master.
    ///
    /// In this mode, osu! stable owns the canonical copies and
    /// osu! lazer links to stable's resources.
    fn setup_stable_master(&mut self, result: &mut SetupResult) -> Result<()> {
        tracing::debug!("Setting up StableMaster mode");

        for resource_type in self.config.shared_resources_iter() {
            let folder_name = resource_type.folder_name();

            let stable_resource = self.stable_path.join(folder_name);
            let lazer_resource = self.lazer_path.join(folder_name);

            // Check if the resource exists in stable
            if !stable_resource.exists() {
                result.add_warning(format!(
                    "{} not found in stable installation",
                    folder_name
                ));
                continue;
            }

            // If lazer already has content at that path, back it up
            if lazer_resource.exists() {
                // Check if it's already a link pointing to the right target
                if LinkManager::is_link(&lazer_resource) {
                    if let Ok(target) = LinkManager::read_link(&lazer_resource) {
                        // Normalize paths for comparison
                        let stable_canonical = stable_resource.canonicalize().ok();
                        let target_canonical = target.canonicalize().ok();
                        if stable_canonical.is_some() && stable_canonical == target_canonical {
                            tracing::debug!(
                                "{} already linked correctly, skipping",
                                folder_name
                            );
                            // Still add to manifest if not already tracked
                            if self.manifest.find_by_source(&stable_resource).is_none() {
                                self.manifest.add_resource(LinkedResource::active(
                                    *resource_type,
                                    stable_resource.clone(),
                                    vec![lazer_resource.clone()],
                                    None,
                                ));
                                result.resources_linked += 1;
                            }
                            continue;
                        }
                    }
                    // Link exists but points to wrong target - remove it
                    tracing::debug!(
                        "Removing existing link at {} (points to wrong target)",
                        lazer_resource.display()
                    );
                    if let Err(e) = LinkManager::remove_link(&lazer_resource) {
                        result.add_warning(format!(
                            "Failed to remove existing link at {}: {}",
                            lazer_resource.display(),
                            e
                        ));
                        continue;
                    }
                } else {
                    // It's a real directory, back it up
                    let backup_path = self.lazer_path.join(format!("{}_backup", folder_name));
                    tracing::info!(
                        "Backing up existing {} to {}",
                        lazer_resource.display(),
                        backup_path.display()
                    );

                    // If backup already exists, remove it first
                    if backup_path.exists() {
                        if let Err(e) = fs::remove_dir_all(&backup_path) {
                            result.add_warning(format!(
                                "Failed to remove old backup at {}: {}",
                                backup_path.display(),
                                e
                            ));
                            continue;
                        }
                    }

                    if let Err(e) = fs::rename(&lazer_resource, &backup_path) {
                        result.add_warning(format!(
                            "Failed to backup {} to {}: {}",
                            lazer_resource.display(),
                            backup_path.display(),
                            e
                        ));
                        continue;
                    }
                }
            }

            // Create link from lazer location to stable location
            tracing::debug!(
                "Creating link {} -> {}",
                lazer_resource.display(),
                stable_resource.display()
            );

            match self.link_manager.link_directory(&stable_resource, &lazer_resource) {
                Ok(link_info) => {
                    tracing::info!(
                        "Created {} link: {} -> {}",
                        link_info.link_type,
                        lazer_resource.display(),
                        stable_resource.display()
                    );

                    // Add to manifest
                    self.manifest.add_resource(LinkedResource::active(
                        *resource_type,
                        stable_resource.clone(),
                        vec![lazer_resource.clone()],
                        None,
                    ));

                    result.links_created += 1;
                    result.resources_linked += 1;
                }
                Err(e) => {
                    result.add_warning(format!(
                        "Failed to create link for {}: {}",
                        folder_name,
                        e
                    ));
                }
            }
        }

        Ok(())
    }

    /// Sets up unified storage with lazer as the master.
    ///
    /// In this mode, osu! lazer owns the canonical copies and
    /// osu! stable links to lazer's resources.
    fn setup_lazer_master(&mut self, result: &mut SetupResult) -> Result<()> {
        tracing::debug!("Setting up LazerMaster mode");

        for resource_type in self.config.shared_resources_iter() {
            let folder_name = resource_type.folder_name();

            let stable_resource = self.stable_path.join(folder_name);
            let lazer_resource = self.lazer_path.join(folder_name);

            // Check if the resource exists in lazer
            if !lazer_resource.exists() {
                result.add_warning(format!(
                    "{} not found in lazer installation",
                    folder_name
                ));
                continue;
            }

            // If stable already has content at that path, handle it
            if stable_resource.exists() {
                // Check if it's already a link pointing to the right target
                if LinkManager::is_link(&stable_resource) {
                    if let Ok(target) = LinkManager::read_link(&stable_resource) {
                        // Normalize paths for comparison
                        let lazer_canonical = lazer_resource.canonicalize().ok();
                        let target_canonical = target.canonicalize().ok();
                        if lazer_canonical.is_some() && lazer_canonical == target_canonical {
                            tracing::debug!(
                                "{} already linked correctly, skipping",
                                folder_name
                            );
                            // Still add to manifest if not already tracked
                            if self.manifest.find_by_source(&lazer_resource).is_none() {
                                self.manifest.add_resource(LinkedResource::active(
                                    *resource_type,
                                    lazer_resource.clone(),
                                    vec![stable_resource.clone()],
                                    None,
                                ));
                                result.resources_linked += 1;
                            }
                            continue;
                        }
                    }
                    // Link exists but points to wrong target - remove it
                    tracing::debug!(
                        "Removing existing link at {} (points to wrong target)",
                        stable_resource.display()
                    );
                    if let Err(e) = LinkManager::remove_link(&stable_resource) {
                        result.add_warning(format!(
                            "Failed to remove existing link at {}: {}",
                            stable_resource.display(),
                            e
                        ));
                        continue;
                    }
                } else {
                    // It's a real directory, back it up
                    let backup_path = self.stable_path.join(format!("{}_backup", folder_name));
                    tracing::info!(
                        "Backing up existing {} to {}",
                        stable_resource.display(),
                        backup_path.display()
                    );

                    // If backup already exists, remove it first
                    if backup_path.exists() {
                        if let Err(e) = fs::remove_dir_all(&backup_path) {
                            result.add_warning(format!(
                                "Failed to remove old backup at {}: {}",
                                backup_path.display(),
                                e
                            ));
                            continue;
                        }
                    }

                    if let Err(e) = fs::rename(&stable_resource, &backup_path) {
                        result.add_warning(format!(
                            "Failed to backup {} to {}: {}",
                            stable_resource.display(),
                            backup_path.display(),
                            e
                        ));
                        continue;
                    }
                }
            }

            // Create link from stable location to lazer location
            tracing::debug!(
                "Creating link {} -> {}",
                stable_resource.display(),
                lazer_resource.display()
            );

            match self.link_manager.create_link(&stable_resource, &lazer_resource) {
                Ok(link_info) => {
                    tracing::info!(
                        "Created {} link: {} -> {}",
                        link_info.link_type,
                        stable_resource.display(),
                        lazer_resource.display()
                    );

                    // Add to manifest (lazer is the source, stable is the link)
                    self.manifest.add_resource(LinkedResource::active(
                        *resource_type,
                        lazer_resource.clone(),
                        vec![stable_resource.clone()],
                        None,
                    ));

                    result.links_created += 1;
                    result.resources_linked += 1;
                }
                Err(e) => {
                    result.add_warning(format!(
                        "Failed to create link for {}: {}",
                        folder_name,
                        e
                    ));
                }
            }
        }

        Ok(())
    }

    /// Sets up unified storage with a shared third-party location.
    ///
    /// In this mode, both installations link to a shared location
    /// that is independent of either installation.
    fn setup_true_unified(&mut self, result: &mut SetupResult) -> Result<()> {
        tracing::debug!("Setting up TrueUnified mode");

        let shared_path = self.config.get_shared_path().ok_or_else(|| {
            Error::Config("TrueUnified mode requires a shared path".to_string())
        })?.clone();

        // Collect resource types to avoid borrow issues
        let resource_types: Vec<SharedResourceType> =
            self.config.shared_resources_iter().cloned().collect();

        for resource_type in resource_types {
            let folder_name = resource_type.folder_name();

            let shared_resource = shared_path.join(folder_name);
            let stable_resource = self.stable_path.join(folder_name);
            let lazer_resource = self.lazer_path.join(folder_name);

            tracing::debug!(
                "Setting up shared {} at {}",
                folder_name,
                shared_resource.display()
            );

            // Step 1: Create the shared folder if it doesn't exist
            if !shared_resource.exists() {
                fs::create_dir_all(&shared_resource).map_err(|e| {
                    Error::Other(format!(
                        "Failed to create shared directory {}: {}",
                        shared_resource.display(),
                        e
                    ))
                })?;
                tracing::debug!("Created shared directory: {}", shared_resource.display());
            }

            // Step 2: Migrate content from BOTH installations to shared location
            // Prefer stable content first, then add unique lazer content
            if stable_resource.exists() && !LinkManager::is_link(&stable_resource) {
                Self::migrate_directory_contents(&stable_resource, &shared_resource)?;
                tracing::debug!(
                    "Migrated stable content from {} to {}",
                    stable_resource.display(),
                    shared_resource.display()
                );
            }

            if lazer_resource.exists() && !LinkManager::is_link(&lazer_resource) {
                // Only copy unique content from lazer (files that don't exist in shared)
                Self::migrate_directory_contents(&lazer_resource, &shared_resource)?;
                tracing::debug!(
                    "Migrated unique lazer content from {} to {}",
                    lazer_resource.display(),
                    shared_resource.display()
                );
            }

            // Step 3: Back up both stable and lazer folders (rename to {folder}_backup)
            let mut links_created_for_resource = 0;

            // Back up stable folder if it exists and is not already a link
            if stable_resource.exists() && !LinkManager::is_link(&stable_resource) {
                let backup_path = self.stable_path.join(format!("{}_backup", folder_name));

                // Remove old backup if it exists
                if backup_path.exists() {
                    if let Err(e) = fs::remove_dir_all(&backup_path) {
                        result.add_warning(format!(
                            "Failed to remove old stable backup at {}: {}",
                            backup_path.display(),
                            e
                        ));
                    }
                }

                if let Err(e) = fs::rename(&stable_resource, &backup_path) {
                    result.add_warning(format!(
                        "Failed to backup stable {} to {}: {}",
                        stable_resource.display(),
                        backup_path.display(),
                        e
                    ));
                    continue;
                }
                tracing::debug!(
                    "Backed up stable {} to {}",
                    stable_resource.display(),
                    backup_path.display()
                );
            }

            // Back up lazer folder if it exists and is not already a link
            if lazer_resource.exists() && !LinkManager::is_link(&lazer_resource) {
                let backup_path = self.lazer_path.join(format!("{}_backup", folder_name));

                // Remove old backup if it exists
                if backup_path.exists() {
                    if let Err(e) = fs::remove_dir_all(&backup_path) {
                        result.add_warning(format!(
                            "Failed to remove old lazer backup at {}: {}",
                            backup_path.display(),
                            e
                        ));
                    }
                }

                if let Err(e) = fs::rename(&lazer_resource, &backup_path) {
                    result.add_warning(format!(
                        "Failed to backup lazer {} to {}: {}",
                        lazer_resource.display(),
                        backup_path.display(),
                        e
                    ));
                    continue;
                }
                tracing::debug!(
                    "Backed up lazer {} to {}",
                    lazer_resource.display(),
                    backup_path.display()
                );
            }

            // Step 4: Create links from BOTH stable and lazer to the shared location
            // Remove existing links that point to wrong target
            if stable_resource.exists() && LinkManager::is_link(&stable_resource) {
                if let Ok(target) = LinkManager::read_link(&stable_resource) {
                    let shared_canonical = shared_resource.canonicalize().ok();
                    let target_canonical = target.canonicalize().ok();
                    if shared_canonical != target_canonical {
                        // Link points to wrong target, remove it
                        if let Err(e) = LinkManager::remove_link(&stable_resource) {
                            result.add_warning(format!(
                                "Failed to remove old stable link: {}",
                                e
                            ));
                        }
                    }
                }
            }

            if lazer_resource.exists() && LinkManager::is_link(&lazer_resource) {
                if let Ok(target) = LinkManager::read_link(&lazer_resource) {
                    let shared_canonical = shared_resource.canonicalize().ok();
                    let target_canonical = target.canonicalize().ok();
                    if shared_canonical != target_canonical {
                        // Link points to wrong target, remove it
                        if let Err(e) = LinkManager::remove_link(&lazer_resource) {
                            result.add_warning(format!(
                                "Failed to remove old lazer link: {}",
                                e
                            ));
                        }
                    }
                }
            }

            // Create stable link to shared location
            if !stable_resource.exists() {
                match self.link_manager.link_directory(&shared_resource, &stable_resource) {
                    Ok(link_info) => {
                        tracing::info!(
                            "Created {} link: {} -> {}",
                            link_info.link_type,
                            stable_resource.display(),
                            shared_resource.display()
                        );
                        links_created_for_resource += 1;
                    }
                    Err(e) => {
                        result.add_warning(format!(
                            "Failed to create stable link for {}: {}",
                            folder_name, e
                        ));
                    }
                }
            } else if LinkManager::is_link(&stable_resource) {
                // Link already exists and points to correct target
                links_created_for_resource += 1;
            }

            // Create lazer link to shared location
            if !lazer_resource.exists() {
                match self.link_manager.link_directory(&shared_resource, &lazer_resource) {
                    Ok(link_info) => {
                        tracing::info!(
                            "Created {} link: {} -> {}",
                            link_info.link_type,
                            lazer_resource.display(),
                            shared_resource.display()
                        );
                        links_created_for_resource += 1;
                    }
                    Err(e) => {
                        result.add_warning(format!(
                            "Failed to create lazer link for {}: {}",
                            folder_name, e
                        ));
                    }
                }
            } else if LinkManager::is_link(&lazer_resource) {
                // Link already exists and points to correct target
                links_created_for_resource += 1;
            }

            // Step 5: Track in manifest (source = shared path, link_paths = [stable_path, lazer_path])
            if links_created_for_resource > 0 {
                let linked_resource = LinkedResource::active(
                    resource_type,
                    shared_resource.clone(),
                    vec![stable_resource.clone(), lazer_resource.clone()],
                    None,
                );
                self.manifest.add_resource(linked_resource);

                result.links_created += links_created_for_resource;
                result.resources_linked += 1;
            }
        }

        Ok(())
    }

    /// Migrates all contents from source directory to destination.
    /// Skips files that already exist in the destination.
    fn migrate_directory_contents(src: &std::path::Path, dst: &std::path::Path) -> Result<()> {
        if !src.is_dir() {
            return Ok(());
        }

        for entry in fs::read_dir(src).map_err(|e| {
            Error::Other(format!("Failed to read directory {}: {}", src.display(), e))
        })? {
            let entry = entry.map_err(|e| {
                Error::Other(format!("Failed to read directory entry: {}", e))
            })?;
            let src_path = entry.path();
            let file_name = entry.file_name();
            let dst_path = dst.join(&file_name);

            // Skip if destination already exists (prefer existing content)
            if dst_path.exists() {
                continue;
            }

            if src_path.is_dir() {
                copy_dir_recursive(&src_path, &dst_path)?;
            } else {
                fs::copy(&src_path, &dst_path).map_err(|e| {
                    Error::Other(format!(
                        "Failed to copy {} to {}: {}",
                        src_path.display(),
                        dst_path.display(),
                        e
                    ))
                })?;
            }
        }

        Ok(())
    }

    // -------------------------------------------------------------------------
    // Mode-specific sync implementations
    // -------------------------------------------------------------------------

    /// Syncs changes in StableMaster mode.
    ///
    /// Detects new resources in stable and creates links in lazer.
    fn sync_stable_master(&mut self) -> Result<SyncResult> {
        tracing::debug!("Syncing in StableMaster mode");

        let mut result = SyncResult::new();

        // For now, focus on Beatmaps (Songs folder) as the primary use case
        // Other resource types (Skins, etc.) are folder-level links and don't
        // need individual item tracking

        let songs_folder = self.stable_path.join("Songs");
        let lazer_songs = self.lazer_path.join("Songs");

        if !songs_folder.exists() {
            tracing::debug!("Songs folder not found in stable, nothing to sync");
            return Ok(result);
        }

        // Collect current beatmap folders in stable
        let mut stable_beatmaps: HashSet<PathBuf> = HashSet::new();
        if let Ok(entries) = fs::read_dir(&songs_folder) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    stable_beatmaps.insert(path);
                }
            }
        }

        // Get all beatmap resources currently in manifest
        let manifest_beatmaps: HashSet<PathBuf> = self
            .manifest
            .find_by_type(SharedResourceType::Beatmaps)
            .iter()
            .map(|r| r.source_path.clone())
            .collect();

        // Find new entries (in stable but not in manifest)
        for beatmap_path in &stable_beatmaps {
            if !manifest_beatmaps.contains(beatmap_path) {
                // This is a new beatmap folder
                let folder_name = match beatmap_path.file_name() {
                    Some(name) => name,
                    None => continue,
                };
                let link_path = lazer_songs.join(folder_name);

                // Check if link already exists and is valid
                if link_path.exists() || link_path.symlink_metadata().is_ok() {
                    if LinkManager::is_link(&link_path) {
                        if let Ok(target) = LinkManager::read_link(&link_path) {
                            let beatmap_canonical = beatmap_path.canonicalize().ok();
                            let target_canonical = target.canonicalize().ok();
                            if beatmap_canonical.is_some() && beatmap_canonical == target_canonical {
                                // Link exists and is correct, just add to manifest
                                self.manifest.add_resource(LinkedResource::active(
                                    SharedResourceType::Beatmaps,
                                    beatmap_path.clone(),
                                    vec![link_path],
                                    None,
                                ));
                                result.new_links += 1;
                                continue;
                            }
                        }
                        // Link exists but points to wrong target - remove and recreate
                        if let Err(e) = LinkManager::remove_link(&link_path) {
                            result.add_error(format!(
                                "Failed to remove stale link {}: {}",
                                link_path.display(),
                                e
                            ));
                            continue;
                        }
                    } else {
                        // Regular directory exists - skip (don't overwrite user data)
                        tracing::debug!(
                            "Skipping {} - directory already exists in lazer",
                            folder_name.to_string_lossy()
                        );
                        continue;
                    }
                }

                // Create the link
                match self.link_manager.link_directory(beatmap_path, &link_path) {
                    Ok(_) => {
                        self.manifest.add_resource(LinkedResource::active(
                            SharedResourceType::Beatmaps,
                            beatmap_path.clone(),
                            vec![link_path],
                            None,
                        ));
                        result.new_links += 1;
                    }
                    Err(e) => {
                        result.add_error(format!(
                            "Failed to create link for {}: {}",
                            folder_name.to_string_lossy(),
                            e
                        ));
                    }
                }
            }
        }

        // Find removed entries (in manifest but not in stable)
        // Collect paths first to avoid borrow issues
        let stale_paths: Vec<PathBuf> = manifest_beatmaps
            .iter()
            .filter(|path| !stable_beatmaps.contains(*path))
            .cloned()
            .collect();

        for stale_path in stale_paths {
            // Mark as stale in manifest
            if self.manifest.update_status(&stale_path, LinkStatus::Stale) {
                result.removed += 1;
                tracing::debug!(
                    "Marked {} as stale (no longer in stable)",
                    stale_path.display()
                );
            }
        }

        // Verify existing links are still valid
        let existing_resources: Vec<(PathBuf, Vec<PathBuf>)> = self
            .manifest
            .find_by_type(SharedResourceType::Beatmaps)
            .iter()
            .filter(|r| r.status == LinkStatus::Active)
            .map(|r| (r.source_path.clone(), r.link_paths.clone()))
            .collect();

        for (source_path, link_paths) in existing_resources {
            for link_path in link_paths {
                // Check if link is still valid
                if !LinkManager::is_link(&link_path) {
                    // Link is broken or missing - try to recreate
                    if source_path.exists() {
                        match self.link_manager.link_directory(&source_path, &link_path) {
                            Ok(_) => {
                                result.updated += 1;
                                tracing::debug!("Recreated link: {}", link_path.display());
                            }
                            Err(e) => {
                                result.add_error(format!(
                                    "Failed to recreate link {}: {}",
                                    link_path.display(),
                                    e
                                ));
                                self.manifest.update_status(&source_path, LinkStatus::Broken);
                            }
                        }
                    } else {
                        // Source no longer exists
                        self.manifest.update_status(&source_path, LinkStatus::Stale);
                        result.removed += 1;
                    }
                }
            }
        }

        Ok(result)
    }

    /// Syncs changes in LazerMaster mode.
    ///
    /// Detects new resources in lazer and creates links in stable.
    fn sync_lazer_master(&mut self) -> Result<SyncResult> {
        tracing::debug!("Syncing in LazerMaster mode");

        let mut result = SyncResult::new();

        // For now, focus on Beatmaps (Songs folder) as the primary use case
        // Other resource types (Skins, etc.) are folder-level links and don't
        // need individual item tracking

        let lazer_songs = self.lazer_path.join("Songs");
        let stable_songs = self.stable_path.join("Songs");

        if !lazer_songs.exists() {
            tracing::debug!("Songs folder not found in lazer, nothing to sync");
            return Ok(result);
        }

        // Collect current beatmap folders in lazer
        let mut lazer_beatmaps: HashSet<PathBuf> = HashSet::new();
        if let Ok(entries) = fs::read_dir(&lazer_songs) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.is_dir() {
                    lazer_beatmaps.insert(path);
                }
            }
        }

        // Get all beatmap resources currently in manifest
        let manifest_beatmaps: HashSet<PathBuf> = self
            .manifest
            .find_by_type(SharedResourceType::Beatmaps)
            .iter()
            .map(|r| r.source_path.clone())
            .collect();

        // Find new entries (in lazer but not in manifest)
        for beatmap_path in &lazer_beatmaps {
            if !manifest_beatmaps.contains(beatmap_path) {
                // This is a new beatmap folder
                let folder_name = match beatmap_path.file_name() {
                    Some(name) => name,
                    None => continue,
                };
                let link_path = stable_songs.join(folder_name);

                // Check if link already exists and is valid
                if link_path.exists() || link_path.symlink_metadata().is_ok() {
                    if LinkManager::is_link(&link_path) {
                        if let Ok(target) = LinkManager::read_link(&link_path) {
                            let beatmap_canonical = beatmap_path.canonicalize().ok();
                            let target_canonical = target.canonicalize().ok();
                            if beatmap_canonical.is_some() && beatmap_canonical == target_canonical {
                                // Link exists and is correct, just add to manifest
                                self.manifest.add_resource(LinkedResource::active(
                                    SharedResourceType::Beatmaps,
                                    beatmap_path.clone(),
                                    vec![link_path],
                                    None,
                                ));
                                result.new_links += 1;
                                continue;
                            }
                        }
                        // Link exists but points to wrong target - remove and recreate
                        if let Err(e) = LinkManager::remove_link(&link_path) {
                            result.add_error(format!(
                                "Failed to remove stale link {}: {}",
                                link_path.display(),
                                e
                            ));
                            continue;
                        }
                    } else {
                        // Regular directory exists - skip (don't overwrite user data)
                        tracing::debug!(
                            "Skipping {} - directory already exists in stable",
                            folder_name.to_string_lossy()
                        );
                        continue;
                    }
                }

                // Create the link (stable link -> lazer source)
                match self.link_manager.create_link(&link_path, beatmap_path) {
                    Ok(_) => {
                        self.manifest.add_resource(LinkedResource::active(
                            SharedResourceType::Beatmaps,
                            beatmap_path.clone(),
                            vec![link_path],
                            None,
                        ));
                        result.new_links += 1;
                    }
                    Err(e) => {
                        result.add_error(format!(
                            "Failed to create link for {}: {}",
                            folder_name.to_string_lossy(),
                            e
                        ));
                    }
                }
            }
        }

        // Find removed entries (in manifest but not in lazer)
        // Collect paths first to avoid borrow issues
        let stale_paths: Vec<PathBuf> = manifest_beatmaps
            .iter()
            .filter(|path| !lazer_beatmaps.contains(*path))
            .cloned()
            .collect();

        for stale_path in stale_paths {
            // Mark as stale in manifest
            if self.manifest.update_status(&stale_path, LinkStatus::Stale) {
                result.removed += 1;
                tracing::debug!(
                    "Marked {} as stale (no longer in lazer)",
                    stale_path.display()
                );
            }
        }

        // Verify existing links are still valid
        let existing_resources: Vec<(PathBuf, Vec<PathBuf>)> = self
            .manifest
            .find_by_type(SharedResourceType::Beatmaps)
            .iter()
            .filter(|r| r.status == LinkStatus::Active)
            .map(|r| (r.source_path.clone(), r.link_paths.clone()))
            .collect();

        for (source_path, link_paths) in existing_resources {
            for link_path in link_paths {
                // Check if link is still valid
                if !LinkManager::is_link(&link_path) {
                    // Link is broken or missing - try to recreate
                    if source_path.exists() {
                        match self.link_manager.create_link(&link_path, &source_path) {
                            Ok(_) => {
                                result.updated += 1;
                                tracing::debug!("Recreated link: {}", link_path.display());
                            }
                            Err(e) => {
                                result.add_error(format!(
                                    "Failed to recreate link {}: {}",
                                    link_path.display(),
                                    e
                                ));
                                self.manifest.update_status(&source_path, LinkStatus::Broken);
                            }
                        }
                    } else {
                        // Source no longer exists
                        self.manifest.update_status(&source_path, LinkStatus::Stale);
                        result.removed += 1;
                    }
                }
            }
        }

        Ok(result)
    }

    /// Syncs changes in TrueUnified mode.
    ///
    /// Ensures both installations are properly linked to shared location.
    fn sync_true_unified(&mut self) -> Result<SyncResult> {
        tracing::debug!("Syncing in TrueUnified mode");

        let mut result = SyncResult::new();

        // Step 1: Verify shared location exists
        let shared_path = match self.config.get_shared_path() {
            Some(path) => path.clone(),
            None => {
                result.add_error("TrueUnified mode requires a shared path".to_string());
                return Ok(result);
            }
        };

        if !shared_path.exists() {
            result.add_error(format!(
                "Shared location does not exist: {}",
                shared_path.display()
            ));
            return Ok(result);
        }

        // Collect resource types to avoid borrow issues
        let resource_types: Vec<SharedResourceType> =
            self.config.shared_resources_iter().cloned().collect();

        for resource_type in resource_types {
            let folder_name = resource_type.folder_name();

            let shared_resource = shared_path.join(folder_name);
            let stable_resource = self.stable_path.join(folder_name);
            let lazer_resource = self.lazer_path.join(folder_name);

            // Skip if shared resource doesn't exist
            if !shared_resource.exists() {
                tracing::debug!(
                    "Shared {} does not exist, skipping",
                    shared_resource.display()
                );
                continue;
            }

            // Step 2: Check both stable and lazer links point to shared location
            let mut stable_link_ok = false;
            let mut lazer_link_ok = false;

            // Check stable link
            if stable_resource.exists() {
                if LinkManager::is_link(&stable_resource) {
                    if let Ok(target) = LinkManager::read_link(&stable_resource) {
                        let shared_canonical = shared_resource.canonicalize().ok();
                        let target_canonical = target.canonicalize().ok();
                        if shared_canonical.is_some() && shared_canonical == target_canonical {
                            stable_link_ok = true;
                        } else {
                            // Link points to wrong target
                            tracing::debug!(
                                "Stable {} link points to wrong target",
                                folder_name
                            );
                        }
                    }
                } else {
                    // Not a link - this is a problem in TrueUnified mode
                    tracing::debug!(
                        "Stable {} is not a link (should be linked to shared)",
                        folder_name
                    );
                }
            }

            // Check lazer link
            if lazer_resource.exists() {
                if LinkManager::is_link(&lazer_resource) {
                    if let Ok(target) = LinkManager::read_link(&lazer_resource) {
                        let shared_canonical = shared_resource.canonicalize().ok();
                        let target_canonical = target.canonicalize().ok();
                        if shared_canonical.is_some() && shared_canonical == target_canonical {
                            lazer_link_ok = true;
                        } else {
                            // Link points to wrong target
                            tracing::debug!(
                                "Lazer {} link points to wrong target",
                                folder_name
                            );
                        }
                    }
                } else {
                    // Not a link - this is a problem in TrueUnified mode
                    tracing::debug!(
                        "Lazer {} is not a link (should be linked to shared)",
                        folder_name
                    );
                }
            }

            // Step 3: Repair any broken links
            // Repair stable link if needed
            if !stable_link_ok {
                // Remove existing path if it's a wrong link
                if stable_resource.exists() && LinkManager::is_link(&stable_resource) {
                    if let Err(e) = LinkManager::remove_link(&stable_resource) {
                        result.add_error(format!(
                            "Failed to remove broken stable link for {}: {}",
                            folder_name, e
                        ));
                        continue;
                    }
                }

                // Create link if path doesn't exist
                if !stable_resource.exists() {
                    match self.link_manager.link_directory(&shared_resource, &stable_resource) {
                        Ok(link_info) => {
                            tracing::info!(
                                "Repaired stable {} link: {} -> {}",
                                link_info.link_type,
                                stable_resource.display(),
                                shared_resource.display()
                            );
                            result.new_links += 1;
                            stable_link_ok = true;
                        }
                        Err(e) => {
                            result.add_error(format!(
                                "Failed to create stable link for {}: {}",
                                folder_name, e
                            ));
                        }
                    }
                }
            }

            // Repair lazer link if needed
            if !lazer_link_ok {
                // Remove existing path if it's a wrong link
                if lazer_resource.exists() && LinkManager::is_link(&lazer_resource) {
                    if let Err(e) = LinkManager::remove_link(&lazer_resource) {
                        result.add_error(format!(
                            "Failed to remove broken lazer link for {}: {}",
                            folder_name, e
                        ));
                        continue;
                    }
                }

                // Create link if path doesn't exist
                if !lazer_resource.exists() {
                    match self.link_manager.link_directory(&shared_resource, &lazer_resource) {
                        Ok(link_info) => {
                            tracing::info!(
                                "Repaired lazer {} link: {} -> {}",
                                link_info.link_type,
                                lazer_resource.display(),
                                shared_resource.display()
                            );
                            result.new_links += 1;
                            lazer_link_ok = true;
                        }
                        Err(e) => {
                            result.add_error(format!(
                                "Failed to create lazer link for {}: {}",
                                folder_name, e
                            ));
                        }
                    }
                }
            }

            // Step 4: Update manifest
            // Check if this resource is already tracked
            if let Some(existing) = self.manifest.find_by_source_mut(&shared_resource) {
                // Update status based on link states
                if stable_link_ok && lazer_link_ok {
                    existing.set_status(LinkStatus::Active);
                    result.updated += 1;
                } else {
                    existing.set_status(LinkStatus::Broken);
                }
            } else if stable_link_ok || lazer_link_ok {
                // Add new resource to manifest
                let mut link_paths = Vec::new();
                if stable_link_ok {
                    link_paths.push(stable_resource.clone());
                }
                if lazer_link_ok {
                    link_paths.push(lazer_resource.clone());
                }

                let status = if stable_link_ok && lazer_link_ok {
                    LinkStatus::Active
                } else {
                    LinkStatus::Broken
                };

                let mut linked_resource = LinkedResource::new(
                    resource_type,
                    shared_resource.clone(),
                    link_paths,
                );
                linked_resource.set_status(status);
                self.manifest.add_resource(linked_resource);
            }
        }

        // Check for stale manifest entries (resources no longer in shared location)
        let stale_resources: Vec<PathBuf> = self
            .manifest
            .iter()
            .filter(|r| !r.source_path.exists())
            .map(|r| r.source_path.clone())
            .collect();

        for stale_path in stale_resources {
            self.manifest.update_status(&stale_path, LinkStatus::Stale);
            result.removed += 1;
            tracing::debug!(
                "Marked {} as stale (no longer exists)",
                stale_path.display()
            );
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_result() {
        let mut result = SetupResult::new();
        assert!(result.is_clean());

        result.add_warning("Test warning");
        assert!(!result.is_clean());
        assert_eq!(result.warnings.len(), 1);
    }

    #[test]
    fn test_sync_result() {
        let mut result = SyncResult::new();
        assert!(result.is_success());
        assert_eq!(result.total_changes(), 0);

        result.new_links = 5;
        result.updated = 3;
        result.removed = 1;
        assert_eq!(result.total_changes(), 9);

        result.add_error("Test error");
        assert!(!result.is_success());
    }

    #[test]
    fn test_sync_result_merge() {
        let mut result1 = SyncResult::new();
        result1.new_links = 5;
        result1.updated = 2;

        let mut result2 = SyncResult::new();
        result2.new_links = 3;
        result2.removed = 1;

        result1.merge(result2);

        assert_eq!(result1.new_links, 8);
        assert_eq!(result1.updated, 2);
        assert_eq!(result1.removed, 1);
    }

    #[test]
    fn test_verification_result() {
        let mut result = VerificationResult::new();
        assert!(result.is_healthy());
        assert_eq!(result.health_percentage(), 100.0);

        result.total_links = 10;
        result.active = 8;
        result.broken = 2;
        assert!(!result.is_healthy());
        assert_eq!(result.health_percentage(), 80.0);
    }

    #[test]
    fn test_repair_result() {
        let mut result = RepairResult::new();
        assert!(result.is_success());
        assert_eq!(result.total_actions(), 0);

        result.repaired = 5;
        result.removed = 2;
        assert!(result.is_success());
        assert_eq!(result.total_actions(), 7);

        result.failed = 1;
        assert!(!result.is_success());
        assert_eq!(result.total_actions(), 8);
    }
}
