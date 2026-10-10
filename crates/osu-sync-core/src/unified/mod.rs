//! Unified storage: stable's Songs folder built from hard links into lazer's `files` store.
//!
//! The linked store is the only mode. One step writes every lazer set stable lacks
//! into Songs as hard links to lazer's blobs, then relinks stable copies of lazer
//! blobs. Setup, the watcher and "sync now" run the same step, and a rerun with
//! nothing new changes nothing. The junction modes of older versions are gone; their
//! configs load as disabled, and the junctions and records they made stay in place.

mod config;
mod engine;
mod game_detect;
mod link;
mod manifest;
mod watcher;

pub use config::{
    retired_mode_notice, save_mode, SyncTriggers, UnifiedStorageConfig, UnifiedStorageMode,
    DISABLED_NOTE,
};

pub use engine::{LinkedStoreStatus, StepPhase, StepReport, UnifiedStorageEngine};

pub use manifest::legacy_notes;

pub use watcher::{watch, WatchEvent, WatchTiming};

// Kept public although nothing in the workspace uses most of it; shrinking
// game_detect to the one check the linked store needs is a separate change.
pub use game_detect::{
    find_running_processes, is_process_running, GameEvent, GameLaunchDetector, OsuGame, ProcessInfo,
};

pub(crate) use link::{classify_hard_link_error, rename_no_replace, same_volume, HardLinkFailure};

#[cfg(test)]
pub(crate) use link::LINK_LIMIT_OS_ERROR;
