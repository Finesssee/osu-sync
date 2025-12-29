//! osu!lazer Realm database reader
//!
//! This module provides database reading capabilities for osu! beatmap data.
//!
//! ## Supported formats:
//! - **osu!stable osu!.db**: Full support via the `osu-db` crate
//! - **osu!lazer Realm**: Full support via the `realm-db-reader` crate
//!
//! ## osu!lazer Realm Schema:
//! - **BeatmapSet** table: Contains beatmap set metadata (OnlineID, Hash, Status, etc.)
//! - **Beatmap** table: Contains individual beatmap difficulties
//! - **File** table: Content-addressed file storage (SHA-256 hash as key)

use crate::beatmap::{
    BeatmapDifficulty, BeatmapFile, BeatmapInfo, BeatmapMetadata, BeatmapSet, GameMode,
};
use crate::error::{Error, Result};
use crate::lazer::LazerFileStore;
use crate::stats::RankedStatus;
use realm_db_reader::{Group, Realm, Row, Table, Value};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Timing breakdown for lazer scan operations
#[derive(Debug, Clone, Default)]
pub struct LazerScanTiming {
    /// Total scan duration
    pub total: Duration,
    /// Time spent listing files in the store
    pub file_listing: Duration,
    /// Time spent detecting .osu files (header check)
    pub header_detection: Duration,
    /// Time spent parsing .osu files
    pub osu_parsing: Duration,
    /// Time spent grouping beatmaps into sets
    pub grouping: Duration,
    /// Number of files in the store
    pub total_files: usize,
    /// Number of .osu files found
    pub osu_files_found: usize,
    /// Number of .osu files successfully parsed
    pub osu_files_parsed: usize,
    /// Number of beatmap sets created
    pub sets_created: usize,
    /// Whether result was loaded from cache
    pub from_cache: bool,
    /// Whether parallel scanning was used
    pub parallel: bool,
    /// Number of threads used
    pub thread_count: usize,
}

impl LazerScanTiming {
    /// Format as human-readable report (similar to stable's format)
    pub fn report(&self) -> String {
        if self.from_cache {
            format!(
                "Lazer scan completed in {:.2}s (cached)\n\
                 - Cache load: {:.2}s\n\
                 - {} sets, {} beatmaps",
                self.total.as_secs_f64(),
                self.total.as_secs_f64(),
                self.sets_created,
                self.osu_files_parsed,
            )
        } else {
            let mode_info = if self.parallel {
                format!(" (parallel, {} threads)", self.thread_count)
            } else {
                " (sequential)".to_string()
            };

            let parse_speed = if self.osu_parsing.as_secs_f64() > 0.0 {
                self.osu_files_parsed as f64 / self.osu_parsing.as_secs_f64()
            } else {
                0.0
            };

            format!(
                "Lazer scan completed in {:.2}s{}\n\
                 - File listing: {:.2}s ({} files)\n\
                 - Header detection: {:.2}s ({} .osu files found)\n\
                 - .osu parsing: {:.2}s ({} files, {:.0} files/sec)\n\
                 - Grouping: {:.2}s ({} sets)",
                self.total.as_secs_f64(),
                mode_info,
                self.file_listing.as_secs_f64(),
                self.total_files,
                self.header_detection.as_secs_f64(),
                self.osu_files_found,
                self.osu_parsing.as_secs_f64(),
                self.osu_files_parsed,
                parse_speed,
                self.grouping.as_secs_f64(),
                self.sets_created,
            )
        }
    }
}

/// Cache for file-scanned beatmap sets
#[derive(Debug, Clone, Serialize, Deserialize)]
struct BeatmapCache {
    /// Number of files in the file store when cache was created
    file_count: usize,
    /// Number of beatmaps parsed
    beatmaps_parsed: usize,
    /// Cached beatmap sets
    sets: Vec<LazerBeatmapSet>,
}

/// Reader for osu!lazer's Realm database
pub struct LazerDatabase {
    #[allow(dead_code)]
    data_path: PathBuf,
    file_store: LazerFileStore,
    /// The Realm database group (root of all tables)
    realm_group: Option<Group>,
}

/// Beatmap info as stored in lazer's Realm database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LazerBeatmapInfo {
    /// Unique ID (GUID in Realm)
    pub id: String,
    /// Online beatmap ID
    pub online_id: Option<i32>,
    /// SHA-256 hash
    pub hash: String,
    /// MD5 hash (for online matching)
    pub md5_hash: String,
    /// Beatmap metadata
    pub metadata: BeatmapMetadata,
    /// Difficulty settings
    pub difficulty: BeatmapDifficulty,
    /// Difficulty/version name
    pub version: String,
    /// Game mode
    pub mode: GameMode,
    /// Length in milliseconds
    pub length_ms: u64,
    /// BPM
    pub bpm: f64,
    /// Star rating for this difficulty (from osu! database)
    pub star_rating: Option<f32>,
    /// Ranked status of this beatmap
    pub ranked_status: Option<RankedStatus>,
}

/// Beatmap set as stored in lazer's Realm database
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LazerBeatmapSet {
    /// Unique ID (GUID in Realm)
    pub id: String,
    /// Online beatmap set ID
    pub online_id: Option<i32>,
    /// All beatmaps in this set
    pub beatmaps: Vec<LazerBeatmapInfo>,
    /// Files in this set (with original names)
    pub files: Vec<LazerNamedFile>,
}

/// File reference with original filename
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LazerNamedFile {
    /// Original filename
    pub filename: String,
    /// SHA-256 hash (content address)
    pub hash: String,
}

impl LazerDatabase {
    /// Open the lazer database at the given path
    pub fn open(data_path: &Path) -> Result<Self> {
        let realm_path = data_path.join("client.realm");
        if !realm_path.exists() {
            return Err(Error::OsuNotFound(data_path.to_path_buf()));
        }

        // Try to open the Realm database
        // Note: This may fail if osu!lazer is running (database locked)
        let realm_group = match Realm::open(&realm_path) {
            Ok(realm) => match realm.into_group() {
                Ok(group) => {
                    tracing::info!("Successfully opened Realm database at {:?}", realm_path);
                    Some(group)
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to read Realm database group: {}. Database may be locked by osu!lazer.",
                        e
                    );
                    None
                }
            },
            Err(e) => {
                tracing::warn!(
                    "Failed to open Realm database: {}. Database may be locked by osu!lazer.",
                    e
                );
                None
            }
        };

        Ok(Self {
            data_path: data_path.to_path_buf(),
            file_store: LazerFileStore::new(data_path),
            realm_group,
        })
    }

    /// Check if the Realm database is available for reading
    pub fn is_realm_available(&self) -> bool {
        self.realm_group.is_some()
    }

    /// Get the file store
    pub fn file_store(&self) -> &LazerFileStore {
        &self.file_store
    }

    /// Get all beatmap sets from the database
    ///
    /// Tries to read from the Realm database first. If that fails (unsupported version,
    /// database locked, etc.), falls back to scanning .osu files from the file store.
    pub fn get_all_beatmap_sets(&self) -> Result<Vec<LazerBeatmapSet>> {
        let (sets, _timing) = self.get_all_beatmap_sets_timed()?;
        Ok(sets)
    }

    /// Get all beatmap sets with detailed timing information
    ///
    /// Tries to read from the Realm database first. If that fails (unsupported version,
    /// database locked, etc.), falls back to scanning .osu files from the file store.
    pub fn get_all_beatmap_sets_timed(&self) -> Result<(Vec<LazerBeatmapSet>, LazerScanTiming)> {
        // Try Realm database first
        if let Some(group) = &self.realm_group {
            match self.get_beatmap_sets_from_realm(group) {
                Ok(sets) if !sets.is_empty() => {
                    let timing = LazerScanTiming {
                        sets_created: sets.len(),
                        osu_files_parsed: sets.iter().map(|s| s.beatmaps.len()).sum(),
                        from_cache: false, // Realm is not cache
                        ..Default::default()
                    };
                    return Ok((sets, timing));
                }
                Ok(_) => {
                    tracing::info!("Realm returned empty, trying file scan fallback");
                }
                Err(e) => {
                    tracing::warn!("Realm read failed: {}, trying file scan fallback", e);
                }
            }
        }

        // Fallback: scan .osu files from the file store
        tracing::info!("Using file scan fallback to enumerate beatmaps");
        self.get_beatmap_sets_from_file_scan_timed()
    }

    /// Get the cache file path
    fn cache_path(&self) -> PathBuf {
        self.data_path.join(".osu-sync-cache.json")
    }

    /// Try to load beatmap sets from cache
    fn load_from_cache(&self, current_file_count: usize) -> Option<(Vec<LazerBeatmapSet>, usize)> {
        let cache_path = self.cache_path();
        if !cache_path.exists() {
            return None;
        }

        let data = std::fs::read_to_string(&cache_path).ok()?;
        let cache: BeatmapCache = serde_json::from_str(&data).ok()?;

        // Validate cache - file count must match
        if cache.file_count != current_file_count {
            tracing::info!(
                "Cache invalidated: file count changed ({} -> {})",
                cache.file_count,
                current_file_count
            );
            return None;
        }

        tracing::info!("Loaded {} beatmap sets from cache", cache.sets.len());
        Some((cache.sets, cache.beatmaps_parsed))
    }

    /// Save beatmap sets to cache
    fn save_to_cache(&self, sets: &[LazerBeatmapSet], file_count: usize, beatmaps_parsed: usize) {
        let cache = BeatmapCache {
            file_count,
            beatmaps_parsed,
            sets: sets.to_vec(),
        };

        if let Ok(data) = serde_json::to_string(&cache) {
            if let Err(e) = std::fs::write(self.cache_path(), data) {
                tracing::warn!("Failed to save cache: {}", e);
            } else {
                tracing::debug!("Saved {} beatmap sets to cache", sets.len());
            }
        }
    }

    /// Scan .osu files from the file store to build beatmap sets (without timing)
    fn get_beatmap_sets_from_file_scan(&self) -> Result<Vec<LazerBeatmapSet>> {
        let (sets, _timing) = self.get_beatmap_sets_from_file_scan_timed()?;
        Ok(sets)
    }

    /// Scan .osu files from the file store to build beatmap sets with detailed timing
    ///
    /// This is a fallback method when Realm database reading isn't available.
    /// Uses parallel processing for fast scanning and caching for instant subsequent loads.
    fn get_beatmap_sets_from_file_scan_timed(&self) -> Result<(Vec<LazerBeatmapSet>, LazerScanTiming)> {
        use rayon::prelude::*;
        use std::sync::atomic::{AtomicUsize, Ordering};

        let total_start = Instant::now();
        let mut timing = LazerScanTiming {
            parallel: true,
            thread_count: rayon::current_num_threads(),
            ..Default::default()
        };

        // Phase 0: List all files
        let listing_start = Instant::now();
        let all_hashes = self.file_store.list_all()?;
        timing.file_listing = listing_start.elapsed();
        timing.total_files = all_hashes.len();
        tracing::info!("Found {} files in lazer file store", timing.total_files);

        // Try to load from cache first
        if let Some((cached_sets, beatmaps_parsed)) = self.load_from_cache(timing.total_files) {
            timing.total = total_start.elapsed();
            timing.from_cache = true;
            timing.sets_created = cached_sets.len();
            timing.osu_files_parsed = beatmaps_parsed;
            timing.osu_files_found = beatmaps_parsed;
            return Ok((cached_sets, timing));
        }

        let scanned = AtomicUsize::new(0);
        let parsed = AtomicUsize::new(0);

        // Phase 1: Filter to only .osu files by checking header (parallel, fast)
        let header_start = Instant::now();
        let osu_file_header = b"osu file format";
        let osu_hashes: Vec<&String> = all_hashes
            .par_iter()
            .filter(|hash| {
                // Read just the first 20 bytes to check header
                if let Ok(prefix) = self.file_store.read_prefix(hash, 20) {
                    if prefix.len() >= 15 && prefix.starts_with(osu_file_header) {
                        return true;
                    }
                }
                false
            })
            .collect();
        timing.header_detection = header_start.elapsed();
        timing.osu_files_found = osu_hashes.len();

        tracing::info!(
            "Found {} .osu files out of {} total files",
            timing.osu_files_found,
            timing.total_files
        );

        // Phase 2: Parse all .osu files in parallel
        let parsing_start = Instant::now();
        let beatmap_infos: Vec<(String, LazerBeatmapInfo)> = osu_hashes
            .par_iter()
            .filter_map(|hash| {
                scanned.fetch_add(1, Ordering::Relaxed);

                // Read full file content
                let content = match self.file_store.read(hash) {
                    Ok(c) => c,
                    Err(_) => return None,
                };

                // Parse the .osu file using rosu-map
                let beatmap = match rosu_map::from_bytes::<rosu_map::Beatmap>(&content) {
                    Ok(b) => b,
                    Err(_) => return None,
                };

                parsed.fetch_add(1, Ordering::Relaxed);

                // Convert to LazerBeatmapInfo
                let beatmap_info = self.convert_rosu_beatmap(&beatmap, hash);
                Some((hash.to_string(), beatmap_info))
            })
            .collect();
        timing.osu_parsing = parsing_start.elapsed();
        timing.osu_files_parsed = parsed.load(Ordering::Relaxed);

        // Phase 3: Group by beatmapset_id (single-threaded, fast)
        let grouping_start = Instant::now();
        let mut sets_map: HashMap<i32, LazerBeatmapSet> = HashMap::new();
        let mut orphan_beatmaps: Vec<LazerBeatmapInfo> = Vec::new();

        for (_hash, beatmap_info) in beatmap_infos {
            if let Some(set_id) = beatmap_info.metadata.beatmap_set_id {
                let set = sets_map.entry(set_id).or_insert_with(|| LazerBeatmapSet {
                    id: format!("scan-{}", set_id),
                    online_id: Some(set_id),
                    beatmaps: Vec::new(),
                    files: Vec::new(),
                });
                set.beatmaps.push(beatmap_info);
            } else {
                orphan_beatmaps.push(beatmap_info);
            }
        }
        timing.grouping = grouping_start.elapsed();

        tracing::info!(
            "Scanned {} .osu files, parsed {} successfully, found {} sets",
            scanned.load(Ordering::Relaxed),
            timing.osu_files_parsed,
            sets_map.len()
        );

        // Convert to Vec and add orphans as individual sets
        let mut result: Vec<LazerBeatmapSet> = sets_map.into_values().collect();

        for (i, beatmap) in orphan_beatmaps.into_iter().enumerate() {
            result.push(LazerBeatmapSet {
                id: format!("orphan-{}", i),
                online_id: None,
                beatmaps: vec![beatmap],
                files: Vec::new(),
            });
        }

        timing.sets_created = result.len();
        timing.total = total_start.elapsed();

        // Save to cache for next time
        self.save_to_cache(&result, timing.total_files, timing.osu_files_parsed);

        Ok((result, timing))
    }

    /// Convert a rosu_map::Beatmap to LazerBeatmapInfo
    fn convert_rosu_beatmap(&self, beatmap: &rosu_map::Beatmap, hash: &str) -> LazerBeatmapInfo {
        let mode = match beatmap.mode {
            rosu_map::section::general::GameMode::Osu => GameMode::Osu,
            rosu_map::section::general::GameMode::Taiko => GameMode::Taiko,
            rosu_map::section::general::GameMode::Catch => GameMode::Catch,
            rosu_map::section::general::GameMode::Mania => GameMode::Mania,
        };

        let metadata = BeatmapMetadata {
            title: beatmap.title.clone(),
            title_unicode: if beatmap.title_unicode.is_empty() {
                None
            } else {
                Some(beatmap.title_unicode.clone())
            },
            artist: beatmap.artist.clone(),
            artist_unicode: if beatmap.artist_unicode.is_empty() {
                None
            } else {
                Some(beatmap.artist_unicode.clone())
            },
            creator: beatmap.creator.clone(),
            source: if beatmap.source.is_empty() {
                None
            } else {
                Some(beatmap.source.clone())
            },
            tags: beatmap
                .tags
                .split_whitespace()
                .map(String::from)
                .collect(),
            beatmap_id: if beatmap.beatmap_id > 0 {
                Some(beatmap.beatmap_id as i32)
            } else {
                None
            },
            beatmap_set_id: if beatmap.beatmap_set_id > 0 {
                Some(beatmap.beatmap_set_id as i32)
            } else {
                None
            },
        };

        let difficulty = BeatmapDifficulty {
            hp_drain: beatmap.hp_drain_rate,
            circle_size: beatmap.circle_size,
            overall_difficulty: beatmap.overall_difficulty,
            approach_rate: beatmap.approach_rate,
            slider_multiplier: beatmap.slider_multiplier,
            slider_tick_rate: beatmap.slider_tick_rate,
        };

        // Calculate BPM from timing points
        let bpm = beatmap
            .control_points
            .timing_points
            .first()
            .map(|tp| 60000.0 / tp.beat_len)
            .unwrap_or(120.0);

        // Calculate length from hit objects
        let length_ms = beatmap
            .hit_objects
            .last()
            .map(|ho| ho.start_time as u64)
            .unwrap_or(0);

        LazerBeatmapInfo {
            id: format!("scan-{}", hash),
            online_id: metadata.beatmap_id,
            hash: hash.to_string(),
            md5_hash: String::new(), // Would need to calculate
            metadata,
            difficulty,
            version: beatmap.version.clone(),
            mode,
            length_ms,
            bpm,
            star_rating: None, // Not available from .osu file
            ranked_status: None,
        }
    }

    /// Get beatmap sets from the Realm database
    fn get_beatmap_sets_from_realm(&self, group: &Group) -> Result<Vec<LazerBeatmapSet>> {
        // Log available table names for debugging
        let table_names = group.get_table_names();
        tracing::debug!("Available tables in Realm: {:?}", table_names);

        // Get the BeatmapSet table (class_BeatmapSetInfo in Realm)
        let beatmap_set_table = match group.get_table_by_name("class_BeatmapSetInfo") {
            Ok(table) => table,
            Err(e) => {
                tracing::warn!("BeatmapSetInfo table not found: {}. Trying alternative names...", e);
                // Try alternative table names
                match group.get_table_by_name("BeatmapSetInfo") {
                    Ok(table) => table,
                    Err(_) => {
                        tracing::error!("Could not find BeatmapSetInfo table in Realm database");
                        return Ok(Vec::new());
                    }
                }
            }
        };

        // Get the Beatmap table for beatmap info
        let beatmap_table = group.get_table_by_name("class_BeatmapInfo").ok();

        // Get the Metadata table
        let metadata_table = group.get_table_by_name("class_BeatmapMetadata").ok();

        // Get the RulesetInfo table
        let ruleset_table = group.get_table_by_name("class_RulesetInfo").ok();

        // Get the File table for hash lookups
        let file_table = group.get_table_by_name("class_RealmFile").ok();

        let row_count = beatmap_set_table.row_count().unwrap_or(0);
        tracing::info!("Found {} beatmap sets in Realm database", row_count);

        let mut result = Vec::with_capacity(row_count);

        for row_idx in 0..row_count {
            let row = match beatmap_set_table.get_row(row_idx) {
                Ok(row) => row,
                Err(e) => {
                    tracing::debug!("Failed to get row {}: {}", row_idx, e);
                    continue;
                }
            };

            // Skip sets marked for deletion
            if let Some(Value::Bool(true)) = row.get("DeletePending") {
                continue;
            }

            // Parse the beatmap set
            if let Some(set) = self.parse_beatmap_set(
                &row,
                beatmap_table.as_ref(),
                metadata_table.as_ref(),
                ruleset_table.as_ref(),
                file_table.as_ref(),
            ) {
                result.push(set);
            }
        }

        tracing::info!(
            "Successfully loaded {} beatmap sets from Realm database",
            result.len()
        );
        Ok(result)
    }

    /// Parse a BeatmapSetInfo row into a LazerBeatmapSet
    fn parse_beatmap_set(
        &self,
        row: &Row,
        beatmap_table: Option<&Table>,
        metadata_table: Option<&Table>,
        ruleset_table: Option<&Table>,
        file_table: Option<&Table>,
    ) -> Option<LazerBeatmapSet> {
        // Get the ID (stored as string in Realm for UUIDs)
        let id = match row.get("ID") {
            Some(Value::String(uuid)) => uuid.clone(),
            Some(Value::Binary(bytes)) => {
                // UUID might be stored as binary
                hex::encode(bytes)
            }
            _ => {
                // Generate a fallback ID
                format!("set-{}", row.entries().count())
            }
        };

        // Get online ID
        let online_id = match row.get("OnlineID") {
            Some(Value::Int(id)) if *id > 0 => Some(*id as i32),
            _ => None,
        };

        // Parse beatmaps (linked list)
        let beatmaps =
            self.parse_linked_beatmaps(row, beatmap_table, metadata_table, ruleset_table);

        // Parse files (embedded list of RealmNamedFileUsage)
        let files = self.parse_files(row, file_table);

        Some(LazerBeatmapSet {
            id,
            online_id,
            beatmaps,
            files,
        })
    }

    /// Parse beatmaps linked to a beatmap set
    fn parse_linked_beatmaps(
        &self,
        set_row: &Row,
        beatmap_table: Option<&Table>,
        metadata_table: Option<&Table>,
        ruleset_table: Option<&Table>,
    ) -> Vec<LazerBeatmapInfo> {
        let beatmap_table = match beatmap_table {
            Some(t) => t,
            None => return Vec::new(),
        };

        let mut beatmaps = Vec::new();

        // Get the Beatmaps link list from the set
        if let Some(Value::LinkList(links)) = set_row.get("Beatmaps") {
            for link in links {
                if let Ok(beatmap_row) = beatmap_table.get_row(link.row_number) {
                    // Skip hidden beatmaps
                    if let Some(Value::Bool(true)) = beatmap_row.get("Hidden") {
                        continue;
                    }

                    if let Some(beatmap) =
                        self.parse_beatmap(&beatmap_row, metadata_table, ruleset_table)
                    {
                        beatmaps.push(beatmap);
                    }
                }
            }
        }

        beatmaps
    }

    /// Parse a single beatmap row
    fn parse_beatmap(
        &self,
        row: &Row,
        metadata_table: Option<&Table>,
        ruleset_table: Option<&Table>,
    ) -> Option<LazerBeatmapInfo> {
        // Get ID (stored as string or binary UUID)
        let id = match row.get("ID") {
            Some(Value::String(uuid)) => uuid.clone(),
            Some(Value::Binary(bytes)) => hex::encode(bytes),
            _ => format!("beatmap-{}", row.entries().count()),
        };

        // Get online ID
        let online_id = match row.get("OnlineID") {
            Some(Value::Int(id)) if *id > 0 => Some(*id as i32),
            _ => None,
        };

        // Get hash (SHA-256)
        let hash = match row.get("Hash") {
            Some(Value::String(h)) => h.clone(),
            _ => String::new(),
        };

        // Get MD5 hash
        let md5_hash = match row.get("MD5Hash") {
            Some(Value::String(h)) => h.clone(),
            _ => String::new(),
        };

        // Get version/difficulty name
        let version = match row.get("DifficultyName") {
            Some(Value::String(v)) => v.clone(),
            _ => String::new(),
        };

        // Get length in milliseconds (Length is stored in seconds as double)
        let length_ms = match row.get("Length") {
            Some(Value::Double(len)) => (*len * 1000.0) as u64,
            _ => 0,
        };

        // Get BPM
        let bpm = match row.get("BPM") {
            Some(Value::Double(b)) => *b,
            _ => 120.0,
        };

        // Get star rating
        let star_rating = match row.get("StarRating") {
            Some(Value::Double(sr)) => Some(*sr as f32),
            _ => None,
        };

        // Get status
        let ranked_status = match row.get("StatusInt") {
            Some(Value::Int(status)) => Self::convert_lazer_status(*status as i32),
            _ => None,
        };

        // Get game mode from Ruleset link
        let mode = self.parse_ruleset(row, ruleset_table);

        // Get metadata from linked BeatmapMetadata
        let metadata = self.parse_metadata(row, metadata_table);

        // Get difficulty settings from embedded Difficulty object
        let difficulty = self.parse_difficulty(row);

        Some(LazerBeatmapInfo {
            id,
            online_id,
            hash,
            md5_hash,
            metadata,
            difficulty,
            version,
            mode,
            length_ms,
            bpm,
            star_rating,
            ranked_status,
        })
    }

    /// Parse metadata from a linked BeatmapMetadata object
    fn parse_metadata(&self, beatmap_row: &Row, metadata_table: Option<&Table>) -> BeatmapMetadata {
        let metadata_table = match metadata_table {
            Some(t) => t,
            None => return BeatmapMetadata::default(),
        };

        // Get the Metadata link
        let metadata_row = match beatmap_row.get("Metadata") {
            Some(Value::Link(link)) => match metadata_table.get_row(link.row_number) {
                Ok(row) => row,
                Err(_) => return BeatmapMetadata::default(),
            },
            _ => return BeatmapMetadata::default(),
        };

        let title = match metadata_row.get("Title") {
            Some(Value::String(t)) => t.clone(),
            _ => String::new(),
        };

        let title_unicode = match metadata_row.get("TitleUnicode") {
            Some(Value::String(t)) if !t.is_empty() => Some(t.clone()),
            _ => None,
        };

        let artist = match metadata_row.get("Artist") {
            Some(Value::String(a)) => a.clone(),
            _ => String::new(),
        };

        let artist_unicode = match metadata_row.get("ArtistUnicode") {
            Some(Value::String(a)) if !a.is_empty() => Some(a.clone()),
            _ => None,
        };

        let creator = match metadata_row.get("Author") {
            // Note: lazer uses "Author" not "Creator"
            Some(Value::String(c)) => c.clone(),
            _ => String::new(),
        };

        let source = match metadata_row.get("Source") {
            Some(Value::String(s)) if !s.is_empty() => Some(s.clone()),
            _ => None,
        };

        let tags: Vec<String> = match metadata_row.get("Tags") {
            Some(Value::String(t)) => t.split_whitespace().map(String::from).collect(),
            _ => Vec::new(),
        };

        // Get beatmap IDs from the beatmap row, not metadata
        let beatmap_id = match beatmap_row.get("OnlineID") {
            Some(Value::Int(id)) if *id > 0 => Some(*id as i32),
            _ => None,
        };

        // Get set ID from linked BeatmapSet if available
        let beatmap_set_id = None; // Will be set by the caller

        BeatmapMetadata {
            title,
            title_unicode,
            artist,
            artist_unicode,
            creator,
            source,
            tags,
            beatmap_id,
            beatmap_set_id,
        }
    }

    /// Parse difficulty settings from a beatmap row
    fn parse_difficulty(&self, beatmap_row: &Row) -> BeatmapDifficulty {
        // In lazer, Difficulty is stored as an embedded object (subtable)
        // Try to access it as a Table value first
        if let Some(Value::Table(difficulty_rows)) = beatmap_row.get("Difficulty") {
            if let Some(diff_row) = difficulty_rows.first() {
                return BeatmapDifficulty {
                    hp_drain: Self::get_float_value(diff_row, "DrainRate").unwrap_or(5.0),
                    circle_size: Self::get_float_value(diff_row, "CircleSize").unwrap_or(5.0),
                    overall_difficulty: Self::get_float_value(diff_row, "OverallDifficulty")
                        .unwrap_or(5.0),
                    approach_rate: Self::get_float_value(diff_row, "ApproachRate").unwrap_or(5.0),
                    slider_multiplier: Self::get_double_value(diff_row, "SliderMultiplier")
                        .unwrap_or(1.4),
                    slider_tick_rate: Self::get_double_value(diff_row, "SliderTickRate")
                        .unwrap_or(1.0),
                };
            }
        }

        // Fallback: try to get difficulty values directly from the beatmap row
        BeatmapDifficulty {
            hp_drain: Self::get_float_value(beatmap_row, "DrainRate").unwrap_or(5.0),
            circle_size: Self::get_float_value(beatmap_row, "CircleSize").unwrap_or(5.0),
            overall_difficulty: Self::get_float_value(beatmap_row, "OverallDifficulty")
                .unwrap_or(5.0),
            approach_rate: Self::get_float_value(beatmap_row, "ApproachRate").unwrap_or(5.0),
            slider_multiplier: Self::get_double_value(beatmap_row, "SliderMultiplier")
                .unwrap_or(1.4),
            slider_tick_rate: Self::get_double_value(beatmap_row, "SliderTickRate").unwrap_or(1.0),
        }
    }

    /// Helper to get a float value from a row
    fn get_float_value(row: &Row, name: &str) -> Option<f32> {
        match row.get(name) {
            Some(Value::Float(v)) => Some(*v),
            Some(Value::Double(v)) => Some(*v as f32),
            _ => None,
        }
    }

    /// Helper to get a double value from a row
    fn get_double_value(row: &Row, name: &str) -> Option<f64> {
        match row.get(name) {
            Some(Value::Double(v)) => Some(*v),
            Some(Value::Float(v)) => Some(*v as f64),
            _ => None,
        }
    }

    /// Parse ruleset (game mode) from a linked RulesetInfo
    fn parse_ruleset(&self, beatmap_row: &Row, ruleset_table: Option<&Table>) -> GameMode {
        let ruleset_table = match ruleset_table {
            Some(t) => t,
            None => return GameMode::Osu,
        };

        let ruleset_row = match beatmap_row.get("Ruleset") {
            Some(Value::Link(link)) => match ruleset_table.get_row(link.row_number) {
                Ok(row) => row,
                Err(_) => return GameMode::Osu,
            },
            _ => return GameMode::Osu,
        };

        // Get OnlineID which corresponds to game mode
        match ruleset_row.get("OnlineID") {
            Some(Value::Int(0)) => GameMode::Osu,
            Some(Value::Int(1)) => GameMode::Taiko,
            Some(Value::Int(2)) => GameMode::Catch,
            Some(Value::Int(3)) => GameMode::Mania,
            _ => GameMode::Osu,
        }
    }

    /// Parse files from embedded RealmNamedFileUsage list
    fn parse_files(&self, set_row: &Row, file_table: Option<&Table>) -> Vec<LazerNamedFile> {
        let mut files = Vec::new();

        // Files are stored as an embedded list (subtable)
        if let Some(Value::Table(file_rows)) = set_row.get("Files") {
            for file_row in file_rows {
                let filename = match file_row.get("Filename") {
                    Some(Value::String(f)) => f.clone(),
                    _ => continue,
                };

                // Get the hash from the linked File object
                let hash = match file_row.get("File") {
                    Some(Value::Link(link)) => {
                        // Look up the File in the file table to get its Hash
                        if let Some(ft) = file_table {
                            if let Ok(file_entry) = ft.get_row(link.row_number) {
                                match file_entry.get("Hash") {
                                    Some(Value::String(h)) => h.clone(),
                                    _ => String::new(),
                                }
                            } else {
                                String::new()
                            }
                        } else {
                            // Can't look up, use placeholder
                            format!("file-{}", link.row_number)
                        }
                    }
                    _ => String::new(),
                };

                files.push(LazerNamedFile { filename, hash });
            }
        }

        files
    }

    /// Convert lazer's BeatmapOnlineStatus enum to our RankedStatus
    fn convert_lazer_status(status: i32) -> Option<RankedStatus> {
        // osu!lazer BeatmapOnlineStatus enum values:
        // -3 = None, -2 = Graveyard, -1 = WIP, 0 = Pending
        // 1 = Ranked, 2 = Approved, 3 = Qualified, 4 = Loved
        Some(match status {
            -3 => return None,
            -2 => RankedStatus::Graveyard,
            -1 | 0 => RankedStatus::Pending,
            1 => RankedStatus::Ranked,
            2 => RankedStatus::Approved,
            3 => RankedStatus::Qualified,
            4 => RankedStatus::Loved,
            _ => return None,
        })
    }

    /// Get a beatmap set by its online ID
    ///
    /// # Deprecated
    /// This method loads ALL beatmap sets to find one, which is O(n).
    /// For efficient O(1) lookups, use [`LazerIndex::get_set`] instead:
    /// ```ignore
    /// let index = LazerIndex::build(&db)?;
    /// if let Some(set) = index.get_set(online_id) {
    ///     // use set
    /// }
    /// ```
    #[deprecated(
        since = "0.1.0",
        note = "Inefficient O(n) lookup. Use LazerIndex::get_set() for O(1) lookups."
    )]
    pub fn get_set_by_online_id(&self, online_id: i32) -> Result<Option<LazerBeatmapSet>> {
        let sets = self.get_all_beatmap_sets()?;
        Ok(sets.into_iter().find(|s| s.online_id == Some(online_id)))
    }

    /// Get a beatmap by its MD5 hash
    ///
    /// # Deprecated
    /// This method loads ALL beatmap sets to find one beatmap, which is O(n).
    /// For efficient O(1) lookups, use [`LazerIndex::get_beatmap`] instead:
    /// ```ignore
    /// let index = LazerIndex::build(&db)?;
    /// if let Some((set, beatmap)) = index.get_beatmap(md5) {
    ///     // use set and beatmap
    /// }
    /// ```
    #[deprecated(
        since = "0.1.0",
        note = "Inefficient O(n) lookup. Use LazerIndex::get_beatmap() for O(1) lookups."
    )]
    pub fn get_beatmap_by_md5(
        &self,
        md5: &str,
    ) -> Result<Option<(LazerBeatmapSet, LazerBeatmapInfo)>> {
        let sets = self.get_all_beatmap_sets()?;
        for set in sets {
            for beatmap in &set.beatmaps {
                if beatmap.md5_hash == md5 {
                    return Ok(Some((set.clone(), beatmap.clone())));
                }
            }
        }
        Ok(None)
    }

    /// Convert a LazerBeatmapSet to the common BeatmapSet type
    pub fn to_beatmap_set(&self, lazer_set: &LazerBeatmapSet) -> BeatmapSet {
        let beatmaps: Vec<BeatmapInfo> = lazer_set
            .beatmaps
            .iter()
            .map(|lb| BeatmapInfo {
                metadata: lb.metadata.clone(),
                difficulty: lb.difficulty.clone(),
                hash: lb.hash.clone(),
                md5_hash: lb.md5_hash.clone(),
                audio_file: String::new(), // Would need to find from files
                background_file: None,
                length_ms: lb.length_ms,
                bpm: lb.bpm,
                mode: lb.mode,
                version: lb.version.clone(),
                star_rating: lb.star_rating,
                ranked_status: lb.ranked_status,
            })
            .collect();

        let files: Vec<BeatmapFile> = lazer_set
            .files
            .iter()
            .map(|f| BeatmapFile {
                filename: f.filename.clone(),
                hash: f.hash.clone(),
                size: 0, // Would need to check file
            })
            .collect();

        BeatmapSet {
            id: lazer_set.online_id,
            beatmaps,
            files,
            folder_name: None,
        }
    }
}

/// Build an index of lazer beatmaps for fast lookup
pub struct LazerIndex {
    pub sets: Vec<LazerBeatmapSet>,
    by_online_id: std::collections::HashMap<i32, usize>,
    by_md5: std::collections::HashMap<String, (usize, usize)>,
}

impl LazerIndex {
    /// Build an index from the database
    pub fn build(db: &LazerDatabase) -> Result<Self> {
        let sets = db.get_all_beatmap_sets()?;

        let mut by_online_id = std::collections::HashMap::new();
        let mut by_md5 = std::collections::HashMap::new();

        for (set_idx, set) in sets.iter().enumerate() {
            if let Some(id) = set.online_id {
                by_online_id.insert(id, set_idx);
            }
            for (beatmap_idx, beatmap) in set.beatmaps.iter().enumerate() {
                by_md5.insert(beatmap.md5_hash.clone(), (set_idx, beatmap_idx));
            }
        }

        Ok(Self {
            sets,
            by_online_id,
            by_md5,
        })
    }

    /// Check if a beatmap set exists by online ID
    pub fn contains_set(&self, online_id: i32) -> bool {
        self.by_online_id.contains_key(&online_id)
    }

    /// Check if a beatmap exists by MD5 hash
    pub fn contains_hash(&self, md5: &str) -> bool {
        self.by_md5.contains_key(md5)
    }

    /// Get a beatmap set by online ID (O(1) lookup)
    pub fn get_set(&self, online_id: i32) -> Option<&LazerBeatmapSet> {
        self.by_online_id
            .get(&online_id)
            .map(|&idx| &self.sets[idx])
    }

    /// Get a beatmap by MD5 hash (O(1) lookup)
    pub fn get_beatmap(&self, md5: &str) -> Option<(&LazerBeatmapSet, &LazerBeatmapInfo)> {
        self.by_md5.get(md5).map(|&(set_idx, beatmap_idx)| {
            let set = &self.sets[set_idx];
            let beatmap = &set.beatmaps[beatmap_idx];
            (set, beatmap)
        })
    }

    /// Get number of sets
    pub fn len(&self) -> usize {
        self.sets.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.sets.is_empty()
    }

    /// Get total number of beatmaps (difficulties)
    pub fn beatmap_count(&self) -> usize {
        self.sets.iter().map(|s| s.beatmaps.len()).sum()
    }
}

// =============================================================================
// osu!stable database reader using osu-db crate
// =============================================================================

/// Reader for osu!stable's osu!.db file using the osu-db crate
///
/// This provides full support for reading the osu!.db binary format
/// which contains cached beatmap metadata for all installed beatmaps.
pub struct StableDatabase {
    /// Path to the osu! data directory
    data_path: PathBuf,
    /// Parsed listing from osu!.db
    listing: osu_db::Listing,
}

impl StableDatabase {
    /// Open and parse the osu!.db file at the given osu! directory
    ///
    /// # Arguments
    /// * `osu_path` - Path to the osu! installation directory (containing osu!.db)
    ///
    /// # Example
    /// ```no_run
    /// use osu_sync_core::lazer::StableDatabase;
    /// use std::path::Path;
    ///
    /// let db = StableDatabase::open(Path::new("C:/osu!"))?;
    /// let sets = db.get_all_beatmap_sets()?;
    /// println!("Found {} beatmap sets", sets.len());
    /// # Ok::<(), osu_sync_core::error::Error>(())
    /// ```
    pub fn open(osu_path: &Path) -> Result<Self> {
        let db_path = osu_path.join("osu!.db");
        if !db_path.exists() {
            return Err(Error::OsuNotFound(osu_path.to_path_buf()));
        }

        let listing = osu_db::Listing::from_file(&db_path)
            .map_err(|e| Error::Realm(format!("Failed to parse osu!.db: {}", e)))?;

        Ok(Self {
            data_path: osu_path.to_path_buf(),
            listing,
        })
    }

    /// Get the osu! data path
    pub fn data_path(&self) -> &Path {
        &self.data_path
    }

    /// Get the osu!.db version
    pub fn version(&self) -> u32 {
        self.listing.version
    }

    /// Get the player name from the database
    pub fn player_name(&self) -> Option<&str> {
        self.listing.player_name.as_deref()
    }

    /// Get the folder count (number of beatmap folders)
    pub fn folder_count(&self) -> u32 {
        self.listing.folder_count
    }

    /// Get the raw beatmap listing
    pub fn listing(&self) -> &osu_db::Listing {
        &self.listing
    }

    /// Get all beatmaps as raw osu-db Beatmap structs
    pub fn raw_beatmaps(&self) -> &[osu_db::listing::Beatmap] {
        &self.listing.beatmaps
    }

    /// Get all beatmap sets, grouped by beatmapset_id
    ///
    /// This groups individual beatmap difficulties into sets and converts
    /// them to the common `LazerBeatmapSet` type for compatibility with
    /// the rest of osu-sync.
    pub fn get_all_beatmap_sets(&self) -> Result<Vec<LazerBeatmapSet>> {
        use std::collections::HashMap;

        // Group beatmaps by set ID
        let mut sets_map: HashMap<i32, Vec<&osu_db::listing::Beatmap>> = HashMap::new();
        let mut no_set_id: Vec<&osu_db::listing::Beatmap> = Vec::new();

        for beatmap in &self.listing.beatmaps {
            if beatmap.beatmapset_id > 0 {
                sets_map
                    .entry(beatmap.beatmapset_id)
                    .or_default()
                    .push(beatmap);
            } else {
                // Beatmaps without a set ID get their own "set"
                no_set_id.push(beatmap);
            }
        }

        let mut result = Vec::new();

        // Convert grouped beatmaps to LazerBeatmapSet
        for (set_id, beatmaps) in sets_map {
            let lazer_beatmaps: Vec<LazerBeatmapInfo> =
                beatmaps.iter().map(|b| self.convert_beatmap(b)).collect();

            // Extract files from the first beatmap's folder
            let files = if let Some(first) = beatmaps.first() {
                self.get_files_for_beatmap(first)
            } else {
                Vec::new()
            };

            result.push(LazerBeatmapSet {
                id: format!("stable-{}", set_id),
                online_id: Some(set_id),
                beatmaps: lazer_beatmaps,
                files,
            });
        }

        // Handle beatmaps without set ID (create individual "sets")
        for beatmap in no_set_id {
            let lazer_beatmap = self.convert_beatmap(beatmap);
            let files = self.get_files_for_beatmap(beatmap);

            result.push(LazerBeatmapSet {
                id: format!("stable-orphan-{}", beatmap.beatmap_id),
                online_id: None,
                beatmaps: vec![lazer_beatmap],
                files,
            });
        }

        Ok(result)
    }

    /// Convert an osu-db Beatmap to LazerBeatmapInfo
    fn convert_beatmap(&self, beatmap: &osu_db::listing::Beatmap) -> LazerBeatmapInfo {
        let mode = match beatmap.mode {
            osu_db::Mode::Standard => GameMode::Osu,
            osu_db::Mode::Taiko => GameMode::Taiko,
            osu_db::Mode::CatchTheBeat => GameMode::Catch,
            osu_db::Mode::Mania => GameMode::Mania,
        };

        let metadata = BeatmapMetadata {
            title: beatmap.title_ascii.clone().unwrap_or_default(),
            title_unicode: beatmap.title_unicode.clone(),
            artist: beatmap.artist_ascii.clone().unwrap_or_default(),
            artist_unicode: beatmap.artist_unicode.clone(),
            creator: beatmap.creator.clone().unwrap_or_default(),
            source: beatmap.song_source.clone(),
            tags: beatmap
                .tags
                .clone()
                .map(|t| t.split_whitespace().map(String::from).collect())
                .unwrap_or_default(),
            beatmap_id: if beatmap.beatmap_id > 0 {
                Some(beatmap.beatmap_id)
            } else {
                None
            },
            beatmap_set_id: if beatmap.beatmapset_id > 0 {
                Some(beatmap.beatmapset_id)
            } else {
                None
            },
        };

        let difficulty = BeatmapDifficulty {
            hp_drain: beatmap.hp_drain,
            circle_size: beatmap.circle_size,
            overall_difficulty: beatmap.overall_difficulty,
            approach_rate: beatmap.approach_rate,
            slider_multiplier: beatmap.slider_velocity,
            slider_tick_rate: 1.0, // Not stored in osu!.db
        };

        // Calculate approximate BPM from timing points
        let bpm = self.calculate_bpm(beatmap);

        // Extract star rating for the beatmap's mode (no-mods, key 0)
        let star_rating = Self::extract_star_rating(beatmap, &mode);

        // Convert ranked status
        let ranked_status = Self::convert_ranked_status(beatmap.status);

        LazerBeatmapInfo {
            id: format!("stable-{}", beatmap.beatmap_id),
            online_id: if beatmap.beatmap_id > 0 {
                Some(beatmap.beatmap_id)
            } else {
                None
            },
            hash: String::new(), // osu!.db only has MD5, not SHA-256
            md5_hash: beatmap.hash.clone().unwrap_or_default(),
            metadata,
            difficulty,
            version: beatmap.difficulty_name.clone().unwrap_or_default(),
            mode,
            length_ms: beatmap.total_time as u64,
            bpm,
            star_rating,
            ranked_status,
        }
    }

    /// Extract star rating from osu-db beatmap for the given mode (no-mods)
    fn extract_star_rating(beatmap: &osu_db::listing::Beatmap, mode: &GameMode) -> Option<f32> {
        // Star ratings are stored per mode as Vec<(ModSet, f64)>
        // ModSet with raw value 0 = no mods
        let ratings = match mode {
            GameMode::Osu => &beatmap.std_ratings,
            GameMode::Taiko => &beatmap.taiko_ratings,
            GameMode::Catch => &beatmap.ctb_ratings,
            GameMode::Mania => &beatmap.mania_ratings,
        };

        // Find no-mod star rating (mods with bits value 0)
        ratings
            .iter()
            .find(|(mods, _)| mods.bits() == 0)
            .map(|(_, sr)| *sr as f32)
    }

    /// Convert osu-db ranked status to our RankedStatus enum
    fn convert_ranked_status(status: osu_db::listing::RankedStatus) -> Option<RankedStatus> {
        Some(match status {
            osu_db::listing::RankedStatus::Unknown => return None,
            osu_db::listing::RankedStatus::Unsubmitted => RankedStatus::Graveyard,
            osu_db::listing::RankedStatus::PendingWipGraveyard => RankedStatus::Pending,
            osu_db::listing::RankedStatus::Ranked => RankedStatus::Ranked,
            osu_db::listing::RankedStatus::Approved => RankedStatus::Approved,
            osu_db::listing::RankedStatus::Qualified => RankedStatus::Qualified,
            osu_db::listing::RankedStatus::Loved => RankedStatus::Loved,
        })
    }

    /// Calculate the main BPM from timing points
    fn calculate_bpm(&self, beatmap: &osu_db::listing::Beatmap) -> f64 {
        // Find the first non-inherited timing point (inherits=false means it defines BPM)
        for tp in &beatmap.timing_points {
            if !tp.inherits && tp.bpm > 0.0 {
                return tp.bpm;
            }
        }

        // Default BPM if no timing points found
        120.0
    }

    /// Get files associated with a beatmap from its folder
    fn get_files_for_beatmap(&self, beatmap: &osu_db::listing::Beatmap) -> Vec<LazerNamedFile> {
        let mut files = Vec::new();

        // Add the .osu file
        if let Some(ref osu_file) = &beatmap.file_name {
            files.push(LazerNamedFile {
                filename: osu_file.clone(),
                hash: beatmap.hash.clone().unwrap_or_default(),
            });
        }

        // Add audio file
        if let Some(ref audio) = &beatmap.audio {
            files.push(LazerNamedFile {
                filename: audio.clone(),
                hash: String::new(), // Would need to compute
            });
        }

        // Note: Full file listing would require scanning the folder on disk
        // The osu!.db only stores the .osu filename and audio filename

        files
    }

    /// Get a beatmap set by its online ID
    ///
    /// # Deprecated
    /// This method loads ALL beatmap sets to find one, which is O(n).
    /// For efficient O(1) lookups, use [`StableIndex::get_set`] instead:
    /// ```ignore
    /// let index = StableIndex::build(&db)?;
    /// if let Some(set) = index.get_set(online_id) {
    ///     // use set
    /// }
    /// ```
    #[deprecated(
        since = "0.1.0",
        note = "Inefficient O(n) lookup. Use StableIndex::get_set() for O(1) lookups."
    )]
    pub fn get_set_by_online_id(&self, online_id: i32) -> Result<Option<LazerBeatmapSet>> {
        let sets = self.get_all_beatmap_sets()?;
        Ok(sets.into_iter().find(|s| s.online_id == Some(online_id)))
    }

    /// Get a beatmap by its MD5 hash
    ///
    /// # Deprecated
    /// This method loads ALL beatmap sets to find one beatmap, which is O(n).
    /// For efficient O(1) lookups, use [`StableIndex::get_beatmap`] instead:
    /// ```ignore
    /// let index = StableIndex::build(&db)?;
    /// if let Some((set, beatmap)) = index.get_beatmap(md5) {
    ///     // use set and beatmap
    /// }
    /// ```
    #[deprecated(
        since = "0.1.0",
        note = "Inefficient O(n) lookup. Use StableIndex::get_beatmap() for O(1) lookups."
    )]
    pub fn get_beatmap_by_md5(
        &self,
        md5: &str,
    ) -> Result<Option<(LazerBeatmapSet, LazerBeatmapInfo)>> {
        let sets = self.get_all_beatmap_sets()?;
        for set in sets {
            for beatmap in &set.beatmaps {
                if beatmap.md5_hash == md5 {
                    return Ok(Some((set.clone(), beatmap.clone())));
                }
            }
        }
        Ok(None)
    }

    /// Convert a LazerBeatmapSet to the common BeatmapSet type
    pub fn to_beatmap_set(&self, lazer_set: &LazerBeatmapSet) -> BeatmapSet {
        let beatmaps: Vec<BeatmapInfo> = lazer_set
            .beatmaps
            .iter()
            .map(|lb| BeatmapInfo {
                metadata: lb.metadata.clone(),
                difficulty: lb.difficulty.clone(),
                hash: lb.hash.clone(),
                md5_hash: lb.md5_hash.clone(),
                audio_file: String::new(),
                background_file: None,
                length_ms: lb.length_ms,
                bpm: lb.bpm,
                mode: lb.mode,
                version: lb.version.clone(),
                star_rating: lb.star_rating,
                ranked_status: lb.ranked_status,
            })
            .collect();

        let files: Vec<BeatmapFile> = lazer_set
            .files
            .iter()
            .map(|f| BeatmapFile {
                filename: f.filename.clone(),
                hash: f.hash.clone(),
                size: 0,
            })
            .collect();

        BeatmapSet {
            id: lazer_set.online_id,
            beatmaps,
            files,
            folder_name: None,
        }
    }

    /// Get the Songs folder path
    pub fn songs_path(&self) -> PathBuf {
        self.data_path.join("Songs")
    }

    /// Get the full path to a beatmap folder
    pub fn get_beatmap_folder_path(&self, beatmap: &osu_db::listing::Beatmap) -> Option<PathBuf> {
        beatmap
            .folder_name
            .as_ref()
            .map(|f| self.songs_path().join(f))
    }
}

/// Build an index of stable beatmaps for fast lookup
pub struct StableIndex {
    pub sets: Vec<LazerBeatmapSet>,
    by_online_id: std::collections::HashMap<i32, usize>,
    by_md5: std::collections::HashMap<String, (usize, usize)>,
}

impl StableIndex {
    /// Build an index from the database
    pub fn build(db: &StableDatabase) -> Result<Self> {
        let sets = db.get_all_beatmap_sets()?;

        let mut by_online_id = std::collections::HashMap::new();
        let mut by_md5 = std::collections::HashMap::new();

        for (set_idx, set) in sets.iter().enumerate() {
            if let Some(id) = set.online_id {
                by_online_id.insert(id, set_idx);
            }
            for (beatmap_idx, beatmap) in set.beatmaps.iter().enumerate() {
                if !beatmap.md5_hash.is_empty() {
                    by_md5.insert(beatmap.md5_hash.clone(), (set_idx, beatmap_idx));
                }
            }
        }

        Ok(Self {
            sets,
            by_online_id,
            by_md5,
        })
    }

    /// Check if a beatmap set exists by online ID
    pub fn contains_set(&self, online_id: i32) -> bool {
        self.by_online_id.contains_key(&online_id)
    }

    /// Check if a beatmap exists by MD5 hash
    pub fn contains_hash(&self, md5: &str) -> bool {
        self.by_md5.contains_key(md5)
    }

    /// Get a beatmap set by online ID
    pub fn get_set(&self, online_id: i32) -> Option<&LazerBeatmapSet> {
        self.by_online_id
            .get(&online_id)
            .map(|&idx| &self.sets[idx])
    }

    /// Get a beatmap by MD5 hash
    pub fn get_beatmap(&self, md5: &str) -> Option<(&LazerBeatmapSet, &LazerBeatmapInfo)> {
        self.by_md5.get(md5).map(|&(set_idx, beatmap_idx)| {
            let set = &self.sets[set_idx];
            let beatmap = &set.beatmaps[beatmap_idx];
            (set, beatmap)
        })
    }

    /// Get number of sets
    pub fn len(&self) -> usize {
        self.sets.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.sets.is_empty()
    }

    /// Get total number of beatmaps (difficulties)
    pub fn beatmap_count(&self) -> usize {
        self.sets.iter().map(|s| s.beatmaps.len()).sum()
    }
}
