//! osu!lazer beatmap scanner using file system scanning
//!
//! This module provides beatmap scanning for osu!lazer by scanning the
//! content-addressed file store and parsing .osu files directly.
//!
//! ## Approach
//!
//! osu!lazer stores files in a content-addressed store where each file
//! is named by its SHA-256 hash. This scanner:
//!
//! 1. Walks the `files/` directory
//! 2. Identifies .osu files by checking file content (starts with "osu file format")
//! 3. Parses each .osu file using rosu-map
//! 4. Groups beatmaps by BeatmapSetID
//!
//! This approach works regardless of Realm database version and provides
//! reliable access to all beatmap data.

use crate::beatmap::{BeatmapDifficulty, BeatmapMetadata, GameMode};
use crate::error::{Error, Result};
use crate::lazer::{LazerBeatmapInfo, LazerBeatmapSet};
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Scanner for osu!lazer beatmaps using file system scanning
pub struct LazerScanner {
    /// Path to osu!lazer data directory
    data_path: PathBuf,
    /// Path to files directory
    files_path: PathBuf,
}

/// Timing information for lazer scanning
#[derive(Debug, Clone)]
pub struct LazerScanTiming {
    /// Total scan time
    pub total: Duration,
    /// Time spent finding .osu files
    pub file_discovery: Duration,
    /// Time spent parsing .osu files
    pub parsing: Duration,
    /// Number of files scanned
    pub files_scanned: usize,
    /// Number of .osu files found
    pub osu_files_found: usize,
    /// Number of beatmap sets created
    pub sets_created: usize,
}

impl LazerScanTiming {
    /// Generate a human-readable report
    pub fn report(&self) -> String {
        format!(
            "Lazer scan completed in {:.2}s ({} sets, {} beatmaps)\n\
             - File discovery: {:.2}s ({} files)\n\
             - Parsing: {:.2}s ({} .osu files, {:.0} files/sec)",
            self.total.as_secs_f64(),
            self.sets_created,
            self.osu_files_found,
            self.file_discovery.as_secs_f64(),
            self.files_scanned,
            self.parsing.as_secs_f64(),
            self.osu_files_found,
            if self.parsing.as_secs_f64() > 0.0 {
                self.osu_files_found as f64 / self.parsing.as_secs_f64()
            } else {
                0.0
            }
        )
    }
}

impl LazerScanner {
    /// Create a new scanner for the given lazer data directory
    pub fn new(data_path: PathBuf) -> Self {
        let files_path = data_path.join("files");
        Self { data_path, files_path }
    }

    /// Check if the lazer data directory is valid
    ///
    /// Checks for files directory and client.realm existence.
    /// If files_path exists, data_path must also exist (it's the parent).
    pub fn is_valid(&self) -> bool {
        self.files_path.is_dir() && self.data_path.join("client.realm").is_file()
    }

    /// Scan all beatmaps and return timing information
    pub fn scan_with_timing(&self) -> Result<(Vec<LazerBeatmapSet>, LazerScanTiming)> {
        let start = Instant::now();

        if !self.is_valid() {
            return Err(Error::OsuNotFound(self.data_path.clone()));
        }

        // Phase 1: Discover all files
        let discovery_start = Instant::now();
        let all_files = self.discover_files()?;
        let files_scanned = all_files.len();
        let file_discovery = discovery_start.elapsed();

        // Phase 2: Find and parse .osu files in parallel
        let parsing_start = Instant::now();
        let beatmaps = self.parse_osu_files_parallel(&all_files)?;
        let osu_files_found = beatmaps.len();
        let parsing = parsing_start.elapsed();

        // Phase 3: Group into sets
        let sets = self.group_into_sets(beatmaps);
        let sets_created = sets.len();

        let timing = LazerScanTiming {
            total: start.elapsed(),
            file_discovery,
            parsing,
            files_scanned,
            osu_files_found,
            sets_created,
        };

        Ok((sets, timing))
    }

    /// Scan all beatmaps (without timing)
    pub fn scan(&self) -> Result<Vec<LazerBeatmapSet>> {
        self.scan_with_timing().map(|(sets, _)| sets)
    }

    /// Discover all files in the files directory
    fn discover_files(&self) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();

        // Walk the hex directory structure (0-9, a-f)
        for first_char in "0123456789abcdef".chars() {
            let first_dir = self.files_path.join(first_char.to_string());
            if !first_dir.exists() {
                continue;
            }

            // Second level (00-ff)
            for entry in fs::read_dir(&first_dir)? {
                let entry = entry?;
                let second_dir = entry.path();
                if !second_dir.is_dir() {
                    continue;
                }

                // Files in second level
                for file_entry in fs::read_dir(&second_dir)? {
                    let file_entry = file_entry?;
                    let path = file_entry.path();
                    if path.is_file() {
                        files.push(path);
                    }
                }
            }
        }

        Ok(files)
    }

    /// Check if a file is an .osu file by reading its header
    fn is_osu_file(path: &Path) -> bool {
        if let Ok(file) = fs::File::open(path) {
            let mut reader = BufReader::new(file);
            let mut first_line = String::new();
            if reader.read_line(&mut first_line).is_ok() {
                return first_line.starts_with("osu file format");
            }
        }
        false
    }

    /// Parse .osu files in parallel
    fn parse_osu_files_parallel(&self, files: &[PathBuf]) -> Result<Vec<ParsedBeatmap>> {
        let beatmaps: Vec<ParsedBeatmap> = files
            .par_iter()
            .filter_map(|path| {
                if Self::is_osu_file(path) {
                    self.parse_osu_file(path).ok()
                } else {
                    None
                }
            })
            .collect();

        Ok(beatmaps)
    }

    /// Parse a single .osu file
    fn parse_osu_file(&self, path: &Path) -> Result<ParsedBeatmap> {
        let content = fs::read(path)?;

        // Use rosu-map for parsing
        let beatmap = rosu_map::Beatmap::from_bytes(&content)
            .map_err(|e| Error::Other(format!("Failed to parse .osu file: {}", e)))?;

        // Extract hash from filename
        let hash = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        // Convert to our format
        let mode = match beatmap.mode {
            rosu_map::section::general::GameMode::Osu => GameMode::Osu,
            rosu_map::section::general::GameMode::Taiko => GameMode::Taiko,
            rosu_map::section::general::GameMode::Catch => GameMode::Catch,
            rosu_map::section::general::GameMode::Mania => GameMode::Mania,
        };

        let metadata = BeatmapMetadata {
            title: beatmap.title.clone(),
            title_unicode: if beatmap.title_unicode.is_empty() { None } else { Some(beatmap.title_unicode.clone()) },
            artist: beatmap.artist.clone(),
            artist_unicode: if beatmap.artist_unicode.is_empty() { None } else { Some(beatmap.artist_unicode.clone()) },
            creator: beatmap.creator.clone(),
            source: if beatmap.source.is_empty() { None } else { Some(beatmap.source.clone()) },
            tags: beatmap.tags.split_whitespace().map(String::from).collect(),
            beatmap_id: if beatmap.beatmap_id > 0 { Some(beatmap.beatmap_id as i32) } else { None },
            beatmap_set_id: if beatmap.beatmap_set_id > 0 { Some(beatmap.beatmap_set_id as i32) } else { None },
        };

        let difficulty = BeatmapDifficulty {
            hp_drain: beatmap.hp_drain_rate,
            circle_size: beatmap.circle_size,
            overall_difficulty: beatmap.overall_difficulty,
            approach_rate: beatmap.approach_rate,
            slider_multiplier: beatmap.slider_multiplier,
            slider_tick_rate: beatmap.slider_tick_rate,
        };

        // Calculate length from hit objects
        let length_ms = beatmap.hit_objects.last()
            .map(|ho| ho.start_time as u64)
            .unwrap_or(0);

        // Calculate BPM from timing points
        let timing_points = &beatmap.control_points.timing_points;
        let bpm = timing_points.first()
            .filter(|tp| tp.beat_len > 0.0)
            .map(|tp| 60000.0 / tp.beat_len)
            .unwrap_or(120.0);

        Ok(ParsedBeatmap {
            hash,
            set_id: beatmap.beatmap_set_id as i32,
            beatmap_id: beatmap.beatmap_id as i32,
            metadata,
            difficulty,
            version: beatmap.version.clone(),
            mode,
            length_ms,
            bpm,
            audio_file: beatmap.audio_file.clone(),
            background_file: None, // Would need to parse events
        })
    }

    /// Group parsed beatmaps into sets by BeatmapSetID
    fn group_into_sets(&self, beatmaps: Vec<ParsedBeatmap>) -> Vec<LazerBeatmapSet> {
        let mut sets_map: HashMap<i32, Vec<ParsedBeatmap>> = HashMap::new();
        let mut orphans: Vec<ParsedBeatmap> = Vec::new();

        for beatmap in beatmaps {
            if beatmap.set_id > 0 {
                sets_map.entry(beatmap.set_id).or_default().push(beatmap);
            } else {
                orphans.push(beatmap);
            }
        }

        let mut result = Vec::new();

        // Convert grouped beatmaps to LazerBeatmapSet
        for (set_id, beatmaps) in sets_map {
            let lazer_beatmaps: Vec<LazerBeatmapInfo> = beatmaps
                .iter()
                .map(|b| LazerBeatmapInfo {
                    id: format!("lazer-{}", b.beatmap_id),
                    online_id: if b.beatmap_id > 0 { Some(b.beatmap_id) } else { None },
                    hash: b.hash.clone(),
                    md5_hash: String::new(), // Would need to compute
                    metadata: b.metadata.clone(),
                    difficulty: b.difficulty.clone(),
                    version: b.version.clone(),
                    mode: b.mode,
                    length_ms: b.length_ms,
                    bpm: b.bpm,
                    star_rating: None, // Would need to compute
                    ranked_status: None, // Would need to check online
                })
                .collect();

            result.push(LazerBeatmapSet {
                id: format!("lazer-set-{}", set_id),
                online_id: Some(set_id),
                beatmaps: lazer_beatmaps,
                files: Vec::new(), // Would need file mapping from Realm
            });
        }

        // Handle orphan beatmaps (no set ID)
        for beatmap in orphans {
            result.push(LazerBeatmapSet {
                id: format!("lazer-orphan-{}", beatmap.beatmap_id),
                online_id: None,
                beatmaps: vec![LazerBeatmapInfo {
                    id: format!("lazer-{}", beatmap.beatmap_id),
                    online_id: if beatmap.beatmap_id > 0 { Some(beatmap.beatmap_id) } else { None },
                    hash: beatmap.hash.clone(),
                    md5_hash: String::new(),
                    metadata: beatmap.metadata.clone(),
                    difficulty: beatmap.difficulty.clone(),
                    version: beatmap.version.clone(),
                    mode: beatmap.mode,
                    length_ms: beatmap.length_ms,
                    bpm: beatmap.bpm,
                    star_rating: None,
                    ranked_status: None,
                }],
                files: Vec::new(),
            });
        }

        result
    }
}

/// Internal struct for parsed beatmap data
#[derive(Debug, Clone)]
struct ParsedBeatmap {
    hash: String,
    set_id: i32,
    beatmap_id: i32,
    metadata: BeatmapMetadata,
    difficulty: BeatmapDifficulty,
    version: String,
    mode: GameMode,
    length_ms: u64,
    bpm: f64,
    audio_file: String,
    background_file: Option<String>,
}

// =============================================================================
// Realm Database Approach (experimental)
// =============================================================================

/// Attempt to read the Realm database directly
///
/// Note: This is experimental and may not work with newer Realm versions.
/// The realm-db-reader crate only supports version 9.9, but osu!lazer
/// currently uses version 24.24.
pub fn try_realm_reader(data_path: &Path) -> Result<Vec<LazerBeatmapSet>> {
    let realm_path = data_path.join("client.realm");

    // Try to open with realm-db-reader
    match realm_db_reader::Realm::open(&realm_path) {
        Ok(realm) => {
            tracing::info!("Successfully opened Realm database");

            // Try to read the group
            match realm.into_group() {
                Ok(group) => {
                    tracing::info!("Realm has {} tables", group.table_count());

                    // List all tables
                    for i in 0..group.table_count() {
                        if let Ok(table) = group.get_table(i) {
                            let row_count = table.row_count().unwrap_or(0);
                            tracing::info!("Table {}: {} rows", i, row_count);
                        }
                    }

                    // Try to get the BeatmapSetInfo table
                    if let Ok(table) = group.get_table_by_name("class_BeatmapSetInfo") {
                        let row_count = table.row_count().unwrap_or(0);
                        tracing::info!("Found BeatmapSetInfo table with {} rows", row_count);
                        // TODO: Parse the table contents
                    }

                    Ok(Vec::new())
                }
                Err(e) => {
                    tracing::warn!("Failed to read Realm group: {}", e);
                    Err(Error::Realm(format!("Failed to read Realm group: {}", e)))
                }
            }
        }
        Err(e) => {
            tracing::warn!("Failed to open Realm database (likely version mismatch): {}", e);
            Err(Error::Realm(format!("Failed to open Realm database: {}", e)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lazer_scanner_new() {
        let scanner = LazerScanner::new(PathBuf::from("/tmp/osu"));
        assert!(!scanner.is_valid());
    }
}
