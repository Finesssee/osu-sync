//! Beatmap metadata structures

use serde::{Deserialize, Serialize};

/// Metadata for a beatmap
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BeatmapMetadata {
    /// Romanized song title
    pub title: String,
    /// Unicode song title
    pub title_unicode: Option<String>,
    /// Romanized artist name
    pub artist: String,
    /// Unicode artist name
    pub artist_unicode: Option<String>,
    /// Beatmap creator username
    pub creator: String,
    /// Source (game, anime, etc.)
    pub source: Option<String>,
    /// Tags for searching
    pub tags: Vec<String>,
    /// Online beatmap ID
    pub beatmap_id: Option<i32>,
    /// Online beatmap set ID
    pub beatmap_set_id: Option<i32>,
}

impl BeatmapMetadata {
    /// Get display title (unicode if available, otherwise romanized)
    pub fn display_title(&self) -> &str {
        self.title_unicode.as_deref().unwrap_or(&self.title)
    }

    /// Get display artist (unicode if available, otherwise romanized)
    pub fn display_artist(&self) -> &str {
        self.artist_unicode.as_deref().unwrap_or(&self.artist)
    }

    /// Check if this metadata matches another (by beatmap ID or title+artist+creator)
    pub fn matches(&self, other: &Self) -> bool {
        // Match by beatmap set ID if available
        if let (Some(a), Some(b)) = (self.beatmap_set_id, other.beatmap_set_id) {
            return a == b;
        }

        // Otherwise match by title + artist + creator
        self.title.eq_ignore_ascii_case(&other.title)
            && self.artist.eq_ignore_ascii_case(&other.artist)
            && self.creator.eq_ignore_ascii_case(&other.creator)
    }
}
