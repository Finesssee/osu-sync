//! Duplicate detection strategies

use serde::{Deserialize, Serialize};

/// Strategy for detecting duplicate beatmaps
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DuplicateStrategy {
    /// Match by MD5/SHA-256 hash (exact file match)
    ByHash,
    /// Match by online beatmap set ID
    BySetId,
    /// Match by metadata (title + artist + creator)
    ByMetadata,
    /// Try all methods (hash -> set ID -> metadata)
    #[default]
    Composite,
}

/// Action to take when a duplicate is found
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DuplicateAction {
    /// Skip importing this beatmap
    Skip,
    /// Replace the existing beatmap
    Replace,
    /// Keep both versions (import with different folder name)
    KeepBoth,
}

/// Resolution for a duplicate detection
#[derive(Debug, Clone)]
pub struct DuplicateResolution {
    /// The action to take
    pub action: DuplicateAction,
    /// Whether to apply this action to all similar matches
    pub apply_to_all: bool,
}

impl Default for DuplicateResolution {
    fn default() -> Self {
        Self {
            action: DuplicateAction::Skip,
            apply_to_all: false,
        }
    }
}

impl DuplicateResolution {
    pub fn skip() -> Self {
        Self {
            action: DuplicateAction::Skip,
            apply_to_all: false,
        }
    }

    pub fn replace() -> Self {
        Self {
            action: DuplicateAction::Replace,
            apply_to_all: false,
        }
    }

    pub fn keep_both() -> Self {
        Self {
            action: DuplicateAction::KeepBoth,
            apply_to_all: false,
        }
    }

    pub fn with_apply_to_all(mut self) -> Self {
        self.apply_to_all = true;
        self
    }
}
