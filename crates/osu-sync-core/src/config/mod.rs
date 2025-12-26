//! Configuration and path detection

mod paths;

pub use paths::*;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Configuration for osu-sync
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Path to osu!stable installation (Songs folder parent)
    pub stable_path: Option<PathBuf>,
    /// Path to osu!lazer data directory
    pub lazer_path: Option<PathBuf>,
    /// Default duplicate handling strategy
    pub duplicate_strategy: DuplicateStrategy,
}

/// Strategy for handling duplicate beatmaps
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum DuplicateStrategy {
    /// Skip importing duplicates
    Skip,
    /// Replace existing with new version
    Replace,
    /// Keep both versions
    KeepBoth,
    /// Ask user for each duplicate
    #[default]
    Ask,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            stable_path: detect_stable_path(),
            lazer_path: detect_lazer_path(),
            duplicate_strategy: DuplicateStrategy::Ask,
        }
    }
}

impl Config {
    /// Create a new config with auto-detected paths
    pub fn auto_detect() -> Self {
        Self::default()
    }

    /// Get the config file path
    fn config_path() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("osu-sync").join("config.json"))
    }

    /// Load config from disk, falling back to auto-detection if not found
    pub fn load() -> Self {
        Self::config_path()
            .and_then(|path| std::fs::read_to_string(&path).ok())
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default()
    }

    /// Save config to disk
    pub fn save(&self) -> std::io::Result<()> {
        if let Some(path) = Self::config_path() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let content = serde_json::to_string_pretty(self)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            std::fs::write(&path, content)?;
        }
        Ok(())
    }

    /// Get the Songs folder path for osu!stable
    pub fn stable_songs_path(&self) -> Option<PathBuf> {
        self.stable_path.as_ref().map(|p| p.join("Songs"))
    }

    /// Get the files directory for osu!lazer
    pub fn lazer_files_path(&self) -> Option<PathBuf> {
        self.lazer_path.as_ref().map(|p| p.join("files"))
    }

    /// Get the import directory for osu!lazer
    pub fn lazer_import_path(&self) -> Option<PathBuf> {
        self.lazer_path.as_ref().map(|p| p.join("import"))
    }

    /// Get the Realm database path for osu!lazer
    pub fn lazer_realm_path(&self) -> Option<PathBuf> {
        self.lazer_path.as_ref().map(|p| p.join("client.realm"))
    }
}
