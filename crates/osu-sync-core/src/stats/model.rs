//! Data models for installation statistics

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::beatmap::GameMode;

/// Ranked status of a beatmap (matches osu! API values)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub enum RankedStatus {
    Graveyard = -2,
    Wip = -1,
    #[default]
    Pending = 0,
    Ranked = 1,
    Approved = 2,
    Qualified = 3,
    Loved = 4,
}

impl fmt::Display for RankedStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RankedStatus::Graveyard => write!(f, "Graveyard"),
            RankedStatus::Wip => write!(f, "WIP"),
            RankedStatus::Pending => write!(f, "Pending"),
            RankedStatus::Ranked => write!(f, "Ranked"),
            RankedStatus::Approved => write!(f, "Approved"),
            RankedStatus::Qualified => write!(f, "Qualified"),
            RankedStatus::Loved => write!(f, "Loved"),
        }
    }
}

/// Star rating distribution bucket
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StarRatingBucket {
    pub min: f32,
    pub max: f32,
    pub count: usize,
}

/// Mode breakdown statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModeBreakdown {
    /// Count of beatmaps per mode in stable
    pub stable_counts: ModeCount,
    /// Count of beatmaps per mode in lazer
    pub lazer_counts: ModeCount,
    /// Percentage breakdown in stable
    pub stable_percentages: ModePercentage,
    /// Percentage breakdown in lazer
    pub lazer_percentages: ModePercentage,
}

/// Beatmap counts by game mode
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModeCount {
    pub osu: usize,
    pub taiko: usize,
    pub catch: usize,
    pub mania: usize,
}

impl ModeCount {
    /// Get total count across all modes
    pub fn total(&self) -> usize {
        self.osu + self.taiko + self.catch + self.mania
    }
}

/// Percentage breakdown by game mode
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModePercentage {
    pub osu: f32,
    pub taiko: f32,
    pub catch: f32,
    pub mania: f32,
}

impl ModePercentage {
    /// Calculate percentages from counts
    pub fn from_counts(counts: &ModeCount) -> Self {
        let total = counts.total() as f32;
        if total == 0.0 {
            return Self::default();
        }
        Self {
            osu: (counts.osu as f32 / total) * 100.0,
            taiko: (counts.taiko as f32 / total) * 100.0,
            catch: (counts.catch as f32 / total) * 100.0,
            mania: (counts.mania as f32 / total) * 100.0,
        }
    }
}

/// A beatmap recommendation entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeatmapRecommendation {
    /// Beatmap set ID (if available)
    pub set_id: Option<i32>,
    /// Artist name
    pub artist: String,
    /// Song title
    pub title: String,
    /// Star rating (if available)
    pub star_rating: Option<f32>,
    /// Game mode
    pub mode: GameMode,
    /// Reason for recommendation
    pub reason: String,
}

/// Recommendations for syncing beatmaps
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Recommendations {
    /// Number of beatmaps in stable that are not in lazer (sync candidates)
    pub stable_to_lazer_count: usize,
    /// Number of beatmaps in lazer that are not in stable
    pub lazer_to_stable_count: usize,
    /// Top 10 highest star rating maps unique to stable
    pub top_star_stable: Vec<BeatmapRecommendation>,
    /// Top 10 highest star rating maps unique to lazer
    pub top_star_lazer: Vec<BeatmapRecommendation>,
    /// Maps by popular artists not yet synced
    pub popular_artists_unsynced: Vec<BeatmapRecommendation>,
    /// Most common unsynced artists (artist name, count)
    pub unsynced_artist_counts: Vec<(String, usize)>,
}

/// Comprehensive statistics for an osu! installation
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InstallationStats {
    /// Total number of beatmap sets
    pub total_beatmap_sets: usize,
    /// Total number of individual beatmaps (difficulties)
    pub total_beatmaps: usize,
    /// Total storage used in bytes
    pub storage_bytes: u64,
    /// Breakdown by game mode
    pub by_mode: HashMap<GameMode, usize>,
    /// Breakdown by ranked status
    pub by_ranked_status: HashMap<RankedStatus, usize>,
    /// Star rating distribution (buckets of 1 star each)
    pub star_rating_distribution: Vec<StarRatingBucket>,
    /// Average star rating
    pub average_star_rating: f32,
    /// Min/max star ratings
    pub min_star_rating: f32,
    pub max_star_rating: f32,
}

impl InstallationStats {
    /// Format storage size as human readable string
    pub fn storage_display(&self) -> String {
        format_bytes(self.storage_bytes)
    }
}

/// Statistics about duplicates between installations
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DuplicateStats {
    /// Number of duplicate beatmap sets
    pub count: usize,
    /// Estimated wasted storage in bytes
    pub wasted_bytes: u64,
    /// Breakdown by match type
    pub by_match_type: HashMap<String, usize>,
}

impl DuplicateStats {
    /// Format wasted space as human readable string
    pub fn wasted_display(&self) -> String {
        format_bytes(self.wasted_bytes)
    }
}

/// Combined statistics comparing both installations
#[derive(Debug, Clone, Default)]
pub struct ComparisonStats {
    /// Statistics for osu!stable
    pub stable: InstallationStats,
    /// Statistics for osu!lazer
    pub lazer: InstallationStats,
    /// Duplicate analysis
    pub duplicates: DuplicateStats,
    /// Beatmap sets unique to stable
    pub unique_to_stable: usize,
    /// Beatmap sets unique to lazer
    pub unique_to_lazer: usize,
    /// Beatmap sets present in both
    pub common_beatmaps: usize,
    /// Mode breakdown statistics
    pub mode_breakdown: ModeBreakdown,
    /// Sync recommendations
    pub recommendations: Recommendations,
}

impl ComparisonStats {
    /// Total unique beatmap sets across both installations
    pub fn total_unique(&self) -> usize {
        self.unique_to_stable + self.unique_to_lazer + self.common_beatmaps
    }
}

/// Convert days since Unix epoch to year/month/day
pub fn days_to_ymd(days: u64) -> (u32, u32, u32) {
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = if m <= 2 { y + 1 } else { y };

    (year as u32, m, d)
}

/// Format bytes as human readable string (KB, MB, GB)
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
