//! Backup and restore functionality for osu! data
//!
//! This module provides functionality to create and restore backups of:
//! - osu!stable Songs folder
//! - osu!stable Collections (collection.db)
//! - osu!stable Scores (scores.db)
//! - osu!lazer data directory

mod archive;
mod options;

pub use archive::*;
pub use options::*;

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use zip::ZipArchive;

/// What to backup
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BackupTarget {
    /// osu!stable Songs folder
    StableSongs,
    /// osu!stable collection.db
    StableCollections,
    /// osu!stable scores.db
    StableScores,
    /// osu!lazer data directory
    LazerData,
    /// Everything (all targets)
    All,
}

impl BackupTarget {
    /// Get all individual backup targets
    pub fn all_targets() -> &'static [BackupTarget] {
        &[
            BackupTarget::StableSongs,
            BackupTarget::StableCollections,
            BackupTarget::StableScores,
            BackupTarget::LazerData,
        ]
    }

    /// Get user-friendly label
    pub fn label(&self) -> &'static str {
        match self {
            BackupTarget::StableSongs => "osu!stable Songs folder",
            BackupTarget::StableCollections => "osu!stable Collections",
            BackupTarget::StableScores => "osu!stable Scores",
            BackupTarget::LazerData => "osu!lazer Data",
            BackupTarget::All => "Everything",
        }
    }

    /// Get the filename prefix for this target
    pub fn file_prefix(&self) -> &'static str {
        match self {
            BackupTarget::StableSongs => "stable-songs",
            BackupTarget::StableCollections => "stable-collections",
            BackupTarget::StableScores => "stable-scores",
            BackupTarget::LazerData => "lazer-data",
            BackupTarget::All => "all",
        }
    }

    /// Parse from filename prefix
    pub fn from_prefix(prefix: &str) -> Option<Self> {
        match prefix {
            "stable-songs" => Some(BackupTarget::StableSongs),
            "stable-collections" => Some(BackupTarget::StableCollections),
            "stable-scores" => Some(BackupTarget::StableScores),
            "lazer-data" => Some(BackupTarget::LazerData),
            "all" => Some(BackupTarget::All),
            _ => None,
        }
    }
}

impl fmt::Display for BackupTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// Information about a backup
#[derive(Debug, Clone)]
pub struct BackupInfo {
    /// Path to the backup file
    pub path: PathBuf,
    /// When the backup was created
    pub created: SystemTime,
    /// What was backed up
    pub target: BackupTarget,
    /// Size of the backup in bytes
    pub size_bytes: u64,
    /// Whether this is an incremental backup
    pub is_incremental: bool,
    /// Metadata from inside the backup (if available)
    pub metadata: Option<BackupMetadata>,
}

impl BackupInfo {
    /// Get human-readable size
    pub fn size_display(&self) -> String {
        format_size(self.size_bytes)
    }

    /// Get human-readable age
    pub fn age_display(&self) -> String {
        format_age(self.created)
    }

    /// Get backup type display string
    pub fn type_display(&self) -> &'static str {
        if self.is_incremental {
            "Incremental"
        } else {
            "Full"
        }
    }
}

/// Progress callback for backup operations
pub type BackupProgressCallback = Box<dyn Fn(BackupProgress) + Send>;

/// Progress information during backup
#[derive(Debug, Clone)]
pub struct BackupProgress {
    /// Current phase
    pub phase: BackupPhase,
    /// Files processed
    pub files_processed: usize,
    /// Total files (if known)
    pub total_files: Option<usize>,
    /// Bytes written
    pub bytes_written: u64,
    /// Current file being processed
    pub current_file: Option<String>,
}

/// Phase of backup operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupPhase {
    /// Scanning files to backup
    Scanning,
    /// Creating archive
    Archiving,
    /// Finalizing
    Finalizing,
    /// Complete
    Complete,
}

impl fmt::Display for BackupPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BackupPhase::Scanning => write!(f, "Scanning files..."),
            BackupPhase::Archiving => write!(f, "Creating archive..."),
            BackupPhase::Finalizing => write!(f, "Finalizing..."),
            BackupPhase::Complete => write!(f, "Complete"),
        }
    }
}

/// Status of backup verification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationStatus {
    /// Backup is valid and can be restored
    Valid,
    /// Backup has issues but may still be restorable
    Warning,
    /// Backup is corrupt and cannot be restored
    Invalid,
}

impl fmt::Display for VerificationStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VerificationStatus::Valid => write!(f, "Valid"),
            VerificationStatus::Warning => write!(f, "Warning"),
            VerificationStatus::Invalid => write!(f, "Invalid"),
        }
    }
}

/// Issue found during backup verification
#[derive(Debug, Clone)]
pub struct VerificationIssue {
    /// Severity of the issue
    pub severity: IssueSeverity,
    /// Description of the issue
    pub message: String,
    /// File path if issue is file-specific
    pub file_path: Option<String>,
}

/// Severity of a verification issue
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueSeverity {
    /// Informational, not a problem
    Info,
    /// May cause issues but not critical
    Warning,
    /// Critical issue that prevents restore
    Error,
}

impl fmt::Display for IssueSeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IssueSeverity::Info => write!(f, "Info"),
            IssueSeverity::Warning => write!(f, "Warning"),
            IssueSeverity::Error => write!(f, "Error"),
        }
    }
}

/// Result of backup verification
#[derive(Debug, Clone)]
pub struct BackupVerificationResult {
    /// Overall verification status
    pub status: VerificationStatus,
    /// Total file count in the backup
    pub file_count: usize,
    /// Total size of files in the backup (uncompressed)
    pub total_size: u64,
    /// List of issues found
    pub issues: Vec<VerificationIssue>,
    /// Whether the backup can be opened
    pub can_open: bool,
    /// Whether all files are readable
    pub all_files_readable: bool,
}

impl BackupVerificationResult {
    /// Create a successful verification result
    pub fn valid(file_count: usize, total_size: u64) -> Self {
        Self {
            status: VerificationStatus::Valid,
            file_count,
            total_size,
            issues: Vec::new(),
            can_open: true,
            all_files_readable: true,
        }
    }

    /// Create a result indicating the backup cannot be opened
    pub fn cannot_open(message: String) -> Self {
        Self {
            status: VerificationStatus::Invalid,
            file_count: 0,
            total_size: 0,
            issues: vec![VerificationIssue {
                severity: IssueSeverity::Error,
                message,
                file_path: None,
            }],
            can_open: false,
            all_files_readable: false,
        }
    }

    /// Add an issue to the result
    pub fn add_issue(&mut self, issue: VerificationIssue) {
        // Update status based on issue severity
        match issue.severity {
            IssueSeverity::Error => self.status = VerificationStatus::Invalid,
            IssueSeverity::Warning => {
                if self.status == VerificationStatus::Valid {
                    self.status = VerificationStatus::Warning;
                }
            }
            IssueSeverity::Info => {}
        }
        self.issues.push(issue);
    }

    /// Check if the backup is restorable
    pub fn is_restorable(&self) -> bool {
        self.can_open && self.status != VerificationStatus::Invalid
    }

    /// Get human-readable size
    pub fn size_display(&self) -> String {
        format_size(self.total_size)
    }
}

/// Information about a file in a backup
#[derive(Debug, Clone)]
pub struct BackupFileInfo {
    /// Path within the archive
    pub path: String,
    /// Uncompressed size in bytes
    pub size: u64,
    /// Compressed size in bytes
    pub compressed_size: u64,
    /// Whether this is a directory
    pub is_directory: bool,
    /// CRC32 checksum if available
    pub crc32: Option<u32>,
}

impl BackupFileInfo {
    /// Get human-readable size
    pub fn size_display(&self) -> String {
        format_size(self.size)
    }

    /// Get compression ratio as a percentage
    pub fn compression_ratio(&self) -> f64 {
        if self.size > 0 {
            (1.0 - (self.compressed_size as f64 / self.size as f64)) * 100.0
        } else {
            0.0
        }
    }
}

/// Mode for handling existing files during restore
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RestoreMode {
    /// Overwrite existing files
    #[default]
    Overwrite,
    /// Skip files that already exist
    Skip,
    /// Rename existing files with a backup suffix
    Rename,
}

impl RestoreMode {
    /// Cycle to next restore mode
    pub fn next(&self) -> Self {
        match self {
            RestoreMode::Overwrite => RestoreMode::Skip,
            RestoreMode::Skip => RestoreMode::Rename,
            RestoreMode::Rename => RestoreMode::Overwrite,
        }
    }
}

impl fmt::Display for RestoreMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RestoreMode::Overwrite => write!(f, "Overwrite existing"),
            RestoreMode::Skip => write!(f, "Skip existing"),
            RestoreMode::Rename => write!(f, "Rename existing"),
        }
    }
}

/// Options for selective restore
#[derive(Debug, Clone, Default)]
pub struct RestoreOptions {
    /// Specific files to restore (None = all files)
    pub files_to_restore: Option<Vec<String>>,
    /// How to handle existing files
    pub restore_mode: RestoreMode,
    /// Whether to verify files after restore
    pub verify_after_restore: bool,
}

impl RestoreOptions {
    /// Create options to restore all files with overwrite mode
    pub fn all() -> Self {
        Self::default()
    }

    /// Create options to restore specific files
    pub fn selective(files: Vec<String>) -> Self {
        Self {
            files_to_restore: Some(files),
            ..Default::default()
        }
    }

    /// Set the restore mode
    pub fn with_mode(mut self, mode: RestoreMode) -> Self {
        self.restore_mode = mode;
        self
    }

    /// Enable verification after restore
    pub fn with_verification(mut self) -> Self {
        self.verify_after_restore = true;
        self
    }

    /// Check if a file should be restored based on these options
    pub fn should_restore(&self, file_path: &str) -> bool {
        match &self.files_to_restore {
            Some(files) => files.iter().any(|f| f == file_path || file_path.starts_with(&format!("{}/", f))),
            None => true,
        }
    }
}

/// Preview of what will happen during restore
#[derive(Debug, Clone)]
pub struct RestorePreview {
    /// Total files that would be restored
    pub files_to_restore: usize,
    /// Total size of files to restore
    pub total_size: u64,
    /// Files that would overwrite existing files
    pub overwrites: Vec<String>,
    /// Files that would be newly created
    pub new_files: Vec<String>,
    /// Files that would be skipped (based on mode)
    pub skipped: Vec<String>,
    /// Files that would be renamed
    pub renames: Vec<(String, String)>,
}

impl RestorePreview {
    /// Create an empty preview
    pub fn new() -> Self {
        Self {
            files_to_restore: 0,
            total_size: 0,
            overwrites: Vec::new(),
            new_files: Vec::new(),
            skipped: Vec::new(),
            renames: Vec::new(),
        }
    }

    /// Get human-readable size
    pub fn size_display(&self) -> String {
        format_size(self.total_size)
    }

    /// Check if there are any overwrites
    pub fn has_overwrites(&self) -> bool {
        !self.overwrites.is_empty()
    }
}

impl Default for RestorePreview {
    fn default() -> Self {
        Self::new()
    }
}

/// Manages backup operations
pub struct BackupManager {
    /// Directory to store backups
    backup_dir: PathBuf,
}

impl BackupManager {
    /// Create a new backup manager
    pub fn new(backup_dir: PathBuf) -> Self {
        Self { backup_dir }
    }

    /// Get the backup directory
    pub fn backup_dir(&self) -> &Path {
        &self.backup_dir
    }

    /// Ensure the backup directory exists
    fn ensure_backup_dir(&self) -> Result<()> {
        if !self.backup_dir.exists() {
            std::fs::create_dir_all(&self.backup_dir)?;
        }
        Ok(())
    }

    /// Create a backup of the specified target
    pub fn create_backup(&self, target: BackupTarget, source_path: &Path) -> Result<PathBuf> {
        self.create_backup_with_options(target, source_path, BackupOptions::default(), None)
    }

    /// Create a backup with progress callback (legacy compatibility)
    pub fn create_backup_with_progress(
        &self,
        target: BackupTarget,
        source_path: &Path,
        progress: Option<BackupProgressCallback>,
    ) -> Result<PathBuf> {
        self.create_backup_with_options(target, source_path, BackupOptions::default(), progress)
    }

    /// Create a backup with full options and progress callback
    pub fn create_backup_with_options(
        &self,
        target: BackupTarget,
        source_path: &Path,
        options: BackupOptions,
        progress: Option<BackupProgressCallback>,
    ) -> Result<PathBuf> {
        self.ensure_backup_dir()?;

        // Generate backup filename with timestamp
        let timestamp = chrono_timestamp();
        let mode_suffix = if options.mode == BackupMode::Incremental {
            "-inc"
        } else {
            ""
        };
        let filename = format!("{}-{}{}.zip", target.file_prefix(), timestamp, mode_suffix);
        let backup_path = self.backup_dir.join(&filename);

        // For incremental backups, try to find the previous manifest
        let previous_manifest = if options.mode == BackupMode::Incremental {
            self.find_latest_manifest(target)
        } else {
            None
        };

        // Create the archive with options
        let result = create_backup_archive_with_options(
            source_path,
            &backup_path,
            target,
            &options,
            previous_manifest.as_ref().map(|(_, m)| m),
            progress,
        )?;

        // Save the new manifest for future incremental backups
        let manifest_filename = BackupManifest::manifest_filename(&filename);
        let manifest_path = self.backup_dir.join(&manifest_filename);
        result.manifest.save(&manifest_path)?;

        Ok(backup_path)
    }

    /// Find the latest manifest for a given target
    fn find_latest_manifest(&self, target: BackupTarget) -> Option<(PathBuf, BackupManifest)> {
        if !self.backup_dir.exists() {
            return None;
        }

        let prefix = target.file_prefix();
        let mut manifests: Vec<_> = std::fs::read_dir(&self.backup_dir)
            .ok()?
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name();
                let name_str = name.to_string_lossy();
                name_str.starts_with(prefix) && name_str.ends_with(".manifest.json")
            })
            .collect();

        // Sort by modification time, newest first
        manifests.sort_by(|a, b| {
            let time_a = a.metadata().and_then(|m| m.modified()).ok();
            let time_b = b.metadata().and_then(|m| m.modified()).ok();
            time_b.cmp(&time_a)
        });

        // Try to load the newest manifest
        for entry in manifests {
            let path = entry.path();
            if let Ok(manifest) = BackupManifest::load(&path) {
                return Some((path, manifest));
            }
        }

        None
    }

    /// List all available backups
    pub fn list_backups(&self) -> Result<Vec<BackupInfo>> {
        if !self.backup_dir.exists() {
            return Ok(Vec::new());
        }

        let mut backups = Vec::new();

        for entry in std::fs::read_dir(&self.backup_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().map(|e| e == "zip").unwrap_or(false) {
                if let Some(info) = self.parse_backup_info(&path) {
                    backups.push(info);
                }
            }
        }

        // Sort by creation time, newest first
        backups.sort_by(|a, b| b.created.cmp(&a.created));

        Ok(backups)
    }

    /// Parse backup info from a backup file
    fn parse_backup_info(&self, path: &Path) -> Option<BackupInfo> {
        let filename = path.file_stem()?.to_str()?;

        // Parse target from filename prefix
        let target = if filename.starts_with("stable-songs") {
            BackupTarget::StableSongs
        } else if filename.starts_with("stable-collections") {
            BackupTarget::StableCollections
        } else if filename.starts_with("stable-scores") {
            BackupTarget::StableScores
        } else if filename.starts_with("lazer-data") {
            BackupTarget::LazerData
        } else if filename.starts_with("all") {
            BackupTarget::All
        } else {
            return None;
        };

        // Check if it's incremental from filename
        let is_incremental_from_name = filename.contains("-inc");

        let file_metadata = std::fs::metadata(path).ok()?;
        let created = file_metadata.modified().ok()?;
        let size_bytes = file_metadata.len();

        // Try to read metadata from inside the backup
        let backup_metadata = self.read_backup_metadata(path);

        Some(BackupInfo {
            path: path.to_path_buf(),
            created,
            target,
            size_bytes,
            is_incremental: backup_metadata
                .as_ref()
                .map(|m| m.is_incremental)
                .unwrap_or(is_incremental_from_name),
            metadata: backup_metadata,
        })
    }

    /// Read backup_info.json from inside a backup archive
    fn read_backup_metadata(&self, path: &Path) -> Option<BackupMetadata> {
        let file = File::open(path).ok()?;
        let mut archive = ZipArchive::new(file).ok()?;

        // Try to find backup_info.json
        let mut info_file = archive.by_name("backup_info.json").ok()?;
        let mut content = String::new();
        std::io::Read::read_to_string(&mut info_file, &mut content).ok()?;

        serde_json::from_str(&content).ok()
    }

    /// Restore a backup to the specified destination
    pub fn restore_backup(&self, backup_path: &Path, dest_path: &Path) -> Result<()> {
        self.restore_backup_with_progress(backup_path, dest_path, None)
    }

    /// Restore a backup with progress callback
    pub fn restore_backup_with_progress(
        &self,
        backup_path: &Path,
        dest_path: &Path,
        progress: Option<BackupProgressCallback>,
    ) -> Result<()> {
        if !backup_path.exists() {
            return Err(Error::Other(format!(
                "Backup file not found: {}",
                backup_path.display()
            )));
        }

        extract_backup_archive(backup_path, dest_path, progress)?;

        Ok(())
    }

    /// Delete a backup
    pub fn delete_backup(&self, backup_path: &Path) -> Result<()> {
        if backup_path.exists() {
            std::fs::remove_file(backup_path)?;
        }
        Ok(())
    }

    /// Verify backup integrity
    ///
    /// Checks that the ZIP file can be opened, lists all files,
    /// and optionally verifies file checksums.
    pub fn verify_backup(&self, backup_path: &Path) -> Result<BackupVerificationResult> {
        if !backup_path.exists() {
            return Ok(BackupVerificationResult::cannot_open(format!(
                "Backup file not found: {}",
                backup_path.display()
            )));
        }

        // Try to open the ZIP file
        let file = match File::open(backup_path) {
            Ok(f) => f,
            Err(e) => {
                return Ok(BackupVerificationResult::cannot_open(format!(
                    "Cannot open backup file: {}",
                    e
                )));
            }
        };

        let mut archive = match ZipArchive::new(file) {
            Ok(a) => a,
            Err(e) => {
                return Ok(BackupVerificationResult::cannot_open(format!(
                    "Invalid ZIP archive: {}",
                    e
                )));
            }
        };

        let mut file_count = 0usize;
        let mut total_size = 0u64;
        let mut result = BackupVerificationResult::valid(0, 0);

        // Check each file in the archive
        for i in 0..archive.len() {
            match archive.by_index(i) {
                Ok(file) => {
                    if !file.is_dir() {
                        file_count += 1;
                        total_size += file.size();
                    }

                    // Check for path traversal attacks
                    if file.name().contains("..") {
                        result.add_issue(VerificationIssue {
                            severity: IssueSeverity::Error,
                            message: "Path traversal detected".to_string(),
                            file_path: Some(file.name().to_string()),
                        });
                    }

                    // Check for empty file names
                    if file.name().is_empty() {
                        result.add_issue(VerificationIssue {
                            severity: IssueSeverity::Warning,
                            message: "Empty file name detected".to_string(),
                            file_path: None,
                        });
                    }
                }
                Err(e) => {
                    result.all_files_readable = false;
                    result.add_issue(VerificationIssue {
                        severity: IssueSeverity::Error,
                        message: format!("Cannot read file at index {}: {}", i, e),
                        file_path: None,
                    });
                }
            }
        }

        result.file_count = file_count;
        result.total_size = total_size;

        // Add informational message about backup size
        if file_count == 0 {
            result.add_issue(VerificationIssue {
                severity: IssueSeverity::Warning,
                message: "Backup contains no files".to_string(),
                file_path: None,
            });
        }

        Ok(result)
    }

    /// List contents of a backup archive
    ///
    /// Returns information about all files in the backup.
    pub fn list_backup_contents(&self, backup_path: &Path) -> Result<Vec<BackupFileInfo>> {
        if !backup_path.exists() {
            return Err(Error::Other(format!(
                "Backup file not found: {}",
                backup_path.display()
            )));
        }

        let file = File::open(backup_path)?;
        let mut archive = ZipArchive::new(file)?;
        let mut contents = Vec::with_capacity(archive.len());

        for i in 0..archive.len() {
            let file = archive.by_index(i)?;
            contents.push(BackupFileInfo {
                path: file.name().to_string(),
                size: file.size(),
                compressed_size: file.compressed_size(),
                is_directory: file.is_dir(),
                crc32: Some(file.crc32()),
            });
        }

        Ok(contents)
    }

    /// Preview what would happen during a restore
    ///
    /// Shows which files would be restored, overwritten, skipped, etc.
    pub fn preview_restore(
        &self,
        backup_path: &Path,
        dest_path: &Path,
        options: &RestoreOptions,
    ) -> Result<RestorePreview> {
        if !backup_path.exists() {
            return Err(Error::Other(format!(
                "Backup file not found: {}",
                backup_path.display()
            )));
        }

        let file = File::open(backup_path)?;
        let mut archive = ZipArchive::new(file)?;
        let mut preview = RestorePreview::new();

        for i in 0..archive.len() {
            let file = archive.by_index(i)?;
            if file.is_dir() {
                continue;
            }

            let file_name = file.name().to_string();

            // Check if this file should be restored
            if !options.should_restore(&file_name) {
                continue;
            }

            let dest_file = dest_path.join(&file_name);
            let file_size = file.size();

            if dest_file.exists() {
                match options.restore_mode {
                    RestoreMode::Overwrite => {
                        preview.overwrites.push(file_name);
                        preview.files_to_restore += 1;
                        preview.total_size += file_size;
                    }
                    RestoreMode::Skip => {
                        preview.skipped.push(file_name);
                    }
                    RestoreMode::Rename => {
                        // Generate new name for existing file
                        let new_name = Self::generate_backup_name(&dest_file);
                        preview.renames.push((dest_file.display().to_string(), new_name));
                        preview.files_to_restore += 1;
                        preview.total_size += file_size;
                    }
                }
            } else {
                preview.new_files.push(file_name);
                preview.files_to_restore += 1;
                preview.total_size += file_size;
            }
        }

        Ok(preview)
    }

    /// Restore backup with options
    ///
    /// Restores selected files from backup with specified mode.
    pub fn restore_backup_with_options(
        &self,
        backup_path: &Path,
        dest_path: &Path,
        options: &RestoreOptions,
        progress: Option<BackupProgressCallback>,
    ) -> Result<usize> {
        if !backup_path.exists() {
            return Err(Error::Other(format!(
                "Backup file not found: {}",
                backup_path.display()
            )));
        }

        let file = File::open(backup_path)?;
        let mut archive = ZipArchive::new(file)?;
        let total_files = archive.len();
        let mut files_restored = 0usize;
        let mut bytes_written = 0u64;

        // Create destination directory
        std::fs::create_dir_all(dest_path)?;

        // Notify scanning phase
        if let Some(ref cb) = progress {
            cb(BackupProgress {
                phase: BackupPhase::Scanning,
                files_processed: 0,
                total_files: Some(total_files),
                bytes_written: 0,
                current_file: None,
            });
        }

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let outpath = match file.enclosed_name() {
                Some(path) => dest_path.join(path),
                None => continue,
            };

            let filename = file.name().to_string();

            // Check if this file should be restored
            if !options.should_restore(&filename) {
                continue;
            }

            // Notify progress
            if let Some(ref cb) = progress {
                cb(BackupProgress {
                    phase: BackupPhase::Archiving,
                    files_processed: files_restored,
                    total_files: Some(total_files),
                    bytes_written,
                    current_file: Some(filename.clone()),
                });
            }

            if file.is_dir() {
                std::fs::create_dir_all(&outpath)?;
            } else {
                // Handle existing files based on mode
                if outpath.exists() {
                    match options.restore_mode {
                        RestoreMode::Overwrite => {
                            // Will overwrite below
                        }
                        RestoreMode::Skip => {
                            continue;
                        }
                        RestoreMode::Rename => {
                            let new_name = Self::generate_backup_name(&outpath);
                            std::fs::rename(&outpath, &new_name)?;
                        }
                    }
                }

                // Create parent directories if needed
                if let Some(parent) = outpath.parent() {
                    if !parent.exists() {
                        std::fs::create_dir_all(parent)?;
                    }
                }

                // Extract file
                let mut outfile = std::fs::File::create(&outpath)?;
                let bytes = std::io::copy(&mut file, &mut outfile)?;
                bytes_written += bytes;
                files_restored += 1;
            }
        }

        // Notify complete
        if let Some(ref cb) = progress {
            cb(BackupProgress {
                phase: BackupPhase::Complete,
                files_processed: files_restored,
                total_files: Some(total_files),
                bytes_written,
                current_file: None,
            });
        }

        Ok(files_restored)
    }

    /// Generate a backup name for an existing file (e.g., "file.txt" -> "file.txt.bak")
    fn generate_backup_name(path: &Path) -> String {
        let mut counter = 0;
        let base = path.display().to_string();
        loop {
            let new_name = if counter == 0 {
                format!("{}.bak", base)
            } else {
                format!("{}.bak.{}", base, counter)
            };
            if !Path::new(&new_name).exists() {
                return new_name;
            }
            counter += 1;
        }
    }

    /// Get the default backup directory
    pub fn default_backup_dir() -> PathBuf {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("osu-sync")
            .join("backups")
    }
}

/// Generate a timestamp string for filenames
fn chrono_timestamp() -> String {
    use std::time::{Duration, UNIX_EPOCH};

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO);

    // Simple timestamp format: YYYYMMDD-HHMMSS
    let secs = now.as_secs();
    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;

    // Calculate date (simplified, not accounting for leap years perfectly)
    let mut year = 1970u32;
    let mut remaining_days = days_since_epoch as u32;

    loop {
        let days_in_year = if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
            366
        } else {
            365
        };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    let days_in_months: [u32; 12] = if year % 4 == 0 && (year % 100 != 0 || year % 400 == 0) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1u32;
    for days in days_in_months.iter() {
        if remaining_days < *days {
            break;
        }
        remaining_days -= days;
        month += 1;
    }

    let day = remaining_days + 1;
    let hours = (time_of_day / 3600) as u32;
    let minutes = ((time_of_day % 3600) / 60) as u32;
    let seconds = (time_of_day % 60) as u32;

    format!("{:04}{:02}{:02}-{:02}{:02}{:02}", year, month, day, hours, minutes, seconds)
}

/// Format bytes to human-readable size
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Format age relative to now
fn format_age(time: SystemTime) -> String {
    let now = SystemTime::now();
    let duration = now.duration_since(time).unwrap_or_default();
    let secs = duration.as_secs();

    if secs < 60 {
        "just now".to_string()
    } else if secs < 3600 {
        format!("{} min ago", secs / 60)
    } else if secs < 86400 {
        format!("{} hours ago", secs / 3600)
    } else if secs < 604800 {
        format!("{} days ago", secs / 86400)
    } else {
        format!("{} weeks ago", secs / 604800)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backup_target_prefix() {
        assert_eq!(BackupTarget::StableSongs.file_prefix(), "stable-songs");
        assert_eq!(
            BackupTarget::from_prefix("stable-songs"),
            Some(BackupTarget::StableSongs)
        );
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(1024 * 1024), "1.00 MB");
        assert_eq!(format_size(1024 * 1024 * 1024), "1.00 GB");
    }
}
