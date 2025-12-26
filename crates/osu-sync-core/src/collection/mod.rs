//! Collection synchronization module for osu! beatmap collections
//!
//! Provides functionality for reading and syncing beatmap collections between
//! osu!stable and osu!lazer installations.

pub mod model;
pub mod stable_reader;
pub mod sync;

pub use model::*;
pub use stable_reader::StableCollectionReader;
pub use sync::CollectionSyncEngine;
