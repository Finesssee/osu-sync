//! Filter criteria definitions

use crate::beatmap::GameMode;
use crate::stats::RankedStatus;
use serde::{Deserialize, Serialize};

/// Criteria for filtering beatmaps
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FilterCriteria {
    /// Minimum star rating (inclusive)
    pub star_rating_min: Option<f32>,
    /// Maximum star rating (inclusive)
    pub star_rating_max: Option<f32>,
    /// Game modes to include (empty = all modes)
    pub modes: Vec<GameMode>,
    /// Ranked statuses to include (empty = all statuses)
    pub ranked_status: Vec<RankedStatus>,
    /// Search query for artist/title matching
    pub search_query: Option<String>,
    /// Filter by artist name (case-insensitive substring match)
    pub artist_filter: Option<String>,
    /// Filter by mapper/creator name (case-insensitive substring match)
    pub mapper_filter: Option<String>,
}

impl FilterCriteria {
    /// Create new empty filter criteria
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if no filters are set
    pub fn is_empty(&self) -> bool {
        self.star_rating_min.is_none()
            && self.star_rating_max.is_none()
            && self.modes.is_empty()
            && self.ranked_status.is_empty()
            && self.search_query.is_none()
            && self.artist_filter.is_none()
            && self.mapper_filter.is_none()
    }

    /// Set minimum star rating
    pub fn with_min_stars(mut self, min: f32) -> Self {
        self.star_rating_min = Some(min);
        self
    }

    /// Set maximum star rating
    pub fn with_max_stars(mut self, max: f32) -> Self {
        self.star_rating_max = Some(max);
        self
    }

    /// Set star rating range
    pub fn with_star_range(mut self, min: f32, max: f32) -> Self {
        self.star_rating_min = Some(min);
        self.star_rating_max = Some(max);
        self
    }

    /// Add a game mode filter
    pub fn with_mode(mut self, mode: GameMode) -> Self {
        if !self.modes.contains(&mode) {
            self.modes.push(mode);
        }
        self
    }

    /// Set game modes filter
    pub fn with_modes(mut self, modes: Vec<GameMode>) -> Self {
        self.modes = modes;
        self
    }

    /// Add a ranked status filter
    pub fn with_status(mut self, status: RankedStatus) -> Self {
        if !self.ranked_status.contains(&status) {
            self.ranked_status.push(status);
        }
        self
    }

    /// Set ranked status filter
    pub fn with_statuses(mut self, statuses: Vec<RankedStatus>) -> Self {
        self.ranked_status = statuses;
        self
    }

    /// Set search query
    pub fn with_search(mut self, query: impl Into<String>) -> Self {
        self.search_query = Some(query.into());
        self
    }

    /// Set artist filter
    pub fn with_artist(mut self, artist: impl Into<String>) -> Self {
        self.artist_filter = Some(artist.into());
        self
    }

    /// Set mapper/creator filter
    pub fn with_mapper(mut self, mapper: impl Into<String>) -> Self {
        self.mapper_filter = Some(mapper.into());
        self
    }

    /// Clear the search query
    pub fn clear_search(&mut self) {
        self.search_query = None;
    }

    /// Clear the artist filter
    pub fn clear_artist(&mut self) {
        self.artist_filter = None;
    }

    /// Clear the mapper filter
    pub fn clear_mapper(&mut self) {
        self.mapper_filter = None;
    }

    /// Clear all filters
    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// Toggle a game mode filter
    pub fn toggle_mode(&mut self, mode: GameMode) {
        if let Some(pos) = self.modes.iter().position(|m| *m == mode) {
            self.modes.remove(pos);
        } else {
            self.modes.push(mode);
        }
    }

    /// Toggle a ranked status filter
    pub fn toggle_status(&mut self, status: RankedStatus) {
        if let Some(pos) = self.ranked_status.iter().position(|s| *s == status) {
            self.ranked_status.remove(pos);
        } else {
            self.ranked_status.push(status);
        }
    }

    /// Check if a mode is enabled (empty means all enabled)
    pub fn is_mode_enabled(&self, mode: GameMode) -> bool {
        self.modes.is_empty() || self.modes.contains(&mode)
    }

    /// Check if a status is enabled (empty means all enabled)
    pub fn is_status_enabled(&self, status: RankedStatus) -> bool {
        self.ranked_status.is_empty() || self.ranked_status.contains(&status)
    }

    /// Get a human-readable summary of the filters
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();

        if let Some(min) = self.star_rating_min {
            if let Some(max) = self.star_rating_max {
                parts.push(format!("{:.1}-{:.1}*", min, max));
            } else {
                parts.push(format!(">{:.1}*", min));
            }
        } else if let Some(max) = self.star_rating_max {
            parts.push(format!("<{:.1}*", max));
        }

        if !self.modes.is_empty() {
            let mode_names: Vec<&str> = self
                .modes
                .iter()
                .map(|m| match m {
                    GameMode::Osu => "osu!",
                    GameMode::Taiko => "Taiko",
                    GameMode::Catch => "Catch",
                    GameMode::Mania => "Mania",
                })
                .collect();
            parts.push(mode_names.join("/"));
        }

        if !self.ranked_status.is_empty() {
            let status_names: Vec<String> = self
                .ranked_status
                .iter()
                .map(|s| s.to_string())
                .collect();
            parts.push(status_names.join("/"));
        }

        if let Some(ref query) = self.search_query {
            if !query.is_empty() {
                parts.push(format!("\"{}\"", query));
            }
        }

        if let Some(ref artist) = self.artist_filter {
            if !artist.is_empty() {
                parts.push(format!("artist:{}", artist));
            }
        }

        if let Some(ref mapper) = self.mapper_filter {
            if !mapper.is_empty() {
                parts.push(format!("mapper:{}", mapper));
            }
        }

        if parts.is_empty() {
            "No filters".to_string()
        } else {
            parts.join(", ")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_filter() {
        let filter = FilterCriteria::new();
        assert!(filter.is_empty());
    }

    #[test]
    fn test_star_rating_filter() {
        let filter = FilterCriteria::new()
            .with_min_stars(4.0)
            .with_max_stars(6.0);

        assert!(!filter.is_empty());
        assert_eq!(filter.star_rating_min, Some(4.0));
        assert_eq!(filter.star_rating_max, Some(6.0));
    }

    #[test]
    fn test_star_range() {
        let filter = FilterCriteria::new().with_star_range(3.0, 5.0);
        assert_eq!(filter.star_rating_min, Some(3.0));
        assert_eq!(filter.star_rating_max, Some(5.0));
    }

    #[test]
    fn test_mode_toggle() {
        let mut filter = FilterCriteria::new();
        assert!(filter.modes.is_empty());

        filter.toggle_mode(GameMode::Osu);
        assert_eq!(filter.modes.len(), 1);
        assert!(filter.modes.contains(&GameMode::Osu));

        filter.toggle_mode(GameMode::Osu);
        assert!(filter.modes.is_empty());
    }

    #[test]
    fn test_status_toggle() {
        let mut filter = FilterCriteria::new();
        assert!(filter.ranked_status.is_empty());

        filter.toggle_status(RankedStatus::Ranked);
        assert_eq!(filter.ranked_status.len(), 1);
        assert!(filter.ranked_status.contains(&RankedStatus::Ranked));

        filter.toggle_status(RankedStatus::Ranked);
        assert!(filter.ranked_status.is_empty());
    }

    #[test]
    fn test_artist_filter() {
        let filter = FilterCriteria::new().with_artist("TestArtist");
        assert!(!filter.is_empty());
        assert_eq!(filter.artist_filter, Some("TestArtist".to_string()));
    }

    #[test]
    fn test_mapper_filter() {
        let filter = FilterCriteria::new().with_mapper("TestMapper");
        assert!(!filter.is_empty());
        assert_eq!(filter.mapper_filter, Some("TestMapper".to_string()));
    }

    #[test]
    fn test_clear_filters() {
        let mut filter = FilterCriteria::new()
            .with_min_stars(4.0)
            .with_artist("test")
            .with_mapper("mapper");

        assert!(!filter.is_empty());
        filter.clear();
        assert!(filter.is_empty());
    }

    #[test]
    fn test_clear_individual_filters() {
        let mut filter = FilterCriteria::new()
            .with_search("query")
            .with_artist("artist")
            .with_mapper("mapper");

        filter.clear_search();
        assert!(filter.search_query.is_none());
        assert!(filter.artist_filter.is_some());

        filter.clear_artist();
        assert!(filter.artist_filter.is_none());
        assert!(filter.mapper_filter.is_some());

        filter.clear_mapper();
        assert!(filter.mapper_filter.is_none());
    }

    #[test]
    fn test_is_mode_enabled() {
        let mut filter = FilterCriteria::new();
        // Empty means all enabled
        assert!(filter.is_mode_enabled(GameMode::Osu));
        assert!(filter.is_mode_enabled(GameMode::Taiko));

        filter.modes.push(GameMode::Osu);
        assert!(filter.is_mode_enabled(GameMode::Osu));
        assert!(!filter.is_mode_enabled(GameMode::Taiko));
    }

    #[test]
    fn test_is_status_enabled() {
        let mut filter = FilterCriteria::new();
        // Empty means all enabled
        assert!(filter.is_status_enabled(RankedStatus::Ranked));
        assert!(filter.is_status_enabled(RankedStatus::Loved));

        filter.ranked_status.push(RankedStatus::Ranked);
        assert!(filter.is_status_enabled(RankedStatus::Ranked));
        assert!(!filter.is_status_enabled(RankedStatus::Loved));
    }

    #[test]
    fn test_with_mode_no_duplicates() {
        let filter = FilterCriteria::new()
            .with_mode(GameMode::Osu)
            .with_mode(GameMode::Osu);
        assert_eq!(filter.modes.len(), 1);
    }

    #[test]
    fn test_with_status_no_duplicates() {
        let filter = FilterCriteria::new()
            .with_status(RankedStatus::Ranked)
            .with_status(RankedStatus::Ranked);
        assert_eq!(filter.ranked_status.len(), 1);
    }

    #[test]
    fn test_summary() {
        let filter = FilterCriteria::new()
            .with_star_range(4.0, 6.0)
            .with_mode(GameMode::Osu)
            .with_search("test");

        let summary = filter.summary();
        assert!(summary.contains("4.0-6.0*"));
        assert!(summary.contains("osu!"));
        assert!(summary.contains("\"test\""));
    }

    #[test]
    fn test_summary_with_artist_mapper() {
        let filter = FilterCriteria::new()
            .with_artist("SomeArtist")
            .with_mapper("SomeMapper");

        let summary = filter.summary();
        assert!(summary.contains("artist:SomeArtist"));
        assert!(summary.contains("mapper:SomeMapper"));
    }

    #[test]
    fn test_summary_min_stars_only() {
        let filter = FilterCriteria::new().with_min_stars(5.0);
        let summary = filter.summary();
        assert!(summary.contains(">5.0*"));
    }

    #[test]
    fn test_summary_max_stars_only() {
        let filter = FilterCriteria::new().with_max_stars(3.0);
        let summary = filter.summary();
        assert!(summary.contains("<3.0*"));
    }

    #[test]
    fn test_summary_empty() {
        let filter = FilterCriteria::new();
        assert_eq!(filter.summary(), "No filters");
    }
}
