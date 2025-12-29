//! Dry run mode for previewing sync operations without making changes

use crate::beatmap::BeatmapSet;
use crate::lazer::LazerBeatmapSet;

/// Action that would be taken for a beatmap set during sync
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DryRunAction {
    /// Will be imported to target
    Import,
    /// Already exists in target, will be skipped
    Skip,
    /// Duplicate detected in target
    Duplicate,
}

impl std::fmt::Display for DryRunAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Import => write!(f, "Import"),
            Self::Skip => write!(f, "Skip"),
            Self::Duplicate => write!(f, "Duplicate"),
        }
    }
}

/// A single item in the dry run preview
#[derive(Debug, Clone)]
pub struct DryRunItem {
    /// Online beatmap set ID (if available)
    pub set_id: Option<i32>,
    /// Folder name in osu!stable (always available for stable sets)
    pub folder_name: Option<String>,
    /// Title of the beatmap set
    pub title: String,
    /// Artist of the beatmap set
    pub artist: String,
    /// Action that would be taken
    pub action: DryRunAction,
    /// Estimated size in bytes
    pub size_bytes: u64,
    /// Number of difficulties in this set
    pub difficulty_count: usize,
}

impl DryRunItem {
    /// Create a new dry run item from a BeatmapSet
    pub fn from_beatmap_set(set: &BeatmapSet, action: DryRunAction) -> Self {
        let (title, artist) = if let Some(meta) = set.metadata() {
            (meta.title.clone(), meta.artist.clone())
        } else {
            ("Unknown".to_string(), "Unknown".to_string())
        };

        let size_bytes: u64 = set.files.iter().map(|f| f.size).sum();
        let difficulty_count = set.beatmaps.len();

        Self {
            set_id: set.id,
            folder_name: set.folder_name.clone(),
            title,
            artist,
            action,
            size_bytes,
            difficulty_count,
        }
    }

    /// Create a new dry run item from a LazerBeatmapSet
    pub fn from_lazer_set(set: &LazerBeatmapSet, action: DryRunAction) -> Self {
        let (title, artist) = if let Some(first) = set.beatmaps.first() {
            (first.metadata.title.clone(), first.metadata.artist.clone())
        } else {
            ("Unknown".to_string(), "Unknown".to_string())
        };

        // Size is not directly available in LazerBeatmapSet, estimate as 0 for now
        let size_bytes = 0u64;
        let difficulty_count = set.beatmaps.len();

        Self {
            set_id: set.online_id,
            folder_name: None, // Lazer doesn't use folder-based storage
            title,
            artist,
            action,
            size_bytes,
            difficulty_count,
        }
    }

    /// Get a display name for the item
    pub fn display_name(&self) -> String {
        if let Some(id) = self.set_id {
            format!("{} - {} [{}]", self.artist, self.title, id)
        } else {
            format!("{} - {}", self.artist, self.title)
        }
    }
}

/// Result of a dry run analysis
#[derive(Debug, Clone, Default)]
pub struct DryRunResult {
    /// All items that would be processed
    pub items: Vec<DryRunItem>,
    /// Total count of items to import
    pub total_import: usize,
    /// Total count of items to skip
    pub total_skip: usize,
    /// Total count of duplicate items
    pub total_duplicate: usize,
    /// Total size in bytes of items to import
    pub total_size_bytes: u64,
}

impl DryRunResult {
    /// Create a new empty dry run result
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an item to the result
    pub fn add_item(&mut self, item: DryRunItem) {
        match item.action {
            DryRunAction::Import => {
                self.total_import += 1;
                self.total_size_bytes += item.size_bytes;
            }
            DryRunAction::Skip => {
                self.total_skip += 1;
            }
            DryRunAction::Duplicate => {
                self.total_duplicate += 1;
            }
        }
        self.items.push(item);
    }

    /// Get total number of items
    pub fn total_items(&self) -> usize {
        self.items.len()
    }

    /// Format the total size as a human-readable string
    pub fn size_display(&self) -> String {
        format_bytes(self.total_size_bytes)
    }

    /// Estimate sync time based on size (rough approximation)
    /// Assumes ~10 MB/s average processing speed
    pub fn estimated_time_display(&self) -> String {
        let seconds = self.total_size_bytes as f64 / (10.0 * 1024.0 * 1024.0);
        if seconds < 60.0 {
            format!("~{:.0} sec", seconds.max(1.0))
        } else if seconds < 3600.0 {
            format!("~{:.0} min", seconds / 60.0)
        } else {
            format!("~{:.1} hr", seconds / 3600.0)
        }
    }

    /// Check if there's anything to import
    pub fn has_imports(&self) -> bool {
        self.total_import > 0
    }
}

/// Format bytes as a human-readable string
pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1 KB");
        assert_eq!(format_bytes(1536), "2 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.00 GB");
    }

    #[test]
    fn test_dry_run_result() {
        let mut result = DryRunResult::new();

        result.add_item(DryRunItem {
            set_id: Some(1),
            folder_name: Some("1 Artist - Test".to_string()),
            title: "Test".to_string(),
            artist: "Artist".to_string(),
            action: DryRunAction::Import,
            size_bytes: 1024 * 1024, // 1 MB
            difficulty_count: 3,
        });

        result.add_item(DryRunItem {
            set_id: Some(2),
            folder_name: Some("2 Artist 2 - Test 2".to_string()),
            title: "Test 2".to_string(),
            artist: "Artist 2".to_string(),
            action: DryRunAction::Skip,
            size_bytes: 512 * 1024,
            difficulty_count: 1,
        });

        assert_eq!(result.total_import, 1);
        assert_eq!(result.total_skip, 1);
        assert_eq!(result.total_duplicate, 0);
        assert_eq!(result.total_size_bytes, 1024 * 1024);
        assert!(result.has_imports());
    }
}
