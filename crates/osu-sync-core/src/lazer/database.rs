//! osu!lazer Realm database reader
//!
//! This module provides database reading capabilities for osu! beatmap data.
//!
//! ## Supported formats:
//! - **osu!stable osu!.db**: Full support via the `osu-db` crate
//! - **osu!lazer Realm**: Placeholder - Realm database reading is complex
//!   and may require alternative approaches (see `LazerDatabase::open`)
//!
//! For lazer, consider these alternatives:
//! 1. Use exported `.osz` files from lazer
//! 2. Scan the lazer files directory and reconstruct from .osu files
//! 3. Use FFI to realm-cpp (requires C++ build)

use crate::beatmap::{
    BeatmapDifficulty, BeatmapFile, BeatmapInfo, BeatmapMetadata, BeatmapSet, GameMode,
};
use crate::error::{Error, Result};
use crate::lazer::LazerFileStore;
use crate::stats::RankedStatus;
use std::path::{Path, PathBuf};

/// Reader for osu!lazer's Realm database
pub struct LazerDatabase {
    #[allow(dead_code)]
    data_path: PathBuf,
    file_store: LazerFileStore,
}

/// Beatmap info as stored in lazer's Realm database
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
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

        Ok(Self {
            data_path: data_path.to_path_buf(),
            file_store: LazerFileStore::new(data_path),
        })
    }

    /// Get the file store
    pub fn file_store(&self) -> &LazerFileStore {
        &self.file_store
    }

    /// Get all beatmap sets from the database
    ///
    /// Note: This is a placeholder. Real implementation would read from Realm.
    pub fn get_all_beatmap_sets(&self) -> Result<Vec<LazerBeatmapSet>> {
        // TODO: Implement actual Realm database reading
        // This would use realm-db-reader or similar

        tracing::warn!("Realm database reading not yet implemented - returning empty list");

        // For now, return empty to allow compilation
        // Real implementation would:
        // 1. Open client.realm with realm-db-reader
        // 2. Query BeatmapSetInfo table
        // 3. For each set, query BeatmapInfo entries
        // 4. For each beatmap, query RealmNamedFileUsage entries

        Ok(Vec::new())
    }

    /// Get a beatmap set by its online ID
    pub fn get_set_by_online_id(&self, online_id: i32) -> Result<Option<LazerBeatmapSet>> {
        let sets = self.get_all_beatmap_sets()?;
        Ok(sets.into_iter().find(|s| s.online_id == Some(online_id)))
    }

    /// Get a beatmap by its MD5 hash
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

    /// Get number of sets
    pub fn len(&self) -> usize {
        self.sets.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.sets.is_empty()
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
    pub fn get_set_by_online_id(&self, online_id: i32) -> Result<Option<LazerBeatmapSet>> {
        let sets = self.get_all_beatmap_sets()?;
        Ok(sets.into_iter().find(|s| s.online_id == Some(online_id)))
    }

    /// Get a beatmap by its MD5 hash
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
