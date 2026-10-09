//! Configuration and path detection

pub mod live_guard;
mod paths;
pub mod scan_cache;

pub use paths::*;

use crate::unified::UnifiedStorageConfig;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::OnceLock;

/// Theme name for UI customization
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeName {
    /// Default osu! pink theme
    #[default]
    Default,
    /// Ocean blue theme
    Ocean,
    /// Monochrome grayscale theme
    Monochrome,
}

impl ThemeName {
    /// Get the display name for this theme
    pub fn display_name(&self) -> &'static str {
        match self {
            ThemeName::Default => "Default (Pink)",
            ThemeName::Ocean => "Ocean (Blue)",
            ThemeName::Monochrome => "Monochrome",
        }
    }

    /// Cycle to the next theme
    pub fn next(&self) -> ThemeName {
        match self {
            ThemeName::Default => ThemeName::Ocean,
            ThemeName::Ocean => ThemeName::Monochrome,
            ThemeName::Monochrome => ThemeName::Default,
        }
    }
}

impl std::fmt::Display for ThemeName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Configuration for osu-sync
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Path to osu!stable installation (Songs folder parent)
    pub stable_path: Option<PathBuf>,
    /// Path to osu!lazer data directory
    pub lazer_path: Option<PathBuf>,
    /// Default duplicate handling strategy
    pub duplicate_strategy: DuplicateStrategy,
    /// UI theme preference
    #[serde(default)]
    pub theme: ThemeName,
    /// Unified storage configuration
    #[serde(default)]
    pub unified_storage: Option<UnifiedStorageConfig>,
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
            theme: ThemeName::Default,
            unified_storage: None,
        }
    }
}

/// Install paths given on the command line. They replace the saved and detected
/// paths for this process, and while any is set the config is never saved.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PathOverrides {
    pub stable: Option<PathBuf>,
    pub lazer: Option<PathBuf>,
}

impl PathOverrides {
    pub fn is_empty(&self) -> bool {
        self.stable.is_none() && self.lazer.is_none()
    }

    fn apply(&self, config: &mut Config) {
        if let Some(stable) = &self.stable {
            config.stable_path = Some(stable.clone());
        }
        if let Some(lazer) = &self.lazer {
            config.lazer_path = Some(lazer.clone());
        }
    }
}

static PATH_OVERRIDES: OnceLock<PathOverrides> = OnceLock::new();

/// Sets the overrides for this process. Only the first call takes effect.
pub fn set_path_overrides(overrides: PathOverrides) {
    let _ = PATH_OVERRIDES.set(overrides);
}

fn active_path_overrides() -> Option<&'static PathOverrides> {
    PATH_OVERRIDES.get().filter(|o| !o.is_empty())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveOutcome {
    Saved,
    SkippedForPathOverrides,
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
        let mut config: Self = Self::config_path()
            .and_then(|path| std::fs::read_to_string(&path).ok())
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default();
        if let Some(overrides) = active_path_overrides() {
            overrides.apply(&mut config);
        }
        config
    }

    /// Save config to disk, unless path overrides are active for this process
    pub fn save(&self) -> std::io::Result<SaveOutcome> {
        self.save_with(active_path_overrides())
    }

    pub fn save_with(&self, overrides: Option<&PathOverrides>) -> std::io::Result<SaveOutcome> {
        if overrides.is_some_and(|o| !o.is_empty()) {
            return Ok(SaveOutcome::SkippedForPathOverrides);
        }
        if let Some(path) = Self::config_path() {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let content = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
            std::fs::write(&path, content)?;
        }
        Ok(SaveOutcome::Saved)
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
