//! Background worker thread for sync operations

use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use osu_sync_core::backup::{BackupManager, BackupTarget};
use osu_sync_core::collection::{CollectionSyncEngine, CollectionSyncStrategy, StableCollectionReader};
use osu_sync_core::config::Config;
use osu_sync_core::lazer::LazerDatabase;
use osu_sync_core::stable::StableScanner;
use osu_sync_core::stats::StatsAnalyzer;
use osu_sync_core::sync::{SyncDirection, SyncEngineBuilder, SyncProgress};

use crate::app::{AppMessage, ScanResult, WorkerMessage};

/// Background worker for handling sync operations
pub struct Worker {
    handle: Option<JoinHandle<()>>,
    tx: Sender<WorkerMessage>,
}

impl Worker {
    /// Spawn a new worker thread
    pub fn spawn(app_tx: Sender<AppMessage>) -> Self {
        let (worker_tx, worker_rx) = mpsc::channel::<WorkerMessage>();
        let (_resolution_tx, resolution_rx) = mpsc::channel();

        let handle = thread::spawn(move || {
            run_worker(worker_rx, app_tx, resolution_rx);
        });

        Self {
            handle: Some(handle),
            tx: worker_tx,
        }
    }

    /// Get a sender for sending messages to the worker
    pub fn sender(&self) -> Sender<WorkerMessage> {
        self.tx.clone()
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
) {
    loop {
        match rx.recv() {
            Ok(WorkerMessage::StartScan { stable, lazer }) => {
                handle_scan(&app_tx, stable, lazer);
            }
            Ok(WorkerMessage::StartSync { direction }) => {
                handle_sync(&app_tx, direction);
            }
            Ok(WorkerMessage::StartDryRun { direction }) => {
                handle_dry_run(&app_tx, direction);
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
            Ok(WorkerMessage::CreateBackup { target }) => {
                handle_create_backup(&app_tx, target);
            }
            Ok(WorkerMessage::LoadBackups) => {
                handle_load_backups(&app_tx);
            }
            Ok(WorkerMessage::RestoreBackup { backup_path }) => {
                handle_restore_backup(&app_tx, backup_path);
            }
            Ok(WorkerMessage::StartMediaExtraction { media_type, organization, output_path }) => {
                handle_media_extraction(&app_tx, media_type, organization, output_path);
            }
            Ok(WorkerMessage::LoadReplays) => {
                handle_load_replays(&app_tx);
            }
            Ok(WorkerMessage::StartReplayExport { organization, output_path }) => {
                handle_replay_export(&app_tx, organization, output_path);
            }
            Ok(WorkerMessage::Cancel) => {
                // TODO: Implement cancellation
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
            match StableScanner::new(songs_path).skip_hashing().scan_parallel_timed() {
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

            let lazer_start = Instant::now();
            match LazerDatabase::open(path) {
                Ok(db) => {
                    match db.get_all_beatmap_sets() {
                        Ok(sets) => {
                            let lazer_time = lazer_start.elapsed();
                            let total_beatmaps: usize = sets.iter().map(|s| s.beatmaps.len()).sum();
                            let timing_report = format!(
                                "Lazer scan completed in {:.2}s ({} sets, {} beatmaps)",
                                lazer_time.as_secs_f64(),
                                sets.len(),
                                total_beatmaps
                            );
                            lazer_result = Some(ScanResult {
                                path: Some(path.display().to_string()),
                                detected: true,
                                beatmap_sets: sets.len(),
                                total_beatmaps,
                                timing_report: Some(timing_report),
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
                    }
                }
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

fn handle_sync(app_tx: &Sender<AppMessage>, direction: SyncDirection) {
    let config = Config::load();

    // Check paths
    let stable_path = match config.stable_path.as_ref() {
        Some(p) => p.clone(),
        None => {
            let _ = app_tx.send(AppMessage::Error("osu!stable path not configured".to_string()));
            return;
        }
    };

    let lazer_path = match config.lazer_path.as_ref() {
        Some(p) => p.clone(),
        None => {
            let _ = app_tx.send(AppMessage::Error("osu!lazer path not configured".to_string()));
            return;
        }
    };

    // Create components (full hashing required for sync to compare files)
    let songs_path = stable_path.join("Songs");
    let scanner = StableScanner::new(songs_path);
    let database = match LazerDatabase::open(&lazer_path) {
        Ok(db) => db,
        Err(e) => {
            let _ = app_tx.send(AppMessage::Error(format!("Failed to open lazer database: {}", e)));
            return;
        }
    };

    // Create progress callback
    let progress_tx = app_tx.clone();
    let progress_callback = Box::new(move |progress: SyncProgress| {
        let _ = progress_tx.send(AppMessage::SyncProgress(progress));
    });

    // Build engine
    let engine = match SyncEngineBuilder::new()
        .config(config)
        .stable_scanner(scanner)
        .lazer_database(database)
        .progress_callback(progress_callback)
        .build()
    {
        Ok(e) => e,
        Err(e) => {
            let _ = app_tx.send(AppMessage::Error(format!("Failed to create sync engine: {}", e)));
            return;
        }
    };

    // Create resolver (for now, auto-skip)
    let resolver = osu_sync_core::sync::AutoResolver::skip_all();

    // Run sync
    match engine.sync(direction, &resolver) {
        Ok(result) => {
            let _ = app_tx.send(AppMessage::SyncComplete(result));
        }
        Err(e) => {
            let _ = app_tx.send(AppMessage::Error(format!("Sync failed: {}", e)));
        }
    }
}

fn handle_calculate_stats(app_tx: &Sender<AppMessage>) {
    let config = Config::load();

    let _ = app_tx.send(AppMessage::StatsProgress("Scanning osu!stable...".to_string()));

    // Scan stable (fast mode - no hashing needed for stats)
    let stable_sets = if let Some(path) = config.stable_path.as_ref() {
        let songs_path = path.join("Songs");
        match StableScanner::new(songs_path).skip_hashing().scan_parallel() {
            Ok(sets) => sets,
            Err(_) => Vec::new(),
        }
    } else {
        Vec::new()
    };

    let _ = app_tx.send(AppMessage::StatsProgress("Scanning osu!lazer...".to_string()));

    // Scan lazer
    let lazer_sets = if let Some(path) = config.lazer_path.as_ref() {
        match LazerDatabase::open(path) {
            Ok(db) => db.get_all_beatmap_sets().unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    } else {
        Vec::new()
    };

    let _ = app_tx.send(AppMessage::StatsProgress("Calculating statistics...".to_string()));

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

fn handle_dry_run(app_tx: &Sender<AppMessage>, direction: SyncDirection) {
    let config = Config::load();

    // Check paths
    let stable_path = match config.stable_path.as_ref() {
        Some(p) => p.clone(),
        None => {
            let _ = app_tx.send(AppMessage::Error("osu!stable path not configured".to_string()));
            return;
        }
    };

    let lazer_path = match config.lazer_path.as_ref() {
        Some(p) => p.clone(),
        None => {
            let _ = app_tx.send(AppMessage::Error("osu!lazer path not configured".to_string()));
            return;
        }
    };

    // Create components (full hashing required for sync to compare files)
    let songs_path = stable_path.join("Songs");
    let scanner = StableScanner::new(songs_path);
    let database = match LazerDatabase::open(&lazer_path) {
        Ok(db) => db,
        Err(e) => {
            let _ = app_tx.send(AppMessage::Error(format!("Failed to open lazer database: {}", e)));
            return;
        }
    };

    // Create progress callback
    let progress_tx = app_tx.clone();
    let progress_callback = Box::new(move |progress: SyncProgress| {
        let _ = progress_tx.send(AppMessage::SyncProgress(progress));
    });

    // Build engine
    let engine = match SyncEngineBuilder::new()
        .config(config)
        .stable_scanner(scanner)
        .lazer_database(database)
        .progress_callback(progress_callback)
        .build()
    {
        Ok(e) => e,
        Err(e) => {
            let _ = app_tx.send(AppMessage::Error(format!("Failed to create sync engine: {}", e)));
            return;
        }
    };

    // Run dry run
    match engine.dry_run(direction) {
        Ok(result) => {
            let _ = app_tx.send(AppMessage::DryRunComplete { result, direction });
        }
        Err(e) => {
            let _ = app_tx.send(AppMessage::Error(format!("Dry run failed: {}", e)));
        }
    }
}

fn handle_create_backup(app_tx: &Sender<AppMessage>, target: BackupTarget) {
    let config = Config::load();
    let backup_manager = BackupManager::new(BackupManager::default_backup_dir());

    // Determine source path based on target
    let source_path = match target {
        BackupTarget::StableSongs => {
            match config.stable_path.as_ref().map(|p| p.join("Songs")) {
                Some(path) if path.exists() => path,
                _ => {
                    let _ = app_tx.send(AppMessage::Error(
                        "osu!stable Songs folder not found".to_string(),
                    ));
                    return;
                }
            }
        }
        BackupTarget::StableCollections => {
            match config.stable_path.as_ref().map(|p| p.join("collection.db")) {
                Some(path) if path.exists() => path,
                _ => {
                    let _ = app_tx.send(AppMessage::Error(
                        "collection.db not found".to_string(),
                    ));
                    return;
                }
            }
        }
        BackupTarget::StableScores => {
            match config.stable_path.as_ref().map(|p| p.join("scores.db")) {
                Some(path) if path.exists() => path,
                _ => {
                    let _ = app_tx.send(AppMessage::Error(
                        "scores.db not found".to_string(),
                    ));
                    return;
                }
            }
        }
        BackupTarget::LazerData => {
            match config.lazer_path.as_ref() {
                Some(path) if path.exists() => path.clone(),
                _ => {
                    let _ = app_tx.send(AppMessage::Error(
                        "osu!lazer data folder not found".to_string(),
                    ));
                    return;
                }
            }
        }
        BackupTarget::All => {
            // For "All", we backup stable folder (which contains Songs, collection.db, scores.db)
            match config.stable_path.as_ref() {
                Some(path) if path.exists() => path.clone(),
                _ => {
                    let _ = app_tx.send(AppMessage::Error(
                        "osu!stable folder not found".to_string(),
                    ));
                    return;
                }
            }
        }
    };

    // Create progress callback
    let progress_tx = app_tx.clone();
    let progress_callback = Box::new(move |progress: osu_sync_core::backup::BackupProgress| {
        let _ = progress_tx.send(AppMessage::BackupProgress(progress));
    });

    // Create backup
    match backup_manager.create_backup_with_progress(target, &source_path, Some(progress_callback)) {
        Ok(backup_path) => {
            let size_bytes = std::fs::metadata(&backup_path)
                .map(|m| m.len())
                .unwrap_or(0);
            let _ = app_tx.send(AppMessage::BackupComplete {
                path: backup_path,
                size_bytes,
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

    let target = backup_info.as_ref().map(|b| b.target).unwrap_or(BackupTarget::All);

    // Determine destination path based on target
    let dest_path = match target {
        BackupTarget::StableSongs => {
            match config.stable_path.as_ref().map(|p| p.join("Songs")) {
                Some(path) => path,
                None => {
                    let _ = app_tx.send(AppMessage::Error(
                        "osu!stable Songs folder not configured".to_string(),
                    ));
                    return;
                }
            }
        }
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
        BackupTarget::LazerData => {
            match config.lazer_path.as_ref() {
                Some(path) => path.clone(),
                None => {
                    let _ = app_tx.send(AppMessage::Error(
                        "osu!lazer folder not configured".to_string(),
                    ));
                    return;
                }
            }
        }
        BackupTarget::All => {
            match config.stable_path.as_ref() {
                Some(path) => path.clone(),
                None => {
                    let _ = app_tx.send(AppMessage::Error(
                        "osu! folder not configured".to_string(),
                    ));
                    return;
                }
            }
        }
    };

    // Create progress callback
    let progress_tx = app_tx.clone();
    let progress_callback = Box::new(move |progress: osu_sync_core::backup::BackupProgress| {
        let _ = progress_tx.send(AppMessage::RestoreProgress(progress));
    });

    // Restore backup
    match backup_manager.restore_backup_with_progress(&backup_path, &dest_path, Some(progress_callback)) {
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
) {
    use osu_sync_core::media::{ExtractionProgress, MediaExtractor};

    let config = Config::load();

    // Get stable path
    let stable_path = match config.stable_path.as_ref() {
        Some(p) => p.clone(),
        None => {
            let _ = app_tx.send(AppMessage::Error("osu!stable path not configured".to_string()));
            return;
        }
    };

    let songs_path = stable_path.join("Songs");

    // Scan beatmap sets first (fast mode - no hashing needed for media extraction)
    let sets = match StableScanner::new(songs_path.clone()).skip_hashing().scan_parallel() {
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

    // Create extractor and run
    let mut extractor = MediaExtractor::new(&output_path)
        .with_media_type(media_type)
        .with_organization(organization);

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
            let _ = app_tx.send(AppMessage::Error("osu!stable path not configured".to_string()));
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
            let _ = app_tx.send(AppMessage::ReplaysLoaded { replays, exportable_count });
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
) {
    use osu_sync_core::replay::{ReplayExporter, ReplayProgress, StableReplayReader};

    let config = Config::load();

    // Get stable path
    let stable_path = match config.stable_path.as_ref() {
        Some(p) => p.clone(),
        None => {
            let _ = app_tx.send(AppMessage::Error("osu!stable path not configured".to_string()));
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

    // Create exporter and run
    let exporter = ReplayExporter::new(output_path)
        .with_organization(organization)
        .with_progress_callback(progress_callback);

    match exporter.export(&replays) {
        Ok(result) => {
            let _ = app_tx.send(AppMessage::ReplayComplete(result));
        }
        Err(e) => {
            let _ = app_tx.send(AppMessage::Error(format!("Replay export failed: {}", e)));
        }
    }
}
