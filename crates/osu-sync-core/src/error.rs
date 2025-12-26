//! Error types for osu-sync-core

use std::path::PathBuf;
use thiserror::Error;

/// Main error type for osu-sync operations
#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Failed to parse beatmap file {path}: {message}")]
    BeatmapParse {
        path: PathBuf,
        message: String,
    },

    #[error("Failed to read/write ZIP archive: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("Invalid .osz archive: {reason}")]
    InvalidOsz { reason: String },

    #[error("Beatmap not found: {0}")]
    BeatmapNotFound(String),

    #[error("osu! installation not found at: {0}")]
    OsuNotFound(PathBuf),

    #[error("Realm database error: {0}")]
    Realm(String),

    #[error("File hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },

    #[error("Sync aborted by user")]
    Aborted,

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("{0}")]
    Other(String),
}

/// Result type alias for osu-sync operations
pub type Result<T> = std::result::Result<T, Error>;
