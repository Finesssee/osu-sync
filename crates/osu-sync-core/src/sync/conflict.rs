//! Conflict resolution for beatmap synchronization

use crate::dedup::{DuplicateAction, DuplicateInfo, DuplicateResolution};

/// Trait for resolving conflicts when duplicate beatmaps are detected
pub trait ConflictResolver: Send + Sync {
    /// Resolve a conflict for a detected duplicate
    ///
    /// Returns the resolution action to take for this duplicate.
    fn resolve(&self, duplicate: &DuplicateInfo) -> DuplicateResolution;

    /// Called when a batch of duplicates needs resolution
    ///
    /// Default implementation calls `resolve` for each duplicate.
    fn resolve_batch(&self, duplicates: &[DuplicateInfo]) -> Vec<DuplicateResolution> {
        duplicates.iter().map(|d| self.resolve(d)).collect()
    }

    /// Human-readable name for this resolver
    fn name(&self) -> &'static str;
}

/// Interactive conflict resolver that prompts the user for each conflict
///
/// This resolver uses a callback function to get user input for each duplicate.
pub struct InteractiveResolver<F>
where
    F: Fn(&DuplicateInfo) -> DuplicateResolution + Send + Sync,
{
    callback: F,
}

impl<F> InteractiveResolver<F>
where
    F: Fn(&DuplicateInfo) -> DuplicateResolution + Send + Sync,
{
    /// Create a new interactive resolver with the given callback
    pub fn new(callback: F) -> Self {
        Self { callback }
    }
}

impl<F> ConflictResolver for InteractiveResolver<F>
where
    F: Fn(&DuplicateInfo) -> DuplicateResolution + Send + Sync,
{
    fn resolve(&self, duplicate: &DuplicateInfo) -> DuplicateResolution {
        (self.callback)(duplicate)
    }

    fn name(&self) -> &'static str {
        "interactive"
    }
}

/// Automatic conflict resolver that uses a predefined action for all conflicts
pub struct AutoResolver {
    action: DuplicateAction,
}

impl AutoResolver {
    /// Create a new auto resolver with the given action
    pub fn new(action: DuplicateAction) -> Self {
        Self { action }
    }

    /// Create an auto resolver that skips all duplicates
    pub fn skip_all() -> Self {
        Self::new(DuplicateAction::Skip)
    }

    /// Create an auto resolver that replaces all duplicates
    pub fn replace_all() -> Self {
        Self::new(DuplicateAction::Replace)
    }

    /// Create an auto resolver that keeps both versions
    pub fn keep_both() -> Self {
        Self::new(DuplicateAction::KeepBoth)
    }
}

impl ConflictResolver for AutoResolver {
    fn resolve(&self, _duplicate: &DuplicateInfo) -> DuplicateResolution {
        DuplicateResolution {
            action: self.action,
            apply_to_all: true,
        }
    }

    fn name(&self) -> &'static str {
        match self.action {
            DuplicateAction::Skip => "auto-skip",
            DuplicateAction::Replace => "auto-replace",
            DuplicateAction::KeepBoth => "auto-keep-both",
        }
    }
}

/// A resolver that remembers decisions and applies them to similar conflicts
pub struct SmartResolver<F>
where
    F: Fn(&DuplicateInfo) -> DuplicateResolution + Send + Sync,
{
    callback: F,
    /// Remembered decisions by match type
    remembered: std::sync::RwLock<Option<DuplicateResolution>>,
}

impl<F> SmartResolver<F>
where
    F: Fn(&DuplicateInfo) -> DuplicateResolution + Send + Sync,
{
    /// Create a new smart resolver with the given callback
    pub fn new(callback: F) -> Self {
        Self {
            callback,
            remembered: std::sync::RwLock::new(None),
        }
    }
}

impl<F> ConflictResolver for SmartResolver<F>
where
    F: Fn(&DuplicateInfo) -> DuplicateResolution + Send + Sync,
{
    fn resolve(&self, duplicate: &DuplicateInfo) -> DuplicateResolution {
        // Check if we have a remembered decision
        if let Ok(guard) = self.remembered.read() {
            if let Some(ref resolution) = *guard {
                if resolution.apply_to_all {
                    return resolution.clone();
                }
            }
        }

        // Get new resolution from callback
        let resolution = (self.callback)(duplicate);

        // Remember if apply_to_all is set
        if resolution.apply_to_all {
            if let Ok(mut guard) = self.remembered.write() {
                *guard = Some(resolution.clone());
            }
        }

        resolution
    }

    fn name(&self) -> &'static str {
        "smart"
    }
}

/// Default resolver that uses configuration to determine action
pub struct ConfigBasedResolver {
    strategy: crate::config::DuplicateStrategy,
}

impl ConfigBasedResolver {
    /// Create a new config-based resolver
    pub fn new(strategy: crate::config::DuplicateStrategy) -> Self {
        Self { strategy }
    }
}

impl ConflictResolver for ConfigBasedResolver {
    fn resolve(&self, _duplicate: &DuplicateInfo) -> DuplicateResolution {
        let action = match self.strategy {
            crate::config::DuplicateStrategy::Skip => DuplicateAction::Skip,
            crate::config::DuplicateStrategy::Replace => DuplicateAction::Replace,
            crate::config::DuplicateStrategy::KeepBoth => DuplicateAction::KeepBoth,
            crate::config::DuplicateStrategy::Ask => {
                // Default to skip when no interactive resolver is available
                DuplicateAction::Skip
            }
        };

        DuplicateResolution {
            action,
            apply_to_all: true,
        }
    }

    fn name(&self) -> &'static str {
        "config-based"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dedup::{BeatmapSetRef, MatchType};

    fn make_duplicate() -> DuplicateInfo {
        DuplicateInfo {
            source: BeatmapSetRef {
                set_id: Some(123),
                title: "Test Song".to_string(),
                artist: "Test Artist".to_string(),
                creator: "Mapper".to_string(),
                hash: Some("abc123".to_string()),
            },
            existing: BeatmapSetRef {
                set_id: Some(123),
                title: "Test Song".to_string(),
                artist: "Test Artist".to_string(),
                creator: "Mapper".to_string(),
                hash: Some("abc123".to_string()),
            },
            match_type: MatchType::ExactHash,
            confidence: 1.0,
        }
    }

    #[test]
    fn test_auto_resolver_skip() {
        let resolver = AutoResolver::skip_all();
        let resolution = resolver.resolve(&make_duplicate());
        assert_eq!(resolution.action, DuplicateAction::Skip);
        assert!(resolution.apply_to_all);
    }

    #[test]
    fn test_auto_resolver_replace() {
        let resolver = AutoResolver::replace_all();
        let resolution = resolver.resolve(&make_duplicate());
        assert_eq!(resolution.action, DuplicateAction::Replace);
    }

    #[test]
    fn test_interactive_resolver() {
        let resolver = InteractiveResolver::new(|_| DuplicateResolution::keep_both());
        let resolution = resolver.resolve(&make_duplicate());
        assert_eq!(resolution.action, DuplicateAction::KeepBoth);
    }

    #[test]
    fn test_config_based_resolver() {
        let resolver = ConfigBasedResolver::new(crate::config::DuplicateStrategy::Replace);
        let resolution = resolver.resolve(&make_duplicate());
        assert_eq!(resolution.action, DuplicateAction::Replace);
    }
}
