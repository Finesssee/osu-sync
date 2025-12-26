//! Beatmap data structures and types

mod metadata;

pub use metadata::*;

use serde::{Deserialize, Serialize};

use crate::stats::RankedStatus;

/// Represents a game mode in osu!
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GameMode {
    Osu = 0,
    Taiko = 1,
    Catch = 2,
    Mania = 3,
}

impl Default for GameMode {
    fn default() -> Self {
        Self::Osu
    }
}

impl From<u8> for GameMode {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::Osu,
            1 => Self::Taiko,
            2 => Self::Catch,
            3 => Self::Mania,
            _ => Self::Osu,
        }
    }
}

/// Difficulty settings for a beatmap
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BeatmapDifficulty {
    pub hp_drain: f32,
    pub circle_size: f32,
    pub overall_difficulty: f32,
    pub approach_rate: f32,
    pub slider_multiplier: f64,
    pub slider_tick_rate: f64,
}

/// A file associated with a beatmap (audio, background, video, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeatmapFile {
    /// Original filename
    pub filename: String,
    /// SHA-256 hash of the file content
    pub hash: String,
    /// File size in bytes
    pub size: u64,
}

/// Information about a single beatmap difficulty
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BeatmapInfo {
    pub metadata: BeatmapMetadata,
    pub difficulty: BeatmapDifficulty,
    /// SHA-256 hash of the .osu file
    pub hash: String,
    /// MD5 hash for online matching
    pub md5_hash: String,
    /// Audio filename
    pub audio_file: String,
    /// Background image filename
    pub background_file: Option<String>,
    /// Total length in milliseconds
    pub length_ms: u64,
    /// Main BPM
    pub bpm: f64,
    /// Game mode
    pub mode: GameMode,
    /// Difficulty name/version
    pub version: String,
    /// Star rating for this difficulty (from osu! database)
    pub star_rating: Option<f32>,
    /// Ranked status of this beatmap
    pub ranked_status: Option<RankedStatus>,
}

/// A beatmap set containing multiple difficulties
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeatmapSet {
    /// Online beatmap set ID (if available)
    pub id: Option<i32>,
    /// All difficulties in this set
    pub beatmaps: Vec<BeatmapInfo>,
    /// All files (audio, backgrounds, videos, storyboards, etc.)
    pub files: Vec<BeatmapFile>,
    /// Folder name in osu!stable
    pub folder_name: Option<String>,
}

impl BeatmapSet {
    /// Create a new empty beatmap set
    pub fn new() -> Self {
        Self {
            id: None,
            beatmaps: Vec::new(),
            files: Vec::new(),
            folder_name: None,
        }
    }

    /// Get the primary metadata (from the first beatmap)
    pub fn metadata(&self) -> Option<&BeatmapMetadata> {
        self.beatmaps.first().map(|b| &b.metadata)
    }

    /// Generate a folder name in osu!stable format: "{SetID} {Artist} - {Title}"
    pub fn generate_folder_name(&self) -> String {
        if let Some(meta) = self.metadata() {
            let id_prefix = self.id.map(|id| format!("{} ", id)).unwrap_or_default();
            let artist = meta.artist.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
            let title = meta.title.replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_");
            format!("{}{} - {}", id_prefix, artist, title)
        } else {
            "Unknown Beatmap".to_string()
        }
    }
}

impl Default for BeatmapSet {
    fn default() -> Self {
        Self::new()
    }
}
