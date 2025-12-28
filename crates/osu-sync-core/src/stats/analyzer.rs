//! Statistics analyzer for osu! installations

use std::collections::{HashMap, HashSet};

use crate::beatmap::{BeatmapFile, BeatmapSet, GameMode};
use crate::dedup::{DuplicateDetector, DuplicateStrategy, MatchType};
use crate::lazer::LazerBeatmapSet;

use super::model::{
    BeatmapRecommendation, ComparisonStats, DuplicateStats, InstallationStats, ModeBreakdown,
    ModeCount, ModePercentage, RankedStatus, Recommendations,
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
        let stable_ids: HashSet<i32> = stable_sets.iter().filter_map(|s| s.id).collect();

        let lazer_ids: HashSet<i32> = lazer_sets.iter().filter_map(|s| s.online_id).collect();

        let common: HashSet<_> = stable_ids.intersection(&lazer_ids).collect();
        let unique_stable = stable_ids.len() - common.len();
        let unique_lazer = lazer_ids.len() - common.len();

        // Duplicate detection
        let duplicates = Self::analyze_duplicates(stable_sets, lazer_sets);

        // Mode breakdown
        let mode_breakdown = Self::analyze_mode_breakdown(stable_sets, lazer_sets);

        // Recommendations
        let recommendations =
            Self::generate_recommendations(stable_sets, lazer_sets, &stable_ids, &lazer_ids);

        ComparisonStats {
            stable: stable_stats,
            lazer: lazer_stats,
            duplicates,
            unique_to_stable: unique_stable,
            unique_to_lazer: unique_lazer,
            common_beatmaps: common.len(),
            mode_breakdown,
            recommendations,
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
        let lazer_as_sets: Vec<BeatmapSet> =
            lazer_sets.iter().map(Self::lazer_to_beatmap_set).collect();

        for stable_set in stable_sets {
            if let Some(dup_info) = detector.find_duplicate(stable_set, &lazer_as_sets) {
                stats.count += 1;

                // Estimate wasted space (size of duplicate)
                let set_size: u64 = stable_set.files.iter().map(|f| f.size).sum();
                stats.wasted_bytes += set_size;

                // Track match type
                let match_type = match dup_info.match_type {
                    MatchType::ExactHash => "Exact Hash",
                    MatchType::SameSetId => "Same Set ID",
                    MatchType::SameBeatmapId => "Same Beatmap ID",
                    MatchType::Metadata => "Metadata Match",
                    MatchType::Similar(_) => "Similar",
                };
                *stats
                    .by_match_type
                    .entry(match_type.to_string())
                    .or_insert(0) += 1;
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

    /// Analyze mode breakdown across both installations
    fn analyze_mode_breakdown(
        stable_sets: &[BeatmapSet],
        lazer_sets: &[LazerBeatmapSet],
    ) -> ModeBreakdown {
        // Count modes for stable
        let stable_counts = Self::count_modes_stable(stable_sets);
        let stable_percentages = ModePercentage::from_counts(&stable_counts);

        // Count modes for lazer
        let lazer_counts = Self::count_modes_lazer(lazer_sets);
        let lazer_percentages = ModePercentage::from_counts(&lazer_counts);

        ModeBreakdown {
            stable_counts,
            lazer_counts,
            stable_percentages,
            lazer_percentages,
        }
    }

    /// Count beatmaps per mode for stable sets
    fn count_modes_stable(sets: &[BeatmapSet]) -> ModeCount {
        let mut counts = ModeCount::default();

        for set in sets {
            for beatmap in &set.beatmaps {
                match beatmap.mode {
                    GameMode::Osu => counts.osu += 1,
                    GameMode::Taiko => counts.taiko += 1,
                    GameMode::Catch => counts.catch += 1,
                    GameMode::Mania => counts.mania += 1,
                }
            }
        }

        counts
    }

    /// Count beatmaps per mode for lazer sets
    fn count_modes_lazer(sets: &[LazerBeatmapSet]) -> ModeCount {
        let mut counts = ModeCount::default();

        for set in sets {
            for beatmap in &set.beatmaps {
                match beatmap.mode {
                    GameMode::Osu => counts.osu += 1,
                    GameMode::Taiko => counts.taiko += 1,
                    GameMode::Catch => counts.catch += 1,
                    GameMode::Mania => counts.mania += 1,
                }
            }
        }

        counts
    }

    /// Generate sync recommendations
    fn generate_recommendations(
        stable_sets: &[BeatmapSet],
        lazer_sets: &[LazerBeatmapSet],
        stable_ids: &HashSet<i32>,
        lazer_ids: &HashSet<i32>,
    ) -> Recommendations {
        // Find sets unique to each installation
        let unique_stable_ids: HashSet<_> = stable_ids.difference(lazer_ids).cloned().collect();
        let unique_lazer_ids: HashSet<_> = lazer_ids.difference(stable_ids).cloned().collect();

        // Get unique stable sets
        let unique_stable_sets: Vec<_> = stable_sets
            .iter()
            .filter(|s| {
                s.id.map(|id| unique_stable_ids.contains(&id))
                    .unwrap_or(false)
            })
            .collect();

        // Get unique lazer sets
        let unique_lazer_sets: Vec<_> = lazer_sets
            .iter()
            .filter(|s| {
                s.online_id
                    .map(|id| unique_lazer_ids.contains(&id))
                    .unwrap_or(false)
            })
            .collect();

        // Generate top 10 highest star rating from stable (unique)
        let top_star_stable = Self::get_top_star_stable(&unique_stable_sets, 10);

        // Generate top 10 highest star rating from lazer (unique)
        let top_star_lazer = Self::get_top_star_lazer(&unique_lazer_sets, 10);

        // Analyze popular artists not yet synced
        let (popular_artists_unsynced, unsynced_artist_counts) =
            Self::analyze_unsynced_artists(&unique_stable_sets);

        Recommendations {
            stable_to_lazer_count: unique_stable_ids.len(),
            lazer_to_stable_count: unique_lazer_ids.len(),
            top_star_stable,
            top_star_lazer,
            popular_artists_unsynced,
            unsynced_artist_counts,
        }
    }

    /// Get top N highest star rating beatmaps unique to stable
    fn get_top_star_stable(sets: &[&BeatmapSet], limit: usize) -> Vec<BeatmapRecommendation> {
        // Collect all beatmaps with star ratings
        let mut rated_beatmaps: Vec<_> = sets
            .iter()
            .flat_map(|set| {
                set.beatmaps
                    .iter()
                    .filter_map(|b| b.star_rating.map(|sr| (*set, b, sr)))
            })
            .collect();

        // Sort by star rating descending
        rated_beatmaps.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

        // Take top N and convert to recommendations
        rated_beatmaps
            .into_iter()
            .take(limit)
            .map(|(set, beatmap, _)| BeatmapRecommendation {
                set_id: set.id,
                artist: beatmap.metadata.artist.clone(),
                title: beatmap.metadata.title.clone(),
                star_rating: beatmap.star_rating,
                mode: beatmap.mode,
                reason: format!(
                    "{:.2}* {} - not in lazer",
                    beatmap.star_rating.unwrap_or(0.0),
                    beatmap.version
                ),
            })
            .collect()
    }

    /// Get top N highest star rating beatmaps unique to lazer
    fn get_top_star_lazer(sets: &[&LazerBeatmapSet], limit: usize) -> Vec<BeatmapRecommendation> {
        // Collect all beatmaps with star ratings
        let mut rated_beatmaps: Vec<_> = sets
            .iter()
            .flat_map(|set| {
                set.beatmaps
                    .iter()
                    .filter_map(|b| b.star_rating.map(|sr| (*set, b, sr)))
            })
            .collect();

        // Sort by star rating descending
        rated_beatmaps.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

        // Take top N and convert to recommendations
        rated_beatmaps
            .into_iter()
            .take(limit)
            .map(|(set, beatmap, _)| BeatmapRecommendation {
                set_id: set.online_id,
                artist: beatmap.metadata.artist.clone(),
                title: beatmap.metadata.title.clone(),
                star_rating: beatmap.star_rating,
                mode: beatmap.mode,
                reason: format!(
                    "{:.2}* {} - not in stable",
                    beatmap.star_rating.unwrap_or(0.0),
                    beatmap.version
                ),
            })
            .collect()
    }

    /// Analyze popular artists with unsynced beatmaps
    fn analyze_unsynced_artists(
        unique_stable_sets: &[&BeatmapSet],
    ) -> (Vec<BeatmapRecommendation>, Vec<(String, usize)>) {
        // Count beatmaps per artist
        let mut artist_counts: HashMap<String, usize> = HashMap::new();
        let mut artist_best_map: HashMap<String, (&BeatmapSet, &crate::beatmap::BeatmapInfo)> =
            HashMap::new();

        for set in unique_stable_sets {
            for beatmap in &set.beatmaps {
                let artist = beatmap.metadata.artist.to_lowercase();
                *artist_counts.entry(artist.clone()).or_insert(0) += 1;

                // Track the best (highest star rating) map per artist
                let current_best = artist_best_map.get(&artist);
                let should_replace = match current_best {
                    None => true,
                    Some((_, existing)) => {
                        beatmap.star_rating.unwrap_or(0.0) > existing.star_rating.unwrap_or(0.0)
                    }
                };
                if should_replace {
                    artist_best_map.insert(artist, (*set, beatmap));
                }
            }
        }

        // Sort artists by count (most popular first)
        let mut artist_list: Vec<_> = artist_counts.into_iter().collect();
        artist_list.sort_by(|a, b| b.1.cmp(&a.1));

        // Top 10 artists
        let top_artists: Vec<(String, usize)> = artist_list.into_iter().take(10).collect();

        // Generate recommendations for top artists
        let popular_artists_unsynced: Vec<BeatmapRecommendation> = top_artists
            .iter()
            .filter_map(|(artist, count)| {
                artist_best_map
                    .get(artist)
                    .map(|(set, beatmap)| BeatmapRecommendation {
                        set_id: set.id,
                        artist: beatmap.metadata.artist.clone(),
                        title: beatmap.metadata.title.clone(),
                        star_rating: beatmap.star_rating,
                        mode: beatmap.mode,
                        reason: format!("{} unsynced maps by this artist", count),
                    })
            })
            .collect();

        (popular_artists_unsynced, top_artists)
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
        let size_bytes: u64 = set.files.iter().map(|f| f.size).sum();

        let modes: Vec<GameMode> = set
            .beatmaps
            .iter()
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
        let modes: Vec<GameMode> = set
            .beatmaps
            .iter()
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
