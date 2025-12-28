//! Replay filtering support

use serde::{Deserialize, Serialize};

use crate::beatmap::GameMode;

use super::model::{Grade, ReplayInfo};

/// Filter criteria for replay export
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReplayFilter {
    /// Minimum grade threshold (inclusive)
    /// E.g., Grade::A means A, S, SS are included
    pub min_grade: Option<Grade>,

    /// Filter by game modes (if empty, all modes included)
    pub modes: Vec<GameMode>,

    /// Only include replays after this Unix timestamp
    pub after_date: Option<i64>,

    /// Only include replays before this Unix timestamp
    pub before_date: Option<i64>,

    /// Filter by player name (case-insensitive contains)
    pub player_name: Option<String>,

    /// Filter by beatmap title/artist (case-insensitive contains)
    pub beatmap_search: Option<String>,
}

impl ReplayFilter {
    /// Create a new empty filter (matches all replays)
    pub fn new() -> Self {
        Self::default()
    }

    /// Set minimum grade threshold
    pub fn with_min_grade(mut self, grade: Grade) -> Self {
        self.min_grade = Some(grade);
        self
    }

    /// Add a game mode to filter by
    pub fn with_mode(mut self, mode: GameMode) -> Self {
        if !self.modes.contains(&mode) {
            self.modes.push(mode);
        }
        self
    }

    /// Set all modes to filter by
    pub fn with_modes(mut self, modes: Vec<GameMode>) -> Self {
        self.modes = modes;
        self
    }

    /// Set after date filter (Unix timestamp)
    pub fn with_after_date(mut self, timestamp: i64) -> Self {
        self.after_date = Some(timestamp);
        self
    }

    /// Set before date filter (Unix timestamp)
    pub fn with_before_date(mut self, timestamp: i64) -> Self {
        self.before_date = Some(timestamp);
        self
    }

    /// Set date range filter
    pub fn with_date_range(mut self, after: i64, before: i64) -> Self {
        self.after_date = Some(after);
        self.before_date = Some(before);
        self
    }

    /// Set player name filter
    pub fn with_player_name(mut self, name: impl Into<String>) -> Self {
        self.player_name = Some(name.into());
        self
    }

    /// Set beatmap search filter
    pub fn with_beatmap_search(mut self, search: impl Into<String>) -> Self {
        self.beatmap_search = Some(search.into());
        self
    }

    /// Check if a replay matches this filter
    pub fn matches(&self, replay: &ReplayInfo) -> bool {
        // Check grade threshold
        if let Some(min_grade) = &self.min_grade {
            if !replay.grade.meets_threshold(min_grade) {
                return false;
            }
        }

        // Check game mode
        if !self.modes.is_empty() && !self.modes.contains(&replay.mode) {
            return false;
        }

        // Check after date
        if let Some(after) = self.after_date {
            if replay.timestamp < after {
                return false;
            }
        }

        // Check before date
        if let Some(before) = self.before_date {
            if replay.timestamp > before {
                return false;
            }
        }

        // Check player name
        if let Some(ref name) = self.player_name {
            if !replay
                .player_name
                .to_lowercase()
                .contains(&name.to_lowercase())
            {
                return false;
            }
        }

        // Check beatmap search
        if let Some(ref search) = self.beatmap_search {
            let search_lower = search.to_lowercase();
            let title_match = replay
                .beatmap_title
                .as_ref()
                .map(|t| t.to_lowercase().contains(&search_lower))
                .unwrap_or(false);
            let artist_match = replay
                .beatmap_artist
                .as_ref()
                .map(|a| a.to_lowercase().contains(&search_lower))
                .unwrap_or(false);
            if !title_match && !artist_match {
                return false;
            }
        }

        true
    }

    /// Apply filter to a list of replays
    pub fn apply(&self, replays: &[ReplayInfo]) -> Vec<ReplayInfo> {
        replays
            .iter()
            .filter(|r| self.matches(r))
            .cloned()
            .collect()
    }

    /// Check if filter is empty (matches everything)
    pub fn is_empty(&self) -> bool {
        self.min_grade.is_none()
            && self.modes.is_empty()
            && self.after_date.is_none()
            && self.before_date.is_none()
            && self.player_name.is_none()
            && self.beatmap_search.is_none()
    }

    /// Get human-readable description of active filters
    pub fn describe(&self) -> String {
        let mut parts = Vec::new();

        if let Some(ref grade) = self.min_grade {
            parts.push(format!("grade >= {}", grade));
        }

        if !self.modes.is_empty() {
            let mode_strs: Vec<&str> = self.modes.iter().map(|m| mode_str(m)).collect();
            parts.push(format!("mode: {}", mode_strs.join("/")));
        }

        if let Some(after) = self.after_date {
            parts.push(format!("after: {}", format_timestamp(after)));
        }

        if let Some(before) = self.before_date {
            parts.push(format!("before: {}", format_timestamp(before)));
        }

        if let Some(ref player) = self.player_name {
            parts.push(format!("player: {}", player));
        }

        if let Some(ref search) = self.beatmap_search {
            parts.push(format!("beatmap: {}", search));
        }

        if parts.is_empty() {
            "No filters".to_string()
        } else {
            parts.join(", ")
        }
    }
}

/// Get display string for game mode
fn mode_str(mode: &GameMode) -> &'static str {
    match mode {
        GameMode::Osu => "osu!",
        GameMode::Taiko => "taiko",
        GameMode::Catch => "catch",
        GameMode::Mania => "mania",
    }
}

/// Format a Unix timestamp as a date string
fn format_timestamp(timestamp: i64) -> String {
    if timestamp <= 0 {
        return "Unknown".to_string();
    }

    let secs_since_epoch = timestamp as u64;
    let days_since_epoch = secs_since_epoch / 86400;
    let (year, month, day) = days_to_ymd(days_since_epoch);
    format!("{:04}-{:02}-{:02}", year, month, day)
}

/// Convert days since Unix epoch to year/month/day
fn days_to_ymd(days: u64) -> (u32, u32, u32) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_replay(
        grade: Grade,
        mode: GameMode,
        timestamp: i64,
        player: &str,
        title: Option<&str>,
    ) -> ReplayInfo {
        ReplayInfo {
            beatmap_hash: "abc123".to_string(),
            player_name: player.to_string(),
            replay_hash: Some("replay_hash".to_string()),
            score: 1000000,
            max_combo: 100,
            count_300: 90,
            count_100: 10,
            count_50: 0,
            count_miss: 0,
            timestamp,
            mode,
            grade,
            has_replay_file: true,
            replay_path: Some("/path/to/replay.osr".to_string()),
            beatmap_title: title.map(String::from),
            beatmap_artist: Some("Test Artist".to_string()),
            beatmap_version: Some("Hard".to_string()),
        }
    }

    #[test]
    fn test_empty_filter_matches_all() {
        let filter = ReplayFilter::new();
        let replay = make_test_replay(Grade::A, GameMode::Osu, 1704024000, "Player", Some("Song"));
        assert!(filter.matches(&replay));
    }

    #[test]
    fn test_grade_filter() {
        let filter = ReplayFilter::new().with_min_grade(Grade::S);

        let ss_replay =
            make_test_replay(Grade::SS, GameMode::Osu, 1704024000, "Player", Some("Song"));
        let s_replay =
            make_test_replay(Grade::S, GameMode::Osu, 1704024000, "Player", Some("Song"));
        let a_replay =
            make_test_replay(Grade::A, GameMode::Osu, 1704024000, "Player", Some("Song"));
        let b_replay =
            make_test_replay(Grade::B, GameMode::Osu, 1704024000, "Player", Some("Song"));

        assert!(filter.matches(&ss_replay));
        assert!(filter.matches(&s_replay));
        assert!(!filter.matches(&a_replay));
        assert!(!filter.matches(&b_replay));
    }

    #[test]
    fn test_mode_filter() {
        let filter = ReplayFilter::new()
            .with_mode(GameMode::Osu)
            .with_mode(GameMode::Taiko);

        let osu_replay =
            make_test_replay(Grade::A, GameMode::Osu, 1704024000, "Player", Some("Song"));
        let taiko_replay = make_test_replay(
            Grade::A,
            GameMode::Taiko,
            1704024000,
            "Player",
            Some("Song"),
        );
        let catch_replay = make_test_replay(
            Grade::A,
            GameMode::Catch,
            1704024000,
            "Player",
            Some("Song"),
        );

        assert!(filter.matches(&osu_replay));
        assert!(filter.matches(&taiko_replay));
        assert!(!filter.matches(&catch_replay));
    }

    #[test]
    fn test_date_filter() {
        let filter = ReplayFilter::new()
            .with_after_date(1700000000)
            .with_before_date(1710000000);

        let in_range =
            make_test_replay(Grade::A, GameMode::Osu, 1705000000, "Player", Some("Song"));
        let before_range =
            make_test_replay(Grade::A, GameMode::Osu, 1690000000, "Player", Some("Song"));
        let after_range =
            make_test_replay(Grade::A, GameMode::Osu, 1720000000, "Player", Some("Song"));

        assert!(filter.matches(&in_range));
        assert!(!filter.matches(&before_range));
        assert!(!filter.matches(&after_range));
    }

    #[test]
    fn test_player_filter() {
        let filter = ReplayFilter::new().with_player_name("test");

        let match_replay = make_test_replay(
            Grade::A,
            GameMode::Osu,
            1704024000,
            "TestPlayer",
            Some("Song"),
        );
        let no_match = make_test_replay(
            Grade::A,
            GameMode::Osu,
            1704024000,
            "OtherPlayer",
            Some("Song"),
        );

        assert!(filter.matches(&match_replay));
        assert!(!filter.matches(&no_match));
    }

    #[test]
    fn test_beatmap_search_filter() {
        let filter = ReplayFilter::new().with_beatmap_search("Freedom");

        let match_title = make_test_replay(
            Grade::A,
            GameMode::Osu,
            1704024000,
            "Player",
            Some("Freedom Dive"),
        );
        let no_match = make_test_replay(
            Grade::A,
            GameMode::Osu,
            1704024000,
            "Player",
            Some("Other Song"),
        );

        assert!(filter.matches(&match_title));
        assert!(!filter.matches(&no_match));
    }

    #[test]
    fn test_combined_filters() {
        let filter = ReplayFilter::new()
            .with_min_grade(Grade::S)
            .with_mode(GameMode::Osu)
            .with_after_date(1700000000);

        // All criteria met
        let good = make_test_replay(Grade::SS, GameMode::Osu, 1705000000, "Player", Some("Song"));
        assert!(filter.matches(&good));

        // Wrong grade
        let bad_grade =
            make_test_replay(Grade::A, GameMode::Osu, 1705000000, "Player", Some("Song"));
        assert!(!filter.matches(&bad_grade));

        // Wrong mode
        let bad_mode = make_test_replay(
            Grade::SS,
            GameMode::Taiko,
            1705000000,
            "Player",
            Some("Song"),
        );
        assert!(!filter.matches(&bad_mode));

        // Before date
        let bad_date =
            make_test_replay(Grade::SS, GameMode::Osu, 1690000000, "Player", Some("Song"));
        assert!(!filter.matches(&bad_date));
    }

    #[test]
    fn test_apply_filter() {
        let filter = ReplayFilter::new().with_min_grade(Grade::S);

        let replays = vec![
            make_test_replay(
                Grade::SS,
                GameMode::Osu,
                1704024000,
                "Player",
                Some("Song1"),
            ),
            make_test_replay(Grade::A, GameMode::Osu, 1704024000, "Player", Some("Song2")),
            make_test_replay(Grade::S, GameMode::Osu, 1704024000, "Player", Some("Song3")),
        ];

        let filtered = filter.apply(&replays);
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_is_empty() {
        let empty = ReplayFilter::new();
        assert!(empty.is_empty());

        let with_grade = ReplayFilter::new().with_min_grade(Grade::A);
        assert!(!with_grade.is_empty());
    }

    #[test]
    fn test_describe() {
        let empty = ReplayFilter::new();
        assert_eq!(empty.describe(), "No filters");

        let with_grade = ReplayFilter::new().with_min_grade(Grade::S);
        assert!(with_grade.describe().contains("grade >= S"));

        let with_mode = ReplayFilter::new().with_mode(GameMode::Osu);
        assert!(with_mode.describe().contains("osu!"));
    }
}
