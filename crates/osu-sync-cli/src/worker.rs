//! Background worker thread for sync operations

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, RwLock};
use std::thread::{self, JoinHandle};

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
use osu_sync_core::unified::{
    legacy_notes, save_mode, UnifiedStorageEngine, UnifiedStorageMode, DISABLED_NOTE,
};
use osu_sync_core::Error as CoreError;

use crate::app::{AppMessage, ScanResult, WorkerMessage};

/// Background worker for handling sync operations
pub struct Worker {
    handle: Option<JoinHandle<()>>,
    tx: Sender<WorkerMessage>,
    /// Shared cancellation flag
    cancelled: Arc<AtomicBool>,
}

fn config_snapshot(config: &Arc<RwLock<Config>>) -> Config {
    if let Ok(guard) = config.read() {
        guard.clone()
    } else {
        Config::load()
    }
}

fn format_core_error(error: &CoreError) -> String {
    match error {
        CoreError::MissingPath { path_type } => format!(
            "{} path not configured. Open Configuration to set it.",
            path_type
        ),
        CoreError::OsuNotFound(path) => format!(
            "osu! installation not found at {}. Update Configuration paths.",
            path.display()
        ),
        CoreError::MissingComponent { component } => format!(
            "Internal error: missing {}. Try restarting the app.",
            component
        ),
        _ => error.to_string(),
    }
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
    // Load config once at session start to avoid repeated disk reads
    // This is cached for the lifetime of the worker thread
    let config = Arc::new(RwLock::new(Config::load()));

    loop {
        match rx.recv() {
            Ok(WorkerMessage::StartScan { stable, lazer }) => {
                cancelled.store(false, Ordering::SeqCst);
                handle_scan(&app_tx, &config, stable, lazer);
            }
            Ok(WorkerMessage::StartSync {
                direction,
                selected_set_ids,
                selected_folders,
            }) => {
                cancelled.store(false, Ordering::SeqCst);
                handle_sync(
                    &app_tx,
                    &config,
                    direction,
                    Arc::clone(&cancelled),
                    selected_set_ids,
                    selected_folders,
                );
            }
            Ok(WorkerMessage::StartDryRun { direction }) => {
                cancelled.store(false, Ordering::SeqCst);
                handle_dry_run(&app_tx, &config, direction, Arc::clone(&cancelled));
            }
            Ok(WorkerMessage::CalculateStats) => {
                handle_calculate_stats(&app_tx, &config);
            }
            Ok(WorkerMessage::ResolveDuplicate(_resolution)) => {
                // This is handled through the TuiResolver
            }
            Ok(WorkerMessage::LoadCollections) => {
                handle_load_collections(&app_tx, &config);
            }
            Ok(WorkerMessage::SyncCollections { strategy }) => {
                handle_sync_collections(&app_tx, &config, strategy);
            }
            Ok(WorkerMessage::CreateBackup {
                target,
                compression,
                mode,
            }) => {
                handle_create_backup(&app_tx, &config, target, compression, mode);
            }
            Ok(WorkerMessage::LoadBackups) => {
                handle_load_backups(&app_tx);
            }
            Ok(WorkerMessage::RestoreBackup { backup_path }) => {
                handle_restore_backup(&app_tx, &config, backup_path);
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
                    &config,
                    media_type,
                    organization,
                    output_path,
                    skip_duplicates,
                    include_metadata,
                );
            }
            Ok(WorkerMessage::LoadReplays) => {
                handle_load_replays(&app_tx, &config);
            }
            Ok(WorkerMessage::StartReplayExport {
                organization,
                output_path,
                filter,
                rename_pattern,
            }) => {
                handle_replay_export(
                    &app_tx,
                    &config,
                    organization,
                    output_path,
                    filter,
                    rename_pattern,
                );
            }
            Ok(WorkerMessage::StartUnifiedSetup) => {
                handle_unified_setup(&app_tx, &config);
            }
            Ok(WorkerMessage::GetUnifiedStatus) => {
                handle_unified_status(&app_tx, &config);
            }
            Ok(WorkerMessage::GetUnifiedMode) => {
                let unified = config_snapshot(&config).unified();
                let _ = app_tx.send(AppMessage::UnifiedMode {
                    mode: unified.mode,
                    notice: unified.retired_mode_notice(),
                });
            }
            Ok(WorkerMessage::DisableUnifiedStorage) => {
                handle_unified_disable(&app_tx, &config);
            }
            Ok(WorkerMessage::UpdateConfig(new_config)) => {
                if let Ok(mut guard) = config.write() {
                    *guard = new_config;
                }
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

fn handle_scan(
    app_tx: &Sender<AppMessage>,
    config: &Arc<RwLock<Config>>,
    scan_stable: bool,
    scan_lazer: bool,
) {
    let config = config_snapshot(config);
    let stable_path = config.stable_path.clone();
    let lazer_path = config.lazer_path.clone();

    // Run both scans in parallel using std::thread::scope
    // This halves the total scan time since stable and lazer scans are independent
    let (stable_result, lazer_result) = std::thread::scope(|s| {
        // Spawn stable scan thread
        let stable_handle = s.spawn(|| -> Option<ScanResult> {
            if !scan_stable {
                return None;
            }

            let _ = app_tx.send(AppMessage::ScanProgress {
                stable: true,
                message: "Detecting osu!stable...".to_string(),
            });

            if let Some(path) = stable_path.as_ref() {
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
                        Some(ScanResult {
                            path: Some(path.display().to_string()),
                            detected: true,
                            beatmap_sets: sets.len(),
                            total_beatmaps,
                            timing_report: Some(timing.report()),
                        })
                    }
                    Err(e) => {
                        let _ = app_tx.send(AppMessage::Error(format!("Stable scan error: {}", e)));
                        Some(ScanResult {
                            path: Some(path.display().to_string()),
                            detected: false,
                            beatmap_sets: 0,
                            total_beatmaps: 0,
                            timing_report: None,
                        })
                    }
                }
            } else {
                Some(ScanResult {
                    path: None,
                    detected: false,
                    beatmap_sets: 0,
                    total_beatmaps: 0,
                    timing_report: None,
                })
            }
        });

        // Spawn lazer scan thread
        let lazer_handle = s.spawn(|| -> Option<ScanResult> {
            if !scan_lazer {
                return None;
            }

            let _ = app_tx.send(AppMessage::ScanProgress {
                stable: false,
                message: "Detecting osu!lazer...".to_string(),
            });

            if let Some(path) = lazer_path.as_ref() {
                let _ = app_tx.send(AppMessage::ScanProgress {
                    stable: false,
                    message: "Loading osu!lazer database...".to_string(),
                });

                match LazerDatabase::open(path) {
                    Ok(db) => match db.get_all_beatmap_sets_timed() {
                        Ok((sets, timing)) => {
                            let total_beatmaps: usize = sets.iter().map(|s| s.beatmaps.len()).sum();
                            Some(ScanResult {
                                path: Some(path.display().to_string()),
                                detected: true,
                                beatmap_sets: sets.len(),
                                total_beatmaps,
                                timing_report: Some(timing.report()),
                            })
                        }
                        Err(e) => {
                            let _ =
                                app_tx.send(AppMessage::Error(format!("Lazer query error: {}", e)));
                            Some(ScanResult {
                                path: Some(path.display().to_string()),
                                detected: false,
                                beatmap_sets: 0,
                                total_beatmaps: 0,
                                timing_report: None,
                            })
                        }
                    },
                    Err(e) => {
                        let _ = app_tx.send(AppMessage::Error(format!("Lazer open error: {}", e)));
                        Some(ScanResult {
                            path: Some(path.display().to_string()),
                            detected: false,
                            beatmap_sets: 0,
                            total_beatmaps: 0,
                            timing_report: None,
                        })
                    }
                }
            } else {
                Some(ScanResult {
                    path: None,
                    detected: false,
                    beatmap_sets: 0,
                    total_beatmaps: 0,
                    timing_report: None,
                })
            }
        });

        // Wait for both threads to complete and collect results
        let stable_result = stable_handle.join().expect("stable scan thread panicked");
        let lazer_result = lazer_handle.join().expect("lazer scan thread panicked");

        (stable_result, lazer_result)
    });

    let _ = app_tx.send(AppMessage::ScanComplete {
        stable: stable_result,
        lazer: lazer_result,
    });
}

fn handle_sync(
    app_tx: &Sender<AppMessage>,
    config: &Arc<RwLock<Config>>,
    direction: SyncDirection,
    cancelled: Arc<AtomicBool>,
    selected_set_ids: Option<HashSet<i32>>,
    selected_folders: Option<HashSet<String>>,
) {
    let config = config_snapshot(config);

    // Check paths
    let stable_path = match config.stable_path.as_ref() {
        Some(p) if p.exists() => p.clone(),
        Some(p) => {
            let _ = app_tx.send(AppMessage::Error(format!(
                "osu!stable path not found at {}. Update Configuration.",
                p.display()
            )));
            return;
        }
        None => {
            let _ = app_tx.send(AppMessage::Error(
                "osu!stable path not configured. Open Configuration to set it.".to_string(),
            ));
            return;
        }
    };

    let lazer_path = match config.lazer_path.as_ref() {
        Some(p) if p.exists() => p.clone(),
        Some(p) => {
            let _ = app_tx.send(AppMessage::Error(format!(
                "osu!lazer path not found at {}. Update Configuration.",
                p.display()
            )));
            return;
        }
        None => {
            let _ = app_tx.send(AppMessage::Error(
                "osu!lazer path not configured. Open Configuration to set it.".to_string(),
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
            let _ = app_tx.send(AppMessage::Error(format_core_error(&e)));
            return;
        }
    };

    // Create progress callback
    let progress_tx = app_tx.clone();
    let progress_callback = Box::new(move |progress: SyncProgress| {
        let _ = progress_tx.send(AppMessage::SyncProgress(progress));
    });

    // Build engine with cancellation support
    // Clone config since SyncEngineBuilder takes ownership
    let mut builder = SyncEngineBuilder::new()
        .config(config.clone())
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
                format_core_error(&e)
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
            let _ = app_tx.send(AppMessage::Error(format!(
                "Sync failed: {}",
                format_core_error(&e)
            )));
        }
    }
}

fn handle_calculate_stats(app_tx: &Sender<AppMessage>, config: &Arc<RwLock<Config>>) {
    let config = config_snapshot(config);

    let _ = app_tx.send(AppMessage::StatsProgress(
        "Scanning osu!stable...".to_string(),
    ));

    // Scan stable (fast mode - no hashing needed for stats)
    let stable_sets = if let Some(path) = config.stable_path.as_ref() {
        let songs_path = path.join("Songs");
        StableScanner::new(songs_path)
            .skip_hashing()
            .scan_parallel()
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let _ = app_tx.send(AppMessage::StatsProgress(
        "Scanning osu!lazer...".to_string(),
    ));

    // Scan lazer
    let (lazer_sets, lazer_problem) = match config.lazer_path.as_ref() {
        Some(path) => match LazerDatabase::open(path)
            .and_then(|db| Ok((db.get_all_beatmap_sets()?, db.skipped().cloned())))
        {
            Ok((sets, skipped)) => (sets, skipped.map(|s| format!("Warning: {}", s))),
            Err(e) => (
                Vec::new(),
                Some(format!("osu!lazer could not be read: {}", e)),
            ),
        },
        None => (Vec::new(), None),
    };

    let _ = app_tx.send(AppMessage::StatsProgress(
        "Calculating statistics...".to_string(),
    ));

    // Calculate comparison stats
    let stats = StatsAnalyzer::compare(&stable_sets, &lazer_sets);

    let _ = app_tx.send(AppMessage::StatsComplete(stats, lazer_problem));
}

fn handle_load_collections(app_tx: &Sender<AppMessage>, config: &Arc<RwLock<Config>>) {
    let config = config_snapshot(config);

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

fn handle_sync_collections(
    app_tx: &Sender<AppMessage>,
    config: &Arc<RwLock<Config>>,
    strategy: CollectionSyncStrategy,
) {
    let config = config_snapshot(config);

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
    config: &Arc<RwLock<Config>>,
    direction: SyncDirection,
    cancelled: Arc<AtomicBool>,
) {
    let config = config_snapshot(config);

    // Check paths
    let stable_path = match config.stable_path.as_ref() {
        Some(p) if p.exists() => p.clone(),
        Some(p) => {
            let _ = app_tx.send(AppMessage::Error(format!(
                "osu!stable path not found at {}. Update Configuration.",
                p.display()
            )));
            return;
        }
        None => {
            let _ = app_tx.send(AppMessage::Error(
                "osu!stable path not configured. Open Configuration to set it.".to_string(),
            ));
            return;
        }
    };

    let lazer_path = match config.lazer_path.as_ref() {
        Some(p) if p.exists() => p.clone(),
        Some(p) => {
            let _ = app_tx.send(AppMessage::Error(format!(
                "osu!lazer path not found at {}. Update Configuration.",
                p.display()
            )));
            return;
        }
        None => {
            let _ = app_tx.send(AppMessage::Error(
                "osu!lazer path not configured. Open Configuration to set it.".to_string(),
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
            let _ = app_tx.send(AppMessage::Error(format_core_error(&e)));
            return;
        }
    };

    // Create progress callback
    let progress_tx = app_tx.clone();
    let progress_callback = Box::new(move |progress: SyncProgress| {
        let _ = progress_tx.send(AppMessage::SyncProgress(progress));
    });

    // Build engine with cancellation support
    // Clone config since SyncEngineBuilder takes ownership
    let engine = match SyncEngineBuilder::new()
        .config(config.clone())
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
                format_core_error(&e)
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
            let _ = app_tx.send(AppMessage::Error(format!(
                "Dry run failed: {}",
                format_core_error(&e)
            )));
        }
    }
}

fn handle_create_backup(
    app_tx: &Sender<AppMessage>,
    config: &Arc<RwLock<Config>>,
    target: BackupTarget,
    compression: CompressionLevel,
    mode: BackupMode,
) {
    let config = config_snapshot(config);
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

fn handle_restore_backup(
    app_tx: &Sender<AppMessage>,
    config: &Arc<RwLock<Config>>,
    backup_path: PathBuf,
) {
    let config = config_snapshot(config);
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
    config: &Arc<RwLock<Config>>,
    media_type: osu_sync_core::media::MediaType,
    organization: osu_sync_core::media::OutputOrganization,
    output_path: PathBuf,
    _skip_duplicates: bool,
    include_metadata: bool,
) {
    let config = config_snapshot(config);
    use osu_sync_core::media::{ExtractionProgress, MediaExtractor};

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

fn handle_load_replays(app_tx: &Sender<AppMessage>, config: &Arc<RwLock<Config>>) {
    let config = config_snapshot(config);
    use osu_sync_core::replay::StableReplayReader;

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
    config: &Arc<RwLock<Config>>,
    organization: osu_sync_core::replay::ExportOrganization,
    output_path: PathBuf,
    filter: osu_sync_core::replay::ReplayFilter,
    rename_pattern: Option<String>,
) {
    let config = config_snapshot(config);
    use osu_sync_core::replay::{ReplayExporter, ReplayProgress, StableReplayReader};

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

/// Saves `mode` in the worker's config and on disk, and says what happened.
fn save_unified_mode(config_lock: &Arc<RwLock<Config>>, mode: UnifiedStorageMode) -> String {
    let mut config = config_snapshot(config_lock);
    let saved = save_mode(&mut config, mode).unwrap_or_else(|e| {
        format!(
            "Unified storage mode {} could not be saved: {e}",
            mode.label()
        )
    });
    if let Ok(mut guard) = config_lock.write() {
        *guard = config;
    }
    saved
}

/// Runs one linked-store step, then saves the linked-store mode.
fn handle_unified_setup(app_tx: &Sender<AppMessage>, config_lock: &Arc<RwLock<Config>>) {
    let config = config_snapshot(config_lock);
    let notes = legacy_notes(&config);
    let result = UnifiedStorageEngine::from_config(&config).and_then(|engine| {
        engine.sync(&mut |phase, current, total| {
            let _ = app_tx.send(AppMessage::UnifiedStorageProgress {
                phase,
                current,
                total,
            });
        })
    });
    let result = match result {
        Ok(mut report) => {
            report.notes.extend(notes);
            let saved = save_unified_mode(config_lock, UnifiedStorageMode::LinkedStore);
            Ok((report, saved))
        }
        Err(e) => Err(e.to_string()),
    };
    let _ = app_tx.send(AppMessage::UnifiedStorageComplete { result });
}

/// Counts linked and copied files in Songs.
fn handle_unified_status(app_tx: &Sender<AppMessage>, config_lock: &Arc<RwLock<Config>>) {
    let config = config_snapshot(config_lock);
    let status = UnifiedStorageEngine::from_config(&config)
        .and_then(|engine| engine.status())
        .map_err(|e| e.to_string());
    let _ = app_tx.send(AppMessage::UnifiedStorageStatus {
        mode: config.unified().mode,
        status,
        notes: legacy_notes(&config),
    });
}

/// Saves the disabled mode. Changes no file in Songs or lazer's store.
fn handle_unified_disable(app_tx: &Sender<AppMessage>, config_lock: &Arc<RwLock<Config>>) {
    let saved = save_unified_mode(config_lock, UnifiedStorageMode::Disabled);
    let _ = app_tx.send(AppMessage::UnifiedStorageDisabled {
        message: format!("{saved}. {DISABLED_NOTE}"),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use osu_sync_core::config::ThemeName;
    use osu_sync_core::unified::UnifiedStorageConfig;
    use std::time::Duration;

    #[test]
    fn config_snapshot_reflects_updates() {
        let config = Arc::new(RwLock::new(Config::default()));
        let updated = Config {
            theme: ThemeName::Monochrome,
            ..Config::default()
        };

        if let Ok(mut guard) = config.write() {
            *guard = updated.clone();
        }

        let snapshot = config_snapshot(&config);
        assert_eq!(snapshot.theme, ThemeName::Monochrome);
    }

    #[test]
    fn worker_uses_updated_config_for_status() {
        let (app_tx, app_rx) = mpsc::channel::<AppMessage>();
        let (worker_tx, worker_rx) = mpsc::channel::<WorkerMessage>();
        let (_resolution_tx, resolution_rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));

        let handle = thread::spawn(move || {
            run_worker(worker_rx, app_tx, resolution_rx, cancelled);
        });

        let config = Config {
            unified_storage: Some(UnifiedStorageConfig::linked_store()),
            stable_path: None,
            ..Config::default()
        };

        worker_tx
            .send(WorkerMessage::UpdateConfig(config))
            .expect("Failed to send UpdateConfig");
        worker_tx
            .send(WorkerMessage::GetUnifiedStatus)
            .expect("Failed to request status");

        let status = app_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("Timed out waiting for UnifiedStorageStatus");

        match status {
            AppMessage::UnifiedStorageStatus { mode, status, .. } => {
                assert_eq!(mode, UnifiedStorageMode::LinkedStore);
                assert_eq!(status, Err("osu!stable path not configured".to_string()));
            }
            other => panic!("Unexpected message: {:?}", other),
        }

        let _ = worker_tx.send(WorkerMessage::Shutdown);
        let _ = handle.join();
    }
}
