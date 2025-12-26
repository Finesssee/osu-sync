//! osu!lazer Realm database and file storage integration
//!
//! This module provides integration with both osu!lazer and osu!stable:
//!
//! - [`LazerDatabase`] - Reader for osu!lazer's Realm database (placeholder)
//! - [`StableDatabase`] - Reader for osu!stable's osu!.db file (fully implemented)
//! - [`LazerFileStore`] - Access to lazer's content-addressed file store
//!
//! ## Example
//!
//! ```no_run
//! use osu_sync_core::lazer::StableDatabase;
//! use std::path::Path;
//!
//! let db = StableDatabase::open(Path::new("C:/osu!"))?;
//! println!("Player: {:?}", db.player_name());
//! println!("Database version: {}", db.version());
//!
//! let sets = db.get_all_beatmap_sets()?;
//! println!("Found {} beatmap sets", sets.len());
//! # Ok::<(), osu_sync_core::error::Error>(())
//! ```

mod file_store;
mod database;
mod importer;
mod exporter;

pub use file_store::*;
pub use database::*;
pub use importer::*;
pub use exporter::*;
