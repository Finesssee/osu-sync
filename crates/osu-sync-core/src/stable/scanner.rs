//! Scan osu!stable Songs folder for beatmaps

use crate::beatmap::{BeatmapInfo, BeatmapSet};
use crate::error::{Error, Result};
use crate::parser::parse_osu_file;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use walkdir::WalkDir;

type StableCacheLoad = (
    Vec<BeatmapSet>,
    usize,
    HashMap<String, CachedFileInfo>,
    HashMap<String, CachedOsuFile>,
);

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
             - Blake3 hashing: {:.2}s ({} files, {:.1} MB, {:.1} MB/s)",
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

/// Cached file metadata for incremental hashing
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedFileInfo {
    /// File modification time (as seconds since UNIX epoch)
    mtime_secs: u64,
    /// File size in bytes
    size: u64,
    /// Computed hash (Blake3)
    hash: String,
}

/// Cached .osu file parse result (avoids re-parsing unchanged files)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CachedOsuFile {
    /// File modification time (as seconds since UNIX epoch)
    mtime_secs: u64,
    /// File size in bytes
    size: u64,
    /// Parsed beatmap info
    beatmap_info: BeatmapInfo,
}

/// Cache for scanned beatmap sets (uses bincode for 5-10x faster serialization)
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StableScanCache {
    /// Cache format version for compatibility
    version: u32,
    /// Number of directories when cache was created
    dir_count: usize,
    /// Number of beatmaps parsed
    beatmaps_parsed: usize,
    /// Cached beatmap sets
    sets: Vec<BeatmapSet>,
    /// File hash cache: path (relative to Songs) -> CachedFileInfo
    file_hashes: HashMap<String, CachedFileInfo>,
    /// Parsed .osu file cache: path (relative to Songs) -> CachedOsuFile
    #[serde(default)]
    osu_cache: HashMap<String, CachedOsuFile>,
}

impl Default for StableScanCache {
    fn default() -> Self {
        Self {
            version: 3, // Bump version for new osu_cache field
            dir_count: 0,
            beatmaps_parsed: 0,
            sets: Vec::new(),
            file_hashes: HashMap::new(),
            osu_cache: HashMap::new(),
        }
    }
}

/// File metadata returned alongside hash to avoid redundant fs::metadata calls
#[derive(Debug, Clone)]
struct FileHashResult {
    /// Blake3 hash as hex string
    hash: String,
    /// File size in bytes
    size: u64,
    /// Modification time as seconds since UNIX epoch
    mtime_secs: u64,
}

/// Hash a file using Blake3 (5-10x faster than SHA-256)
/// Uses memory-mapping for files > 1MB for better performance
/// Returns hash along with file metadata to avoid redundant fs::metadata calls
fn hash_file_blake3(path: &Path) -> std::io::Result<FileHashResult> {
    let metadata = fs::metadata(path)?;
    let size = metadata.len();
    let mtime_secs = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    // Use memory-mapping for large files (> 1MB)
    let hash = if size > 1024 * 1024 {
        let file = fs::File::open(path)?;
        let mmap = unsafe { memmap2::Mmap::map(&file)? };
        blake3::hash(&mmap).to_hex().to_string()
    } else {
        // For small files, regular read is fine
        let content = fs::read(path)?;
        blake3::hash(&content).to_hex().to_string()
    };

    Ok(FileHashResult {
        hash,
        size,
        mtime_secs,
    })
}

/// Check if a file needs rehashing based on mtime/size
#[cfg(test)]
fn needs_rehash(path: &Path, cached: Option<&CachedFileInfo>) -> bool {
    let Some(cached) = cached else {
        return true; // Not in cache
    };

    let Ok(meta) = fs::metadata(path) else {
        return true; // Can't read metadata
    };

    let current_mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);

    current_mtime != cached.mtime_secs || meta.len() != cached.size
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

    /// Get the cache file path (bincode format for 5-10x faster load)
    fn cache_path(&self) -> PathBuf {
        self.songs_path
            .parent()
            .unwrap_or(&self.songs_path)
            .join(".osu-sync-stable-cache.bin")
    }

    /// Try to load from cache if valid
    /// Returns: (sets, beatmaps_parsed, file_hashes, osu_cache)
    fn load_from_cache(&self, current_dir_count: usize) -> Option<StableCacheLoad> {
        let cache_path = self.cache_path();
        if !cache_path.exists() {
            // Also try legacy JSON cache for migration
            let legacy_path = self
                .songs_path
                .parent()
                .unwrap_or(&self.songs_path)
                .join(".osu-sync-stable-cache.json");
            if legacy_path.exists() {
                // Delete legacy cache, will be recreated in new format
                let _ = fs::remove_file(&legacy_path);
            }
            return None;
        }

        let content = fs::read(&cache_path).ok()?;
        let cache: StableScanCache = bincode::deserialize(&content).ok()?;

        // Check cache version (3 = with osu_cache)
        if cache.version < 3 {
            tracing::info!(
                "Stable cache version mismatch ({}), rebuilding",
                cache.version
            );
            return None;
        }

        // Cache is valid if directory count matches
        if cache.dir_count == current_dir_count {
            tracing::info!(
                "Loaded {} beatmap sets from stable cache (bincode), {} cached .osu files",
                cache.sets.len(),
                cache.osu_cache.len()
            );
            Some((
                cache.sets,
                cache.beatmaps_parsed,
                cache.file_hashes,
                cache.osu_cache,
            ))
        } else {
            // Directory count changed - return empty sets but keep osu_cache for incremental parsing
            tracing::info!(
                "Stable cache dir count changed ({} -> {}), will use incremental parsing ({} cached .osu files)",
                cache.dir_count,
                current_dir_count,
                cache.osu_cache.len()
            );
            Some((Vec::new(), 0, cache.file_hashes, cache.osu_cache))
        }
    }

    /// Try to load just the osu_cache for incremental parsing
    fn load_osu_cache(&self) -> HashMap<String, CachedOsuFile> {
        let cache_path = self.cache_path();
        if !cache_path.exists() {
            return HashMap::new();
        }

        let content = match fs::read(&cache_path) {
            Ok(c) => c,
            Err(_) => return HashMap::new(),
        };

        let cache: StableScanCache = match bincode::deserialize(&content) {
            Ok(c) => c,
            Err(_) => return HashMap::new(),
        };

        cache.osu_cache
    }

    /// Save results to cache (bincode format)
    fn save_to_cache(
        &self,
        sets: &[BeatmapSet],
        dir_count: usize,
        beatmaps_parsed: usize,
        file_hashes: HashMap<String, CachedFileInfo>,
        osu_cache: HashMap<String, CachedOsuFile>,
    ) {
        let cache = StableScanCache {
            version: 3,
            dir_count,
            beatmaps_parsed,
            sets: sets.to_vec(),
            file_hashes,
            osu_cache,
        };

        let cache_path = self.cache_path();
        match bincode::serialize(&cache) {
            Ok(bytes) => {
                if let Err(e) = fs::write(&cache_path, bytes) {
                    tracing::warn!("Failed to write stable cache: {}", e);
                } else {
                    tracing::info!(
                        "Saved {} beatmap sets to stable cache (bincode)",
                        sets.len()
                    );
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

        // Try to load from cache (includes file hash cache for incremental updates)
        // Load osu_cache for incremental parsing even if full cache is invalid
        let osu_cache = self.load_osu_cache();
        if let Some((cached_sets, beatmaps_parsed, _file_hashes, _osu_cache)) =
            self.load_from_cache(total)
        {
            if !cached_sets.is_empty() {
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
        }
        let osu_cache = Arc::new(Mutex::new(osu_cache));

        let processed = AtomicUsize::new(0);
        let timing = Mutex::new(ScanTiming {
            dir_enumeration,
            dirs_scanned: total,
            parallel: true,
            thread_count: rayon::current_num_threads(),
            ..Default::default()
        });
        let file_hashes = Mutex::new(HashMap::new());

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

                // Scan with local timing and file hash collection
                let mut local_timing = ScanTiming::default();
                let mut local_hashes = HashMap::new();
                match self.scan_beatmap_set_timed_with_cache(
                    &dir_path,
                    &mut local_timing,
                    &mut local_hashes,
                ) {
                    Ok(mut set) => {
                        set.folder_name = Some(folder_name);

                        // Merge timing (aggregate across threads)
                        let mut t = timing.lock().unwrap();
                        t.osu_parsing += local_timing.osu_parsing;
                        t.file_hashing += local_timing.file_hashing;
                        t.osu_files_parsed += local_timing.osu_files_parsed;
                        t.files_hashed += local_timing.files_hashed;
                        t.bytes_hashed += local_timing.bytes_hashed;
                        drop(t);

                        // Merge file hashes
                        file_hashes.lock().unwrap().extend(local_hashes);

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
        let final_hashes = file_hashes.into_inner().unwrap();
        let final_osu_cache = osu_cache.lock().unwrap().clone();

        // Save to cache for next time (bincode format)
        self.save_to_cache(
            &results,
            total,
            final_timing.osu_files_parsed,
            final_hashes,
            final_osu_cache,
        );

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
        let mut dummy_hashes = HashMap::new();
        self.scan_beatmap_set_timed_with_cache(dir, timing, &mut dummy_hashes)
    }

    /// Scan a single beatmap set directory with timing and file hash caching
    fn scan_beatmap_set_timed_with_cache(
        &self,
        dir: &Path,
        timing: &mut ScanTiming,
        file_hash_cache: &mut HashMap<String, CachedFileInfo>,
    ) -> Result<BeatmapSet> {
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

        // Collect all files in the directory (optionally hash them using Blake3)
        if !self.skip_hashing {
            for entry in WalkDir::new(dir)
                .max_depth(1)
                .into_iter()
                .filter_map(|e| e.ok())
            {
                let path = entry.path();
                if path.is_file() {
                    let hash_start = Instant::now();

                    // Use Blake3 for hashing (5-10x faster than SHA-256)
                    // hash_file_blake3 returns hash + metadata to avoid redundant fs::metadata calls
                    if let Ok(result) = hash_file_blake3(path) {
                        timing.file_hashing += hash_start.elapsed();
                        timing.files_hashed += 1;
                        timing.bytes_hashed += result.size;

                        let filename = path
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();

                        // Cache the file info for incremental updates
                        let relative_path = path
                            .strip_prefix(&self.songs_path)
                            .map(|p| p.to_string_lossy().to_string())
                            .unwrap_or_else(|_| filename.clone());

                        file_hash_cache.insert(
                            relative_path,
                            CachedFileInfo {
                                mtime_secs: result.mtime_secs,
                                size: result.size,
                                hash: result.hash.clone(),
                            },
                        );

                        beatmap_set.files.push(crate::beatmap::BeatmapFile {
                            filename,
                            hash: result.hash,
                            size: result.size,
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ==================== Blake3 Hashing Tests ====================

    #[test]
    fn test_hash_file_blake3_small_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("small.txt");

        // Create a small file (< 1MB, won't use memmap)
        let content = b"Hello, osu-sync!";
        fs::write(&file_path, content).unwrap();

        let result = hash_file_blake3(&file_path).unwrap();

        // Blake3 produces 64 hex characters
        assert_eq!(result.hash.len(), 64);

        // Size should match content length
        assert_eq!(result.size, content.len() as u64);

        // mtime should be non-zero (file was just created)
        assert!(result.mtime_secs > 0);

        // Hash should be consistent
        let result2 = hash_file_blake3(&file_path).unwrap();
        assert_eq!(result.hash, result2.hash);
    }

    #[test]
    fn test_hash_file_blake3_large_file_uses_memmap() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("large.bin");

        // Create a file > 1MB to trigger memmap path
        let content = vec![0u8; 2 * 1024 * 1024]; // 2MB
        fs::write(&file_path, &content).unwrap();

        let result = hash_file_blake3(&file_path).unwrap();

        assert_eq!(result.hash.len(), 64);
        assert_eq!(result.size, content.len() as u64);

        // Verify it matches expected Blake3 hash of zeros
        let expected = blake3::hash(&content).to_hex().to_string();
        assert_eq!(result.hash, expected);
    }

    #[test]
    fn test_hash_file_blake3_different_content_different_hash() {
        let temp_dir = TempDir::new().unwrap();
        let file1 = temp_dir.path().join("file1.txt");
        let file2 = temp_dir.path().join("file2.txt");

        fs::write(&file1, b"content A").unwrap();
        fs::write(&file2, b"content B").unwrap();

        let result1 = hash_file_blake3(&file1).unwrap();
        let result2 = hash_file_blake3(&file2).unwrap();

        assert_ne!(result1.hash, result2.hash);
    }

    #[test]
    fn test_hash_file_blake3_nonexistent_file() {
        let result = hash_file_blake3(Path::new("/nonexistent/path/file.txt"));
        assert!(result.is_err());
    }

    // ==================== Incremental Hashing Tests ====================

    #[test]
    fn test_needs_rehash_no_cache() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, b"test").unwrap();

        // No cache entry -> needs rehash
        assert!(needs_rehash(&file_path, None));
    }

    #[test]
    fn test_needs_rehash_matching_cache() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, b"test content").unwrap();

        let meta = fs::metadata(&file_path).unwrap();
        let mtime_secs = meta
            .modified()
            .unwrap()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let cached = CachedFileInfo {
            mtime_secs,
            size: meta.len(),
            hash: "somehash".to_string(),
        };

        // Matching cache -> no rehash needed
        assert!(!needs_rehash(&file_path, Some(&cached)));
    }

    #[test]
    fn test_needs_rehash_size_changed() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, b"test content").unwrap();

        let meta = fs::metadata(&file_path).unwrap();
        let mtime_secs = meta
            .modified()
            .unwrap()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let cached = CachedFileInfo {
            mtime_secs,
            size: meta.len() + 100, // Different size
            hash: "somehash".to_string(),
        };

        // Size mismatch -> needs rehash
        assert!(needs_rehash(&file_path, Some(&cached)));
    }

    #[test]
    fn test_needs_rehash_mtime_changed() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, b"test content").unwrap();

        let meta = fs::metadata(&file_path).unwrap();

        let cached = CachedFileInfo {
            mtime_secs: 0, // Old timestamp
            size: meta.len(),
            hash: "somehash".to_string(),
        };

        // Mtime mismatch -> needs rehash
        assert!(needs_rehash(&file_path, Some(&cached)));
    }

    #[test]
    fn test_needs_rehash_nonexistent_file() {
        let cached = CachedFileInfo {
            mtime_secs: 12345,
            size: 100,
            hash: "somehash".to_string(),
        };

        // File doesn't exist -> needs rehash (will fail later)
        assert!(needs_rehash(Path::new("/nonexistent"), Some(&cached)));
    }

    // ==================== Bincode Cache Tests ====================

    #[test]
    fn test_cache_serialization_roundtrip() {
        let cache = StableScanCache {
            version: 3,
            dir_count: 100,
            beatmaps_parsed: 500,
            sets: vec![],
            file_hashes: HashMap::new(),
            osu_cache: HashMap::new(),
        };

        let bytes = bincode::serialize(&cache).unwrap();
        let deserialized: StableScanCache = bincode::deserialize(&bytes).unwrap();

        assert_eq!(deserialized.version, 3);
        assert_eq!(deserialized.dir_count, 100);
        assert_eq!(deserialized.beatmaps_parsed, 500);
    }

    #[test]
    fn test_cache_with_file_hashes() {
        let mut file_hashes = HashMap::new();
        file_hashes.insert(
            "song1/audio.mp3".to_string(),
            CachedFileInfo {
                mtime_secs: 1234567890,
                size: 5000000,
                hash: "abc123def456".to_string(),
            },
        );
        file_hashes.insert(
            "song2/bg.jpg".to_string(),
            CachedFileInfo {
                mtime_secs: 1234567891,
                size: 100000,
                hash: "xyz789".to_string(),
            },
        );

        let cache = StableScanCache {
            version: 3,
            dir_count: 2,
            beatmaps_parsed: 10,
            sets: vec![],
            file_hashes,
            osu_cache: HashMap::new(),
        };

        let bytes = bincode::serialize(&cache).unwrap();
        let deserialized: StableScanCache = bincode::deserialize(&bytes).unwrap();

        assert_eq!(deserialized.file_hashes.len(), 2);
        assert!(deserialized.file_hashes.contains_key("song1/audio.mp3"));
        assert_eq!(
            deserialized
                .file_hashes
                .get("song1/audio.mp3")
                .unwrap()
                .size,
            5000000
        );
    }

    #[test]
    fn test_cache_file_save_and_load() {
        let temp_dir = TempDir::new().unwrap();
        let songs_path = temp_dir.path().join("Songs");
        fs::create_dir(&songs_path).unwrap();

        let scanner = StableScanner::new(songs_path.clone());
        let cache_path = scanner.cache_path();

        // Verify cache path uses .bin extension
        assert!(cache_path.to_string_lossy().ends_with(".bin"));

        // Save cache
        let mut file_hashes = HashMap::new();
        file_hashes.insert(
            "test".to_string(),
            CachedFileInfo {
                mtime_secs: 123,
                size: 456,
                hash: "testhash".to_string(),
            },
        );
        scanner.save_to_cache(&[], 5, 10, file_hashes, HashMap::new());

        // Verify file exists
        assert!(cache_path.exists());

        // Load cache
        let loaded = scanner.load_from_cache(5);
        assert!(loaded.is_some());

        let (sets, beatmaps_parsed, hashes, _osu_cache) = loaded.unwrap();
        assert!(sets.is_empty());
        assert_eq!(beatmaps_parsed, 10);
        assert_eq!(hashes.len(), 1);
    }

    #[test]
    fn test_cache_invalidation_on_dir_count_change() {
        let temp_dir = TempDir::new().unwrap();
        let songs_path = temp_dir.path().join("Songs");
        fs::create_dir(&songs_path).unwrap();

        let scanner = StableScanner::new(songs_path);

        // Save with dir_count = 5
        scanner.save_to_cache(&[], 5, 10, HashMap::new(), HashMap::new());

        // Load with different dir_count - should still return the osu_cache for incremental parsing
        let loaded = scanner.load_from_cache(10);
        assert!(loaded.is_some());
        // But sets should be empty (needs rescan)
        let (sets, _, _, _) = loaded.unwrap();
        assert!(sets.is_empty());
    }

    // ==================== Scanner Integration Tests ====================

    #[test]
    fn test_scanner_skip_hashing() {
        let temp_dir = TempDir::new().unwrap();
        let songs_path = temp_dir.path().join("Songs");
        fs::create_dir(&songs_path).unwrap();

        let scanner = StableScanner::new(songs_path).skip_hashing();
        assert!(scanner.skip_hashing);
    }

    #[test]
    fn test_scan_timing_report_cached() {
        let timing = ScanTiming {
            from_cache: true,
            total: Duration::from_secs(1),
            dirs_scanned: 100,
            osu_files_parsed: 500,
            ..Default::default()
        };

        let report = timing.report();
        assert!(report.contains("cached"));
        assert!(report.contains("100 dirs"));
        assert!(report.contains("500 beatmaps"));
    }

    #[test]
    fn test_scan_timing_report_fresh() {
        let timing = ScanTiming {
            from_cache: false,
            total: Duration::from_secs(10),
            dir_enumeration: Duration::from_millis(100),
            osu_parsing: Duration::from_secs(2),
            file_hashing: Duration::from_secs(5),
            dirs_scanned: 1000,
            osu_files_parsed: 5000,
            files_hashed: 50000,
            bytes_hashed: 10 * 1024 * 1024 * 1024, // 10GB
            parallel: true,
            thread_count: 8,
        };

        let report = timing.report();
        assert!(report.contains("Blake3")); // Should mention Blake3, not SHA-256
        assert!(report.contains("parallel"));
        assert!(report.contains("8 threads"));
    }

    #[test]
    fn test_cached_file_info_serialization() {
        let info = CachedFileInfo {
            mtime_secs: 1703836800, // 2023-12-29
            size: 1024 * 1024,      // 1MB
            hash: "abcdef1234567890".to_string(),
        };

        let bytes = bincode::serialize(&info).unwrap();
        let deserialized: CachedFileInfo = bincode::deserialize(&bytes).unwrap();

        assert_eq!(deserialized.mtime_secs, info.mtime_secs);
        assert_eq!(deserialized.size, info.size);
        assert_eq!(deserialized.hash, info.hash);
    }
}
