//! Replay data models

use std::collections::HashMap;

use crate::beatmap::GameMode;
use serde::{Deserialize, Serialize};

/// Information about a replay
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayInfo {
    /// MD5 hash of the beatmap
    pub beatmap_hash: String,
    /// Player name
    pub player_name: String,
    /// Replay file hash (if available)
    pub replay_hash: Option<String>,
    /// Score achieved
    pub score: u64,
    /// Max combo
    pub max_combo: u32,
    /// Number of 300s
    pub count_300: u32,
    /// Number of 100s
    pub count_100: u32,
    /// Number of 50s
    pub count_50: u32,
    /// Number of misses
    pub count_miss: u32,
    /// Timestamp of the play
    pub timestamp: i64,
    /// Game mode
    pub mode: GameMode,
    /// Grade achieved
    pub grade: Grade,
    /// Whether the .osr file exists
    pub has_replay_file: bool,
    /// Path to the .osr file (if available)
    pub replay_path: Option<String>,
    /// Beatmap title (for display)
    pub beatmap_title: Option<String>,
    /// Beatmap artist (for display)
    pub beatmap_artist: Option<String>,
    /// Difficulty name (for display)
    pub beatmap_version: Option<String>,
}

/// Grade/rank achieved on a play
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Grade {
    SS,
    SSilver, // SS with hidden/flashlight
    S,
    SSilver2, // S with hidden/flashlight
    A,
    B,
    C,
    D,
    F, // Failed
}

impl Grade {
    /// Convert from osu-db grade value
    pub fn from_osu_db(value: u8) -> Self {
        match value {
            0 => Grade::SS,
            1 => Grade::SSilver,
            2 => Grade::S,
            3 => Grade::SSilver2,
            4 => Grade::A,
            5 => Grade::B,
            6 => Grade::C,
            7 => Grade::D,
            _ => Grade::F,
        }
    }

    /// Get display string
    pub fn as_str(&self) -> &'static str {
        match self {
            Grade::SS => "SS",
            Grade::SSilver => "SS",
            Grade::S => "S",
            Grade::SSilver2 => "S",
            Grade::A => "A",
            Grade::B => "B",
            Grade::C => "C",
            Grade::D => "D",
            Grade::F => "F",
        }
    }

    /// Get numeric rank value for comparison (lower is better)
    /// SS = 0, S = 1, A = 2, B = 3, C = 4, D = 5, F = 6
    pub fn rank_value(&self) -> u8 {
        match self {
            Grade::SS | Grade::SSilver => 0,
            Grade::S | Grade::SSilver2 => 1,
            Grade::A => 2,
            Grade::B => 3,
            Grade::C => 4,
            Grade::D => 5,
            Grade::F => 6,
        }
    }

    /// Check if this grade meets or exceeds the threshold
    /// E.g., SS.meets_threshold(&Grade::S) returns true
    /// A.meets_threshold(&Grade::S) returns false
    pub fn meets_threshold(&self, threshold: &Grade) -> bool {
        self.rank_value() <= threshold.rank_value()
    }

    /// Get all grade variants in order from best to worst
    pub fn all() -> &'static [Grade] {
        &[
            Grade::SS,
            Grade::SSilver,
            Grade::S,
            Grade::SSilver2,
            Grade::A,
            Grade::B,
            Grade::C,
            Grade::D,
            Grade::F,
        ]
    }

    /// Get simplified grade variants (without silver variants)
    pub fn simplified() -> &'static [Grade] {
        &[
            Grade::SS,
            Grade::S,
            Grade::A,
            Grade::B,
            Grade::C,
            Grade::D,
            Grade::F,
        ]
    }
}

impl std::fmt::Display for Grade {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// How to organize exported replays
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ExportOrganization {
    /// All replays in a single directory
    #[default]
    Flat,
    /// Organize by beatmap
    ByBeatmap,
    /// Organize by date
    ByDate,
    /// Organize by player
    ByPlayer,
    /// Organize by grade
    ByGrade,
}

/// Result of a replay export operation
#[derive(Debug, Clone, Default)]
pub struct ReplayExportResult {
    /// Number of replays exported
    pub replays_exported: usize,
    /// Number of replays skipped (no .osr file)
    pub replays_skipped: usize,
    /// Number of replays filtered out
    pub replays_filtered: usize,
    /// Total bytes written
    pub bytes_written: u64,
    /// Errors encountered
    pub errors: Vec<(String, String)>,
    /// Export statistics
    pub stats: Option<ReplayExportStats>,
}

impl ReplayExportResult {
    /// Create a new empty result
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if there were any errors
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// Statistics breakdown for exported replays
#[derive(Debug, Clone, Default)]
pub struct ReplayExportStats {
    /// Total replays exported
    pub total_exported: usize,
    /// Total replays skipped
    pub total_skipped: usize,
    /// Total replays filtered out
    pub total_filtered: usize,
    /// Breakdown by grade (using simplified grade as key)
    pub by_grade: HashMap<String, usize>,
    /// Breakdown by game mode
    pub by_mode: HashMap<String, usize>,
    /// Earliest replay timestamp
    pub earliest_date: Option<i64>,
    /// Latest replay timestamp
    pub latest_date: Option<i64>,
    /// Total bytes written
    pub total_bytes: u64,
}

impl ReplayExportStats {
    /// Create new empty stats
    pub fn new() -> Self {
        Self::default()
    }

    /// Build stats from exported replays
    pub fn from_replays(replays: &[ReplayInfo], result: &ReplayExportResult) -> Self {
        let mut stats = Self {
            total_exported: result.replays_exported,
            total_skipped: result.replays_skipped,
            total_filtered: result.replays_filtered,
            total_bytes: result.bytes_written,
            ..Default::default()
        };

        // Only count replays that were actually exported (have replay files)
        for replay in replays.iter().filter(|r| r.has_replay_file) {
            // Count by grade (use simplified display string)
            *stats
                .by_grade
                .entry(replay.grade.as_str().to_string())
                .or_insert(0) += 1;

            // Count by mode
            let mode_str = match replay.mode {
                GameMode::Osu => "osu!",
                GameMode::Taiko => "taiko",
                GameMode::Catch => "catch",
                GameMode::Mania => "mania",
            };
            *stats.by_mode.entry(mode_str.to_string()).or_insert(0) += 1;

            // Track date range
            if replay.timestamp > 0 {
                match stats.earliest_date {
                    None => stats.earliest_date = Some(replay.timestamp),
                    Some(earliest) if replay.timestamp < earliest => {
                        stats.earliest_date = Some(replay.timestamp)
                    }
                    _ => {}
                }
                match stats.latest_date {
                    None => stats.latest_date = Some(replay.timestamp),
                    Some(latest) if replay.timestamp > latest => {
                        stats.latest_date = Some(replay.timestamp)
                    }
                    _ => {}
                }
            }
        }

        stats
    }

    /// Get formatted date range string
    pub fn date_range_str(&self) -> String {
        match (self.earliest_date, self.latest_date) {
            (Some(earliest), Some(latest)) => {
                format!("{} to {}", format_date(earliest), format_date(latest))
            }
            (Some(date), None) | (None, Some(date)) => format_date(date),
            (None, None) => "Unknown".to_string(),
        }
    }

    /// Get grade breakdown as sorted vec (SS first, F last)
    pub fn grade_breakdown(&self) -> Vec<(String, usize)> {
        let grade_order = ["SS", "S", "A", "B", "C", "D", "F"];
        let mut result = Vec::new();
        for grade in grade_order {
            if let Some(&count) = self.by_grade.get(grade) {
                result.push((grade.to_string(), count));
            }
        }
        result
    }

    /// Get mode breakdown as sorted vec
    pub fn mode_breakdown(&self) -> Vec<(String, usize)> {
        let mode_order = ["osu!", "taiko", "catch", "mania"];
        let mut result = Vec::new();
        for mode in mode_order {
            if let Some(&count) = self.by_mode.get(mode) {
                result.push((mode.to_string(), count));
            }
        }
        result
    }
}

/// Format a Unix timestamp as a date string (YYYY-MM-DD)
fn format_date(timestamp: i64) -> String {
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

/// Progress information during replay export
#[derive(Debug, Clone)]
pub struct ReplayProgress {
    /// Current replay being processed
    pub current_replay: String,
    /// Number of replays processed
    pub replays_processed: usize,
    /// Total replays to process
    pub total_replays: usize,
    /// Bytes written so far
    pub bytes_written: u64,
}

impl ReplayProgress {
    /// Get progress percentage (0.0 to 100.0)
    pub fn percentage(&self) -> f32 {
        if self.total_replays == 0 {
            0.0
        } else {
            (self.replays_processed as f32 / self.total_replays as f32) * 100.0
        }
    }
}

/// Progress callback for replay export
pub type ReplayProgressCallback = Box<dyn Fn(ReplayProgress) + Send + Sync>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grade_from_osu_db() {
        assert_eq!(Grade::from_osu_db(0), Grade::SS);
        assert_eq!(Grade::from_osu_db(1), Grade::SSilver);
        assert_eq!(Grade::from_osu_db(2), Grade::S);
        assert_eq!(Grade::from_osu_db(3), Grade::SSilver2);
        assert_eq!(Grade::from_osu_db(4), Grade::A);
        assert_eq!(Grade::from_osu_db(5), Grade::B);
        assert_eq!(Grade::from_osu_db(6), Grade::C);
        assert_eq!(Grade::from_osu_db(7), Grade::D);
        assert_eq!(Grade::from_osu_db(8), Grade::F);
        assert_eq!(Grade::from_osu_db(255), Grade::F);
    }

    #[test]
    fn test_grade_as_str() {
        assert_eq!(Grade::SS.as_str(), "SS");
        assert_eq!(Grade::SSilver.as_str(), "SS");
        assert_eq!(Grade::S.as_str(), "S");
        assert_eq!(Grade::SSilver2.as_str(), "S");
        assert_eq!(Grade::A.as_str(), "A");
        assert_eq!(Grade::B.as_str(), "B");
        assert_eq!(Grade::C.as_str(), "C");
        assert_eq!(Grade::D.as_str(), "D");
        assert_eq!(Grade::F.as_str(), "F");
    }

    #[test]
    fn test_grade_display() {
        assert_eq!(format!("{}", Grade::SS), "SS");
        assert_eq!(format!("{}", Grade::A), "A");
        assert_eq!(format!("{}", Grade::F), "F");
    }

    #[test]
    fn test_grade_meets_threshold() {
        assert!(Grade::SS.meets_threshold(&Grade::SS));
        assert!(Grade::SS.meets_threshold(&Grade::S));
        assert!(Grade::SS.meets_threshold(&Grade::A));
        assert!(Grade::S.meets_threshold(&Grade::S));
        assert!(Grade::S.meets_threshold(&Grade::A));
        assert!(!Grade::A.meets_threshold(&Grade::S));
        assert!(!Grade::B.meets_threshold(&Grade::A));
        assert!(Grade::F.meets_threshold(&Grade::F));
    }

    #[test]
    fn test_replay_progress_percentage() {
        let progress = ReplayProgress {
            current_replay: "test".to_string(),
            replays_processed: 50,
            total_replays: 100,
            bytes_written: 1024,
        };
        assert!((progress.percentage() - 50.0).abs() < 0.001);

        let progress_complete = ReplayProgress {
            current_replay: "test".to_string(),
            replays_processed: 100,
            total_replays: 100,
            bytes_written: 2048,
        };
        assert!((progress_complete.percentage() - 100.0).abs() < 0.001);

        let progress_empty = ReplayProgress {
            current_replay: "test".to_string(),
            replays_processed: 0,
            total_replays: 0,
            bytes_written: 0,
        };
        assert!((progress_empty.percentage() - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_replay_export_result_has_errors() {
        let mut result = ReplayExportResult::new();
        assert!(!result.has_errors());

        result
            .errors
            .push(("hash1".to_string(), "error1".to_string()));
        assert!(result.has_errors());
    }

    #[test]
    fn test_export_organization_default() {
        assert_eq!(ExportOrganization::default(), ExportOrganization::Flat);
    }
}
