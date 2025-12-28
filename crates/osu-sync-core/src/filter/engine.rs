//! Filter engine for matching beatmaps against criteria

use super::FilterCriteria;
use crate::beatmap::BeatmapSet;
use crate::lazer::LazerBeatmapSet;

/// Engine for filtering beatmap sets against criteria
pub struct FilterEngine;

impl FilterEngine {
    /// Filter stable beatmap sets, returning references to matching sets
    pub fn filter_stable<'a>(
        sets: &'a [BeatmapSet],
        criteria: &FilterCriteria,
    ) -> Vec<&'a BeatmapSet> {
        if criteria.is_empty() {
            return sets.iter().collect();
        }
        sets.iter()
            .filter(|set| Self::matches_stable(set, criteria))
            .collect()
    }

    /// Filter lazer beatmap sets, returning references to matching sets
    pub fn filter_lazer<'a>(
        sets: &'a [LazerBeatmapSet],
        criteria: &FilterCriteria,
    ) -> Vec<&'a LazerBeatmapSet> {
        if criteria.is_empty() {
            return sets.iter().collect();
        }
        sets.iter()
            .filter(|set| Self::matches_lazer(set, criteria))
            .collect()
    }

    /// Check if a stable beatmap set matches the filter criteria
    pub fn matches_stable(set: &BeatmapSet, criteria: &FilterCriteria) -> bool {
        if criteria.is_empty() {
            return true;
        }

        // Check if any beatmap in the set matches
        let beatmap_match = set.beatmaps.iter().any(|beatmap| {
            // Mode filter
            if !criteria.modes.is_empty() && !criteria.modes.contains(&beatmap.mode) {
                return false;
            }

            // Star rating filter
            if let Some(min_stars) = criteria.star_rating_min {
                if let Some(sr) = beatmap.star_rating {
                    if sr < min_stars {
                        return false;
                    }
                } else {
                    // No star rating data - skip this beatmap for star rating filter
                    return false;
                }
            }

            if let Some(max_stars) = criteria.star_rating_max {
                if let Some(sr) = beatmap.star_rating {
                    if sr > max_stars {
                        return false;
                    }
                } else {
                    return false;
                }
            }

            // Ranked status filter
            if !criteria.ranked_status.is_empty() {
                if let Some(status) = beatmap.ranked_status {
                    if !criteria.ranked_status.contains(&status) {
                        return false;
                    }
                } else {
                    // No status data - don't match if filter is set
                    return false;
                }
            }

            true
        });

        if !beatmap_match && !set.beatmaps.is_empty() {
            return false;
        }

        // Metadata-based filters (check against set metadata)
        if let Some(meta) = set.metadata() {
            // Artist filter
            if let Some(ref artist_filter) = criteria.artist_filter {
                if !artist_filter.is_empty() {
                    let filter_lower = artist_filter.to_lowercase();
                    let matches_artist = meta.artist.to_lowercase().contains(&filter_lower)
                        || meta
                            .artist_unicode
                            .as_ref()
                            .map_or(false, |a| a.to_lowercase().contains(&filter_lower));
                    if !matches_artist {
                        return false;
                    }
                }
            }

            // Mapper/creator filter
            if let Some(ref mapper_filter) = criteria.mapper_filter {
                if !mapper_filter.is_empty() {
                    let filter_lower = mapper_filter.to_lowercase();
                    if !meta.creator.to_lowercase().contains(&filter_lower) {
                        return false;
                    }
                }
            }
        }

        // Search query filter
        if let Some(ref query) = criteria.search_query {
            if !query.is_empty() {
                let query_lower = query.to_lowercase();
                let matches_metadata = set.metadata().map_or(false, |meta| {
                    meta.title.to_lowercase().contains(&query_lower)
                        || meta.artist.to_lowercase().contains(&query_lower)
                        || meta
                            .title_unicode
                            .as_ref()
                            .map_or(false, |t| t.to_lowercase().contains(&query_lower))
                        || meta
                            .artist_unicode
                            .as_ref()
                            .map_or(false, |a| a.to_lowercase().contains(&query_lower))
                        || meta.creator.to_lowercase().contains(&query_lower)
                        || meta
                            .source
                            .as_ref()
                            .map_or(false, |s| s.to_lowercase().contains(&query_lower))
                        || meta
                            .tags
                            .iter()
                            .any(|tag| tag.to_lowercase().contains(&query_lower))
                });

                if !matches_metadata {
                    // Also check folder name
                    let matches_folder = set
                        .folder_name
                        .as_ref()
                        .map_or(false, |f| f.to_lowercase().contains(&query_lower));

                    if !matches_folder {
                        return false;
                    }
                }
            }
        }

        true
    }

    /// Check if a lazer beatmap set matches the filter criteria
    pub fn matches_lazer(set: &LazerBeatmapSet, criteria: &FilterCriteria) -> bool {
        if criteria.is_empty() {
            return true;
        }

        // Check if any beatmap in the set matches
        let beatmap_match = set.beatmaps.iter().any(|beatmap| {
            // Mode filter
            if !criteria.modes.is_empty() && !criteria.modes.contains(&beatmap.mode) {
                return false;
            }

            // Star rating filter
            if let Some(min_stars) = criteria.star_rating_min {
                if let Some(sr) = beatmap.star_rating {
                    if sr < min_stars {
                        return false;
                    }
                } else {
                    return false;
                }
            }

            if let Some(max_stars) = criteria.star_rating_max {
                if let Some(sr) = beatmap.star_rating {
                    if sr > max_stars {
                        return false;
                    }
                } else {
                    return false;
                }
            }

            // Ranked status filter
            if !criteria.ranked_status.is_empty() {
                if let Some(status) = beatmap.ranked_status {
                    if !criteria.ranked_status.contains(&status) {
                        return false;
                    }
                } else {
                    return false;
                }
            }

            true
        });

        if !beatmap_match && !set.beatmaps.is_empty() {
            return false;
        }

        // Get metadata from first beatmap for metadata-based filters
        if let Some(first_beatmap) = set.beatmaps.first() {
            let meta = &first_beatmap.metadata;

            // Artist filter
            if let Some(ref artist_filter) = criteria.artist_filter {
                if !artist_filter.is_empty() {
                    let filter_lower = artist_filter.to_lowercase();
                    let matches_artist = meta.artist.to_lowercase().contains(&filter_lower)
                        || meta
                            .artist_unicode
                            .as_ref()
                            .map_or(false, |a| a.to_lowercase().contains(&filter_lower));
                    if !matches_artist {
                        return false;
                    }
                }
            }

            // Mapper/creator filter
            if let Some(ref mapper_filter) = criteria.mapper_filter {
                if !mapper_filter.is_empty() {
                    let filter_lower = mapper_filter.to_lowercase();
                    if !meta.creator.to_lowercase().contains(&filter_lower) {
                        return false;
                    }
                }
            }
        }

        // Search query filter
        if let Some(ref query) = criteria.search_query {
            if !query.is_empty() {
                let query_lower = query.to_lowercase();

                // Get metadata from first beatmap
                let matches_metadata = set.beatmaps.first().map_or(false, |beatmap| {
                    let meta = &beatmap.metadata;
                    meta.title.to_lowercase().contains(&query_lower)
                        || meta.artist.to_lowercase().contains(&query_lower)
                        || meta
                            .title_unicode
                            .as_ref()
                            .map_or(false, |t| t.to_lowercase().contains(&query_lower))
                        || meta
                            .artist_unicode
                            .as_ref()
                            .map_or(false, |a| a.to_lowercase().contains(&query_lower))
                        || meta.creator.to_lowercase().contains(&query_lower)
                        || meta
                            .source
                            .as_ref()
                            .map_or(false, |s| s.to_lowercase().contains(&query_lower))
                        || meta
                            .tags
                            .iter()
                            .any(|tag| tag.to_lowercase().contains(&query_lower))
                });

                if !matches_metadata {
                    return false;
                }
            }
        }

        true
    }

    /// Count matching stable beatmap sets
    pub fn count_stable(sets: &[BeatmapSet], criteria: &FilterCriteria) -> usize {
        if criteria.is_empty() {
            return sets.len();
        }
        sets.iter()
            .filter(|set| Self::matches_stable(set, criteria))
            .count()
    }

    /// Count matching lazer beatmap sets
    pub fn count_lazer(sets: &[LazerBeatmapSet], criteria: &FilterCriteria) -> usize {
        if criteria.is_empty() {
            return sets.len();
        }
        sets.iter()
            .filter(|set| Self::matches_lazer(set, criteria))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beatmap::{BeatmapDifficulty, BeatmapInfo, BeatmapMetadata, GameMode};
    use crate::stats::RankedStatus;

    fn create_test_set(title: &str, artist: &str, mode: GameMode) -> BeatmapSet {
        BeatmapSet {
            id: Some(1),
            beatmaps: vec![BeatmapInfo {
                metadata: BeatmapMetadata {
                    title: title.to_string(),
                    artist: artist.to_string(),
                    creator: "TestCreator".to_string(),
                    ..Default::default()
                },
                difficulty: BeatmapDifficulty::default(),
                hash: String::new(),
                md5_hash: String::new(),
                audio_file: String::new(),
                background_file: None,
                length_ms: 0,
                bpm: 120.0,
                mode,
                version: "Normal".to_string(),
                star_rating: None,
                ranked_status: None,
            }],
            files: vec![],
            folder_name: Some("1 TestArtist - TestTitle".to_string()),
        }
    }

    fn create_test_set_with_details(
        title: &str,
        artist: &str,
        creator: &str,
        mode: GameMode,
        star_rating: Option<f32>,
        ranked_status: Option<RankedStatus>,
    ) -> BeatmapSet {
        BeatmapSet {
            id: Some(1),
            beatmaps: vec![BeatmapInfo {
                metadata: BeatmapMetadata {
                    title: title.to_string(),
                    artist: artist.to_string(),
                    creator: creator.to_string(),
                    ..Default::default()
                },
                difficulty: BeatmapDifficulty::default(),
                hash: String::new(),
                md5_hash: String::new(),
                audio_file: String::new(),
                background_file: None,
                length_ms: 0,
                bpm: 120.0,
                mode,
                version: "Normal".to_string(),
                star_rating,
                ranked_status,
            }],
            files: vec![],
            folder_name: Some("1 TestArtist - TestTitle".to_string()),
        }
    }

    #[test]
    fn test_empty_criteria_matches_all() {
        let set = create_test_set("Test", "Artist", GameMode::Osu);
        let criteria = FilterCriteria::new();
        assert!(FilterEngine::matches_stable(&set, &criteria));
    }

    #[test]
    fn test_mode_filter() {
        let set = create_test_set("Test", "Artist", GameMode::Osu);

        let mut criteria = FilterCriteria::new();
        criteria.modes = vec![GameMode::Osu];
        assert!(FilterEngine::matches_stable(&set, &criteria));

        criteria.modes = vec![GameMode::Taiko];
        assert!(!FilterEngine::matches_stable(&set, &criteria));
    }

    #[test]
    fn test_search_filter() {
        let set = create_test_set("MyTitle", "MyArtist", GameMode::Osu);

        let mut criteria = FilterCriteria::new();

        // Title match
        criteria.search_query = Some("mytitle".to_string());
        assert!(FilterEngine::matches_stable(&set, &criteria));

        // Artist match
        criteria.search_query = Some("MYARTIST".to_string());
        assert!(FilterEngine::matches_stable(&set, &criteria));

        // No match
        criteria.search_query = Some("nomatch".to_string());
        assert!(!FilterEngine::matches_stable(&set, &criteria));
    }

    #[test]
    fn test_filter_multiple_sets() {
        let sets = vec![
            create_test_set("Song1", "Artist1", GameMode::Osu),
            create_test_set("Song2", "Artist2", GameMode::Taiko),
            create_test_set("Song3", "Artist3", GameMode::Osu),
        ];

        let mut criteria = FilterCriteria::new();
        criteria.modes = vec![GameMode::Osu];

        let filtered = FilterEngine::filter_stable(&sets, &criteria);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_star_rating_filter_min() {
        let set = create_test_set_with_details(
            "Test",
            "Artist",
            "Creator",
            GameMode::Osu,
            Some(5.5),
            None,
        );

        // Should match - 5.5 >= 4.0
        let criteria = FilterCriteria::new().with_min_stars(4.0);
        assert!(FilterEngine::matches_stable(&set, &criteria));

        // Should not match - 5.5 < 6.0
        let criteria = FilterCriteria::new().with_min_stars(6.0);
        assert!(!FilterEngine::matches_stable(&set, &criteria));
    }

    #[test]
    fn test_star_rating_filter_max() {
        let set = create_test_set_with_details(
            "Test",
            "Artist",
            "Creator",
            GameMode::Osu,
            Some(3.5),
            None,
        );

        // Should match - 3.5 <= 5.0
        let criteria = FilterCriteria::new().with_max_stars(5.0);
        assert!(FilterEngine::matches_stable(&set, &criteria));

        // Should not match - 3.5 > 2.0
        let criteria = FilterCriteria::new().with_max_stars(2.0);
        assert!(!FilterEngine::matches_stable(&set, &criteria));
    }

    #[test]
    fn test_star_rating_filter_range() {
        let set = create_test_set_with_details(
            "Test",
            "Artist",
            "Creator",
            GameMode::Osu,
            Some(5.0),
            None,
        );

        // Should match - 5.0 is within 4.0-6.0
        let criteria = FilterCriteria::new().with_star_range(4.0, 6.0);
        assert!(FilterEngine::matches_stable(&set, &criteria));

        // Should not match - 5.0 is not within 1.0-3.0
        let criteria = FilterCriteria::new().with_star_range(1.0, 3.0);
        assert!(!FilterEngine::matches_stable(&set, &criteria));
    }

    #[test]
    fn test_star_rating_filter_no_data() {
        let set = create_test_set_with_details(
            "Test",
            "Artist",
            "Creator",
            GameMode::Osu,
            None, // No star rating
            None,
        );

        // Should not match when filter requires star rating but data is missing
        let criteria = FilterCriteria::new().with_min_stars(4.0);
        assert!(!FilterEngine::matches_stable(&set, &criteria));
    }

    #[test]
    fn test_ranked_status_filter() {
        let set = create_test_set_with_details(
            "Test",
            "Artist",
            "Creator",
            GameMode::Osu,
            None,
            Some(RankedStatus::Ranked),
        );

        // Should match - status is Ranked
        let criteria = FilterCriteria::new().with_status(RankedStatus::Ranked);
        assert!(FilterEngine::matches_stable(&set, &criteria));

        // Should not match - status is not Loved
        let criteria = FilterCriteria::new().with_status(RankedStatus::Loved);
        assert!(!FilterEngine::matches_stable(&set, &criteria));
    }

    #[test]
    fn test_ranked_status_filter_multiple() {
        let set = create_test_set_with_details(
            "Test",
            "Artist",
            "Creator",
            GameMode::Osu,
            None,
            Some(RankedStatus::Approved),
        );

        // Should match - Approved is in [Ranked, Approved]
        let criteria = FilterCriteria::new()
            .with_status(RankedStatus::Ranked)
            .with_status(RankedStatus::Approved);
        assert!(FilterEngine::matches_stable(&set, &criteria));
    }

    #[test]
    fn test_artist_filter() {
        let set = create_test_set("TestTitle", "TestArtist", GameMode::Osu);

        // Should match - case insensitive substring
        let criteria = FilterCriteria::new().with_artist("testartist");
        assert!(FilterEngine::matches_stable(&set, &criteria));

        // Should match - partial
        let criteria = FilterCriteria::new().with_artist("Artist");
        assert!(FilterEngine::matches_stable(&set, &criteria));

        // Should not match
        let criteria = FilterCriteria::new().with_artist("DifferentArtist");
        assert!(!FilterEngine::matches_stable(&set, &criteria));
    }

    #[test]
    fn test_mapper_filter() {
        let set = create_test_set_with_details(
            "TestTitle",
            "TestArtist",
            "MapperName",
            GameMode::Osu,
            None,
            None,
        );

        // Should match - case insensitive substring
        let criteria = FilterCriteria::new().with_mapper("mappername");
        assert!(FilterEngine::matches_stable(&set, &criteria));

        // Should match - partial
        let criteria = FilterCriteria::new().with_mapper("Mapper");
        assert!(FilterEngine::matches_stable(&set, &criteria));

        // Should not match
        let criteria = FilterCriteria::new().with_mapper("OtherMapper");
        assert!(!FilterEngine::matches_stable(&set, &criteria));
    }

    #[test]
    fn test_combined_filters() {
        let set = create_test_set_with_details(
            "TestTitle",
            "TestArtist",
            "TestCreator",
            GameMode::Osu,
            Some(5.0),
            Some(RankedStatus::Ranked),
        );

        // Should match - all criteria satisfied
        let criteria = FilterCriteria::new()
            .with_mode(GameMode::Osu)
            .with_star_range(4.0, 6.0)
            .with_status(RankedStatus::Ranked)
            .with_artist("TestArtist");
        assert!(FilterEngine::matches_stable(&set, &criteria));

        // Should not match - wrong mode
        let criteria = FilterCriteria::new()
            .with_mode(GameMode::Taiko)
            .with_star_range(4.0, 6.0);
        assert!(!FilterEngine::matches_stable(&set, &criteria));
    }

    #[test]
    fn test_count_stable() {
        let sets = vec![
            create_test_set("Song1", "Artist1", GameMode::Osu),
            create_test_set("Song2", "Artist2", GameMode::Taiko),
            create_test_set("Song3", "Artist3", GameMode::Osu),
        ];

        let criteria = FilterCriteria::new();
        assert_eq!(FilterEngine::count_stable(&sets, &criteria), 3);

        let criteria = FilterCriteria::new().with_mode(GameMode::Osu);
        assert_eq!(FilterEngine::count_stable(&sets, &criteria), 2);
    }

    #[test]
    fn test_search_matches_creator() {
        let set = create_test_set_with_details(
            "SongTitle",
            "SongArtist",
            "UniqueCreatorName",
            GameMode::Osu,
            None,
            None,
        );

        let criteria = FilterCriteria::new().with_search("UniqueCreatorName");
        assert!(FilterEngine::matches_stable(&set, &criteria));
    }

    #[test]
    fn test_search_matches_folder_name() {
        let set = create_test_set("Title", "Artist", GameMode::Osu);

        // folder_name is "1 TestArtist - TestTitle"
        let criteria = FilterCriteria::new().with_search("TestArtist");
        assert!(FilterEngine::matches_stable(&set, &criteria));
    }
}
