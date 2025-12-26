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
}

impl ComparisonStats {
    /// Total unique beatmap sets across both installations
    pub fn total_unique(&self) -> usize {
        self.unique_to_stable + self.unique_to_lazer + self.common_beatmaps
    }
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
