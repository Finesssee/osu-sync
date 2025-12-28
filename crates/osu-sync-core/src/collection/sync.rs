//! Collection synchronization engine
//!
//! Handles syncing collections between osu!stable and osu!lazer.
//! Note: osu!lazer uses a Realm database which requires special handling.
//! Currently, lazer sync is not implemented and returns a placeholder message.

use std::collections::HashMap;

use super::{
    Collection, CollectionPreviewItem, CollectionSyncDirection, CollectionSyncResult,
    CollectionSyncStrategy,
};
use crate::error::Result;

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
            error_message: Some("Lazer to Stable collection sync not yet implemented.".to_string()),
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

    /// Get a detailed summary of what would be synced (dry run)
    ///
    /// This includes per-collection details, duplicate detection, and
    /// information about manual steps required for certain sync directions.
    pub fn preview(
        collections: &[Collection],
        direction: CollectionSyncDirection,
    ) -> super::CollectionSyncPreview {
        let total_beatmaps: usize = collections.iter().map(|c| c.len()).sum();

        let (source, target) = match direction {
            CollectionSyncDirection::StableToLazer => ("osu!stable", "osu!lazer"),
            CollectionSyncDirection::LazerToStable => ("osu!lazer", "osu!stable"),
        };

        // Detect duplicates by counting collection names
        let mut name_counts: HashMap<&str, usize> = HashMap::new();
        for collection in collections {
            *name_counts.entry(&collection.name).or_insert(0) += 1;
        }

        // Build per-collection preview items
        let mut collection_previews: Vec<CollectionPreviewItem> = Vec::new();
        let mut seen_names: HashMap<&str, bool> = HashMap::new();

        for collection in collections {
            let count = name_counts
                .get(collection.name.as_str())
                .copied()
                .unwrap_or(1);
            let is_first = !seen_names.contains_key(collection.name.as_str());
            seen_names.insert(&collection.name, true);

            collection_previews.push(CollectionPreviewItem {
                name: collection.name.clone(),
                beatmap_count: collection.len(),
                is_duplicate: count > 1 && !is_first,
                merge_count: if is_first && count > 1 { count - 1 } else { 0 },
            });
        }

        // Count unique collections and duplicates
        let unique_collections = name_counts.len();
        let duplicates_merged = collections.len().saturating_sub(unique_collections);

        // Determine if manual steps are required
        let (requires_manual_steps, manual_steps_message) = match direction {
            CollectionSyncDirection::StableToLazer => (
                true,
                Some(
                    "After sync, drag collection.db into osu!lazer or use File > Import"
                        .to_string(),
                ),
            ),
            CollectionSyncDirection::LazerToStable => (
                true,
                Some(
                    "Note: Lazer uses a Realm database which is read-only from external tools. \
                     You will need to export collections from lazer manually first."
                        .to_string(),
                ),
            ),
        };

        super::CollectionSyncPreview {
            source: source.to_string(),
            target: target.to_string(),
            direction,
            collections: collection_previews,
            unique_collections,
            total_beatmaps,
            duplicates_merged,
            requires_manual_steps,
            manual_steps_message,
        }
    }

    /// Merge collections with duplicate names
    ///
    /// Takes a list of collections and merges any that share the same name,
    /// combining their beatmap hashes and removing duplicates.
    pub fn merge_duplicates(collections: &[Collection]) -> Vec<Collection> {
        let mut merged: HashMap<String, Collection> = HashMap::new();

        for collection in collections {
            if let Some(existing) = merged.get_mut(&collection.name) {
                // Merge beatmap hashes, avoiding duplicates
                for hash in &collection.beatmap_hashes {
                    if !existing.beatmap_hashes.contains(hash) {
                        existing.beatmap_hashes.push(hash.clone());
                    }
                }
            } else {
                merged.insert(collection.name.clone(), collection.clone());
            }
        }

        // Convert to Vec and sort by name for consistent ordering
        let mut result: Vec<Collection> = merged.into_values().collect();
        result.sort_by(|a, b| a.name.cmp(&b.name));
        result
    }

    /// Get info about duplicate collections
    ///
    /// Returns a map of collection names to the number of duplicates found.
    pub fn find_duplicates(collections: &[Collection]) -> HashMap<String, usize> {
        let mut counts: HashMap<String, usize> = HashMap::new();
        for collection in collections {
            *counts.entry(collection.name.clone()).or_insert(0) += 1;
        }
        // Only keep entries with duplicates
        counts.retain(|_, &mut count| count > 1);
        counts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_to_lazer_placeholder() {
        let collections = vec![Collection::with_hashes(
            "Test",
            vec!["hash1".to_string(), "hash2".to_string()],
        )];

        let result =
            CollectionSyncEngine::sync_to_lazer(&collections, CollectionSyncStrategy::Merge)
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

        let preview =
            CollectionSyncEngine::preview(&collections, CollectionSyncDirection::StableToLazer);

        assert_eq!(preview.source, "osu!stable");
        assert_eq!(preview.target, "osu!lazer");
        assert_eq!(preview.unique_collections, 2);
        assert_eq!(preview.total_beatmaps, 3);
        assert_eq!(preview.duplicates_merged, 0);
        assert!(preview.requires_manual_steps);
        assert!(preview.manual_steps_message.is_some());
    }

    #[test]
    fn test_preview_with_duplicates() {
        let collections = vec![
            Collection::with_hashes("Favorites", vec!["h1".to_string(), "h2".to_string()]),
            Collection::with_hashes("Favorites", vec!["h3".to_string()]), // Duplicate name
            Collection::with_hashes("Training", vec!["h4".to_string()]),
        ];

        let preview =
            CollectionSyncEngine::preview(&collections, CollectionSyncDirection::StableToLazer);

        assert_eq!(preview.unique_collections, 2); // Favorites + Training
        assert_eq!(preview.duplicates_merged, 1); // One duplicate Favorites
        assert_eq!(preview.total_beatmaps, 4);

        // Check collection preview items
        assert_eq!(preview.collections.len(), 3);
        assert!(!preview.collections[0].is_duplicate); // First Favorites
        assert_eq!(preview.collections[0].merge_count, 1); // Will merge 1 duplicate
        assert!(preview.collections[1].is_duplicate); // Second Favorites
    }

    #[test]
    fn test_preview_lazer_to_stable_warning() {
        let collections = vec![Collection::with_hashes("Test", vec!["h1".to_string()])];

        let preview =
            CollectionSyncEngine::preview(&collections, CollectionSyncDirection::LazerToStable);

        assert_eq!(preview.source, "osu!lazer");
        assert_eq!(preview.target, "osu!stable");
        assert!(preview.requires_manual_steps);
        assert!(preview
            .manual_steps_message
            .as_ref()
            .unwrap()
            .contains("Realm"));
    }

    #[test]
    fn test_merge_duplicates() {
        let collections = vec![
            Collection::with_hashes("Favorites", vec!["h1".to_string(), "h2".to_string()]),
            Collection::with_hashes("Favorites", vec!["h2".to_string(), "h3".to_string()]), // Overlapping
            Collection::with_hashes("Training", vec!["h4".to_string()]),
        ];

        let merged = CollectionSyncEngine::merge_duplicates(&collections);

        assert_eq!(merged.len(), 2);

        // Find Favorites collection
        let favorites = merged.iter().find(|c| c.name == "Favorites").unwrap();
        assert_eq!(favorites.beatmap_hashes.len(), 3); // h1, h2, h3 (no duplicates)
        assert!(favorites.beatmap_hashes.contains(&"h1".to_string()));
        assert!(favorites.beatmap_hashes.contains(&"h2".to_string()));
        assert!(favorites.beatmap_hashes.contains(&"h3".to_string()));

        // Find Training collection
        let training = merged.iter().find(|c| c.name == "Training").unwrap();
        assert_eq!(training.beatmap_hashes.len(), 1);
    }

    #[test]
    fn test_find_duplicates() {
        let collections = vec![
            Collection::with_hashes("Favorites", vec!["h1".to_string()]),
            Collection::with_hashes("Favorites", vec!["h2".to_string()]),
            Collection::with_hashes("Favorites", vec!["h3".to_string()]),
            Collection::with_hashes("Training", vec!["h4".to_string()]),
        ];

        let duplicates = CollectionSyncEngine::find_duplicates(&collections);

        assert_eq!(duplicates.len(), 1);
        assert_eq!(*duplicates.get("Favorites").unwrap(), 3);
        assert!(!duplicates.contains_key("Training"));
    }
}
