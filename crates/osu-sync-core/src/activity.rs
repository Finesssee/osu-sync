//! Activity log for tracking user actions
//!
//! Provides a persistent log of recent actions like scans, syncs, exports, and errors.

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

/// Maximum number of log entries to keep
pub const MAX_LOG_ENTRIES: usize = 50;

/// Type of activity
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActivityType {
    /// Scanning for beatmaps
    Scan,
    /// Syncing beatmaps
    Sync,
    /// Exporting data
    Export,
    /// Backup operation
    Backup,
    /// Restore operation
    Restore,
    /// Media extraction
    MediaExtract,
    /// Replay export
    ReplayExport,
    /// Collection sync
    CollectionSync,
    /// Error occurred
    Error,
    /// Info message
    Info,
}

impl ActivityType {
    /// Get the display icon for this activity type
    pub fn icon(&self) -> &'static str {
        match self {
            ActivityType::Scan => "\u{1F50D}",       // magnifying glass
            ActivityType::Sync => "\u{1F504}",       // refresh
            ActivityType::Export => "\u{1F4E4}",     // outbox
            ActivityType::Backup => "\u{1F4BE}",     // floppy disk
            ActivityType::Restore => "\u{1F4E5}",    // inbox
            ActivityType::MediaExtract => "\u{1F3B5}", // music note
            ActivityType::ReplayExport => "\u{1F3AE}", // game controller
            ActivityType::CollectionSync => "\u{1F4C1}", // folder
            ActivityType::Error => "\u{274C}",       // cross mark
            ActivityType::Info => "\u{2139}",        // info
        }
    }

    /// Get the display name for this activity type
    pub fn display_name(&self) -> &'static str {
        match self {
            ActivityType::Scan => "Scan",
            ActivityType::Sync => "Sync",
            ActivityType::Export => "Export",
            ActivityType::Backup => "Backup",
            ActivityType::Restore => "Restore",
            ActivityType::MediaExtract => "Media",
            ActivityType::ReplayExport => "Replay",
            ActivityType::CollectionSync => "Collections",
            ActivityType::Error => "Error",
            ActivityType::Info => "Info",
        }
    }
}

/// A single activity log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEntry {
    /// Timestamp of the activity
    pub timestamp: DateTime<Local>,
    /// Type of activity
    pub activity_type: ActivityType,
    /// Description of what happened
    pub description: String,
    /// Additional details (optional)
    pub details: Option<String>,
}

impl ActivityEntry {
    /// Create a new activity entry with the current timestamp
    pub fn new(activity_type: ActivityType, description: impl Into<String>) -> Self {
        Self {
            timestamp: Local::now(),
            activity_type,
            description: description.into(),
            details: None,
        }
    }

    /// Create a new activity entry with details
    pub fn with_details(
        activity_type: ActivityType,
        description: impl Into<String>,
        details: impl Into<String>,
    ) -> Self {
        Self {
            timestamp: Local::now(),
            activity_type,
            description: description.into(),
            details: Some(details.into()),
        }
    }

    /// Format the timestamp for display
    pub fn formatted_time(&self) -> String {
        self.timestamp.format("%H:%M:%S").to_string()
    }

    /// Format the full timestamp with date
    pub fn formatted_datetime(&self) -> String {
        self.timestamp.format("%Y-%m-%d %H:%M:%S").to_string()
    }
}

/// Activity log manager
#[derive(Debug, Default)]
pub struct ActivityLog {
    /// In-memory log entries (most recent first)
    entries: Vec<ActivityEntry>,
}

impl ActivityLog {
    /// Create a new empty activity log
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Load activity log from file
    pub fn load() -> Self {
        let mut log = Self::new();
        if let Some(path) = Self::log_path() {
            if let Ok(file) = File::open(&path) {
                let reader = BufReader::new(file);
                for line in reader.lines().take(MAX_LOG_ENTRIES) {
                    if let Ok(line) = line {
                        if let Ok(entry) = serde_json::from_str::<ActivityEntry>(&line) {
                            log.entries.push(entry);
                        }
                    }
                }
            }
        }
        log
    }

    /// Save activity log to file
    pub fn save(&self) -> std::io::Result<()> {
        if let Some(path) = Self::log_path() {
            // Ensure directory exists
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }

            let mut file = File::create(&path)?;
            for entry in self.entries.iter().take(MAX_LOG_ENTRIES) {
                if let Ok(json) = serde_json::to_string(entry) {
                    writeln!(file, "{}", json)?;
                }
            }
        }
        Ok(())
    }

    /// Get the log file path
    fn log_path() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("osu-sync").join("activity.log"))
    }

    /// Add a new entry to the log
    pub fn add(&mut self, entry: ActivityEntry) {
        // Insert at the beginning (most recent first)
        self.entries.insert(0, entry);

        // Trim to max size
        if self.entries.len() > MAX_LOG_ENTRIES {
            self.entries.truncate(MAX_LOG_ENTRIES);
        }
    }

    /// Add a simple log entry
    pub fn log(&mut self, activity_type: ActivityType, description: impl Into<String>) {
        self.add(ActivityEntry::new(activity_type, description));
    }

    /// Add a log entry with details
    pub fn log_with_details(
        &mut self,
        activity_type: ActivityType,
        description: impl Into<String>,
        details: impl Into<String>,
    ) {
        self.add(ActivityEntry::with_details(activity_type, description, details));
    }

    /// Get all entries
    pub fn entries(&self) -> &[ActivityEntry] {
        &self.entries
    }

    /// Get the most recent N entries
    pub fn recent(&self, count: usize) -> &[ActivityEntry] {
        &self.entries[..count.min(self.entries.len())]
    }

    /// Clear all entries
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Get the number of entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if the log is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_activity_entry_creation() {
        let entry = ActivityEntry::new(ActivityType::Scan, "Scanned 100 beatmaps");
        assert_eq!(entry.activity_type, ActivityType::Scan);
        assert_eq!(entry.description, "Scanned 100 beatmaps");
        assert!(entry.details.is_none());
    }

    #[test]
    fn test_activity_log() {
        let mut log = ActivityLog::new();
        assert!(log.is_empty());

        log.log(ActivityType::Scan, "Scanned beatmaps");
        assert_eq!(log.len(), 1);

        log.log(ActivityType::Sync, "Synced beatmaps");
        assert_eq!(log.len(), 2);

        // Most recent should be first
        assert_eq!(log.entries()[0].activity_type, ActivityType::Sync);
    }
}
