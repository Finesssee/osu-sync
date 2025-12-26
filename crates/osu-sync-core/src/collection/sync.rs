//! Collection synchronization engine
//!
//! Handles syncing collections between osu!stable and osu!lazer.
//! Note: osu!lazer uses a Realm database which requires special handling.
//! Currently, lazer sync is not implemented and returns a placeholder message.

use crate::error::Result;
use super::{Collection, CollectionSyncDirection, CollectionSyncResult, CollectionSyncStrategy};

/// Engine for synchronizing beatmap collections between installations
pub struct CollectionSyncEngine;

impl CollectionSyncEngine {
    /// Sync collections to osu!lazer
    ///
    /// Currently returns a placeholder result as lazer uses Realm database
    /// which requires special handling not yet implemented.
    pub fn sync_to_lazer(
        collections: &[Collection],
        _strategy: CollectionSyncStrategy,
    ) -> Result<CollectionSyncResult> {
        // Count total beatmaps for the placeholder result
        let total_beatmaps: usize = collections.iter().map(|c| c.len()).sum();

        // For now, return a placeholder result indicating this isn't implemented
        Ok(CollectionSyncResult {
            collections_synced: 0,
            beatmaps_added: 0,
            beatmaps_skipped: total_beatmaps,
            missing_beatmaps: Vec::new(),
            success: false,
            error_message: Some(
                "Lazer collection sync not yet implemented. \
                 osu!lazer uses a Realm database which requires special handling."
                    .to_string(),
            ),
        })
    }

    /// Sync collections from osu!lazer to osu!stable
    ///
    /// Currently returns a placeholder result as lazer reading is not implemented.
    pub fn sync_to_stable(
        _collections: &[Collection],
        _strategy: CollectionSyncStrategy,
    ) -> Result<CollectionSyncResult> {
        Ok(CollectionSyncResult {
            collections_synced: 0,
            beatmaps_added: 0,
            beatmaps_skipped: 0,
            missing_beatmaps: Vec::new(),
            success: false,
            error_message: Some(
                "Lazer to Stable collection sync not yet implemented.".to_string(),
            ),
        })
    }

    /// Sync collections based on direction and strategy
    pub fn sync(
        collections: &[Collection],
        direction: CollectionSyncDirection,
        strategy: CollectionSyncStrategy,
    ) -> Result<CollectionSyncResult> {
        match direction {
            CollectionSyncDirection::StableToLazer => Self::sync_to_lazer(collections, strategy),
            CollectionSyncDirection::LazerToStable => Self::sync_to_stable(collections, strategy),
        }
    }

    /// Get a summary of what would be synced (dry run)
    pub fn preview(
        collections: &[Collection],
        direction: CollectionSyncDirection,
    ) -> CollectionSyncPreview {
        let total_collections = collections.len();
        let total_beatmaps: usize = collections.iter().map(|c| c.len()).sum();

        let (source, target) = match direction {
            CollectionSyncDirection::StableToLazer => ("osu!stable", "osu!lazer"),
            CollectionSyncDirection::LazerToStable => ("osu!lazer", "osu!stable"),
        };

        CollectionSyncPreview {
            source: source.to_string(),
            target: target.to_string(),
            total_collections,
            total_beatmaps,
            // TODO: Calculate actual numbers when sync is implemented
            estimated_new_beatmaps: total_beatmaps,
            estimated_skipped: 0,
        }
    }
}

/// Preview information for a collection sync operation
#[derive(Debug, Clone, Default)]
pub struct CollectionSyncPreview {
    /// Source installation name
    pub source: String,
    /// Target installation name
    pub target: String,
    /// Total number of collections to sync
    pub total_collections: usize,
    /// Total number of beatmaps across all collections
    pub total_beatmaps: usize,
    /// Estimated number of new beatmaps to add
    pub estimated_new_beatmaps: usize,
    /// Estimated number of beatmaps that will be skipped
    pub estimated_skipped: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_to_lazer_placeholder() {
        let collections = vec![
            Collection::with_hashes("Test", vec!["hash1".to_string(), "hash2".to_string()]),
        ];

        let result = CollectionSyncEngine::sync_to_lazer(&collections, CollectionSyncStrategy::Merge)
            .unwrap();

        assert!(!result.success);
        assert!(result.error_message.is_some());
        assert_eq!(result.beatmaps_skipped, 2);
    }

    #[test]
    fn test_preview() {
        let collections = vec![
            Collection::with_hashes("Favorites", vec!["h1".to_string(), "h2".to_string()]),
            Collection::with_hashes("Training", vec!["h3".to_string()]),
        ];

        let preview = CollectionSyncEngine::preview(
            &collections,
            CollectionSyncDirection::StableToLazer,
        );

        assert_eq!(preview.source, "osu!stable");
        assert_eq!(preview.target, "osu!lazer");
        assert_eq!(preview.total_collections, 2);
        assert_eq!(preview.total_beatmaps, 3);
    }
}
