//! Main synchronization engine

use crate::beatmap::BeatmapSet;
use crate::config::Config;
use crate::dedup::{DuplicateAction, DuplicateDetector, DuplicateStrategy};
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
#[derive(Debug, Clone)]
pub struct SyncProgress {
    /// Current item being processed
    pub current: usize,
    /// Total items to process
    pub total: usize,
    /// Name of the current beatmap set
    pub current_name: String,
    /// Current phase of sync
    pub phase: SyncPhase,
}

/// Phase of the sync operation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncPhase {
    /// Scanning source beatmaps
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
    progress_callback: Option<ProgressCallback>,
    filter: Option<FilterCriteria>,
}

impl SyncEngine {
    /// Create a new sync engine
    pub fn new(
        config: Config,
        stable_scanner: StableScanner,
        lazer_database: LazerDatabase,
    ) -> Self {
        let duplicate_detector = DuplicateDetector::new(DuplicateStrategy::default());

        Self {
            config,
            stable_scanner,
            lazer_database,
            duplicate_detector,
            progress_callback: None,
            filter: None,
        }
    }

    /// Set the duplicate detection strategy
    pub fn with_duplicate_strategy(mut self, strategy: DuplicateStrategy) -> Self {
        self.duplicate_detector = DuplicateDetector::new(strategy);
        self
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

    /// Apply filter to stable beatmap sets, returning indices of matching sets
    fn filter_stable_sets(&self, sets: &[BeatmapSet]) -> Vec<usize> {
        if let Some(ref filter) = self.filter {
            sets.iter()
                .enumerate()
                .filter(|(_, set)| FilterEngine::matches_stable(set, filter))
                .map(|(i, _)| i)
                .collect()
        } else {
            // No filter, include all
            (0..sets.len()).collect()
        }
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
        });

        // Scan stable beatmaps
        let stable_sets = self.stable_scanner.scan()?;

        // Apply filter to get matching sets
        let filtered_indices = self.filter_stable_sets(&stable_sets);
        let total = filtered_indices.len();

        if self.filter.is_some() {
            tracing::info!("Filter applied: {} of {} beatmap sets match", total, stable_sets.len());
        }

        // Get lazer beatmaps for duplicate detection
        let lazer_sets = self.lazer_database.get_all_beatmap_sets()?;
        let lazer_beatmap_sets: Vec<BeatmapSet> = lazer_sets
            .iter()
            .map(|ls| self.lazer_database.to_beatmap_set(ls))
            .collect();

        // Analyze each filtered stable set
        for (progress_idx, set_idx) in filtered_indices.iter().enumerate() {
            let stable_set = &stable_sets[*set_idx];
            self.report_progress(SyncProgress {
                current: progress_idx + 1,
                total,
                current_name: stable_set
                    .folder_name
                    .clone()
                    .unwrap_or_else(|| stable_set.generate_folder_name()),
                phase: SyncPhase::Deduplicating,
            });

            // Check for duplicates
            let action = if let Some(_duplicate) = self
                .duplicate_detector
                .find_duplicate(stable_set, &lazer_beatmap_sets)
            {
                DryRunAction::Duplicate
            } else {
                // Check if it already exists in lazer by ID
                let exists = stable_set.id.map_or(false, |id| {
                    lazer_beatmap_sets.iter().any(|s| s.id == Some(id))
                });

                if exists {
                    DryRunAction::Skip
                } else {
                    DryRunAction::Import
                }
            };

            // Calculate size from files
            let size_bytes = self.calculate_stable_set_size(stable_set);

            let item = DryRunItem {
                set_id: stable_set.id,
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
        });

        // Get lazer beatmaps
        let lazer_sets = self.lazer_database.get_all_beatmap_sets()?;
        let total = lazer_sets.len();

        // Scan stable for duplicate detection
        let stable_index = self.stable_scanner.build_index()?;

        // Analyze each lazer set
        for (idx, lazer_set) in lazer_sets.iter().enumerate() {
            let beatmap_set = self.lazer_database.to_beatmap_set(lazer_set);

            self.report_progress(SyncProgress {
                current: idx + 1,
                total,
                current_name: beatmap_set.generate_folder_name(),
                phase: SyncPhase::Deduplicating,
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
        });

        let stable_sets = self.stable_scanner.scan()?;

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
        });

        let lazer_sets = self.lazer_database.get_all_beatmap_sets()?;
        let lazer_beatmap_sets: Vec<BeatmapSet> = lazer_sets
            .iter()
            .map(|ls| self.lazer_database.to_beatmap_set(ls))
            .collect();

        // Phase 3: Import to lazer
        let lazer_importer = LazerImporter::new(
            self.config
                .lazer_path
                .as_ref()
                .ok_or_else(|| Error::Config("Lazer path not configured".to_string()))?,
        );

        for (progress_idx, set_idx) in filtered_indices.iter().enumerate() {
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
                    result.errors.push(SyncError::new(Some(set_name), e.to_string()));
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
        });

        let stable_index = self.stable_scanner.build_index()?;

        // Phase 3: Import to stable
        let stable_importer = StableImporter::new(
            self.config
                .stable_songs_path()
                .ok_or_else(|| Error::Config("Stable path not configured".to_string()))?,
        );

        for (idx, lazer_set) in lazer_sets.iter().enumerate() {
            let beatmap_set = self.lazer_database.to_beatmap_set(lazer_set);
            let set_name = beatmap_set.generate_folder_name();

            self.report_progress(SyncProgress {
                current: idx + 1,
                total,
                current_name: set_name.clone(),
                phase: SyncPhase::Importing,
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
                    result.errors.push(SyncError::new(Some(set_name), e.to_string()));
                }
            }
        }

        Ok(result)
    }

    /// Collect files from a stable beatmap folder
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
        let mut files = Vec::new();

        for entry in std::fs::read_dir(&folder_path)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() {
                let filename = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                let content = std::fs::read(&path)?;
                files.push((filename, content));
            }
        }

        Ok(files)
    }

    /// Collect files from the lazer file store
    fn collect_lazer_files(
        &self,
        lazer_set: &crate::lazer::LazerBeatmapSet,
    ) -> Result<Vec<(String, Vec<u8>)>> {
        let file_store = self.lazer_database.file_store();
        let mut files = Vec::new();

        for named_file in &lazer_set.files {
            match file_store.read(&named_file.hash) {
                Ok(content) => {
                    files.push((named_file.filename.clone(), content));
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to read file {} ({}): {}",
                        named_file.filename,
                        named_file.hash,
                        e
                    );
                }
            }
        }

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
}
