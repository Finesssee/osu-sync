//! Unified Storage Module
//!
//! This module provides functionality to combine osu!stable and osu!lazer
//! installations into a shared folder structure using symlinks/junctions.
//!
//! # Features
//!
//! - Three storage modes: StableMaster, LazerMaster, TrueUnified
//! - Platform-specific link operations (junctions on Windows, symlinks on Unix)
//! - File watching for automatic sync
//! - Game launch detection for sync triggers
//! - Migration tools for converting existing installations
//!
//! # Status: Work in Progress
//!
//! Some submodules are not yet implemented.
//!
//! # Example
//!
//! ```rust,ignore
//! use osu_sync_core::unified::{UnifiedStorageConfig, UnifiedWatcher, FileChangeEvent};
//!
//! // Create a file watcher
//! let (mut watcher, rx) = UnifiedWatcher::new()?;
//! watcher.watch(Path::new("/path/to/songs"))?;
//!
//! // Process events
//! while let Ok(event) = rx.recv() {
//!     match event {
//!         FileChangeEvent::Created { path, .. } => println!("Created: {:?}", path),
//!         FileChangeEvent::Modified { path } => println!("Modified: {:?}", path),
//!         FileChangeEvent::Deleted { path } => println!("Deleted: {:?}", path),
//!         FileChangeEvent::Renamed { from, to } => println!("Renamed: {:?} -> {:?}", from, to),
//!     }
//! }
//! ```

mod config;
mod game_detect;
mod link;
mod migration;
mod watcher;

pub use config::{
    SharedResourceType,
    SyncTriggers,
    UnifiedStorageConfig,
    UnifiedStorageMode,
};

pub use migration::{
    BackupManifest,
    MigrationPlan,
    MigrationProgress,
    MigrationResult,
    MigrationStep,
    UnifiedMigration,
};

pub use watcher::{
    FileChangeEvent,
    UnifiedWatcher,
    WatcherEventHandler,
};

pub use game_detect::{
    find_running_processes,
    is_process_running,
    GameEvent,
    GameLaunchDetector,
    OsuGame,
    ProcessInfo,
};
