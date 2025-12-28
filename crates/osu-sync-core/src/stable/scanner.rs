//! Scan osu!stable Songs folder for beatmaps

use crate::beatmap::BeatmapSet;
use crate::error::{Error, Result};
use crate::parser::parse_osu_file;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use walkdir::WalkDir;

/// Timing breakdown for scan operations
#[derive(Debug, Clone, Default)]
pub struct ScanTiming {
    /// Total scan duration
    pub total: Duration,
    /// Time spent enumerating directories
    pub dir_enumeration: Duration,
    /// Time spent parsing .osu files
    pub osu_parsing: Duration,
    /// Time spent hashing files (SHA-256)
    pub file_hashing: Duration,
    /// Number of directories scanned
    pub dirs_scanned: usize,
    /// Number of .osu files parsed
    pub osu_files_parsed: usize,
    /// Number of files hashed
    pub files_hashed: usize,
    /// Total bytes hashed
    pub bytes_hashed: u64,
    /// Whether parallel scanning was used
    pub parallel: bool,
    /// Number of threads used (for parallel mode)
    pub thread_count: usize,
    /// Whether result was loaded from cache
    pub from_cache: bool,
}

impl ScanTiming {
    /// Format as human-readable report
    pub fn report(&self) -> String {
        if self.from_cache {
            return format!(
                "Scan completed in {:.2}s (cached)\n\
                 - Cache load: {:.2}s\n\
                 - {} dirs, {} beatmaps",
                self.total.as_secs_f64(),
                self.total.as_secs_f64(),
                self.dirs_scanned,
                self.osu_files_parsed,
            );
        }

        let hash_speed = if self.file_hashing.as_secs_f64() > 0.0 {
            (self.bytes_hashed as f64 / 1024.0 / 1024.0) / self.file_hashing.as_secs_f64()
        } else {
            0.0
        };

        let mode_info = if self.parallel {
            format!(" (parallel, {} threads)", self.thread_count)
        } else {
            " (sequential)".to_string()
        };

        format!(
            "Scan completed in {:.2}s{}\n\
             - Dir enumeration: {:.2}s ({} dirs)\n\
             - .osu parsing: {:.2}s ({} files, {:.0} files/sec)\n\
             - SHA-256 hashing: {:.2}s ({} files, {:.1} MB, {:.1} MB/s)",
            self.total.as_secs_f64(),
            mode_info,
            self.dir_enumeration.as_secs_f64(),
            self.dirs_scanned,
            self.osu_parsing.as_secs_f64(),
            self.osu_files_parsed,
            if self.osu_parsing.as_secs_f64() > 0.0 {
                self.osu_files_parsed as f64 / self.osu_parsing.as_secs_f64()
            } else {
                0.0
            },
            self.file_hashing.as_secs_f64(),
            self.files_hashed,
            self.bytes_hashed as f64 / 1024.0 / 1024.0,
            hash_speed,
        )
    }
}

/// Cache for scanned beatmap sets
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StableScanCache {
    /// Number of directories when cache was created
    dir_count: usize,
    /// Number of beatmaps parsed
    beatmaps_parsed: usize,
    /// Cached beatmap sets
    sets: Vec<BeatmapSet>,
}

/// Scanner for osu!stable Songs folder
pub struct StableScanner {
    songs_path: PathBuf,
    /// Skip file hashing for faster scans (hashes won't be available)
    skip_hashing: bool,
}

/// Progress callback for scanning (must be Sync for parallel scanning)
pub type ScanProgress = Box<dyn Fn(usize, usize, &str) + Send + Sync>;

impl StableScanner {
    /// Create a new scanner for the given Songs folder
    pub fn new(songs_path: PathBuf) -> Self {
        Self {
            songs_path,
            skip_hashing: false,
        }
    }

    /// Skip file hashing for faster scans (~3x speedup)
    /// File hashes won't be available in the results
    pub fn skip_hashing(mut self) -> Self {
        self.skip_hashing = true;
        self
    }

    /// Get the cache file path
    fn cache_path(&self) -> PathBuf {
        self.songs_path
            .parent()
            .unwrap_or(&self.songs_path)
            .join(".osu-sync-stable-cache.json")
    }

    /// Try to load from cache if valid
    fn load_from_cache(&self, current_dir_count: usize) -> Option<(Vec<BeatmapSet>, usize)> {
        let cache_path = self.cache_path();
        if !cache_path.exists() {
            return None;
        }

        let content = fs::read_to_string(&cache_path).ok()?;
        let cache: StableScanCache = serde_json::from_str(&content).ok()?;

        // Cache is valid if directory count matches
        if cache.dir_count == current_dir_count {
            tracing::info!(
                "Loaded {} beatmap sets from stable cache",
                cache.sets.len()
            );
            Some((cache.sets, cache.beatmaps_parsed))
        } else {
            tracing::info!(
                "Stable cache invalidated: dir count changed ({} -> {})",
                cache.dir_count,
                current_dir_count
            );
            None
        }
    }

    /// Save results to cache
    fn save_to_cache(&self, sets: &[BeatmapSet], dir_count: usize, beatmaps_parsed: usize) {
        let cache = StableScanCache {
            dir_count,
            beatmaps_parsed,
            sets: sets.to_vec(),
        };

        let cache_path = self.cache_path();
        match serde_json::to_string(&cache) {
            Ok(json) => {
                if let Err(e) = fs::write(&cache_path, json) {
                    tracing::warn!("Failed to write stable cache: {}", e);
                } else {
                    tracing::info!("Saved {} beatmap sets to stable cache", sets.len());
                }
            }
            Err(e) => {
                tracing::warn!("Failed to serialize stable cache: {}", e);
            }
        }
    }

    /// Scan all beatmap sets in the Songs folder
    pub fn scan(&self) -> Result<Vec<BeatmapSet>> {
        self.scan_with_progress(None)
    }

    /// Scan all beatmap sets with timing information
    pub fn scan_timed(&self) -> Result<(Vec<BeatmapSet>, ScanTiming)> {
        self.scan_timed_with_progress(None)
    }

    /// Scan all beatmap sets with progress callback
    pub fn scan_with_progress(&self, progress: Option<ScanProgress>) -> Result<Vec<BeatmapSet>> {
        let (sets, _timing) = self.scan_timed_with_progress(progress)?;
        Ok(sets)
    }

    /// Scan all beatmap sets with timing and progress callback
    pub fn scan_timed_with_progress(
        &self,
        progress: Option<ScanProgress>,
    ) -> Result<(Vec<BeatmapSet>, ScanTiming)> {
        let total_start = Instant::now();
        let mut timing = ScanTiming::default();

        if !self.songs_path.exists() {
            return Err(Error::OsuNotFound(self.songs_path.clone()));
        }

        let mut beatmap_sets = Vec::new();

        // Get all subdirectories (each is a beatmap set)
        let dir_start = Instant::now();
        let entries: Vec<_> = fs::read_dir(&self.songs_path)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();
        timing.dir_enumeration = dir_start.elapsed();
        timing.dirs_scanned = entries.len();

        let total = entries.len();

        for (idx, entry) in entries.into_iter().enumerate() {
            let dir_path = entry.path();
            let folder_name = dir_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            if let Some(ref cb) = progress {
                cb(idx + 1, total, &folder_name);
            }

            match self.scan_beatmap_set_timed(&dir_path, &mut timing) {
                Ok(mut set) => {
                    set.folder_name = Some(folder_name);
                    beatmap_sets.push(set);
                }
                Err(e) => {
                    tracing::warn!("Failed to scan {}: {}", dir_path.display(), e);
                }
            }
        }

        timing.total = total_start.elapsed();
        Ok((beatmap_sets, timing))
    }

    /// Scan all beatmap sets in parallel
    pub fn scan_parallel(&self) -> Result<Vec<BeatmapSet>> {
        let (sets, _timing) = self.scan_parallel_with_progress(None)?;
        Ok(sets)
    }

    /// Scan all beatmap sets in parallel with timing information
    pub fn scan_parallel_timed(&self) -> Result<(Vec<BeatmapSet>, ScanTiming)> {
        self.scan_parallel_with_progress(None)
    }

    /// Scan all beatmap sets in parallel with progress callback
    pub fn scan_parallel_with_progress(
        &self,
        progress: Option<ScanProgress>,
    ) -> Result<(Vec<BeatmapSet>, ScanTiming)> {
        let total_start = Instant::now();

        if !self.songs_path.exists() {
            return Err(Error::OsuNotFound(self.songs_path.clone()));
        }

        // Collect directories first (sequential, fast)
        let dir_start = Instant::now();
        let entries: Vec<_> = fs::read_dir(&self.songs_path)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();
        let dir_enumeration = dir_start.elapsed();

        let total = entries.len();

        // Try to load from cache
        if let Some((cached_sets, beatmaps_parsed)) = self.load_from_cache(total) {
            let timing = ScanTiming {
                total: total_start.elapsed(),
                dir_enumeration,
                dirs_scanned: total,
                osu_files_parsed: beatmaps_parsed,
                parallel: true,
                thread_count: rayon::current_num_threads(),
                from_cache: true,
                ..Default::default()
            };
            return Ok((cached_sets, timing));
        }

        let processed = AtomicUsize::new(0);
        let timing = Mutex::new(ScanTiming {
            dir_enumeration,
            dirs_scanned: total,
            parallel: true,
            thread_count: rayon::current_num_threads(),
            ..Default::default()
        });

        // Wrap progress callback in Arc for thread-safe sharing
        let progress = progress.map(std::sync::Arc::new);

        // Process in parallel
        let results: Vec<_> = entries
            .par_iter()
            .filter_map(|entry| {
                let dir_path = entry.path();
                let folder_name = dir_path.file_name()?.to_string_lossy().to_string();

                // Update progress
                let current = processed.fetch_add(1, Ordering::SeqCst);
                if let Some(ref cb) = progress {
                    cb(current + 1, total, &folder_name);
                }

                // Scan with local timing
                let mut local_timing = ScanTiming::default();
                match self.scan_beatmap_set_timed(&dir_path, &mut local_timing) {
                    Ok(mut set) => {
                        set.folder_name = Some(folder_name);

                        // Merge timing (aggregate across threads)
                        let mut t = timing.lock().unwrap();
                        t.osu_parsing += local_timing.osu_parsing;
                        t.file_hashing += local_timing.file_hashing;
                        t.osu_files_parsed += local_timing.osu_files_parsed;
                        t.files_hashed += local_timing.files_hashed;
                        t.bytes_hashed += local_timing.bytes_hashed;

                        Some(set)
                    }
                    Err(e) => {
                        tracing::warn!("Failed to scan {}: {}", dir_path.display(), e);
                        None
                    }
                }
            })
            .collect();

        let mut final_timing = timing.into_inner().unwrap();
        final_timing.total = total_start.elapsed();

        // Save to cache for next time
        self.save_to_cache(&results, total, final_timing.osu_files_parsed);

        Ok((results, final_timing))
    }

    /// Scan a single beatmap set directory
    #[allow(dead_code)]
    fn scan_beatmap_set(&self, dir: &Path) -> Result<BeatmapSet> {
        let mut timing = ScanTiming::default();
        self.scan_beatmap_set_timed(dir, &mut timing)
    }

    /// Scan a single beatmap set directory with timing
    fn scan_beatmap_set_timed(&self, dir: &Path, timing: &mut ScanTiming) -> Result<BeatmapSet> {
        let mut beatmap_set = BeatmapSet::new();

        // Find all .osu files
        let osu_files: Vec<_> = fs::read_dir(dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext.eq_ignore_ascii_case("osu"))
                    .unwrap_or(false)
            })
            .collect();

        if osu_files.is_empty() {
            return Err(Error::InvalidOsz {
                reason: "No .osu files found".to_string(),
            });
        }

        // Parse all .osu files
        for entry in osu_files {
            let path = entry.path();
            let parse_start = Instant::now();
            match parse_osu_file(&path) {
                Ok(info) => {
                    timing.osu_parsing += parse_start.elapsed();
                    timing.osu_files_parsed += 1;
                    if beatmap_set.id.is_none() {
                        beatmap_set.id = info.metadata.beatmap_set_id;
                    }
                    beatmap_set.beatmaps.push(info);
                }
                Err(e) => {
                    timing.osu_parsing += parse_start.elapsed();
                    tracing::warn!("Failed to parse {}: {}", path.display(), e);
                }
            }
        }

        // Collect all files in the directory (optionally hash them)
        if !self.skip_hashing {
            for entry in WalkDir::new(dir)
                .max_depth(1)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                if path.is_file() {
                    let hash_start = Instant::now();
                    if let Ok(content) = fs::read(path) {
                        let hash = format!("{:x}", Sha256::digest(&content));
                        timing.file_hashing += hash_start.elapsed();
                        timing.files_hashed += 1;
                        timing.bytes_hashed += content.len() as u64;

                        let filename = path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();

                        beatmap_set.files.push(crate::beatmap::BeatmapFile {
                            filename,
                            hash,
                            size: content.len() as u64,
                        });
                    }
                }
            }
        }

        if beatmap_set.beatmaps.is_empty() {
            return Err(Error::InvalidOsz {
                reason: "No valid beatmaps found".to_string(),
            });
        }

        Ok(beatmap_set)
    }

    /// Find a beatmap set by its online ID
    pub fn find_by_set_id(&self, set_id: i32) -> Result<Option<BeatmapSet>> {
        // Scan all sets and find matching one
        // In production, we'd want to build an index
        let sets = self.scan()?;
        Ok(sets.into_iter().find(|s| s.id == Some(set_id)))
    }

    /// Find a beatmap by its MD5 hash
    pub fn find_by_hash(&self, md5: &str) -> Result<Option<(BeatmapSet, usize)>> {
        let sets = self.scan()?;
        for set in sets {
            for (idx, beatmap) in set.beatmaps.iter().enumerate() {
                if beatmap.md5_hash == md5 {
                    return Ok(Some((set.clone(), idx)));
                }
            }
        }
        Ok(None)
    }

    /// Build an index of all beatmaps for faster lookups
    pub fn build_index(&self) -> Result<BeatmapIndex> {
        let sets = self.scan()?;
        Ok(BeatmapIndex::new(sets))
    }
}

/// Index for fast beatmap lookups
pub struct BeatmapIndex {
    /// All beatmap sets
    pub sets: Vec<BeatmapSet>,
    /// Index by beatmap set ID
    by_set_id: HashMap<i32, usize>,
    /// Index by beatmap MD5 hash
    by_md5: HashMap<String, (usize, usize)>, // (set_index, beatmap_index)
}

impl BeatmapIndex {
    /// Create a new index from beatmap sets
    pub fn new(sets: Vec<BeatmapSet>) -> Self {
        let mut by_set_id = HashMap::new();
        let mut by_md5 = HashMap::new();

        for (set_idx, set) in sets.iter().enumerate() {
            if let Some(id) = set.id {
                by_set_id.insert(id, set_idx);
            }
            for (beatmap_idx, beatmap) in set.beatmaps.iter().enumerate() {
                by_md5.insert(beatmap.md5_hash.clone(), (set_idx, beatmap_idx));
            }
        }

        Self {
            sets,
            by_set_id,
            by_md5,
        }
    }

    /// Find a beatmap set by ID
    pub fn get_set_by_id(&self, set_id: i32) -> Option<&BeatmapSet> {
        self.by_set_id.get(&set_id).map(|&idx| &self.sets[idx])
    }

    /// Find a beatmap by MD5 hash
    pub fn get_by_md5(&self, md5: &str) -> Option<(&BeatmapSet, &crate::beatmap::BeatmapInfo)> {
        self.by_md5.get(md5).map(|&(set_idx, beatmap_idx)| {
            (
                &self.sets[set_idx],
                &self.sets[set_idx].beatmaps[beatmap_idx],
            )
        })
    }

    /// Check if a beatmap set exists
    pub fn contains_set(&self, set_id: i32) -> bool {
        self.by_set_id.contains_key(&set_id)
    }

    /// Check if a beatmap exists by hash
    pub fn contains_hash(&self, md5: &str) -> bool {
        self.by_md5.contains_key(md5)
    }

    /// Get total number of beatmap sets
    pub fn len(&self) -> usize {
        self.sets.len()
    }

    /// Check if index is empty
    pub fn is_empty(&self) -> bool {
        self.sets.is_empty()
    }
}
