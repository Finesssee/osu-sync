//! Configuration types for the Unified Storage feature.
//!
//! This module provides configuration structures that control how osu! stable
//! and lazer installations share resources through unified storage.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

/// Mode for unified storage - determines which installation is the "master".
///
/// The master installation owns the canonical copy of shared resources,
/// while the other installation uses symbolic links or junctions to access them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum UnifiedStorageMode {
    /// Unified storage is disabled; installations are independent.
    #[default]
    Disabled,
    /// osu! stable is the master; lazer links to stable's resources.
    StableMaster,
    /// osu! lazer is the master; stable links to lazer's resources.
    LazerMaster,
    /// Both installations link to a shared third-party location.
    TrueUnified,
}

impl UnifiedStorageMode {
    /// Returns `true` if unified storage is enabled in any mode.
    #[inline]
    pub fn is_enabled(&self) -> bool {
        !matches!(self, Self::Disabled)
    }

    /// Returns a human-readable description of the mode.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Disabled => "Unified storage is disabled",
            Self::StableMaster => "osu! stable is the master installation",
            Self::LazerMaster => "osu! lazer is the master installation",
            Self::TrueUnified => "Using shared storage location for both installations",
        }
    }
}

/// Resource types that can be shared between installations.
///
/// Each resource type represents a category of files that can be
/// synchronized or shared between osu! stable and lazer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SharedResourceType {
    /// Beatmap files (.osz, folders with .osu files).
    Beatmaps,
    /// Skin folders and files.
    Skins,
    /// Replay files (.osr).
    Replays,
    /// Screenshot images.
    Screenshots,
    /// Exported files (scores, beatmaps, etc.).
    Exports,
    /// Background images and videos.
    Backgrounds,
}

impl SharedResourceType {
    /// Returns all available resource types.
    pub fn all() -> &'static [SharedResourceType] {
        &[
            Self::Beatmaps,
            Self::Skins,
            Self::Replays,
            Self::Screenshots,
            Self::Exports,
            Self::Backgrounds,
        ]
    }

    /// Returns the default folder name for this resource type.
    pub fn folder_name(&self) -> &'static str {
        match self {
            Self::Beatmaps => "Songs",
            Self::Skins => "Skins",
            Self::Replays => "Replays",
            Self::Screenshots => "Screenshots",
            Self::Exports => "Exports",
            Self::Backgrounds => "Backgrounds",
        }
    }

    /// Returns a human-readable display name for this resource type.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Beatmaps => "Beatmaps",
            Self::Skins => "Skins",
            Self::Replays => "Replays",
            Self::Screenshots => "Screenshots",
            Self::Exports => "Exports",
            Self::Backgrounds => "Backgrounds",
        }
    }
}

/// Sync trigger configuration.
///
/// Controls when and how synchronization operations are initiated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncTriggers {
    /// Enable automatic file watching for changes.
    pub file_watcher: bool,
    /// Trigger sync when a game is launched.
    pub on_game_launch: bool,
    /// Allow manual sync triggering.
    pub manual: bool,
    /// Interval in seconds for the file watcher polling.
    pub watcher_interval_secs: u64,
}

impl Default for SyncTriggers {
    fn default() -> Self {
        Self {
            file_watcher: false,
            on_game_launch: false,
            manual: true,
            watcher_interval_secs: 5,
        }
    }
}

impl SyncTriggers {
    /// Returns `true` if any automatic trigger is enabled.
    pub fn has_automatic_triggers(&self) -> bool {
        self.file_watcher || self.on_game_launch
    }

    /// Creates a configuration with all triggers enabled.
    pub fn all_enabled() -> Self {
        Self {
            file_watcher: true,
            on_game_launch: true,
            manual: true,
            watcher_interval_secs: 5,
        }
    }

    /// Creates a configuration with only manual triggering enabled.
    pub fn manual_only() -> Self {
        Self {
            file_watcher: false,
            on_game_launch: false,
            manual: true,
            watcher_interval_secs: 5,
        }
    }
}

/// Configuration for unified storage.
///
/// This struct contains all settings needed to configure how osu! stable
/// and lazer share resources through the unified storage system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedStorageConfig {
    /// The unified storage mode (disabled, stable master, lazer master, or true unified).
    pub mode: UnifiedStorageMode,
    /// Path to the shared storage location (required for `TrueUnified` mode).
    pub shared_path: Option<PathBuf>,
    /// Set of resource types that should be shared between installations.
    pub shared_resources: HashSet<SharedResourceType>,
    /// Configuration for sync triggers.
    pub triggers: SyncTriggers,
    /// Use NTFS junctions instead of symbolic links on Windows.
    pub use_junctions: bool,
    /// Track changes in a manifest file for efficient syncing.
    pub track_manifest: bool,
}

impl Default for UnifiedStorageConfig {
    fn default() -> Self {
        let mut resources = HashSet::new();
        resources.insert(SharedResourceType::Beatmaps);
        resources.insert(SharedResourceType::Skins);

        Self {
            mode: UnifiedStorageMode::Disabled,
            shared_path: None,
            shared_resources: resources,
            triggers: SyncTriggers {
                file_watcher: true,
                on_game_launch: false,
                manual: true,
                watcher_interval_secs: 5,
            },
            use_junctions: true,
            track_manifest: true,
        }
    }
}

impl UnifiedStorageConfig {
    /// Creates a new disabled configuration.
    pub fn disabled() -> Self {
        Self {
            mode: UnifiedStorageMode::Disabled,
            ..Default::default()
        }
    }

    /// Creates a new configuration with stable as the master.
    pub fn stable_master() -> Self {
        Self {
            mode: UnifiedStorageMode::StableMaster,
            ..Default::default()
        }
    }

    /// Creates a new configuration with lazer as the master.
    pub fn lazer_master() -> Self {
        Self {
            mode: UnifiedStorageMode::LazerMaster,
            ..Default::default()
        }
    }

    /// Creates a new true unified configuration with the specified shared path.
    pub fn true_unified(shared_path: PathBuf) -> Self {
        Self {
            mode: UnifiedStorageMode::TrueUnified,
            shared_path: Some(shared_path),
            ..Default::default()
        }
    }

    /// Returns `true` if unified storage is enabled.
    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.mode.is_enabled()
    }

    /// Returns the shared path, if configured.
    ///
    /// For `TrueUnified` mode, this returns the configured shared path.
    /// For other modes, this returns `None`.
    pub fn get_shared_path(&self) -> Option<&PathBuf> {
        self.shared_path.as_ref()
    }

    /// Returns `true` if the specified resource type is shared.
    pub fn is_resource_shared(&self, resource: SharedResourceType) -> bool {
        self.shared_resources.contains(&resource)
    }

    /// Adds a resource type to the shared resources set.
    pub fn share_resource(&mut self, resource: SharedResourceType) {
        self.shared_resources.insert(resource);
    }

    /// Removes a resource type from the shared resources set.
    pub fn unshare_resource(&mut self, resource: SharedResourceType) {
        self.shared_resources.remove(&resource);
    }

    /// Sets all resource types as shared.
    pub fn share_all_resources(&mut self) {
        for resource in SharedResourceType::all() {
            self.shared_resources.insert(*resource);
        }
    }

    /// Clears all shared resources.
    pub fn unshare_all_resources(&mut self) {
        self.shared_resources.clear();
    }

    /// Returns an iterator over the shared resource types.
    pub fn shared_resources_iter(&self) -> impl Iterator<Item = &SharedResourceType> {
        self.shared_resources.iter()
    }

    /// Returns the number of shared resource types.
    pub fn shared_resources_count(&self) -> usize {
        self.shared_resources.len()
    }

    /// Validates the configuration and returns any errors.
    ///
    /// # Errors
    ///
    /// Returns an error message if:
    /// - `TrueUnified` mode is set but no shared path is configured
    /// - The shared path is set but doesn't exist (optional check)
    pub fn validate(&self) -> Result<(), String> {
        if self.mode == UnifiedStorageMode::TrueUnified && self.shared_path.is_none() {
            return Err(
                "TrueUnified mode requires a shared_path to be configured".to_string()
            );
        }

        if self.is_enabled() && self.shared_resources.is_empty() {
            return Err(
                "At least one resource type must be selected for sharing".to_string()
            );
        }

        Ok(())
    }

    /// Returns `true` if junctions should be used (Windows-specific).
    ///
    /// Junctions are preferred on Windows as they don't require administrator
    /// privileges, unlike symbolic links.
    #[inline]
    pub fn should_use_junctions(&self) -> bool {
        self.use_junctions && cfg!(windows)
    }

    /// Returns `true` if manifest tracking is enabled.
    #[inline]
    pub fn should_track_manifest(&self) -> bool {
        self.track_manifest
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = UnifiedStorageConfig::default();
        assert!(!config.is_enabled());
        assert!(config.is_resource_shared(SharedResourceType::Beatmaps));
        assert!(config.is_resource_shared(SharedResourceType::Skins));
        assert!(!config.is_resource_shared(SharedResourceType::Replays));
    }

    #[test]
    fn test_unified_storage_mode() {
        assert!(!UnifiedStorageMode::Disabled.is_enabled());
        assert!(UnifiedStorageMode::StableMaster.is_enabled());
        assert!(UnifiedStorageMode::LazerMaster.is_enabled());
        assert!(UnifiedStorageMode::TrueUnified.is_enabled());
    }

    #[test]
    fn test_config_validation() {
        let mut config = UnifiedStorageConfig::default();
        assert!(config.validate().is_ok());

        config.mode = UnifiedStorageMode::TrueUnified;
        assert!(config.validate().is_err());

        config.shared_path = Some(PathBuf::from("/shared"));
        assert!(config.validate().is_ok());

        config.unshare_all_resources();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_resource_management() {
        let mut config = UnifiedStorageConfig::disabled();
        config.unshare_all_resources();
        assert_eq!(config.shared_resources_count(), 0);

        config.share_resource(SharedResourceType::Beatmaps);
        assert_eq!(config.shared_resources_count(), 1);
        assert!(config.is_resource_shared(SharedResourceType::Beatmaps));

        config.share_all_resources();
        assert_eq!(config.shared_resources_count(), SharedResourceType::all().len());

        config.unshare_resource(SharedResourceType::Beatmaps);
        assert!(!config.is_resource_shared(SharedResourceType::Beatmaps));
    }

    #[test]
    fn test_sync_triggers() {
        let triggers = SyncTriggers::default();
        assert!(!triggers.has_automatic_triggers());

        let triggers = SyncTriggers::all_enabled();
        assert!(triggers.has_automatic_triggers());

        let triggers = SyncTriggers::manual_only();
        assert!(!triggers.has_automatic_triggers());
        assert!(triggers.manual);
    }

    #[test]
    fn test_factory_methods() {
        let config = UnifiedStorageConfig::stable_master();
        assert_eq!(config.mode, UnifiedStorageMode::StableMaster);

        let config = UnifiedStorageConfig::lazer_master();
        assert_eq!(config.mode, UnifiedStorageMode::LazerMaster);

        let config = UnifiedStorageConfig::true_unified(PathBuf::from("/shared"));
        assert_eq!(config.mode, UnifiedStorageMode::TrueUnified);
        assert_eq!(config.get_shared_path(), Some(&PathBuf::from("/shared")));
    }
}
