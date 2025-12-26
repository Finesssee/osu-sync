//! # osu-sync-core
//!
//! Core library for synchronizing beatmaps between osu!stable and osu!lazer.
//!
//! This crate provides the foundational functionality for:
//! - Parsing `.osu` beatmap files and `.osz` archives
//! - Scanning and managing beatmaps in osu!stable's Songs folder
//! - Reading and writing to osu!lazer's file store and Realm database
//! - Detecting and handling duplicate beatmaps
//! - Cross-platform path detection for osu! installations
//!
//! ## Modules
//!
//! - [`beatmap`] - Beatmap data structures (metadata, difficulty, files)
//! - [`config`] - Configuration and path detection
//! - [`dedup`] - Duplicate detection and resolution
//! - [`error`] - Error types and Result alias
//! - [`lazer`] - osu!lazer file store and database integration
//! - [`parser`] - .osu file and .osz archive parsing
//! - [`stable`] - osu!stable Songs folder integration
//! - [`sync`] - Synchronization engine and conflict resolution
//!
//! ## Example
//!
//! ```no_run
//! use osu_sync_core::{Config, StableScanner, BeatmapSet};
//!
//! // Auto-detect osu! installations
//! let config = Config::auto_detect();
//!
//! // Scan stable's Songs folder
//! if let Some(songs_path) = config.stable_songs_path() {
//!     let scanner = StableScanner::new(songs_path);
//!     let beatmap_sets = scanner.scan().expect("Failed to scan");
//!     println!("Found {} beatmap sets", beatmap_sets.len());
//! }
//! ```

// Module declarations
pub mod backup;
pub mod beatmap;
pub mod collection;
pub mod config;
pub mod dedup;
pub mod error;
pub mod filter;
pub mod lazer;
pub mod media;
pub mod parser;
pub mod replay;
pub mod stable;
pub mod stats;
pub mod sync;

// Re-export key types for convenience

// Error types
pub use error::{Error, Result};

// Beatmap types
pub use beatmap::{
    BeatmapDifficulty, BeatmapFile, BeatmapInfo, BeatmapMetadata, BeatmapSet, GameMode,
};

// Configuration
pub use config::{
    detect_lazer_path, detect_stable_path, validate_lazer_path, validate_stable_path, Config,
    DuplicateStrategy as DuplicateHandling,
};

// Parsing
pub use parser::{create_osz, create_osz_from_set, extract_osz, parse_osu_file};

// osu!stable integration
pub use stable::{BeatmapIndex, ImportResult, ScanProgress, StableExporter, StableImporter, StableScanner};

// osu!lazer integration
pub use lazer::{
    LazerBeatmapInfo, LazerBeatmapSet, LazerDatabase, LazerExporter, LazerFileStore, LazerImporter,
    LazerIndex, LazerNamedFile,
};

// Duplicate detection
pub use dedup::{
    BeatmapSetRef, DuplicateAction, DuplicateDetector, DuplicateInfo, DuplicateResolution,
    DuplicateStrategy, MatchType,
};

// Sync engine
pub use sync::{
    format_bytes, AutoResolver, ConfigBasedResolver, ConflictResolver, DryRunAction, DryRunItem,
    DryRunResult, InteractiveResolver, ProgressCallback, SmartResolver, SyncDirection, SyncEngine,
    SyncEngineBuilder, SyncError, SyncPhase, SyncProgress, SyncResult,
};

// Statistics
pub use stats::{
    export_csv, export_json, ComparisonStats, DuplicateStats, ExportFormat, InstallationStats,
    RankedStatus, StarRatingBucket, StatsAnalyzer,
};

// Filtering
pub use filter::{FilterCriteria, FilterEngine};

// Collections
pub use collection::{
    Collection, CollectionSyncDirection, CollectionSyncEngine, CollectionSyncProgress,
    CollectionSyncResult, CollectionSyncStrategy, StableCollectionReader,
};

// Backup
pub use backup::{
    BackupInfo, BackupManager, BackupPhase, BackupProgress, BackupProgressCallback, BackupTarget,
};

// Media extraction
pub use media::{
    ExtractionProgress, ExtractionProgressCallback, ExtractionResult, ExtractionSource,
    MediaExtractor, MediaType, OutputOrganization,
};

// Replay export
pub use replay::{
    ExportOrganization as ReplayOrganization, Grade, ReplayExportResult, ReplayExporter,
    ReplayInfo, ReplayProgress, ReplayProgressCallback, ReplayStats, StableReplayReader,
};
