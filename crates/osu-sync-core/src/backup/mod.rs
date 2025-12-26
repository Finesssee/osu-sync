//! Backup and restore functionality for osu! data
//!
//! This module provides functionality to create and restore backups of:
//! - osu!stable Songs folder
//! - osu!stable Collections (collection.db)
//! - osu!stable Scores (scores.db)
//! - osu!lazer data directory

mod archive;

pub use archive::*;

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

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
        self.create_backup_with_progress(target, source_path, None)
    }

    /// Create a backup with progress callback
    pub fn create_backup_with_progress(
        &self,
        target: BackupTarget,
        source_path: &Path,
        progress: Option<BackupProgressCallback>,
    ) -> Result<PathBuf> {
        self.ensure_backup_dir()?;

        // Generate backup filename with timestamp
        let timestamp = chrono_timestamp();
        let filename = format!("{}-{}.zip", target.file_prefix(), timestamp);
        let backup_path = self.backup_dir.join(&filename);

        // Create the archive
        create_backup_archive(source_path, &backup_path, target, progress)?;

        Ok(backup_path)
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

        let metadata = std::fs::metadata(path).ok()?;
        let created = metadata.modified().ok()?;
        let size_bytes = metadata.len();

        Some(BackupInfo {
            path: path.to_path_buf(),
            created,
            target,
            size_bytes,
        })
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
