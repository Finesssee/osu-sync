//! Replay data models

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
}

/// Grade/rank achieved on a play
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Total bytes written
    pub bytes_written: u64,
    /// Errors encountered
    pub errors: Vec<(String, String)>,
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

        result.errors.push(("hash1".to_string(), "error1".to_string()));
        assert!(result.has_errors());
    }

    #[test]
    fn test_export_organization_default() {
        assert_eq!(ExportOrganization::default(), ExportOrganization::Flat);
    }
}
