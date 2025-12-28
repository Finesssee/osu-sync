//! Backup options for compression and incremental backups

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::time::SystemTime;

use super::BackupTarget;

/// Compression level for backups
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CompressionLevel {
    /// Fast compression (level 0 - store only)
    Fast,
    /// Normal/balanced compression (level 6)
    #[default]
    Normal,
    /// Best compression (level 9)
    Best,
}

impl CompressionLevel {
    /// Get the zip compression level value
    pub fn to_zip_level(&self) -> u32 {
        match self {
            CompressionLevel::Fast => 0,
            CompressionLevel::Normal => 6,
            CompressionLevel::Best => 9,
        }
    }

    /// Get user-friendly label
    pub fn label(&self) -> &'static str {
        match self {
            CompressionLevel::Fast => "Fast (no compression)",
            CompressionLevel::Normal => "Normal (balanced)",
            CompressionLevel::Best => "Best (slower, smaller)",
        }
    }

    /// Get short label for display
    pub fn short_label(&self) -> &'static str {
        match self {
            CompressionLevel::Fast => "Fast",
            CompressionLevel::Normal => "Normal",
            CompressionLevel::Best => "Best",
        }
    }

    /// Cycle to next compression level
    pub fn next(&self) -> Self {
        match self {
            CompressionLevel::Fast => CompressionLevel::Normal,
            CompressionLevel::Normal => CompressionLevel::Best,
            CompressionLevel::Best => CompressionLevel::Fast,
        }
    }
}

impl fmt::Display for CompressionLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// Backup mode - full or incremental
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum BackupMode {
    /// Full backup - includes all files
    #[default]
    Full,
    /// Incremental backup - only changed/new files since last backup
    Incremental,
}

impl BackupMode {
    /// Get user-friendly label
    pub fn label(&self) -> &'static str {
        match self {
            BackupMode::Full => "Full backup",
            BackupMode::Incremental => "Incremental (changes only)",
        }
    }

    /// Get short label for display
    pub fn short_label(&self) -> &'static str {
        match self {
            BackupMode::Full => "Full",
            BackupMode::Incremental => "Incremental",
        }
    }

    /// Toggle between modes
    pub fn toggle(&self) -> Self {
        match self {
            BackupMode::Full => BackupMode::Incremental,
            BackupMode::Incremental => BackupMode::Full,
        }
    }
}

impl fmt::Display for BackupMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// Options for creating a backup
#[derive(Debug, Clone, Default)]
pub struct BackupOptions {
    /// Compression level
    pub compression: CompressionLevel,
    /// Backup mode (full or incremental)
    pub mode: BackupMode,
}

impl BackupOptions {
    /// Create new backup options with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Set compression level
    pub fn with_compression(mut self, level: CompressionLevel) -> Self {
        self.compression = level;
        self
    }

    /// Set backup mode
    pub fn with_mode(mut self, mode: BackupMode) -> Self {
        self.mode = mode;
        self
    }
}

/// Manifest entry for tracking file state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestEntry {
    /// Relative file path
    pub path: String,
    /// File modification time (unix timestamp)
    pub modified: u64,
    /// Simple hash of file content (first 64KB + size)
    pub hash: String,
    /// File size in bytes
    pub size: u64,
}

/// Manifest file stored alongside backups for incremental tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupManifest {
    /// Backup target type
    pub target: BackupTarget,
    /// Timestamp when this manifest was created
    pub created: u64,
    /// Map of file paths to their manifest entries
    pub files: HashMap<String, ManifestEntry>,
    /// Whether this was an incremental backup
    pub incremental: bool,
    /// Base backup filename this incremental is based on (if incremental)
    pub base_backup: Option<String>,
}

impl BackupManifest {
    /// Create a new empty manifest
    pub fn new(target: BackupTarget, incremental: bool, base_backup: Option<String>) -> Self {
        let now = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            target,
            created: now,
            files: HashMap::new(),
            incremental,
            base_backup,
        }
    }

    /// Add a file entry to the manifest
    pub fn add_entry(&mut self, entry: ManifestEntry) {
        self.files.insert(entry.path.clone(), entry);
    }

    /// Check if a file has changed compared to a previous manifest
    pub fn file_changed(&self, path: &str, modified: u64, hash: &str) -> bool {
        match self.files.get(path) {
            Some(entry) => entry.modified != modified || entry.hash != hash,
            None => true, // New file
        }
    }

    /// Get manifest filename for a backup
    pub fn manifest_filename(backup_filename: &str) -> String {
        if let Some(base) = backup_filename.strip_suffix(".zip") {
            format!("{}.manifest.json", base)
        } else {
            format!("{}.manifest.json", backup_filename)
        }
    }

    /// Load manifest from file
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        serde_json::from_str(&content).map_err(|e| Error::Other(format!("Invalid manifest: {}", e)))
    }

    /// Save manifest to file
    pub fn save(&self, path: &Path) -> Result<()> {
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| Error::Other(format!("Failed to serialize manifest: {}", e)))?;
        std::fs::write(path, content)?;
        Ok(())
    }
}

/// Metadata stored inside each backup archive (backup_info.json)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupMetadata {
    /// Human-readable creation date
    pub creation_date: String,
    /// Unix timestamp of creation
    pub created_timestamp: u64,
    /// Target type
    pub target: BackupTarget,
    /// Backup mode
    pub mode: BackupMode,
    /// Compression level used
    pub compression: CompressionLevel,
    /// Number of files in backup
    pub file_count: usize,
    /// Total uncompressed size in bytes
    pub total_size: u64,
    /// Whether this is an incremental backup
    pub is_incremental: bool,
    /// Base backup this incremental is based on
    pub base_backup: Option<String>,
    /// osu-sync version that created this backup
    pub osu_sync_version: String,
}

impl BackupMetadata {
    /// Create new metadata for a backup
    pub fn new(
        target: BackupTarget,
        mode: BackupMode,
        compression: CompressionLevel,
        file_count: usize,
        total_size: u64,
        base_backup: Option<String>,
    ) -> Self {
        let now = SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            creation_date: format_timestamp(now),
            created_timestamp: now,
            target,
            mode,
            compression,
            file_count,
            total_size,
            is_incremental: mode == BackupMode::Incremental,
            base_backup,
            osu_sync_version: env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// Serialize to JSON bytes for inclusion in archive
    pub fn to_json_bytes(&self) -> Result<Vec<u8>> {
        serde_json::to_vec_pretty(self)
            .map_err(|e| Error::Other(format!("Failed to serialize metadata: {}", e)))
    }
}

/// Compute a simple hash for incremental backup comparison
/// Uses first 64KB of content + file size
pub fn compute_simple_hash(path: &Path) -> Result<String> {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut file = File::open(path)?;
    let metadata = file.metadata()?;
    let size = metadata.len();

    let mut buffer = vec![0u8; 65536.min(size as usize)];
    let bytes_read = file.read(&mut buffer)?;

    let mut hasher = DefaultHasher::new();
    size.hash(&mut hasher);
    buffer[..bytes_read].hash(&mut hasher);

    Ok(format!("{:016x}", hasher.finish()))
}

/// Format a unix timestamp to a human-readable date string
fn format_timestamp(secs: u64) -> String {
    let days_since_epoch = secs / 86400;
    let time_of_day = secs % 86400;

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

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02} UTC",
        year, month, day, hours, minutes, seconds
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compression_level() {
        assert_eq!(CompressionLevel::Fast.to_zip_level(), 0);
        assert_eq!(CompressionLevel::Normal.to_zip_level(), 6);
        assert_eq!(CompressionLevel::Best.to_zip_level(), 9);

        assert_eq!(CompressionLevel::Fast.next(), CompressionLevel::Normal);
        assert_eq!(CompressionLevel::Normal.next(), CompressionLevel::Best);
        assert_eq!(CompressionLevel::Best.next(), CompressionLevel::Fast);
    }

    #[test]
    fn test_backup_mode() {
        assert_eq!(BackupMode::Full.toggle(), BackupMode::Incremental);
        assert_eq!(BackupMode::Incremental.toggle(), BackupMode::Full);
    }
}
