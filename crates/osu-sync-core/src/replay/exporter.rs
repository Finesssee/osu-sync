//! Replay exporter for exporting .osr files

use std::fs;
use std::path::{Path, PathBuf};

use crate::beatmap::GameMode;
use crate::error::Result;

use super::filter::ReplayFilter;
use super::model::{
    ExportOrganization, ReplayExportResult, ReplayExportStats, ReplayInfo, ReplayProgress,
    ReplayProgressCallback,
};

/// Exporter for replay files
pub struct ReplayExporter {
    /// Output directory
    output_path: PathBuf,
    /// How to organize exported replays
    organization: ExportOrganization,
    /// Progress callback
    progress_callback: Option<ReplayProgressCallback>,
    /// Optional filter to apply before export
    filter: Option<ReplayFilter>,
    /// Optional rename pattern for output files
    rename_pattern: Option<String>,
}

impl ReplayExporter {
    /// Create a new replay exporter
    pub fn new(output_path: impl AsRef<Path>) -> Self {
        Self {
            output_path: output_path.as_ref().to_path_buf(),
            organization: ExportOrganization::default(),
            progress_callback: None,
            filter: None,
            rename_pattern: None,
        }
    }

    /// Set the organization method
    pub fn with_organization(mut self, organization: ExportOrganization) -> Self {
        self.organization = organization;
        self
    }

    /// Set progress callback
    pub fn with_progress_callback(mut self, callback: ReplayProgressCallback) -> Self {
        self.progress_callback = Some(callback);
        self
    }

    /// Set filter to apply before export
    pub fn with_filter(mut self, filter: ReplayFilter) -> Self {
        self.filter = Some(filter);
        self
    }

    /// Set rename pattern for output files
    ///
    /// Supported placeholders:
    /// - {artist} - Beatmap artist
    /// - {title} - Beatmap title
    /// - {diff} - Difficulty name
    /// - {grade} - Grade achieved (SS, S, A, etc.)
    /// - {date} - Date of the play (YYYY-MM-DD)
    /// - {player} - Player name
    /// - {score} - Score achieved
    /// - {mode} - Game mode (osu, taiko, catch, mania)
    /// - {hash} - Beatmap hash (first 8 chars)
    pub fn with_rename_pattern(mut self, pattern: impl Into<String>) -> Self {
        self.rename_pattern = Some(pattern.into());
        self
    }

    /// Export replays
    pub fn export(&self, replays: &[ReplayInfo]) -> Result<ReplayExportResult> {
        // Apply filter if set
        let original_count = replays.len();
        let filtered_replays: Vec<ReplayInfo> = if let Some(ref filter) = self.filter {
            filter.apply(replays)
        } else {
            replays.to_vec()
        };
        let filtered_count = original_count - filtered_replays.len();

        // Create output directory
        fs::create_dir_all(&self.output_path)?;

        let mut result = ReplayExportResult::new();
        result.replays_filtered = filtered_count;
        let total = filtered_replays.len();

        for (i, replay) in filtered_replays.iter().enumerate() {
            // Report progress
            if let Some(ref callback) = self.progress_callback {
                let display_name = replay
                    .beatmap_title
                    .clone()
                    .unwrap_or_else(|| replay.beatmap_hash.clone());
                callback(ReplayProgress {
                    current_replay: display_name,
                    replays_processed: i,
                    total_replays: total,
                    bytes_written: result.bytes_written,
                });
            }

            // Skip replays without files
            if !replay.has_replay_file {
                result.replays_skipped += 1;
                continue;
            }

            let source_path = match &replay.replay_path {
                Some(p) => PathBuf::from(p),
                None => {
                    result.replays_skipped += 1;
                    continue;
                }
            };

            if !source_path.exists() {
                result.replays_skipped += 1;
                continue;
            }

            // Determine output path based on organization
            let dest_path = self.get_output_path(replay)?;

            // Create parent directories
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }

            // Copy the replay file
            match fs::copy(&source_path, &dest_path) {
                Ok(bytes) => {
                    result.replays_exported += 1;
                    result.bytes_written += bytes;
                }
                Err(e) => {
                    result.errors.push((
                        replay.replay_hash.clone().unwrap_or_default(),
                        e.to_string(),
                    ));
                }
            }
        }

        // Final progress update
        if let Some(ref callback) = self.progress_callback {
            callback(ReplayProgress {
                current_replay: "Complete".to_string(),
                replays_processed: total,
                total_replays: total,
                bytes_written: result.bytes_written,
            });
        }

        // Build statistics
        result.stats = Some(ReplayExportStats::from_replays(&filtered_replays, &result));

        Ok(result)
    }

    /// Get the output path for a replay based on organization settings
    fn get_output_path(&self, replay: &ReplayInfo) -> Result<PathBuf> {
        let filename = self.generate_filename(replay);

        let path = match self.organization {
            ExportOrganization::Flat => self.output_path.join(&filename),

            ExportOrganization::ByBeatmap => {
                let beatmap_folder = replay
                    .beatmap_title
                    .as_ref()
                    .map(|t| sanitize_filename(t))
                    .unwrap_or_else(|| replay.beatmap_hash.clone());
                self.output_path.join(beatmap_folder).join(&filename)
            }

            ExportOrganization::ByDate => {
                let date = format_date(replay.timestamp);
                self.output_path.join(date).join(&filename)
            }

            ExportOrganization::ByPlayer => {
                let player = sanitize_filename(&replay.player_name);
                self.output_path.join(player).join(&filename)
            }

            ExportOrganization::ByGrade => {
                let grade = replay.grade.as_str();
                self.output_path.join(grade).join(&filename)
            }
        };

        Ok(path)
    }

    /// Generate a filename for a replay
    fn generate_filename(&self, replay: &ReplayInfo) -> String {
        // Use custom pattern if set
        if let Some(ref pattern) = self.rename_pattern {
            return self.apply_rename_pattern(pattern, replay);
        }

        // Default naming
        let base_name = if let Some(ref title) = replay.beatmap_title {
            let artist = replay.beatmap_artist.as_deref().unwrap_or("Unknown");
            format!(
                "{} - {} [{}] ({}).osr",
                sanitize_filename(artist),
                sanitize_filename(title),
                replay.grade.as_str(),
                replay.score
            )
        } else {
            // Fallback to hash-based naming
            format!(
                "{}_{}_{}.osr",
                replay.beatmap_hash,
                replay.grade.as_str(),
                replay.score
            )
        };

        base_name
    }

    /// Apply rename pattern to generate filename
    fn apply_rename_pattern(&self, pattern: &str, replay: &ReplayInfo) -> String {
        let artist = replay.beatmap_artist.as_deref().unwrap_or("Unknown");
        let title = replay.beatmap_title.as_deref().unwrap_or("Unknown");
        let diff = replay.beatmap_version.as_deref().unwrap_or("Unknown");
        let date = format_date(replay.timestamp);
        let mode = match replay.mode {
            GameMode::Osu => "osu",
            GameMode::Taiko => "taiko",
            GameMode::Catch => "catch",
            GameMode::Mania => "mania",
        };
        let hash_short = if replay.beatmap_hash.len() >= 8 {
            &replay.beatmap_hash[..8]
        } else {
            &replay.beatmap_hash
        };

        let mut result = pattern.to_string();
        result = result.replace("{artist}", &sanitize_filename(artist));
        result = result.replace("{title}", &sanitize_filename(title));
        result = result.replace("{diff}", &sanitize_filename(diff));
        result = result.replace("{grade}", replay.grade.as_str());
        result = result.replace("{date}", &date);
        result = result.replace("{player}", &sanitize_filename(&replay.player_name));
        result = result.replace("{score}", &replay.score.to_string());
        result = result.replace("{mode}", mode);
        result = result.replace("{hash}", hash_short);

        // Ensure .osr extension
        if !result.ends_with(".osr") {
            result.push_str(".osr");
        }

        result
    }
}

/// Sanitize a string for use as a filename
pub fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// Format a Unix timestamp as a date string (YYYY-MM-DD)
fn format_date(timestamp: i64) -> String {
    use std::time::{Duration, UNIX_EPOCH};

    if timestamp <= 0 {
        return "Unknown".to_string();
    }

    let _datetime = UNIX_EPOCH + Duration::from_secs(timestamp as u64);

    // Simple date formatting without external crate
    let secs_since_epoch = timestamp as u64;
    let days_since_epoch = secs_since_epoch / 86400;

    // Calculate year, month, day from days since epoch
    let (year, month, day) = days_to_ymd(days_since_epoch);

    format!("{:04}-{:02}-{:02}", year, month, day)
}

/// Convert days since Unix epoch to year/month/day
fn days_to_ymd(days: u64) -> (u32, u32, u32) {
    // Algorithm based on Howard Hinnant's date algorithms
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
    use crate::replay::Grade;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("normal_name"), "normal_name");
        assert_eq!(sanitize_filename("path/with/slashes"), "path_with_slashes");
        assert_eq!(sanitize_filename("file:name"), "file_name");
        assert_eq!(sanitize_filename("file*name?"), "file_name_");
        assert_eq!(sanitize_filename("file<>|name"), "file___name");
        assert_eq!(sanitize_filename("\"quoted\""), "_quoted_");
        assert_eq!(sanitize_filename("  trimmed  "), "trimmed");
        assert_eq!(sanitize_filename("Artist\\Song"), "Artist_Song");
    }

    #[test]
    fn test_format_date() {
        // January 1, 1970 00:00:00 UTC
        assert_eq!(format_date(0), "Unknown");

        // January 1, 2000 00:00:00 UTC = 946684800
        assert_eq!(format_date(946684800), "2000-01-01");

        // December 31, 2023 12:00:00 UTC = 1704024000
        assert_eq!(format_date(1704024000), "2023-12-31");

        // Negative timestamp
        assert_eq!(format_date(-1), "Unknown");
    }

    #[test]
    fn test_days_to_ymd() {
        // Day 0 = January 1, 1970
        assert_eq!(days_to_ymd(0), (1970, 1, 1));

        // Day 365 = January 1, 1971
        assert_eq!(days_to_ymd(365), (1971, 1, 1));

        // Test year 2000 (leap year)
        // 2000-01-01 is day 10957 from epoch
        assert_eq!(days_to_ymd(10957), (2000, 1, 1));
    }

    fn make_test_replay(
        title: Option<&str>,
        artist: Option<&str>,
        grade: Grade,
        score: u64,
    ) -> ReplayInfo {
        ReplayInfo {
            beatmap_hash: "abc123".to_string(),
            player_name: "TestPlayer".to_string(),
            replay_hash: Some("replay_hash".to_string()),
            score,
            max_combo: 100,
            count_300: 90,
            count_100: 10,
            count_50: 0,
            count_miss: 0,
            timestamp: 1704024000,
            mode: GameMode::Osu,
            grade,
            has_replay_file: true,
            replay_path: Some("/path/to/replay.osr".to_string()),
            beatmap_title: title.map(String::from),
            beatmap_artist: artist.map(String::from),
            beatmap_version: Some("Hard".to_string()),
        }
    }

    #[test]
    fn test_generate_filename_with_metadata() {
        let exporter = ReplayExporter::new("/output");
        let replay = make_test_replay(Some("Test Song"), Some("Test Artist"), Grade::S, 1000000);

        let filename = exporter.generate_filename(&replay);
        assert_eq!(filename, "Test Artist - Test Song [S] (1000000).osr");
    }

    #[test]
    fn test_generate_filename_without_metadata() {
        let exporter = ReplayExporter::new("/output");
        let replay = make_test_replay(None, None, Grade::A, 500000);

        let filename = exporter.generate_filename(&replay);
        assert_eq!(filename, "abc123_A_500000.osr");
    }

    #[test]
    fn test_generate_filename_sanitizes_special_chars() {
        let exporter = ReplayExporter::new("/output");
        let replay = make_test_replay(
            Some("Song:With*Special?Chars"),
            Some("Artist/With\\Slashes"),
            Grade::SS,
            999999,
        );

        let filename = exporter.generate_filename(&replay);
        assert!(!filename.contains(':'));
        assert!(!filename.contains('*'));
        assert!(!filename.contains('?'));
        assert!(!filename.contains('/'));
        assert!(!filename.contains('\\'));
    }

    #[test]
    fn test_rename_pattern() {
        let exporter =
            ReplayExporter::new("/output").with_rename_pattern("{artist} - {title} [{grade}]");
        let replay = make_test_replay(Some("Test Song"), Some("Test Artist"), Grade::S, 1000000);

        let filename = exporter.generate_filename(&replay);
        assert_eq!(filename, "Test Artist - Test Song [S].osr");
    }

    #[test]
    fn test_rename_pattern_all_placeholders() {
        let exporter = ReplayExporter::new("/output")
            .with_rename_pattern("{player}_{date}_{mode}_{grade}_{score}");
        let replay = make_test_replay(Some("Test Song"), Some("Test Artist"), Grade::SS, 12345);

        let filename = exporter.generate_filename(&replay);
        assert!(filename.contains("TestPlayer"));
        assert!(filename.contains("2023-12-31"));
        assert!(filename.contains("osu"));
        assert!(filename.contains("SS"));
        assert!(filename.contains("12345"));
    }

    #[test]
    fn test_get_output_path_flat() {
        let exporter = ReplayExporter::new("/output").with_organization(ExportOrganization::Flat);
        let replay = make_test_replay(Some("Song"), Some("Artist"), Grade::A, 100);

        let path = exporter.get_output_path(&replay).unwrap();
        assert!(path.starts_with("/output"));
        assert!(path.to_string_lossy().ends_with(".osr"));
    }

    #[test]
    fn test_get_output_path_by_grade() {
        let exporter =
            ReplayExporter::new("/output").with_organization(ExportOrganization::ByGrade);
        let replay = make_test_replay(Some("Song"), Some("Artist"), Grade::SS, 100);

        let path = exporter.get_output_path(&replay).unwrap();
        assert!(path.to_string_lossy().contains("SS"));
    }

    #[test]
    fn test_get_output_path_by_player() {
        let exporter =
            ReplayExporter::new("/output").with_organization(ExportOrganization::ByPlayer);
        let replay = make_test_replay(Some("Song"), Some("Artist"), Grade::A, 100);

        let path = exporter.get_output_path(&replay).unwrap();
        assert!(path.to_string_lossy().contains("TestPlayer"));
    }

    #[test]
    fn test_exporter_builder_pattern() {
        let exporter =
            ReplayExporter::new("/output").with_organization(ExportOrganization::ByBeatmap);

        // Verify the exporter was constructed correctly
        let replay = make_test_replay(Some("TestSong"), Some("Artist"), Grade::A, 100);
        let path = exporter.get_output_path(&replay).unwrap();
        assert!(path.to_string_lossy().contains("TestSong"));
    }

    #[test]
    fn test_export_empty_replays() {
        let temp_dir = tempfile::tempdir().unwrap();
        let exporter = ReplayExporter::new(temp_dir.path());

        let result = exporter.export(&[]).unwrap();
        assert_eq!(result.replays_exported, 0);
        assert_eq!(result.replays_skipped, 0);
        assert_eq!(result.bytes_written, 0);
        assert!(!result.has_errors());
    }

    #[test]
    fn test_export_skips_replays_without_files() {
        let temp_dir = tempfile::tempdir().unwrap();
        let exporter = ReplayExporter::new(temp_dir.path());

        let mut replay = make_test_replay(Some("Song"), Some("Artist"), Grade::A, 100);
        replay.has_replay_file = false;

        let result = exporter.export(&[replay]).unwrap();
        assert_eq!(result.replays_exported, 0);
        assert_eq!(result.replays_skipped, 1);
    }

    #[test]
    fn test_export_skips_replays_without_path() {
        let temp_dir = tempfile::tempdir().unwrap();
        let exporter = ReplayExporter::new(temp_dir.path());

        let mut replay = make_test_replay(Some("Song"), Some("Artist"), Grade::A, 100);
        replay.replay_path = None;

        let result = exporter.export(&[replay]).unwrap();
        assert_eq!(result.replays_exported, 0);
        assert_eq!(result.replays_skipped, 1);
    }

    #[test]
    fn test_export_with_filter() {
        let temp_dir = tempfile::tempdir().unwrap();
        let filter = ReplayFilter::new().with_min_grade(Grade::S);
        let exporter = ReplayExporter::new(temp_dir.path()).with_filter(filter);

        let ss_replay = make_test_replay(Some("Song1"), Some("Artist"), Grade::SS, 100);
        let a_replay = make_test_replay(Some("Song2"), Some("Artist"), Grade::A, 200);

        let replays = vec![ss_replay, a_replay];
        let result = exporter.export(&replays).unwrap();

        // Only SS replay should pass filter, but since file doesn't exist, it'll be skipped
        // The A replay should be filtered out
        assert_eq!(result.replays_filtered, 1); // A was filtered
    }
}
