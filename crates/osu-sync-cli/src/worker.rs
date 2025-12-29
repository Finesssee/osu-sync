//! Background worker thread for sync operations

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;
use std::time::Instant;

use osu_sync_core::backup::{
    BackupManager, BackupMode, BackupOptions, BackupTarget, CompressionLevel,
};
use osu_sync_core::collection::{
    CollectionSyncEngine, CollectionSyncStrategy, StableCollectionReader,
};
use osu_sync_core::config::Config;
use osu_sync_core::lazer::LazerDatabase;
use osu_sync_core::stable::StableScanner;
use osu_sync_core::stats::StatsAnalyzer;
use osu_sync_core::sync::{SyncDirection, SyncEngineBuilder, SyncProgress};
use osu_sync_core::unified::{SharedResourceType, UnifiedStorageMode};

use crate::app::{AppMessage, ScanResult, WorkerMessage};

/// Background worker for handling sync operations
pub struct Worker {
    handle: Option<JoinHandle<()>>,
    tx: Sender<WorkerMessage>,
    /// Shared cancellation flag
    cancelled: Arc<AtomicBool>,
}

impl Worker {
    /// Spawn a new worker thread
    pub fn spawn(app_tx: Sender<AppMessage>) -> Self {
        let (worker_tx, worker_rx) = mpsc::channel::<WorkerMessage>();
        let (_resolution_tx, resolution_rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancelled_clone = Arc::clone(&cancelled);

        let handle = thread::spawn(move || {
            run_worker(worker_rx, app_tx, resolution_rx, cancelled_clone);
        });

        Self {
            handle: Some(handle),
            tx: worker_tx,
            cancelled,
        }
    }

    /// Get a sender for sending messages to the worker
    pub fn sender(&self) -> Sender<WorkerMessage> {
        self.tx.clone()
    }

    /// Get a clone of the cancellation flag for sharing with other components
    pub fn cancellation_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancelled)
    }

    /// Shutdown the worker and wait for it to finish
    pub fn shutdown(mut self) {
        let _ = self.tx.send(WorkerMessage::Shutdown);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

fn run_worker(
    rx: Receiver<WorkerMessage>,
    app_tx: Sender<AppMessage>,
    _resolution_rx: Receiver<osu_sync_core::dedup::DuplicateResolution>,
    cancelled: Arc<AtomicBool>,
) {
    // Cancellation flag is now passed from Worker struct

    loop {
        match rx.recv() {
            Ok(WorkerMessage::StartScan { stable, lazer }) => {
                cancelled.store(false, Ordering::SeqCst);
                handle_scan(&app_tx, stable, lazer);
            }
            Ok(WorkerMessage::StartSync { direction, selected_set_ids, selected_folders }) => {
                cancelled.store(false, Ordering::SeqCst);
                handle_sync(&app_tx, direction, Arc::clone(&cancelled), selected_set_ids, selected_folders);
            }
            Ok(WorkerMessage::StartDryRun { direction }) => {
                cancelled.store(false, Ordering::SeqCst);
                handle_dry_run(&app_tx, direction, Arc::clone(&cancelled));
            }
            Ok(WorkerMessage::CalculateStats) => {
                handle_calculate_stats(&app_tx);
            }
            Ok(WorkerMessage::ResolveDuplicate(_resolution)) => {
                // This is handled through the TuiResolver
            }
            Ok(WorkerMessage::LoadCollections) => {
                handle_load_collections(&app_tx);
            }
            Ok(WorkerMessage::SyncCollections { strategy }) => {
                handle_sync_collections(&app_tx, strategy);
            }
            Ok(WorkerMessage::CreateBackup {
                target,
                compression,
                mode,
            }) => {
                handle_create_backup(&app_tx, target, compression, mode);
            }
            Ok(WorkerMessage::LoadBackups) => {
                handle_load_backups(&app_tx);
            }
            Ok(WorkerMessage::RestoreBackup { backup_path }) => {
                handle_restore_backup(&app_tx, backup_path);
            }
            Ok(WorkerMessage::StartMediaExtraction {
                media_type,
                organization,
                output_path,
                skip_duplicates,
                include_metadata,
            }) => {
                handle_media_extraction(
                    &app_tx,
                    media_type,
                    organization,
                    output_path,
                    skip_duplicates,
                    include_metadata,
                );
            }
            Ok(WorkerMessage::LoadReplays) => {
                handle_load_replays(&app_tx);
            }
            Ok(WorkerMessage::StartReplayExport {
                organization,
                output_path,
                filter,
                rename_pattern,
            }) => {
                handle_replay_export(&app_tx, organization, output_path, filter, rename_pattern);
            }
            Ok(WorkerMessage::StartUnifiedSetup {
                mode,
                shared_path,
                resources,
            }) => {
                handle_unified_setup(&app_tx, mode, shared_path, resources);
            }
            Ok(WorkerMessage::GetUnifiedStatus) => {
                handle_unified_status(&app_tx);
            }
            Ok(WorkerMessage::VerifyUnifiedLinks) => {
                handle_unified_verify(&app_tx);
            }
            Ok(WorkerMessage::RepairUnifiedLinks) => {
                handle_unified_repair(&app_tx);
            }
            Ok(WorkerMessage::DisableUnifiedStorage) => {
                handle_unified_disable(&app_tx);
            }
            Ok(WorkerMessage::Cancel) => {
                cancelled.store(true, Ordering::SeqCst);
            }
            Ok(WorkerMessage::Shutdown) | Err(_) => {
                break;
            }
        }
    }
}

fn handle_scan(app_tx: &Sender<AppMessage>, scan_stable: bool, scan_lazer: bool) {
    let config = Config::load();

    let mut stable_result = None;
    let mut lazer_result = None;

    if scan_stable {
        let _ = app_tx.send(AppMessage::ScanProgress {
            stable: true,
            message: "Detecting osu!stable...".to_string(),
        });

        if let Some(path) = config.stable_path.as_ref() {
            let songs_path = path.join("Songs");
            let _ = app_tx.send(AppMessage::ScanProgress {
                stable: true,
                message: "Scanning osu!stable beatmaps...".to_string(),
            });

            // Use fast mode (skip hashing) for browsing - 5x faster
            match StableScanner::new(songs_path)
                .skip_hashing()
                .scan_parallel_timed()
            {
                Ok((sets, timing)) => {
                    let total_beatmaps: usize = sets.iter().map(|s| s.beatmaps.len()).sum();
                    stable_result = Some(ScanResult {
                        path: Some(path.display().to_string()),
                        detected: true,
                        beatmap_sets: sets.len(),
                        total_beatmaps,
                        timing_report: Some(timing.report()),
                    });
                }
                Err(e) => {
                    let _ = app_tx.send(AppMessage::Error(format!("Stable scan error: {}", e)));
                    stable_result = Some(ScanResult {
                        path: Some(path.display().to_string()),
                        detected: false,
                        beatmap_sets: 0,
                        total_beatmaps: 0,
                        timing_report: None,
                    });
                }
            }
        } else {
            stable_result = Some(ScanResult {
                path: None,
                detected: false,
                beatmap_sets: 0,
                total_beatmaps: 0,
                timing_report: None,
            });
        }
    }

    if scan_lazer {
        let _ = app_tx.send(AppMessage::ScanProgress {
            stable: false,
            message: "Detecting osu!lazer...".to_string(),
        });

        if let Some(path) = config.lazer_path.as_ref() {
            let _ = app_tx.send(AppMessage::ScanProgress {
                stable: false,
                message: "Loading osu!lazer database...".to_string(),
            });

            match LazerDatabase::open(path) {
                Ok(db) => match db.get_all_beatmap_sets_timed() {
                    Ok((sets, timing)) => {
                        let total_beatmaps: usize = sets.iter().map(|s| s.beatmaps.len()).sum();
                        lazer_result = Some(ScanResult {
                            path: Some(path.display().to_string()),
                            detected: true,
                            beatmap_sets: sets.len(),
                            total_beatmaps,
                            timing_report: Some(timing.report()),
                        });
                    }
                    Err(e) => {
                        let _ = app_tx.send(AppMessage::Error(format!("Lazer query error: {}", e)));
                        lazer_result = Some(ScanResult {
                            path: Some(path.display().to_string()),
                            detected: false,
                            beatmap_sets: 0,
                            total_beatmaps: 0,
                            timing_report: None,
                        });
                    }
                },
                Err(e) => {
                    let _ = app_tx.send(AppMessage::Error(format!("Lazer open error: {}", e)));
                    lazer_result = Some(ScanResult {
                        path: Some(path.display().to_string()),
                        detected: false,
                        beatmap_sets: 0,
                        total_beatmaps: 0,
                        timing_report: None,
                    });
                }
            }
        } else {
            lazer_result = Some(ScanResult {
                path: None,
                detected: false,
                beatmap_sets: 0,
                total_beatmaps: 0,
                timing_report: None,
            });
        }
    }

    let _ = app_tx.send(AppMessage::ScanComplete {
        stable: stable_result,
        lazer: lazer_result,
    });
}

fn handle_sync(
    app_tx: &Sender<AppMessage>,
    direction: SyncDirection,
    cancelled: Arc<AtomicBool>,
    selected_set_ids: Option<HashSet<i32>>,
    selected_folders: Option<HashSet<String>>,
) {
    let config = Config::load();

    // Check paths
    let stable_path = match config.stable_path.as_ref() {
        Some(p) => p.clone(),
        None => {
            let _ = app_tx.send(AppMessage::Error(
                "osu!stable path not configured".to_string(),
            ));
            return;
        }
    };

    let lazer_path = match config.lazer_path.as_ref() {
        Some(p) => p.clone(),
        None => {
            let _ = app_tx.send(AppMessage::Error(
                "osu!lazer path not configured".to_string(),
            ));
            return;
        }
    };

    // Create components (skip hashing - MD5s come from .osu file parsing, not file hashing)
    let songs_path = stable_path.join("Songs");
    let scanner = StableScanner::new(songs_path).skip_hashing();
    let database = match LazerDatabase::open(&lazer_path) {
        Ok(db) => db,
        Err(e) => {
            let _ = app_tx.send(AppMessage::Error(format!(
                "Failed to open lazer database: {}",
                e
            )));
            return;
        }
    };

    // Create progress callback
    let progress_tx = app_tx.clone();
    let progress_callback = Box::new(move |progress: SyncProgress| {
        let _ = progress_tx.send(AppMessage::SyncProgress(progress));
    });

    // Build engine with cancellation support
    let mut builder = SyncEngineBuilder::new()
        .config(config)
        .stable_scanner(scanner)
        .lazer_database(database)
        .progress_callback(progress_callback)
        .cancellation(Arc::clone(&cancelled));

    // Add selected set IDs if provided (for user selection from dry run)
    if let Some(set_ids) = selected_set_ids {
        builder = builder.selected_set_ids(set_ids);
    }

    // Add selected folders if provided (fallback for sets without IDs)
    if let Some(folders) = selected_folders {
        builder = builder.selected_folders(folders);
    }

    let engine = match builder.build() {
        Ok(e) => e,
        Err(e) => {
            let _ = app_tx.send(AppMessage::Error(format!(
                "Failed to create sync engine: {}",
                e
            )));
            return;
        }
    };

    // Check for cancel before starting
    if cancelled.load(Ordering::SeqCst) {
        let _ = app_tx.send(AppMessage::SyncCancelled);
        return;
    }

    // Create resolver (for now, auto-skip)
    let resolver = osu_sync_core::sync::AutoResolver::skip_all();

    // Run sync - the engine will check is_cancelled() via the shared flag
    let sync_result = engine.sync(direction, &resolver);

    match sync_result {
        Ok(result) => {
            if cancelled.load(Ordering::SeqCst) {
                let _ = app_tx.send(AppMessage::SyncCancelled);
            } else {
                let _ = app_tx.send(AppMessage::SyncComplete(result));
            }
        }
        Err(e) => {
            let _ = app_tx.send(AppMessage::Error(format!("Sync failed: {}", e)));
        }
    }
}

fn handle_calculate_stats(app_tx: &Sender<AppMessage>) {
    let config = Config::load();

    let _ = app_tx.send(AppMessage::StatsProgress(
        "Scanning osu!stable...".to_string(),
    ));

    // Scan stable (fast mode - no hashing needed for stats)
    let stable_sets = if let Some(path) = config.stable_path.as_ref() {
        let songs_path = path.join("Songs");
        match StableScanner::new(songs_path)
            .skip_hashing()
            .scan_parallel()
        {
            Ok(sets) => sets,
            Err(_) => Vec::new(),
        }
    } else {
        Vec::new()
    };

    let _ = app_tx.send(AppMessage::StatsProgress(
        "Scanning osu!lazer...".to_string(),
    ));

    // Scan lazer
    let lazer_sets = if let Some(path) = config.lazer_path.as_ref() {
        match LazerDatabase::open(path) {
            Ok(db) => db.get_all_beatmap_sets().unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    } else {
        Vec::new()
    };

    let _ = app_tx.send(AppMessage::StatsProgress(
        "Calculating statistics...".to_string(),
    ));

    // Calculate comparison stats
    let stats = StatsAnalyzer::compare(&stable_sets, &lazer_sets);

    let _ = app_tx.send(AppMessage::StatsComplete(stats));
}

fn handle_load_collections(app_tx: &Sender<AppMessage>) {
    let config = Config::load();

    let collections = if let Some(stable_path) = config.stable_path.as_ref() {
        // The collection.db is in the root osu! folder, not in Songs
        // stable_path is the Songs folder, so we need to go up one level
        let osu_root = stable_path.parent().unwrap_or(stable_path);
        let collection_db_path = osu_root.join("collection.db");

        match StableCollectionReader::read(&collection_db_path) {
            Ok(collections) => collections,
            Err(e) => {
                let _ = app_tx.send(AppMessage::Error(format!(
                    "Failed to read collection.db: {}",
                    e
                )));
                Vec::new()
            }
        }
    } else {
        let _ = app_tx.send(AppMessage::Error(
            "osu!stable path not configured".to_string(),
        ));
        Vec::new()
    };

    let _ = app_tx.send(AppMessage::CollectionsLoaded(collections));
}

fn handle_sync_collections(app_tx: &Sender<AppMessage>, strategy: CollectionSyncStrategy) {
    let config = Config::load();

    // Load collections from stable
    let collections = if let Some(stable_path) = config.stable_path.as_ref() {
        let osu_root = stable_path.parent().unwrap_or(stable_path);
        let collection_db_path = osu_root.join("collection.db");

        match StableCollectionReader::read(&collection_db_path) {
            Ok(collections) => collections,
            Err(e) => {
                let _ = app_tx.send(AppMessage::CollectionSyncComplete(
                    osu_sync_core::collection::CollectionSyncResult::failure(format!(
                        "Failed to read collections: {}",
                        e
                    )),
                ));
                return;
            }
        }
    } else {
        let _ = app_tx.send(AppMessage::CollectionSyncComplete(
            osu_sync_core::collection::CollectionSyncResult::failure(
                "osu!stable path not configured",
            ),
        ));
        return;
    };

    // Send progress updates for each collection
    let total = collections.len();
    for (i, collection) in collections.iter().enumerate() {
        let progress = (i as f32) / (total as f32).max(1.0);
        let _ = app_tx.send(AppMessage::CollectionSyncProgress {
            collection: collection.name.clone(),
            progress,
        });
    }

    // Perform the sync
    match CollectionSyncEngine::sync_to_lazer(&collections, strategy) {
        Ok(result) => {
            let _ = app_tx.send(AppMessage::CollectionSyncComplete(result));
        }
        Err(e) => {
            let _ = app_tx.send(AppMessage::CollectionSyncComplete(
                osu_sync_core::collection::CollectionSyncResult::failure(format!(
                    "Collection sync failed: {}",
                    e
                )),
            ));
        }
    }
}

fn handle_dry_run(
    app_tx: &Sender<AppMessage>,
    direction: SyncDirection,
    cancelled: Arc<AtomicBool>,
) {
    let config = Config::load();

    // Check paths
    let stable_path = match config.stable_path.as_ref() {
        Some(p) => p.clone(),
        None => {
            let _ = app_tx.send(AppMessage::Error(
                "osu!stable path not configured".to_string(),
            ));
            return;
        }
    };

    let lazer_path = match config.lazer_path.as_ref() {
        Some(p) => p.clone(),
        None => {
            let _ = app_tx.send(AppMessage::Error(
                "osu!lazer path not configured".to_string(),
            ));
            return;
        }
    };

    // Check for cancel before starting
    if cancelled.load(Ordering::SeqCst) {
        let _ = app_tx.send(AppMessage::SyncCancelled);
        return;
    }

    // Create components (skip hashing - MD5s come from .osu file parsing, not file hashing)
    let songs_path = stable_path.join("Songs");
    let scanner = StableScanner::new(songs_path).skip_hashing();
    let database = match LazerDatabase::open(&lazer_path) {
        Ok(db) => db,
        Err(e) => {
            let _ = app_tx.send(AppMessage::Error(format!(
                "Failed to open lazer database: {}",
                e
            )));
            return;
        }
    };

    // Create progress callback
    let progress_tx = app_tx.clone();
    let progress_callback = Box::new(move |progress: SyncProgress| {
        let _ = progress_tx.send(AppMessage::SyncProgress(progress));
    });

    // Build engine with cancellation support
    let engine = match SyncEngineBuilder::new()
        .config(config)
        .stable_scanner(scanner)
        .lazer_database(database)
        .progress_callback(progress_callback)
        .cancellation(Arc::clone(&cancelled))
        .build()
    {
        Ok(e) => e,
        Err(e) => {
            let _ = app_tx.send(AppMessage::Error(format!(
                "Failed to create sync engine: {}",
                e
            )));
            return;
        }
    };

    // Run dry run - the engine will check is_cancelled() via the shared flag
    match engine.dry_run(direction) {
        Ok(result) => {
            if cancelled.load(Ordering::SeqCst) {
                let _ = app_tx.send(AppMessage::SyncCancelled);
            } else {
                let _ = app_tx.send(AppMessage::DryRunComplete { result, direction });
            }
        }
        Err(e) => {
            let _ = app_tx.send(AppMessage::Error(format!("Dry run failed: {}", e)));
        }
    }
}

fn handle_create_backup(
    app_tx: &Sender<AppMessage>,
    target: BackupTarget,
    compression: CompressionLevel,
    mode: BackupMode,
) {
    let config = Config::load();
    let backup_manager = BackupManager::new(BackupManager::default_backup_dir());

    // Determine source path based on target
    let source_path = match target {
        BackupTarget::StableSongs => match config.stable_path.as_ref().map(|p| p.join("Songs")) {
            Some(path) if path.exists() => path,
            _ => {
                let _ = app_tx.send(AppMessage::Error(
                    "osu!stable Songs folder not found".to_string(),
                ));
                return;
            }
        },
        BackupTarget::StableCollections => {
            match config.stable_path.as_ref().map(|p| p.join("collection.db")) {
                Some(path) if path.exists() => path,
                _ => {
                    let _ = app_tx.send(AppMessage::Error("collection.db not found".to_string()));
                    return;
                }
            }
        }
        BackupTarget::StableScores => {
            match config.stable_path.as_ref().map(|p| p.join("scores.db")) {
                Some(path) if path.exists() => path,
                _ => {
                    let _ = app_tx.send(AppMessage::Error("scores.db not found".to_string()));
                    return;
                }
            }
        }
        BackupTarget::LazerData => match config.lazer_path.as_ref() {
            Some(path) if path.exists() => path.clone(),
            _ => {
                let _ = app_tx.send(AppMessage::Error(
                    "osu!lazer data folder not found".to_string(),
                ));
                return;
            }
        },
        BackupTarget::All => {
            // For "All", we backup stable folder (which contains Songs, collection.db, scores.db)
            match config.stable_path.as_ref() {
                Some(path) if path.exists() => path.clone(),
                _ => {
                    let _ =
                        app_tx.send(AppMessage::Error("osu!stable folder not found".to_string()));
                    return;
                }
            }
        }
    };

    // Create backup options
    let options = BackupOptions::new()
        .with_compression(compression)
        .with_mode(mode);

    let is_incremental = mode == BackupMode::Incremental;

    // Create progress callback
    let progress_tx = app_tx.clone();
    let progress_callback = Box::new(move |progress: osu_sync_core::backup::BackupProgress| {
        let _ = progress_tx.send(AppMessage::BackupProgress(progress));
    });

    // Create backup with options
    match backup_manager.create_backup_with_options(
        target,
        &source_path,
        options,
        Some(progress_callback),
    ) {
        Ok(backup_path) => {
            let size_bytes = std::fs::metadata(&backup_path)
                .map(|m| m.len())
                .unwrap_or(0);
            let _ = app_tx.send(AppMessage::BackupComplete {
                path: backup_path,
                size_bytes,
                is_incremental,
            });
        }
        Err(e) => {
            let _ = app_tx.send(AppMessage::Error(format!("Backup failed: {}", e)));
        }
    }
}

fn handle_load_backups(app_tx: &Sender<AppMessage>) {
    let backup_manager = BackupManager::new(BackupManager::default_backup_dir());

    match backup_manager.list_backups() {
        Ok(backups) => {
            let _ = app_tx.send(AppMessage::BackupsLoaded(backups));
        }
        Err(e) => {
            let _ = app_tx.send(AppMessage::Error(format!("Failed to load backups: {}", e)));
            let _ = app_tx.send(AppMessage::BackupsLoaded(Vec::new()));
        }
    }
}

fn handle_restore_backup(app_tx: &Sender<AppMessage>, backup_path: PathBuf) {
    let config = Config::load();
    let backup_manager = BackupManager::new(BackupManager::default_backup_dir());

    // Parse backup info to determine target
    let backup_info = match backup_manager.list_backups() {
        Ok(backups) => backups.into_iter().find(|b| b.path == backup_path),
        Err(_) => None,
    };

    let target = backup_info
        .as_ref()
        .map(|b| b.target)
        .unwrap_or(BackupTarget::All);

    // Determine destination path based on target
    let dest_path = match target {
        BackupTarget::StableSongs => match config.stable_path.as_ref().map(|p| p.join("Songs")) {
            Some(path) => path,
            None => {
                let _ = app_tx.send(AppMessage::Error(
                    "osu!stable Songs folder not configured".to_string(),
                ));
                return;
            }
        },
        BackupTarget::StableCollections | BackupTarget::StableScores => {
            match config.stable_path.as_ref() {
                Some(path) => path.clone(),
                None => {
                    let _ = app_tx.send(AppMessage::Error(
                        "osu!stable folder not configured".to_string(),
                    ));
                    return;
                }
            }
        }
        BackupTarget::LazerData => match config.lazer_path.as_ref() {
            Some(path) => path.clone(),
            None => {
                let _ = app_tx.send(AppMessage::Error(
                    "osu!lazer folder not configured".to_string(),
                ));
                return;
            }
        },
        BackupTarget::All => match config.stable_path.as_ref() {
            Some(path) => path.clone(),
            None => {
                let _ = app_tx.send(AppMessage::Error("osu! folder not configured".to_string()));
                return;
            }
        },
    };

    // Create progress callback
    let progress_tx = app_tx.clone();
    let progress_callback = Box::new(move |progress: osu_sync_core::backup::BackupProgress| {
        let _ = progress_tx.send(AppMessage::RestoreProgress(progress));
    });

    // Restore backup
    match backup_manager.restore_backup_with_progress(
        &backup_path,
        &dest_path,
        Some(progress_callback),
    ) {
        Ok(()) => {
            // Get file count from last progress or estimate
            let files_restored = 0; // We don't track this currently
            let _ = app_tx.send(AppMessage::RestoreComplete {
                dest_path,
                files_restored,
            });
        }
        Err(e) => {
            let _ = app_tx.send(AppMessage::Error(format!("Restore failed: {}", e)));
        }
    }
}

fn handle_media_extraction(
    app_tx: &Sender<AppMessage>,
    media_type: osu_sync_core::media::MediaType,
    organization: osu_sync_core::media::OutputOrganization,
    output_path: PathBuf,
    _skip_duplicates: bool,
    include_metadata: bool,
) {
    use osu_sync_core::media::{ExtractionProgress, MediaExtractor};

    let config = Config::load();

    // Get stable path
    let stable_path = match config.stable_path.as_ref() {
        Some(p) => p.clone(),
        None => {
            let _ = app_tx.send(AppMessage::Error(
                "osu!stable path not configured".to_string(),
            ));
            return;
        }
    };

    let songs_path = stable_path.join("Songs");

    // Scan beatmap sets first (fast mode - no hashing needed for media extraction)
    let sets = match StableScanner::new(songs_path.clone())
        .skip_hashing()
        .scan_parallel()
    {
        Ok(s) => s,
        Err(e) => {
            let _ = app_tx.send(AppMessage::Error(format!("Failed to scan beatmaps: {}", e)));
            return;
        }
    };

    // Create progress callback
    let progress_tx = app_tx.clone();
    let progress_callback = Box::new(move |progress: ExtractionProgress| {
        let _ = progress_tx.send(AppMessage::MediaProgress(progress));
    });

    // Create extractor with metadata option
    let mut extractor = MediaExtractor::new(&output_path)
        .with_media_type(media_type)
        .with_organization(organization)
        .with_metadata(include_metadata);

    match extractor.extract_from_stable(&songs_path, &sets, Some(progress_callback)) {
        Ok(result) => {
            let _ = app_tx.send(AppMessage::MediaComplete(result));
        }
        Err(e) => {
            let _ = app_tx.send(AppMessage::Error(format!("Media extraction failed: {}", e)));
        }
    }
}

fn handle_load_replays(app_tx: &Sender<AppMessage>) {
    use osu_sync_core::replay::StableReplayReader;

    let config = Config::load();

    // Get stable path
    let stable_path = match config.stable_path.as_ref() {
        Some(p) => p.clone(),
        None => {
            let _ = app_tx.send(AppMessage::Error(
                "osu!stable path not configured".to_string(),
            ));
            return;
        }
    };

    let mut reader = StableReplayReader::new(&stable_path);

    // Try to load beatmap metadata for enrichment
    let _ = reader.load_beatmap_metadata();

    // Load replays
    match reader.read_replays() {
        Ok(replays) => {
            let exportable_count = replays.iter().filter(|r| r.has_replay_file).count();
            let _ = app_tx.send(AppMessage::ReplaysLoaded {
                replays,
                exportable_count,
            });
        }
        Err(e) => {
            let _ = app_tx.send(AppMessage::Error(format!("Failed to load replays: {}", e)));
        }
    }
}

fn handle_replay_export(
    app_tx: &Sender<AppMessage>,
    organization: osu_sync_core::replay::ExportOrganization,
    output_path: PathBuf,
    filter: osu_sync_core::replay::ReplayFilter,
    rename_pattern: Option<String>,
) {
    use osu_sync_core::replay::{ReplayExporter, ReplayProgress, StableReplayReader};

    let config = Config::load();

    // Get stable path
    let stable_path = match config.stable_path.as_ref() {
        Some(p) => p.clone(),
        None => {
            let _ = app_tx.send(AppMessage::Error(
                "osu!stable path not configured".to_string(),
            ));
            return;
        }
    };

    // Load replays
    let mut reader = StableReplayReader::new(&stable_path);
    let _ = reader.load_beatmap_metadata();

    let replays = match reader.read_exportable_replays() {
        Ok(r) => r,
        Err(e) => {
            let _ = app_tx.send(AppMessage::Error(format!("Failed to load replays: {}", e)));
            return;
        }
    };

    // Create progress callback
    let progress_tx = app_tx.clone();
    let progress_callback = Box::new(move |progress: ReplayProgress| {
        let _ = progress_tx.send(AppMessage::ReplayProgress(progress));
    });

    // Create exporter with filter and rename pattern
    let mut exporter = ReplayExporter::new(output_path)
        .with_organization(organization)
        .with_filter(filter)
        .with_progress_callback(progress_callback);

    // Add rename pattern if provided
    if let Some(pattern) = rename_pattern {
        exporter = exporter.with_rename_pattern(pattern);
    }

    match exporter.export(&replays) {
        Ok(result) => {
            let _ = app_tx.send(AppMessage::ReplayComplete(result));
        }
        Err(e) => {
            let _ = app_tx.send(AppMessage::Error(format!("Replay export failed: {}", e)));
        }
    }
}

fn handle_unified_setup(
    app_tx: &Sender<AppMessage>,
    mode: UnifiedStorageMode,
    shared_path: Option<PathBuf>,
    _resources: Vec<SharedResourceType>,
) {
    use osu_sync_core::unified::{UnifiedMigration, UnifiedStorageConfig};

    let config = Config::load();

    // Get stable and lazer paths
    let stable_path = match config.stable_path.as_ref() {
        Some(p) => p.clone(),
        None => {
            let _ = app_tx.send(AppMessage::Error(
                "osu!stable path not configured".to_string(),
            ));
            return;
        }
    };

    let lazer_path = match config.lazer_path.as_ref() {
        Some(p) => p.clone(),
        None => {
            let _ = app_tx.send(AppMessage::Error(
                "osu!lazer path not configured".to_string(),
            ));
            return;
        }
    };

    // Send initial progress
    let _ = app_tx.send(AppMessage::UnifiedStorageProgress {
        phase: "Preparing".to_string(),
        current: 0,
        total: 100,
        message: "Initializing unified storage setup...".to_string(),
    });

    // Create unified storage config based on mode
    let unified_config = match mode {
        UnifiedStorageMode::Disabled => UnifiedStorageConfig::disabled(),
        UnifiedStorageMode::StableMaster => UnifiedStorageConfig::stable_master(),
        UnifiedStorageMode::LazerMaster => UnifiedStorageConfig::lazer_master(),
        UnifiedStorageMode::TrueUnified => {
            if let Some(path) = shared_path {
                UnifiedStorageConfig::true_unified(path)
            } else {
                let _ = app_tx.send(AppMessage::UnifiedStorageComplete {
                    success: false,
                    message: "Shared path required for True Unified mode".to_string(),
                    links_created: 0,
                    space_saved: 0,
                });
                return;
            }
        }
    };

    // Create migration
    let mut migration = UnifiedMigration::new(unified_config, stable_path, lazer_path);

    // Create progress callback
    let progress_tx = app_tx.clone();
    let progress_callback = move |progress: osu_sync_core::unified::MigrationProgress| {
        let _ = progress_tx.send(AppMessage::UnifiedStorageProgress {
            phase: progress.step_name.clone(),
            current: progress.current_step,
            total: progress.total_steps,
            message: format!(
                "Step {}/{}: {}",
                progress.current_step, progress.total_steps, progress.step_name
            ),
        });
    };

    // Execute the migration
    match migration.execute(progress_callback) {
        Ok(result) => {
            let _ = app_tx.send(AppMessage::UnifiedStorageComplete {
                success: result.success,
                message: if result.success {
                    "Unified storage setup complete!".to_string()
                } else {
                    result.errors.first().cloned().unwrap_or_else(|| "Unknown error".to_string())
                },
                links_created: result.links_created,
                space_saved: result.space_saved,
            });
        }
        Err(e) => {
            let _ = app_tx.send(AppMessage::UnifiedStorageComplete {
                success: false,
                message: format!("Migration failed: {}", e),
                links_created: 0,
                space_saved: 0,
            });
        }
    }
}

fn handle_unified_status(app_tx: &Sender<AppMessage>) {
    let config = Config::load();

    // Get current unified storage config
    let unified_config = config.unified_storage.unwrap_or_default();
    let mode = format!("{:?}", unified_config.mode);

    // For now, return basic status
    // In a full implementation, we'd scan the manifest and verify links
    let _ = app_tx.send(AppMessage::UnifiedStorageStatus {
        mode,
        active_links: 0,
        broken_links: 0,
        space_saved: 0,
    });
}

fn handle_unified_verify(app_tx: &Sender<AppMessage>) {
    let config = Config::load();

    // Check if unified storage is enabled
    let unified_config = config.unified_storage.unwrap_or_default();
    if unified_config.mode == UnifiedStorageMode::Disabled {
        let _ = app_tx.send(AppMessage::UnifiedStorageVerifyComplete {
            healthy: 0,
            broken: 0,
            repaired: 0,
        });
        return;
    }

    // For now, just return a basic status
    // In a full implementation, we'd check each junction/symlink
    let _ = app_tx.send(AppMessage::UnifiedStorageVerifyComplete {
        healthy: 0,
        broken: 0,
        repaired: 0,
    });
}

fn handle_unified_repair(app_tx: &Sender<AppMessage>) {
    // Similar to verify but attempts to fix broken links
    let _ = app_tx.send(AppMessage::UnifiedStorageVerifyComplete {
        healthy: 0,
        broken: 0,
        repaired: 0,
    });
}

fn handle_unified_disable(app_tx: &Sender<AppMessage>) {
    let config = Config::load();

    // Check for manifest file
    let manifest_path = config
        .stable_path
        .as_ref()
        .map(|p| p.join(".osu-sync-unified.json"));

    if let Some(path) = manifest_path {
        if path.exists() {
            // In a full implementation, we would:
            // 1. Load manifest
            // 2. Remove all junctions/symlinks
            // 3. Restore original folder structure
            // 4. Delete manifest

            // For now, just delete the manifest
            let _ = std::fs::remove_file(&path);
        }
    }

    // Update config to disable unified storage
    let mut new_config = config.clone();
    new_config.unified_storage = Some(osu_sync_core::unified::UnifiedStorageConfig::disabled());
    let _ = new_config.save();

    let _ = app_tx.send(AppMessage::UnifiedStorageComplete {
        success: true,
        message: "Unified storage disabled".to_string(),
        links_created: 0,
        space_saved: 0,
    });
}
