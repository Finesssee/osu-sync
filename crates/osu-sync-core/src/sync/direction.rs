//! Sync direction types

use std::fmt;

/// Direction of beatmap synchronization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SyncDirection {
    /// Sync beatmaps from osu!stable to osu!lazer
    StableToLazer,
    /// Sync beatmaps from osu!lazer to osu!stable
    LazerToStable,
    /// Sync beatmaps in both directions (merge)
    Bidirectional,
}

impl SyncDirection {
    /// Returns true if this direction syncs from stable
    pub fn syncs_from_stable(&self) -> bool {
        matches!(self, Self::StableToLazer | Self::Bidirectional)
    }

    /// Returns true if this direction syncs from lazer
    pub fn syncs_from_lazer(&self) -> bool {
        matches!(self, Self::LazerToStable | Self::Bidirectional)
    }

    /// Returns the source name for display purposes
    pub fn source_name(&self) -> &'static str {
        match self {
            Self::StableToLazer => "osu!stable",
            Self::LazerToStable => "osu!lazer",
            Self::Bidirectional => "both",
        }
    }

    /// Returns the destination name for display purposes
    pub fn destination_name(&self) -> &'static str {
        match self {
            Self::StableToLazer => "osu!lazer",
            Self::LazerToStable => "osu!stable",
            Self::Bidirectional => "both",
        }
    }
}

impl Default for SyncDirection {
    fn default() -> Self {
        Self::StableToLazer
    }
}

impl fmt::Display for SyncDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StableToLazer => write!(f, "stable -> lazer"),
            Self::LazerToStable => write!(f, "lazer -> stable"),
            Self::Bidirectional => write!(f, "stable <-> lazer"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display() {
        assert_eq!(SyncDirection::StableToLazer.to_string(), "stable -> lazer");
        assert_eq!(SyncDirection::LazerToStable.to_string(), "lazer -> stable");
        assert_eq!(SyncDirection::Bidirectional.to_string(), "stable <-> lazer");
    }

    #[test]
    fn test_sync_directions() {
        assert!(SyncDirection::StableToLazer.syncs_from_stable());
        assert!(!SyncDirection::StableToLazer.syncs_from_lazer());

        assert!(!SyncDirection::LazerToStable.syncs_from_stable());
        assert!(SyncDirection::LazerToStable.syncs_from_lazer());

        assert!(SyncDirection::Bidirectional.syncs_from_stable());
        assert!(SyncDirection::Bidirectional.syncs_from_lazer());
    }
}
