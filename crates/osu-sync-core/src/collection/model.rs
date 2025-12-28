//! Data models for beatmap collections

use serde::{Deserialize, Serialize};
use std::fmt;

/// A beatmap collection containing a name and list of beatmap MD5 hashes
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Collection {
    /// Name of the collection
    pub name: String,
    /// MD5 hashes of beatmaps in this collection
    pub beatmap_hashes: Vec<String>,
}

impl Collection {
    /// Create a new empty collection with the given name
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            beatmap_hashes: Vec::new(),
        }
    }

    /// Create a new collection with the given name and hashes
    pub fn with_hashes(name: impl Into<String>, hashes: Vec<String>) -> Self {
        Self {
            name: name.into(),
            beatmap_hashes: hashes,
        }
    }

    /// Number of beatmaps in this collection
    pub fn len(&self) -> usize {
        self.beatmap_hashes.len()
    }

    /// Check if the collection is empty
    pub fn is_empty(&self) -> bool {
        self.beatmap_hashes.is_empty()
    }
}

/// Strategy for syncing collections
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CollectionSyncStrategy {
    /// Merge: Add source beatmaps to existing target collection
    #[default]
    Merge,
    /// Replace: Overwrite target collection entirely with source
    Replace,
}

impl fmt::Display for CollectionSyncStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CollectionSyncStrategy::Merge => write!(f, "Merge"),
            CollectionSyncStrategy::Replace => write!(f, "Replace"),
        }
    }
}

/// Direction for collection sync
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CollectionSyncDirection {
    /// Sync from osu!stable to osu!lazer
    #[default]
    StableToLazer,
    /// Sync from osu!lazer to osu!stable
    LazerToStable,
}

impl fmt::Display for CollectionSyncDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CollectionSyncDirection::StableToLazer => write!(f, "Stable -> Lazer"),
            CollectionSyncDirection::LazerToStable => write!(f, "Lazer -> Stable"),
        }
    }
}

/// Result of a collection sync operation
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CollectionSyncResult {
    /// Number of collections synced
    pub collections_synced: usize,
    /// Number of beatmaps added to collections
    pub beatmaps_added: usize,
    /// Number of beatmaps skipped (already present)
    pub beatmaps_skipped: usize,
    /// MD5 hashes of beatmaps that were not found in target
    pub missing_beatmaps: Vec<String>,
    /// Whether the sync completed successfully
    pub success: bool,
    /// Error message if sync failed
    pub error_message: Option<String>,
}

impl CollectionSyncResult {
    /// Create a successful result
    pub fn success(
        collections_synced: usize,
        beatmaps_added: usize,
        beatmaps_skipped: usize,
        missing_beatmaps: Vec<String>,
    ) -> Self {
        Self {
            collections_synced,
            beatmaps_added,
            beatmaps_skipped,
            missing_beatmaps,
            success: true,
            error_message: None,
        }
    }

    /// Create a failed result
    pub fn failure(message: impl Into<String>) -> Self {
        Self {
            success: false,
            error_message: Some(message.into()),
            ..Default::default()
        }
    }

    /// Check if there are missing beatmaps
    pub fn has_missing(&self) -> bool {
        !self.missing_beatmaps.is_empty()
    }
}

/// Progress information during collection sync
#[derive(Debug, Clone, Default)]
pub struct CollectionSyncProgress {
    /// Current collection being processed
    pub current_collection: String,
    /// Index of current collection (0-based)
    pub current_index: usize,
    /// Total number of collections
    pub total_collections: usize,
    /// Current beatmap being processed within the collection
    pub current_beatmap: usize,
    /// Total beatmaps in current collection
    pub total_beatmaps: usize,
}

impl CollectionSyncProgress {
    /// Calculate overall progress as a ratio (0.0 to 1.0)
    pub fn ratio(&self) -> f64 {
        if self.total_collections == 0 {
            return 0.0;
        }

        let collection_progress = self.current_index as f64 / self.total_collections as f64;
        let beatmap_progress = if self.total_beatmaps > 0 {
            self.current_beatmap as f64 / self.total_beatmaps as f64
        } else {
            0.0
        };

        // Weight: each collection contributes equally, beatmap progress within each collection
        collection_progress + (beatmap_progress / self.total_collections as f64)
    }
}

/// Preview information for a single collection before sync
#[derive(Debug, Clone, Default)]
pub struct CollectionPreviewItem {
    /// Name of the collection
    pub name: String,
    /// Total beatmaps in this collection
    pub beatmap_count: usize,
    /// Whether this is a duplicate (same name as another collection)
    pub is_duplicate: bool,
    /// Number of duplicates that will be merged with this collection
    pub merge_count: usize,
}

/// Detailed preview information for a collection sync operation
#[derive(Debug, Clone, Default)]
pub struct CollectionSyncPreview {
    /// Source installation name (e.g., "osu!stable")
    pub source: String,
    /// Target installation name (e.g., "osu!lazer")
    pub target: String,
    /// Sync direction
    pub direction: CollectionSyncDirection,
    /// Per-collection preview information
    pub collections: Vec<CollectionPreviewItem>,
    /// Total number of unique collections (after merging duplicates)
    pub unique_collections: usize,
    /// Total number of beatmaps across all collections
    pub total_beatmaps: usize,
    /// Number of duplicate collections that will be merged
    pub duplicates_merged: usize,
    /// Whether the direction requires manual steps (e.g., lazer -> stable)
    pub requires_manual_steps: bool,
    /// Message explaining manual steps if required
    pub manual_steps_message: Option<String>,
}
