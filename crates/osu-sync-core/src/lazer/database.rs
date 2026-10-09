//! osu!lazer and osu!stable database readers
//!
//! - **osu!stable osu!.db**: read with the `osu-db` crate
//! - **osu!lazer client.realm**: read by the `realm-export` helper in `tools/realm-export`,
//!   which loads the `Realm.dll` shipped with osu!lazer and prints the library as JSON

use crate::beatmap::{
    BeatmapDifficulty, BeatmapFile, BeatmapInfo, BeatmapMetadata, BeatmapSet, GameMode,
};
use crate::error::{Error, Result};
use crate::lazer::LazerFileStore;
use crate::stats::RankedStatus;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Environment variable naming the `realm-export` helper when it is not next to osu-sync
pub const REALM_EXPORT_ENV: &str = "OSU_SYNC_REALM_EXPORT";

const DOTNET_DOWNLOAD: &str = "https://dotnet.microsoft.com/download/dotnet/8.0";

/// Timing for reading the lazer library
#[derive(Debug, Clone, Default)]
pub struct LazerScanTiming {
    /// Time from starting the realm export to a parsed set list
    pub total: Duration,
    pub sets: usize,
    pub beatmaps: usize,
    pub named_files: usize,
}

impl LazerScanTiming {
    pub fn report(&self) -> String {
        format!(
            "Lazer realm export completed in {:.2}s\n - {} sets, {} beatmaps, {} named files",
            self.total.as_secs_f64(),
            self.sets,
            self.beatmaps,
            self.named_files,
        )
    }
}

/// Reader for osu!lazer's Realm database
pub struct LazerDatabase {
    file_store: LazerFileStore,
    sets: Vec<LazerBeatmapSet>,
    export_time: Duration,
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

#[derive(Deserialize)]
struct ExportedSet {
    id: String,
    online_id: i32,
    delete_pending: bool,
    beatmaps: Vec<ExportedBeatmap>,
    files: Vec<ExportedFile>,
}

#[derive(Deserialize)]
struct ExportedBeatmap {
    id: String,
    online_id: i32,
    hash: Option<String>,
    md5_hash: Option<String>,
    difficulty_name: Option<String>,
    ruleset: i32,
    length_ms: f64,
    bpm: f64,
    star_rating: f64,
    status: i32,
    hidden: bool,
    title: Option<String>,
    title_unicode: Option<String>,
    artist: Option<String>,
    artist_unicode: Option<String>,
    author: Option<String>,
    source: Option<String>,
    tags: Option<String>,
    drain_rate: f32,
    circle_size: f32,
    overall_difficulty: f32,
    approach_rate: f32,
    slider_multiplier: f64,
    slider_tick_rate: f64,
}

#[derive(Deserialize)]
struct ExportedFile {
    filename: Option<String>,
    hash: Option<String>,
}

impl LazerDatabase {
    /// Read the lazer library at `data_path` through the `realm-export` helper
    ///
    /// Fails with a message naming the missing piece when the helper, the .NET 8 runtime
    /// or osu!lazer's `Realm.dll` cannot be found.
    pub fn open(data_path: &Path) -> Result<Self> {
        let realm_path = data_path.join("client.realm");
        if !realm_path.exists() {
            return Err(Error::OsuNotFound(data_path.to_path_buf()));
        }

        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf));
        let helper = find_realm_export(exe_dir.as_deref(), std::env::var_os(REALM_EXPORT_ENV))?;

        let start = Instant::now();
        let json = run_realm_export(&helper, &realm_path)?;
        let sets = parse_realm_export(&json)?;
        let export_time = start.elapsed();
        tracing::info!(
            "Read {} lazer sets from {:?} in {:.2}s",
            sets.len(),
            realm_path,
            export_time.as_secs_f64()
        );

        Ok(Self {
            file_store: LazerFileStore::new(data_path),
            sets,
            export_time,
        })
    }

    /// Always true, because `open` fails when the realm cannot be read
    pub fn is_realm_available(&self) -> bool {
        true
    }

    /// Get the file store
    pub fn file_store(&self) -> &LazerFileStore {
        &self.file_store
    }

    /// Get all beatmap sets that are not pending deletion
    pub fn get_all_beatmap_sets(&self) -> Result<Vec<LazerBeatmapSet>> {
        Ok(self.sets.clone())
    }

    /// Get all beatmap sets with the time the realm export took
    pub fn get_all_beatmap_sets_timed(&self) -> Result<(Vec<LazerBeatmapSet>, LazerScanTiming)> {
        let timing = LazerScanTiming {
            total: self.export_time,
            sets: self.sets.len(),
            beatmaps: self.sets.iter().map(|s| s.beatmaps.len()).sum(),
            named_files: self.sets.iter().map(|s| s.files.len()).sum(),
        };
        Ok((self.sets.clone(), timing))
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

fn find_realm_export(exe_dir: Option<&Path>, env_value: Option<OsString>) -> Result<PathBuf> {
    if let Some(dir) = exe_dir {
        for name in ["realm-export.exe", "realm-export.dll"] {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    if let Some(value) = env_value.filter(|value| !value.is_empty()) {
        let path = PathBuf::from(value);
        if path.is_file() {
            return Ok(path);
        }
        return Err(Error::Realm(format!(
            "realm-export helper not found: {} points to {}, which is not a file",
            REALM_EXPORT_ENV,
            path.display()
        )));
    }

    let searched = exe_dir.map_or_else(
        || "the osu-sync folder".to_string(),
        |dir| dir.display().to_string(),
    );
    Err(Error::Realm(format!(
        "realm-export helper not found in {searched} and {REALM_EXPORT_ENV} is not set. \
         Build it with `dotnet publish tools/realm-export -c Release -o <osu-sync folder>` \
         or set {REALM_EXPORT_ENV} to the path of realm-export.exe"
    )))
}

fn run_realm_export(helper: &Path, realm_path: &Path) -> Result<Vec<u8>> {
    let is_dll = helper
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("dll"));
    let mut command = if is_dll {
        let mut dotnet = Command::new("dotnet");
        dotnet.arg(helper);
        dotnet
    } else {
        Command::new(helper)
    };

    let output = command
        .arg("export")
        .arg(realm_path)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| {
            if is_dll && e.kind() == std::io::ErrorKind::NotFound {
                Error::Realm(missing_runtime_message("`dotnet` is not on PATH."))
            } else {
                Error::Realm(format!("could not start {}: {}", helper.display(), e))
            }
        })?;

    if output.status.success() {
        return Ok(output.stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(Error::Realm(match output.status.code() {
        Some(code) if is_dotnet_host_failure(code) => missing_runtime_message(&stderr),
        _ => format!("realm-export failed ({}): {}", output.status, stderr),
    }))
}

/// The .NET host reports a missing or unusable runtime with HRESULTs 0x800080xx
fn is_dotnet_host_failure(code: i32) -> bool {
    (code as u32) & 0xFFFF_FF00 == 0x8000_8000
}

fn missing_runtime_message(detail: &str) -> String {
    format!(
        "realm-export needs the .NET 8 runtime, which was not found. \
         Install it from {DOTNET_DOWNLOAD}. {detail}"
    )
}

fn parse_realm_export(json: &[u8]) -> Result<Vec<LazerBeatmapSet>> {
    let exported: Vec<ExportedSet> = serde_json::from_slice(json)
        .map_err(|e| Error::Realm(format!("could not parse realm-export output: {}", e)))?;
    Ok(exported
        .into_iter()
        .filter(|set| !set.delete_pending)
        .map(convert_exported_set)
        .collect())
}

fn positive(id: i32) -> Option<i32> {
    (id > 0).then_some(id)
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.is_empty())
}

fn convert_exported_set(set: ExportedSet) -> LazerBeatmapSet {
    let online_id = positive(set.online_id);
    LazerBeatmapSet {
        id: set.id,
        online_id,
        beatmaps: set
            .beatmaps
            .into_iter()
            .filter(|beatmap| !beatmap.hidden)
            .map(|beatmap| convert_exported_beatmap(beatmap, online_id))
            .collect(),
        files: set
            .files
            .into_iter()
            .filter_map(|file| {
                Some(LazerNamedFile {
                    filename: file.filename?,
                    hash: file.hash?,
                })
            })
            .collect(),
    }
}

fn convert_exported_beatmap(
    beatmap: ExportedBeatmap,
    set_online_id: Option<i32>,
) -> LazerBeatmapInfo {
    let online_id = positive(beatmap.online_id);
    LazerBeatmapInfo {
        id: beatmap.id,
        online_id,
        hash: beatmap.hash.unwrap_or_default(),
        md5_hash: beatmap.md5_hash.unwrap_or_default(),
        metadata: BeatmapMetadata {
            title: beatmap.title.unwrap_or_default(),
            title_unicode: non_empty(beatmap.title_unicode),
            artist: beatmap.artist.unwrap_or_default(),
            artist_unicode: non_empty(beatmap.artist_unicode),
            creator: beatmap.author.unwrap_or_default(),
            source: non_empty(beatmap.source),
            tags: beatmap
                .tags
                .unwrap_or_default()
                .split_whitespace()
                .map(String::from)
                .collect(),
            beatmap_id: online_id,
            beatmap_set_id: set_online_id,
        },
        difficulty: BeatmapDifficulty {
            hp_drain: beatmap.drain_rate,
            circle_size: beatmap.circle_size,
            overall_difficulty: beatmap.overall_difficulty,
            approach_rate: beatmap.approach_rate,
            slider_multiplier: beatmap.slider_multiplier,
            slider_tick_rate: beatmap.slider_tick_rate,
        },
        version: beatmap.difficulty_name.unwrap_or_default(),
        mode: match beatmap.ruleset {
            1 => GameMode::Taiko,
            2 => GameMode::Catch,
            3 => GameMode::Mania,
            _ => GameMode::Osu,
        },
        length_ms: beatmap.length_ms as u64,
        bpm: beatmap.bpm,
        star_rating: (beatmap.star_rating >= 0.0).then_some(beatmap.star_rating as f32),
        ranked_status: LazerDatabase::convert_lazer_status(beatmap.status),
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn parses_realm_export_json() {
        let sets = parse_realm_export(include_bytes!("fixtures/realm-export.json")).unwrap();

        let ids: Vec<&str> = sets.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "095c3dda-8978-4df0-ba2e-c76e1198636c",
                "28beffbe-be5f-436a-9264-51be6287a34c"
            ]
        );

        let triangles = &sets[0];
        assert_eq!(triangles.online_id, None);
        assert_eq!(triangles.beatmaps.len(), 1);
        assert_eq!(triangles.beatmaps[0].version, "peppy");
        assert_eq!(triangles.beatmaps[0].ranked_status, None);
        assert_eq!(triangles.files.len(), 2);
        assert_eq!(triangles.files[0].filename, "audio.mp3");
        assert_eq!(
            triangles.files[0].hash,
            "47b895484e7751f3ab429694ff6dbf21e774ab023e4f6c5b481476f04ff22f0f"
        );

        let marisa = &sets[1];
        assert_eq!(marisa.online_id, Some(243));
        assert_eq!(marisa.files.len(), 5);
        let versions: Vec<&str> = marisa.beatmaps.iter().map(|b| b.version.as_str()).collect();
        assert_eq!(versions, ["Easy", "Normal"]);

        let easy = &marisa.beatmaps[0];
        assert_eq!(easy.id, "3e8bafd7-0772-40db-86d3-3673752153dd");
        assert_eq!(easy.online_id, Some(1145));
        assert_eq!(easy.md5_hash, "8b29e773161340bc1fff247a6ab749d1");
        assert_eq!(easy.mode, GameMode::Osu);
        assert_eq!(easy.length_ms, 220942);
        assert_eq!(easy.star_rating, Some(1.918_921_2));
        assert_eq!(easy.ranked_status, Some(RankedStatus::Ranked));
        assert_eq!(easy.metadata.artist, "IOSYS");
        assert_eq!(easy.metadata.creator, "DJPop");
        assert_eq!(easy.metadata.beatmap_id, Some(1145));
        assert_eq!(easy.metadata.beatmap_set_id, Some(243));
        assert_eq!(easy.difficulty.circle_size, 5.0);
        assert_eq!(easy.difficulty.slider_multiplier, 0.5);
    }

    #[test]
    fn missing_helper_is_an_error() {
        let dir = TempDir::new().unwrap();

        let err = find_realm_export(Some(dir.path()), None)
            .unwrap_err()
            .to_string();
        assert!(err.contains("realm-export helper not found"), "{err}");
        assert!(err.contains(REALM_EXPORT_ENV), "{err}");

        let missing = dir.path().join("nowhere").join("realm-export.exe");
        let err = find_realm_export(Some(dir.path()), Some(missing.clone().into_os_string()))
            .unwrap_err()
            .to_string();
        assert!(err.contains(&missing.display().to_string()), "{err}");

        std::fs::write(dir.path().join("realm-export.dll"), b"").unwrap();
        assert_eq!(
            find_realm_export(Some(dir.path()), Some(missing.into_os_string())).unwrap(),
            dir.path().join("realm-export.dll")
        );
    }

    #[test]
    fn missing_runtime_names_dotnet_8() {
        assert!(is_dotnet_host_failure(0x8000_8096_u32 as i32));
        assert!(!is_dotnet_host_failure(1));
        let message = missing_runtime_message("");
        assert!(message.contains(".NET 8"), "{message}");
        assert!(message.contains(DOTNET_DOWNLOAD), "{message}");
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
