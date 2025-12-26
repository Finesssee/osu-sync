//! Beatmap filtering module
//!
//! This module provides filtering capabilities for beatmaps before sync operations.
//! Users can filter beatmaps by star rating, game mode, ranked status, and search terms.

mod criteria;
mod engine;

pub use criteria::FilterCriteria;
pub use engine::FilterEngine;
