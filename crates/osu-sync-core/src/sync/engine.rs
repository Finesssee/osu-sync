//! Main synchronization engine

use rayon::prelude::*;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::beatmap::BeatmapSet;
use crate::config::Config;
use crate::dedup::{DuplicateAction, DuplicateDetector, DuplicateIndex, DuplicateStrategy};
use crate::error::{Error, Result};
use crate::filter::{FilterCriteria, FilterEngine};
use crate::lazer::{LazerDatabase, LazerImporter};
use crate::stable::{StableImporter, StableScanner};
use crate::sync::conflict::ConflictResolver;
use crate::sync::direction::SyncDirection;
use crate::sync::dry_run::{DryRunAction, DryRunItem, DryRunResult};

/// Result of a sync operation
#[derive(Debug, Clone, Default)]
pub struct SyncResult {
    /// Number of beatmaps successfully imported
    pub imported: usize,
    /// Number of beatmaps skipped (duplicates or user choice)
    pub skipped: usize,
    /// Number of beatmaps that failed to import
    pub failed: usize,
    /// Errors encountered during sync
    pub errors: Vec<SyncError>,
    /// Direction of the sync
    pub direction: SyncDirection,
}

impl SyncResult {
    /// Create a new empty sync result
    pub fn new(direction: SyncDirection) -> Self {
        Self {
            direction,
            ..Default::default()
        }
    }

    /// Total number of beatmaps processed
    pub fn total(&self) -> usize {
        self.imported + self.skipped + self.failed
    }

    /// Check if the sync completed without errors
    pub fn is_success(&self) -> bool {
        self.errors.is_empty() && self.failed == 0
    }

    /// Merge another result into this one
    pub fn merge(&mut self, other: SyncResult) {
        self.imported += other.imported;
        self.skipped += other.skipped;
        self.failed += other.failed;
        self.errors.extend(other.errors);
    }
}

/// A single sync error
#[derive(Debug, Clone)]
pub struct SyncError {
    /// The beatmap set that failed
    pub beatmap_set: Option<String>,
    /// Error message
    pub message: String,
}

impl SyncError {
    /// Create a new sync error
    pub fn new(beatmap_set: Option<String>, message: impl Into<String>) -> Self {
        Self {
            beatmap_set,
            message: message.into(),
        }
    }
}

/// Progress information for sync callbacks
#[derive(Debug, Clone, Default)]
pub struct SyncProgress {
    /// Current item being processed
    pub current: usize,
    /// Total items to process
    pub total: usize,
    /// Name of the current beatmap set
    pub current_name: String,
    /// Current phase of sync
    pub phase: SyncPhase,
    /// Items processed per second
    pub items_per_second: f32,
    /// Elapsed time in seconds
    pub elapsed_seconds: u64,
    /// Estimated remaining time in seconds
    pub estimated_remaining_seconds: Option<u64>,
}

/// Phase of the sync operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SyncPhase {
    /// Scanning source beatmaps
    #[default]
    Scanning,
    /// Checking for duplicates
    Deduplicating,
    /// Importing beatmaps
    Importing,
    /// Sync complete
    Complete,
}

impl std::fmt::Display for SyncPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Scanning => write!(f, "Scanning"),
            Self::Deduplicating => write!(f, "Checking duplicates"),
            Self::Importing => write!(f, "Importing"),
            Self::Complete => write!(f, "Complete"),
        }
    }
}

/// Progress callback type
pub type ProgressCallback = Box<dyn Fn(SyncProgress) + Send + Sync>;

/// Main synchronization engine
pub struct SyncEngine {
    config: Config,
    stable_scanner: StableScanner,
    lazer_database: LazerDatabase,
    duplicate_detector: DuplicateDetector,
    duplicate_strategy: DuplicateStrategy,
    progress_callback: Option<ProgressCallback>,
    filter: Option<FilterCriteria>,
    /// Optional set of beatmap set IDs to sync (for user selection)
    selected_set_ids: Option<HashSet<i32>>,
    /// Optional set of folder names to sync (fallback for sets without IDs)
    selected_folders: Option<HashSet<String>>,
    /// Optional cancellation token for aborting sync
    cancellation: Option<Arc<AtomicBool>>,
}

impl SyncEngine {
    /// Create a new sync engine
    pub fn new(
        config: Config,
        stable_scanner: StableScanner,
        lazer_database: LazerDatabase,
    ) -> Self {
        let strategy = DuplicateStrategy::default();
        let duplicate_detector = DuplicateDetector::new(strategy);

        Self {
            config,
            stable_scanner,
            lazer_database,
            duplicate_detector,
            duplicate_strategy: strategy,
            progress_callback: None,
            filter: None,
            selected_set_ids: None,
            selected_folders: None,
            cancellation: None,
        }
    }

    /// Set the duplicate detection strategy
    pub fn with_duplicate_strategy(mut self, strategy: DuplicateStrategy) -> Self {
        self.duplicate_detector = DuplicateDetector::new(strategy);
        self.duplicate_strategy = strategy;
        self
    }

    /// Get the current duplicate detection strategy
    fn duplicate_detector_strategy(&self) -> DuplicateStrategy {
        self.duplicate_strategy
    }

    /// Set the progress callback
    pub fn with_progress_callback(mut self, callback: ProgressCallback) -> Self {
        self.progress_callback = Some(callback);
        self
    }

    /// Set the filter criteria for syncing
    pub fn with_filter(mut self, filter: FilterCriteria) -> Self {
        if filter.is_empty() {
            self.filter = None;
        } else {
            self.filter = Some(filter);
        }
        self
    }

    /// Get the current filter criteria
    pub fn filter(&self) -> Option<&FilterCriteria> {
        self.filter.as_ref()
    }

    /// Clear the filter criteria
    pub fn clear_filter(&mut self) {
        self.filter = None;
    }

    /// Set selected beatmap set IDs for user selection
    pub fn with_selected_set_ids(mut self, set_ids: HashSet<i32>) -> Self {
        if set_ids.is_empty() {
            self.selected_set_ids = None;
        } else {
            self.selected_set_ids = Some(set_ids);
        }
        self
    }

    /// Set selected folder names for user selection (fallback when set_id unavailable)
    pub fn with_selected_folders(mut self, folders: HashSet<String>) -> Self {
        if folders.is_empty() {
            self.selected_folders = None;
        } else {
            self.selected_folders = Some(folders);
        }
        self
    }

    /// Set a cancellation token for aborting sync operations
    pub fn with_cancellation(mut self, token: Arc<AtomicBool>) -> Self {
        self.cancellation = Some(token);
        self
    }

    /// Check if cancellation has been requested
    fn is_cancelled(&self) -> bool {
        self.cancellation
            .as_ref()
            .map(|c| c.load(Ordering::SeqCst))
            .unwrap_or(false)
    }

    /// Apply filter to stable beatmap sets, returning indices of matching sets
    fn filter_stable_sets(&self, sets: &[BeatmapSet]) -> Vec<usize> {
        let mut indices: Vec<usize> = if let Some(ref filter) = self.filter {
            sets.iter()
                .enumerate()
                .filter(|(_, set)| FilterEngine::matches_stable(set, filter))
                .map(|(i, _)| i)
                .collect()
        } else {
            // No filter, include all
            (0..sets.len()).collect()
        };

        // Apply user selection filter if set (by ID or folder name)
        let has_id_filter = self.selected_set_ids.is_some();
        let has_folder_filter = self.selected_folders.is_some();

        if has_id_filter || has_folder_filter {
            indices.retain(|&i| {
                let set = &sets[i];

                // Check by set_id first
                if let Some(ref selected_ids) = self.selected_set_ids {
                    if let Some(id) = set.id {
                        if selected_ids.contains(&id) {
                            return true;
                        }
                    }
                }

                // Fallback to folder_name
                if let Some(ref selected_folders) = self.selected_folders {
                    if let Some(ref folder) = set.folder_name {
                        if selected_folders.contains(folder) {
                            return true;
                        }
                    }
                }

                false
            });
        }

        indices
    }

    /// Report progress to the callback if set
    fn report_progress(&self, progress: SyncProgress) {
        if let Some(ref callback) = self.progress_callback {
            callback(progress);
        }
    }

    /// Perform a dry run to preview what would happen during sync
    ///
    /// This analyzes the source and target installations and determines
    /// what action would be taken for each beatmap set without making
    /// any actual changes.
    pub fn dry_run(&self, direction: SyncDirection) -> Result<DryRunResult> {
        tracing::info!("Starting dry run: {}", direction);

        let mut result = DryRunResult::new();

        match direction {
            SyncDirection::StableToLazer => {
                self.dry_run_stable_to_lazer(&mut result)?;
            }
            SyncDirection::LazerToStable => {
                self.dry_run_lazer_to_stable(&mut result)?;
            }
            SyncDirection::Bidirectional => {
                self.dry_run_stable_to_lazer(&mut result)?;
                self.dry_run_lazer_to_stable(&mut result)?;
            }
        }

        tracing::info!(
            "Dry run complete: {} to import, {} to skip, {} duplicates",
            result.total_import,
            result.total_skip,
            result.total_duplicate
        );

        Ok(result)
    }

    /// Dry run for stable to lazer sync
    fn dry_run_stable_to_lazer(&self, result: &mut DryRunResult) -> Result<()> {
        self.report_progress(SyncProgress {
            current: 0,
            total: 0,
            current_name: "Analyzing osu!stable beatmaps...".to_string(),
            phase: SyncPhase::Scanning,
            ..Default::default()
        });

        // Scan stable beatmaps (uses parallel scanning with caching)
        let stable_sets = self.stable_scanner.scan_parallel()?;

        // Apply filter to get matching sets
        let filtered_indices = self.filter_stable_sets(&stable_sets);
        let total = filtered_indices.len();

        if self.filter.is_some() {
            tracing::info!(
                "Filter applied: {} of {} beatmap sets match",
                total,
                stable_sets.len()
            );
        }

        self.report_progress(SyncProgress {
            current: 0,
            total,
            current_name: "Building duplicate index...".to_string(),
            phase: SyncPhase::Deduplicating,
            ..Default::default()
        });

        // Get lazer beatmaps and build fast lookup index (O(n) once)
        let lazer_sets = self.lazer_database.get_all_beatmap_sets()?;
        let lazer_beatmap_sets: Vec<BeatmapSet> = lazer_sets
            .iter()
            .map(|ls| self.lazer_database.to_beatmap_set(ls))
            .collect();

        // Build O(1) lookup index
        let dup_index = DuplicateIndex::build(&lazer_beatmap_sets);
        let strategy = self.duplicate_detector_strategy();

        // Process in parallel with rayon
        let progress_counter = AtomicUsize::new(0);
        let start_time = Instant::now();
        // Store last report time as millis since start (atomic u64)
        let last_report_millis = AtomicU64::new(0);
        let results_mutex = Mutex::new(Vec::with_capacity(total));

        // Collect filtered sets for parallel processing
        let filtered_sets: Vec<_> = filtered_indices
            .iter()
            .map(|&idx| &stable_sets[idx])
            .collect();

        // Process in parallel
        filtered_sets.par_iter().for_each(|stable_set| {
            // Check for cancellation early
            if self.is_cancelled() {
                return;
            }

            // Fast O(1) duplicate check using index
            let action = if dup_index.is_duplicate(stable_set, strategy) {
                DryRunAction::Duplicate
            } else if stable_set.id.map_or(false, |id| dup_index.exists_by_id(id)) {
                DryRunAction::Skip
            } else {
                DryRunAction::Import
            };

            // Calculate size
            let size_bytes = self.calculate_stable_set_size(stable_set);

            let item = DryRunItem {
                set_id: stable_set.id,
                folder_name: stable_set.folder_name.clone(),
                title: stable_set
                    .metadata()
                    .map(|m| m.title.clone())
                    .unwrap_or_else(|| "Unknown".to_string()),
                artist: stable_set
                    .metadata()
                    .map(|m| m.artist.clone())
                    .unwrap_or_else(|| "Unknown".to_string()),
                action,
                size_bytes,
                difficulty_count: stable_set.beatmaps.len(),
            };

            // Add to results
            results_mutex.lock().unwrap().push(item);

            // Update progress periodically (time-based: every 50ms to reduce lock contention)
            let current = progress_counter.fetch_add(1, Ordering::Relaxed) + 1;
            let elapsed_millis = start_time.elapsed().as_millis() as u64;
            let last = last_report_millis.load(Ordering::Relaxed);
            
            // Report every 50ms or at completion
            if elapsed_millis >= last + 50 || current == total {
                last_report_millis.store(elapsed_millis, Ordering::Relaxed);
                let elapsed_secs = start_time.elapsed().as_secs();
                let items_per_sec = if elapsed_secs > 0 {
                    current as f32 / elapsed_secs as f32
                } else {
                    0.0
                };
                let estimated_remaining = if items_per_sec > 0.0 && current < total {
                    Some(((total - current) as f32 / items_per_sec) as u64)
                } else {
                    None
                };
                
                self.report_progress(SyncProgress {
                    current,
                    total,
                    current_name: stable_set
                        .folder_name
                        .clone()
                        .unwrap_or_else(|| stable_set.generate_folder_name()),
                    phase: SyncPhase::Deduplicating,
                    items_per_second: items_per_sec,
                    elapsed_seconds: elapsed_secs,
                    estimated_remaining_seconds: estimated_remaining,
                });
            }
        });

        // Add all items to result
        let items = results_mutex.into_inner().unwrap();
        for item in items {
            result.add_item(item);
        }

        Ok(())
    }

    /// Dry run for lazer to stable sync
    fn dry_run_lazer_to_stable(&self, result: &mut DryRunResult) -> Result<()> {
        self.report_progress(SyncProgress {
            current: 0,
            total: 0,
            current_name: "Analyzing osu!lazer beatmaps...".to_string(),
            phase: SyncPhase::Scanning,
            ..Default::default()
        });

        // Get lazer beatmaps
        let lazer_sets = self.lazer_database.get_all_beatmap_sets()?;
        let total = lazer_sets.len();

        // Scan stable for duplicate detection (uses parallel scanning with caching)
        let stable_sets = self.stable_scanner.scan_parallel()?;
        let stable_index = crate::stable::BeatmapIndex::new(stable_sets);

        // Analyze each lazer set
        for (idx, lazer_set) in lazer_sets.iter().enumerate() {
            // Check for cancellation
            if self.is_cancelled() {
                tracing::info!("Dry run cancelled by user at item {}/{}", idx, total);
                break;
            }

            let beatmap_set = self.lazer_database.to_beatmap_set(lazer_set);

            self.report_progress(SyncProgress {
                current: idx + 1,
                total,
                current_name: beatmap_set.generate_folder_name(),
                phase: SyncPhase::Deduplicating,
                ..Default::default()
            });

            // Check for duplicates
            let action = if let Some(_duplicate) = self
                .duplicate_detector
                .find_duplicate(&beatmap_set, &stable_index.sets)
            {
                DryRunAction::Duplicate
            } else {
                // Check if it already exists in stable by ID
                let exists = beatmap_set.id.map_or(false, |id| {
                    stable_index.sets.iter().any(|s| s.id == Some(id))
                });

                if exists {
                    DryRunAction::Skip
                } else {
                    DryRunAction::Import
                }
            };

            let item = DryRunItem::from_lazer_set(lazer_set, action);
            result.add_item(item);
        }

        Ok(())
    }

    /// Calculate the total size of files in a stable beatmap set folder
    fn calculate_stable_set_size(&self, beatmap_set: &BeatmapSet) -> u64 {
        let folder_name = match &beatmap_set.folder_name {
            Some(name) => name,
            None => return 0,
        };

        let songs_path = match self.config.stable_songs_path() {
            Some(path) => path,
            None => return 0,
        };

        let folder_path = songs_path.join(folder_name);

        if !folder_path.exists() {
            return 0;
        }

        std::fs::read_dir(&folder_path)
            .ok()
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .filter(|e| e.path().is_file())
                    .filter_map(|e| e.metadata().ok())
                    .map(|m| m.len())
                    .sum()
            })
            .unwrap_or(0)
    }

    /// Perform synchronization in the specified direction
    pub fn sync(
        &self,
        direction: SyncDirection,
        resolver: &dyn ConflictResolver,
    ) -> Result<SyncResult> {
        tracing::info!("Starting sync: {}", direction);

        let mut result = SyncResult::new(direction);

        match direction {
            SyncDirection::StableToLazer => {
                result.merge(self.sync_stable_to_lazer(resolver)?);
            }
            SyncDirection::LazerToStable => {
                result.merge(self.sync_lazer_to_stable(resolver)?);
            }
            SyncDirection::Bidirectional => {
                result.merge(self.sync_stable_to_lazer(resolver)?);
                result.merge(self.sync_lazer_to_stable(resolver)?);
            }
        }

        self.report_progress(SyncProgress {
            current: result.total(),
            total: result.total(),
            current_name: String::new(),
            phase: SyncPhase::Complete,
            ..Default::default()
        });

        tracing::info!(
            "Sync complete: {} imported, {} skipped, {} failed",
            result.imported,
            result.skipped,
            result.failed
        );

        Ok(result)
    }

    /// Sync beatmaps from osu!stable to osu!lazer
    fn sync_stable_to_lazer(&self, resolver: &dyn ConflictResolver) -> Result<SyncResult> {
        let mut result = SyncResult::new(SyncDirection::StableToLazer);

        // Phase 1: Scan stable beatmaps
        self.report_progress(SyncProgress {
            current: 0,
            total: 0,
            current_name: "Scanning osu!stable...".to_string(),
            phase: SyncPhase::Scanning,
            ..Default::default()
        });

        let stable_sets = self.stable_scanner.scan_parallel()?;

        // Apply filter to get matching sets
        let filtered_indices = self.filter_stable_sets(&stable_sets);
        let total = filtered_indices.len();

        if let Some(ref filter) = self.filter {
            tracing::info!(
                "Filter applied: {} of {} beatmap sets match ({})",
                total,
                stable_sets.len(),
                filter.summary()
            );
        } else {
            tracing::info!("Found {} beatmap sets in osu!stable", total);
        }

        // Phase 2: Get lazer beatmaps for duplicate detection
        self.report_progress(SyncProgress {
            current: 0,
            total,
            current_name: "Loading osu!lazer database...".to_string(),
            phase: SyncPhase::Deduplicating,
            ..Default::default()
        });

        let lazer_sets = self.lazer_database.get_all_beatmap_sets()?;
        let lazer_beatmap_sets: Vec<BeatmapSet> = lazer_sets
            .iter()
            .map(|ls| self.lazer_database.to_beatmap_set(ls))
            .collect();

        // Phase 3: Import to lazer
        // Use batch mode - create all .osz files first, then trigger lazer once at the end
        let mut lazer_importer = LazerImporter::new(
            self.config
                .lazer_path
                .as_ref()
                .ok_or_else(|| Error::Config("Lazer path not configured".to_string()))?,
        )
        .batch_mode(); // Don't launch lazer for each beatmap

        for (progress_idx, set_idx) in filtered_indices.iter().enumerate() {
            // Check for cancellation
            if self.is_cancelled() {
                tracing::info!("Sync cancelled by user at item {}/{}", progress_idx, total);
                break;
            }

            let stable_set = &stable_sets[*set_idx];
            let set_name = stable_set
                .folder_name
                .clone()
                .unwrap_or_else(|| stable_set.generate_folder_name());

            self.report_progress(SyncProgress {
                current: progress_idx + 1,
                total,
                current_name: set_name.clone(),
                phase: SyncPhase::Importing,
                ..Default::default()
            });

            // Check for duplicates
            if let Some(duplicate) = self
                .duplicate_detector
                .find_duplicate(stable_set, &lazer_beatmap_sets)
            {
                let resolution = resolver.resolve(&duplicate);

                match resolution.action {
                    DuplicateAction::Skip => {
                        tracing::debug!("Skipping duplicate: {}", set_name);
                        result.skipped += 1;
                        continue;
                    }
                    DuplicateAction::Replace => {
                        tracing::debug!("Replacing duplicate: {}", set_name);
                        // For lazer, we just import and let lazer handle the replacement
                    }
                    DuplicateAction::KeepBoth => {
                        tracing::debug!("Keeping both versions: {}", set_name);
                    }
                }
            }

            // Collect files from the stable folder
            let files = self.collect_stable_files(stable_set)?;

            // Import to lazer
            match lazer_importer.import_beatmap_set(stable_set, &files) {
                Ok(_) => {
                    result.imported += 1;
                }
                Err(e) => {
                    tracing::error!("Failed to import {}: {}", set_name, e);
                    result.failed += 1;
                    result
                        .errors
                        .push(SyncError::new(Some(set_name), e.to_string()));
                }
            }
        }

        // Trigger lazer to process all pending imports
        if result.imported > 0 {
            match lazer_importer.trigger_batch_import() {
                Ok(true) => {
                    tracing::info!("Lazer launched to process {} imports", result.imported);
                }
                Ok(false) => {
                    tracing::info!(
                        "Lazer not found. {} beatmaps placed in import folder for manual import.",
                        result.imported
                    );
                }
                Err(e) => {
                    tracing::warn!("Failed to trigger lazer import: {}", e);
                }
            }
        }

        Ok(result)
    }

    /// Sync beatmaps from osu!lazer to osu!stable
    fn sync_lazer_to_stable(&self, resolver: &dyn ConflictResolver) -> Result<SyncResult> {
        let mut result = SyncResult::new(SyncDirection::LazerToStable);

        // Phase 1: Get lazer beatmaps
        self.report_progress(SyncProgress {
            current: 0,
            total: 0,
            current_name: "Loading osu!lazer database...".to_string(),
            phase: SyncPhase::Scanning,
            ..Default::default()
        });

        let lazer_sets = self.lazer_database.get_all_beatmap_sets()?;
        let total = lazer_sets.len();

        tracing::info!("Found {} beatmap sets in osu!lazer", total);

        // Phase 2: Scan stable for duplicate detection
        self.report_progress(SyncProgress {
            current: 0,
            total,
            current_name: "Scanning osu!stable...".to_string(),
            phase: SyncPhase::Deduplicating,
            ..Default::default()
        });

        let stable_sets = self.stable_scanner.scan_parallel()?;
        let stable_index = crate::stable::BeatmapIndex::new(stable_sets);

        // Phase 3: Import to stable
        let stable_importer = StableImporter::new(
            self.config
                .stable_songs_path()
                .ok_or_else(|| Error::Config("Stable path not configured".to_string()))?,
        );

        for (idx, lazer_set) in lazer_sets.iter().enumerate() {
            // Check for cancellation
            if self.is_cancelled() {
                tracing::info!("Sync cancelled by user at item {}/{}", idx, total);
                break;
            }

            let beatmap_set = self.lazer_database.to_beatmap_set(lazer_set);
            let set_name = beatmap_set.generate_folder_name();

            self.report_progress(SyncProgress {
                current: idx + 1,
                total,
                current_name: set_name.clone(),
                phase: SyncPhase::Importing,
                ..Default::default()
            });

            // Check for duplicates
            if let Some(duplicate) = self
                .duplicate_detector
                .find_duplicate(&beatmap_set, &stable_index.sets)
            {
                let resolution = resolver.resolve(&duplicate);

                match resolution.action {
                    DuplicateAction::Skip => {
                        tracing::debug!("Skipping duplicate: {}", set_name);
                        result.skipped += 1;
                        continue;
                    }
                    DuplicateAction::Replace => {
                        tracing::debug!("Replacing duplicate: {}", set_name);
                        // Would need to delete existing folder first
                    }
                    DuplicateAction::KeepBoth => {
                        tracing::debug!("Keeping both versions: {}", set_name);
                    }
                }
            }

            // Collect files from lazer file store
            let files = self.collect_lazer_files(lazer_set)?;

            // Import to stable
            match stable_importer.import_files(&files, &beatmap_set) {
                Ok(import_result) => {
                    if import_result.success {
                        result.imported += 1;
                    } else {
                        result.skipped += 1;
                        if let Some(error) = import_result.error {
                            tracing::debug!("Skipped {}: {}", set_name, error);
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to import {}: {}", set_name, e);
                    result.failed += 1;
                    result
                        .errors
                        .push(SyncError::new(Some(set_name), e.to_string()));
                }
            }
        }

        Ok(result)
    }

    /// Collect files from a stable beatmap folder (parallel I/O for 2-3x speedup)
    fn collect_stable_files(&self, beatmap_set: &BeatmapSet) -> Result<Vec<(String, Vec<u8>)>> {
        let folder_name = beatmap_set
            .folder_name
            .as_ref()
            .ok_or_else(|| Error::Other("Beatmap set has no folder name".to_string()))?;

        let songs_path = self
            .config
            .stable_songs_path()
            .ok_or_else(|| Error::Config("Stable path not configured".to_string()))?;

        let folder_path = songs_path.join(folder_name);
        
        // Collect entries first
        let entries: Vec<_> = std::fs::read_dir(&folder_path)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .collect();

        // Read files in parallel using rayon (2-3x speedup for large beatmap sets)
        let files: Vec<_> = entries
            .par_iter()
            .filter_map(|entry| {
                let path = entry.path();
                let filename = path.file_name()?.to_string_lossy().to_string();
                let content = std::fs::read(&path).ok()?;
                Some((filename, content))
            })
            .collect();

        Ok(files)
    }

    /// Collect files from the lazer file store (parallel I/O)
    fn collect_lazer_files(
        &self,
        lazer_set: &crate::lazer::LazerBeatmapSet,
    ) -> Result<Vec<(String, Vec<u8>)>> {
        let file_store = self.lazer_database.file_store();
        
        // Read files in parallel using rayon
        let files: Vec<_> = lazer_set.files
            .par_iter()
            .filter_map(|named_file| {
                match file_store.read(&named_file.hash) {
                    Ok(content) => Some((named_file.filename.clone(), content)),
                    Err(e) => {
                        tracing::warn!(
                            "Failed to read file {} ({}): {}",
                            named_file.filename,
                            named_file.hash,
                            e
                        );
                        None
                    }
                }
            })
            .collect();

        Ok(files)
    }
}

/// Builder for creating a SyncEngine with options
pub struct SyncEngineBuilder {
    config: Option<Config>,
    stable_scanner: Option<StableScanner>,
    lazer_database: Option<LazerDatabase>,
    duplicate_strategy: DuplicateStrategy,
    progress_callback: Option<ProgressCallback>,
    selected_set_ids: Option<HashSet<i32>>,
    selected_folders: Option<HashSet<String>>,
    cancellation: Option<Arc<AtomicBool>>,
}

impl SyncEngineBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            config: None,
            stable_scanner: None,
            lazer_database: None,
            duplicate_strategy: DuplicateStrategy::default(),
            progress_callback: None,
            selected_set_ids: None,
            selected_folders: None,
            cancellation: None,
        }
    }

    /// Set the configuration
    pub fn config(mut self, config: Config) -> Self {
        self.config = Some(config);
        self
    }

    /// Set the stable scanner
    pub fn stable_scanner(mut self, scanner: StableScanner) -> Self {
        self.stable_scanner = Some(scanner);
        self
    }

    /// Set the lazer database
    pub fn lazer_database(mut self, database: LazerDatabase) -> Self {
        self.lazer_database = Some(database);
        self
    }

    /// Set the duplicate detection strategy
    pub fn duplicate_strategy(mut self, strategy: DuplicateStrategy) -> Self {
        self.duplicate_strategy = strategy;
        self
    }

    /// Set the progress callback
    pub fn progress_callback(mut self, callback: ProgressCallback) -> Self {
        self.progress_callback = Some(callback);
        self
    }

    /// Set selected beatmap set IDs for user selection
    pub fn selected_set_ids(mut self, set_ids: HashSet<i32>) -> Self {
        if set_ids.is_empty() {
            self.selected_set_ids = None;
        } else {
            self.selected_set_ids = Some(set_ids);
        }
        self
    }

    /// Set selected folder names for user selection (fallback when set_id unavailable)
    pub fn selected_folders(mut self, folders: HashSet<String>) -> Self {
        if folders.is_empty() {
            self.selected_folders = None;
        } else {
            self.selected_folders = Some(folders);
        }
        self
    }

    /// Set a cancellation token for aborting sync operations
    pub fn cancellation(mut self, token: Arc<AtomicBool>) -> Self {
        self.cancellation = Some(token);
        self
    }

    /// Build the sync engine
    pub fn build(self) -> Result<SyncEngine> {
        let config = self
            .config
            .ok_or_else(|| Error::Config("Config is required".to_string()))?;

        let stable_scanner = self
            .stable_scanner
            .ok_or_else(|| Error::Config("StableScanner is required".to_string()))?;

        let lazer_database = self
            .lazer_database
            .ok_or_else(|| Error::Config("LazerDatabase is required".to_string()))?;

        let mut engine = SyncEngine::new(config, stable_scanner, lazer_database)
            .with_duplicate_strategy(self.duplicate_strategy);

        if let Some(callback) = self.progress_callback {
            engine = engine.with_progress_callback(callback);
        }

        if let Some(set_ids) = self.selected_set_ids {
            engine = engine.with_selected_set_ids(set_ids);
        }

        if let Some(folders) = self.selected_folders {
            engine = engine.with_selected_folders(folders);
        }

        if let Some(token) = self.cancellation {
            engine = engine.with_cancellation(token);
        }

        Ok(engine)
    }
}

impl Default for SyncEngineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_result() {
        let mut result = SyncResult::new(SyncDirection::StableToLazer);
        result.imported = 5;
        result.skipped = 2;
        result.failed = 1;

        assert_eq!(result.total(), 8);
        assert!(!result.is_success()); // has failed

        result.failed = 0;
        assert!(result.is_success());
    }

    #[test]
    fn test_sync_result_merge() {
        let mut result1 = SyncResult::new(SyncDirection::StableToLazer);
        result1.imported = 5;
        result1.skipped = 2;

        let mut result2 = SyncResult::new(SyncDirection::LazerToStable);
        result2.imported = 3;
        result2.failed = 1;

        result1.merge(result2);

        assert_eq!(result1.imported, 8);
        assert_eq!(result1.skipped, 2);
        assert_eq!(result1.failed, 1);
    }

    // ==================== SyncProgress Tests ====================

    #[test]
    fn test_sync_progress_default() {
        let progress = SyncProgress::default();
        assert_eq!(progress.current, 0);
        assert_eq!(progress.total, 0);
        assert!(progress.current_name.is_empty());
        assert_eq!(progress.phase, SyncPhase::Scanning);
        assert_eq!(progress.items_per_second, 0.0);
        assert_eq!(progress.elapsed_seconds, 0);
        assert!(progress.estimated_remaining_seconds.is_none());
    }

    #[test]
    fn test_sync_progress_with_values() {
        let progress = SyncProgress {
            current: 50,
            total: 100,
            current_name: "Test Beatmap".to_string(),
            phase: SyncPhase::Importing,
            items_per_second: 25.0,
            elapsed_seconds: 2,
            estimated_remaining_seconds: Some(2),
        };

        assert_eq!(progress.current, 50);
        assert_eq!(progress.total, 100);
        assert_eq!(progress.current_name, "Test Beatmap");
        assert_eq!(progress.phase, SyncPhase::Importing);
        assert_eq!(progress.items_per_second, 25.0);
        assert_eq!(progress.elapsed_seconds, 2);
        assert_eq!(progress.estimated_remaining_seconds, Some(2));
    }

    // ==================== SyncPhase Tests ====================

    #[test]
    fn test_sync_phase_display() {
        assert_eq!(format!("{}", SyncPhase::Scanning), "Scanning");
        assert_eq!(format!("{}", SyncPhase::Deduplicating), "Checking duplicates");
        assert_eq!(format!("{}", SyncPhase::Importing), "Importing");
        assert_eq!(format!("{}", SyncPhase::Complete), "Complete");
    }

    #[test]
    fn test_sync_phase_equality() {
        assert_eq!(SyncPhase::Scanning, SyncPhase::Scanning);
        assert_ne!(SyncPhase::Scanning, SyncPhase::Importing);
    }

    #[test]
    fn test_sync_phase_default() {
        assert_eq!(SyncPhase::default(), SyncPhase::Scanning);
    }

    // ==================== SyncError Tests ====================

    #[test]
    fn test_sync_error_new_with_beatmap() {
        let error = SyncError::new(Some("Artist - Title".to_string()), "Failed to import");
        assert_eq!(error.beatmap_set, Some("Artist - Title".to_string()));
        assert_eq!(error.message, "Failed to import");
    }

    #[test]
    fn test_sync_error_new_without_beatmap() {
        let error = SyncError::new(None, "Generic error");
        assert!(error.beatmap_set.is_none());
        assert_eq!(error.message, "Generic error");
    }

    // ==================== SyncResult Extended Tests ====================

    #[test]
    fn test_sync_result_with_errors() {
        let mut result = SyncResult::new(SyncDirection::StableToLazer);
        result.imported = 5;
        result.errors.push(SyncError::new(Some("Failed Map".to_string()), "IO error"));

        assert!(!result.is_success()); // has errors
        assert_eq!(result.errors.len(), 1);
    }

    #[test]
    fn test_sync_result_merge_preserves_errors() {
        let mut result1 = SyncResult::new(SyncDirection::StableToLazer);
        result1.errors.push(SyncError::new(None, "Error 1"));

        let mut result2 = SyncResult::new(SyncDirection::LazerToStable);
        result2.errors.push(SyncError::new(None, "Error 2"));
        result2.errors.push(SyncError::new(None, "Error 3"));

        result1.merge(result2);

        assert_eq!(result1.errors.len(), 3);
    }

    #[test]
    fn test_sync_result_empty_is_success() {
        let result = SyncResult::new(SyncDirection::Bidirectional);
        assert!(result.is_success());
        assert_eq!(result.total(), 0);
    }

    // ==================== Time-Based Progress Tests ====================

    #[test]
    fn test_atomic_time_tracking() {
        // Test the atomic time tracking pattern used in sync engine
        let last_report = AtomicU64::new(0);
        let now_ms = 50u64; // 50ms elapsed

        // First check - should report (0ms since last)
        let last = last_report.load(Ordering::Relaxed);
        assert!(now_ms - last >= 50); // 50ms threshold

        // Update last report time
        last_report.store(now_ms, Ordering::Relaxed);

        // Second check - should not report (0ms since last update)
        let last = last_report.load(Ordering::Relaxed);
        assert!(now_ms - last < 50); // Not enough time passed

        // Third check after another 50ms - should report
        let now_ms = 100u64;
        let last = last_report.load(Ordering::Relaxed);
        assert!(now_ms - last >= 50); // 50ms threshold met
    }

    #[test]
    fn test_estimated_remaining_calculation() {
        // Test the ETA calculation logic
        let current = 50usize;
        let total = 100usize;
        let elapsed_seconds = 10u64;

        // Calculate items per second
        let items_per_second = if elapsed_seconds > 0 {
            current as f32 / elapsed_seconds as f32
        } else {
            0.0
        };
        assert_eq!(items_per_second, 5.0); // 50 items in 10 seconds = 5/sec

        // Calculate estimated remaining
        let remaining = total - current;
        let estimated_remaining = if items_per_second > 0.0 {
            Some((remaining as f32 / items_per_second) as u64)
        } else {
            None
        };
        assert_eq!(estimated_remaining, Some(10)); // 50 remaining at 5/sec = 10 seconds
    }

    // ==================== Parallel Processing Tests ====================

    #[test]
    fn test_rayon_parallel_iteration() {
        // Verify rayon parallel iteration works as expected
        let items: Vec<i32> = (0..100).collect();
        let sum: i32 = items.par_iter().sum();
        assert_eq!(sum, 4950); // Sum of 0..99
    }

    #[test]
    fn test_atomic_counter_in_parallel() {
        // Test atomic counters work correctly with parallel iteration
        let counter = AtomicUsize::new(0);
        let items: Vec<i32> = (0..100).collect();

        items.par_iter().for_each(|_| {
            counter.fetch_add(1, Ordering::Relaxed);
        });

        assert_eq!(counter.load(Ordering::Relaxed), 100);
    }

    #[test]
    fn test_parallel_file_collection_pattern() {
        // Test the pattern used in collect_stable_files/collect_lazer_files
        use tempfile::TempDir;
        use std::fs;

        let temp_dir = TempDir::new().unwrap();
        let dir_path = temp_dir.path();

        // Create test files
        for i in 0..5 {
            fs::write(dir_path.join(format!("file{}.txt", i)), format!("content{}", i)).unwrap();
        }

        // Parallel file read pattern
        let entries: Vec<_> = fs::read_dir(dir_path)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .collect();

        let files: Vec<_> = entries
            .par_iter()
            .filter_map(|entry| {
                let path = entry.path();
                let filename = path.file_name()?.to_string_lossy().to_string();
                let content = fs::read(&path).ok()?;
                Some((filename, content))
            })
            .collect();

        assert_eq!(files.len(), 5);

        // Verify all files were read
        let filenames: HashSet<_> = files.iter().map(|(name, _)| name.clone()).collect();
        for i in 0..5 {
            assert!(filenames.contains(&format!("file{}.txt", i)));
        }
    }
}
