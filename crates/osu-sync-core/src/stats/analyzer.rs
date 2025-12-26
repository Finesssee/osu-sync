//! Statistics analyzer for osu! installations

use std::collections::HashSet;

use crate::beatmap::{BeatmapFile, BeatmapSet, GameMode};
use crate::dedup::{DuplicateDetector, DuplicateStrategy, MatchType};
use crate::lazer::LazerBeatmapSet;

use super::model::{
    ComparisonStats, DuplicateStats, InstallationStats, RankedStatus,
};

/// Analyzer for generating statistics from beatmap collections
pub struct StatsAnalyzer;

impl StatsAnalyzer {
    /// Analyze osu!stable beatmap sets
    pub fn analyze_stable(sets: &[BeatmapSet]) -> InstallationStats {
        Self::analyze_sets(sets.iter().map(|s| SetView::from_stable(s)))
    }

    /// Analyze osu!lazer beatmap sets
    pub fn analyze_lazer(sets: &[LazerBeatmapSet]) -> InstallationStats {
        Self::analyze_sets(sets.iter().map(|s| SetView::from_lazer(s)))
    }

    /// Generic analysis for any beatmap set collection
    fn analyze_sets<'a>(sets: impl Iterator<Item = SetView<'a>>) -> InstallationStats {
        let mut stats = InstallationStats::default();

        for set in sets {
            stats.total_beatmap_sets += 1;
            stats.total_beatmaps += set.beatmap_count;
            stats.storage_bytes += set.size_bytes;

            for mode in &set.modes {
                *stats.by_mode.entry(*mode).or_insert(0) += 1;
            }

            *stats.by_ranked_status.entry(set.ranked_status).or_insert(0) += 1;
        }

        // Note: Star rating analysis would require beatmaps to have star_rating field
        // which is typically calculated by osu! client, not stored in .osu files

        stats
    }

    /// Compare two installations and generate combined statistics
    pub fn compare(stable_sets: &[BeatmapSet], lazer_sets: &[LazerBeatmapSet]) -> ComparisonStats {
        let stable_stats = Self::analyze_stable(stable_sets);
        let lazer_stats = Self::analyze_lazer(lazer_sets);

        // Find common and unique sets using set IDs
        let stable_ids: HashSet<i32> = stable_sets
            .iter()
            .filter_map(|s| s.id)
            .collect();

        let lazer_ids: HashSet<i32> = lazer_sets
            .iter()
            .filter_map(|s| s.online_id)
            .collect();

        let common: HashSet<_> = stable_ids.intersection(&lazer_ids).collect();
        let unique_stable = stable_ids.len() - common.len();
        let unique_lazer = lazer_ids.len() - common.len();

        // Duplicate detection
        let duplicates = Self::analyze_duplicates(stable_sets, lazer_sets);

        ComparisonStats {
            stable: stable_stats,
            lazer: lazer_stats,
            duplicates,
            unique_to_stable: unique_stable,
            unique_to_lazer: unique_lazer,
            common_beatmaps: common.len(),
        }
    }

    /// Analyze duplicates between installations
    fn analyze_duplicates(
        stable_sets: &[BeatmapSet],
        lazer_sets: &[LazerBeatmapSet],
    ) -> DuplicateStats {
        let detector = DuplicateDetector::new(DuplicateStrategy::Composite);
        let mut stats = DuplicateStats::default();

        // Convert lazer sets to beatmap sets for comparison
        let lazer_as_sets: Vec<BeatmapSet> = lazer_sets
            .iter()
            .map(Self::lazer_to_beatmap_set)
            .collect();

        for stable_set in stable_sets {
            if let Some(dup_info) = detector.find_duplicate(stable_set, &lazer_as_sets) {
                stats.count += 1;

                // Estimate wasted space (size of duplicate)
                let set_size: u64 = stable_set.files.iter()
                    .map(|f| f.size)
                    .sum();
                stats.wasted_bytes += set_size;

                // Track match type
                let match_type = match dup_info.match_type {
                    MatchType::ExactHash => "Exact Hash",
                    MatchType::SameSetId => "Same Set ID",
                    MatchType::SameBeatmapId => "Same Beatmap ID",
                    MatchType::Metadata => "Metadata Match",
                    MatchType::Similar(_) => "Similar",
                };
                *stats.by_match_type.entry(match_type.to_string()).or_insert(0) += 1;
            }
        }

        stats
    }

    /// Convert a LazerBeatmapSet to BeatmapSet for comparison
    fn lazer_to_beatmap_set(lazer_set: &LazerBeatmapSet) -> BeatmapSet {
        use crate::beatmap::BeatmapInfo;

        let beatmaps: Vec<BeatmapInfo> = lazer_set
            .beatmaps
            .iter()
            .map(|lb| BeatmapInfo {
                metadata: lb.metadata.clone(),
                difficulty: lb.difficulty.clone(),
                hash: lb.hash.clone(),
                md5_hash: lb.md5_hash.clone(),
                audio_file: String::new(),
                background_file: None,
                length_ms: lb.length_ms,
                bpm: lb.bpm,
                mode: lb.mode,
                version: lb.version.clone(),
                star_rating: lb.star_rating,
                ranked_status: lb.ranked_status,
            })
            .collect();

        let files: Vec<BeatmapFile> = lazer_set
            .files
            .iter()
            .map(|f| BeatmapFile {
                filename: f.filename.clone(),
                hash: f.hash.clone(),
                size: 0, // Size not available from lazer file refs
            })
            .collect();

        BeatmapSet {
            id: lazer_set.online_id,
            beatmaps,
            files,
            folder_name: None,
        }
    }
}

/// Unified view of a beatmap set for analysis
struct SetView<'a> {
    beatmap_count: usize,
    size_bytes: u64,
    modes: Vec<GameMode>,
    ranked_status: RankedStatus,
    #[allow(dead_code)]
    online_id: Option<i32>,
    _marker: std::marker::PhantomData<&'a ()>,
}

impl<'a> SetView<'a> {
    fn from_stable(set: &'a BeatmapSet) -> Self {
        let size_bytes: u64 = set.files.iter()
            .map(|f| f.size)
            .sum();

        let modes: Vec<GameMode> = set.beatmaps.iter()
            .map(|b| b.mode)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        Self {
            beatmap_count: set.beatmaps.len(),
            size_bytes,
            modes,
            ranked_status: RankedStatus::Pending, // Default, would need API data
            online_id: set.id,
            _marker: std::marker::PhantomData,
        }
    }

    fn from_lazer(set: &'a LazerBeatmapSet) -> Self {
        let modes: Vec<GameMode> = set.beatmaps.iter()
            .map(|b| b.mode)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        Self {
            beatmap_count: set.beatmaps.len(),
            size_bytes: 0, // Would need to calculate from file store
            modes,
            ranked_status: RankedStatus::Pending,
            online_id: set.online_id,
            _marker: std::marker::PhantomData,
        }
    }
}
