//! Statistics and analysis module for osu! installations
//!
//! Provides functionality for analyzing beatmap collections and generating
//! comparison statistics between osu!stable and osu!lazer.

mod analyzer;
mod export;
mod model;

pub use analyzer::StatsAnalyzer;
pub use export::{export_csv, export_json, ExportFormat};
pub use model::*;
