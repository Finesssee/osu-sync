//! Settings for unified storage.
//!
//! Unified storage has one mode, the linked store: stable's Songs folder holds hard
//! links into lazer's `files` store. Configs from older versions name junction modes
//! that no longer exist; they load as disabled with a notice instead of failing.

use serde::{Deserialize, Deserializer, Serialize};

use crate::config::{Config, SaveOutcome};

/// What disabling unified storage leaves, for the user to read.
pub const DISABLED_NOTE: &str =
    "Files in Songs and lazer's store were not changed. Linked files stay readable from both games.";

/// Whether unified storage is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum UnifiedStorageMode {
    /// Stable and lazer keep separate copies.
    #[default]
    Disabled,
    /// Stable's beatmap assets are hard links to the blobs in lazer's `files` store.
    LinkedStore,
}

impl UnifiedStorageMode {
    /// Every mode, in the order the config screen lists them.
    pub const ALL: [UnifiedStorageMode; 2] = [Self::LinkedStore, Self::Disabled];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Disabled => "Disabled",
            Self::LinkedStore => "Linked store",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Disabled => "Stable and lazer keep separate copies of every beatmap",
            Self::LinkedStore => {
                "Stable's Songs folder links to lazer's files, so each asset is stored once"
            }
        }
    }

    /// The mode a saved name stands for; `None` for a name this version does not have.
    fn from_saved(name: &str) -> Option<Self> {
        match name {
            "Disabled" => Some(Self::Disabled),
            "LinkedStore" => Some(Self::LinkedStore),
            _ => None,
        }
    }
}

/// When the watcher runs the linked-store step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncTriggers {
    /// Longest wait, in seconds, from the first change to the step.
    pub watcher_interval_secs: u64,
}

impl Default for SyncTriggers {
    fn default() -> Self {
        Self {
            watcher_interval_secs: 5,
        }
    }
}

/// Unified storage settings, saved inside the osu-sync config.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct UnifiedStorageConfig {
    pub mode: UnifiedStorageMode,
    pub triggers: SyncTriggers,
    /// The mode name an older version saved, when this version no longer has it.
    #[serde(skip)]
    pub retired_mode: Option<String>,
}

impl UnifiedStorageConfig {
    pub fn linked_store() -> Self {
        Self {
            mode: UnifiedStorageMode::LinkedStore,
            ..Self::default()
        }
    }

    /// Chooses `mode`, which replaces any retired mode an older version saved.
    pub fn set_mode(&mut self, mode: UnifiedStorageMode) {
        self.mode = mode;
        self.retired_mode = None;
    }

    /// What happened to a mode from an older version, for the user to read.
    pub fn retired_mode_notice(&self) -> Option<String> {
        self.retired_mode.as_deref().map(retired_mode_notice)
    }
}

/// The notice for a saved mode this version no longer has.
pub fn retired_mode_notice(name: &str) -> String {
    format!(
        "The saved unified storage mode \"{name}\" was removed in this version, so it loaded \
         as disabled. Junctions it made were left in place."
    )
}

/// Saves `mode` into `config` and the config file, and says what happened to the file.
pub fn save_mode(config: &mut Config, mode: UnifiedStorageMode) -> std::io::Result<String> {
    config
        .unified_storage
        .get_or_insert_with(Default::default)
        .set_mode(mode);
    Ok(match config.save()? {
        SaveOutcome::Saved => format!("Saved unified storage mode: {}", mode.label()),
        SaveOutcome::SkippedForPathOverrides => format!(
            "Unified storage mode {} was not saved, because --stable-path or --lazer-path is set",
            mode.label()
        ),
    })
}

/// The saved form, which accepts every mode name older versions wrote and ignores
/// their other fields (`shared_path`, `shared_resources`, `use_junctions`, ...).
#[derive(Deserialize)]
struct SavedConfig {
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    triggers: SyncTriggers,
}

impl<'de> Deserialize<'de> for UnifiedStorageConfig {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let saved = SavedConfig::deserialize(deserializer)?;
        let mut config = Self {
            triggers: saved.triggers,
            ..Self::default()
        };
        if let Some(name) = saved.mode {
            match UnifiedStorageMode::from_saved(&name) {
                Some(mode) => config.mode = mode,
                None => {
                    tracing::warn!("{}", retired_mode_notice(&name));
                    config.retired_mode = Some(name);
                }
            }
        }
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The unified storage block a version with junction modes saved for TrueUnified.
    const TRUE_UNIFIED: &str = r#"{
        "mode": "TrueUnified",
        "shared_path": "D:\\osu-shared",
        "shared_resources": ["Beatmaps", "Skins"],
        "triggers": {"file_watcher": true, "on_game_launch": false, "manual": true, "watcher_interval_secs": 7},
        "use_junctions": true,
        "track_manifest": true
    }"#;

    #[test]
    fn retired_modes_load_as_disabled_with_a_notice() {
        for name in ["StableMaster", "LazerMaster", "TrueUnified"] {
            let json = TRUE_UNIFIED.replace("TrueUnified", name);
            let config: UnifiedStorageConfig = serde_json::from_str(&json).unwrap();
            assert_eq!(config.mode, UnifiedStorageMode::Disabled);
            assert_eq!(config.retired_mode.as_deref(), Some(name));
            assert_eq!(
                config.retired_mode_notice(),
                Some(retired_mode_notice(name))
            );
            assert!(retired_mode_notice(name).contains(&format!("\"{name}\" was removed")));
            assert_eq!(config.triggers.watcher_interval_secs, 7);
        }
    }

    #[test]
    fn choosing_a_mode_clears_the_retired_one_and_keeps_the_triggers() {
        let mut config: UnifiedStorageConfig = serde_json::from_str(TRUE_UNIFIED).unwrap();
        config.set_mode(UnifiedStorageMode::LinkedStore);
        assert_eq!(config.mode, UnifiedStorageMode::LinkedStore);
        assert_eq!(config.retired_mode_notice(), None);
        assert_eq!(config.triggers.watcher_interval_secs, 7);
    }

    #[test]
    fn linked_store_round_trips_without_a_notice() {
        let json = serde_json::to_string(&UnifiedStorageConfig::linked_store()).unwrap();
        assert_eq!(
            json,
            r#"{"mode":"LinkedStore","triggers":{"watcher_interval_secs":5}}"#
        );
        let config: UnifiedStorageConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, UnifiedStorageConfig::linked_store());
    }

    #[test]
    fn an_unknown_or_missing_mode_never_fails_to_load() {
        let config: UnifiedStorageConfig = serde_json::from_str(r#"{"mode":"Mirror"}"#).unwrap();
        assert_eq!(config.mode, UnifiedStorageMode::Disabled);
        assert_eq!(config.retired_mode.as_deref(), Some("Mirror"));

        let config: UnifiedStorageConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(config, UnifiedStorageConfig::default());
    }

    #[test]
    fn the_config_screen_offers_two_modes() {
        let labels: Vec<_> = UnifiedStorageMode::ALL.iter().map(|m| m.label()).collect();
        assert_eq!(labels, ["Linked store", "Disabled"]);
    }
}
