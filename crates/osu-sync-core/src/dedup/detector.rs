//! Duplicate detection logic

use crate::beatmap::BeatmapSet;
use crate::dedup::DuplicateStrategy;
use std::collections::HashSet;

/// Information about a detected duplicate
#[derive(Debug, Clone)]
pub struct DuplicateInfo {
    /// The source beatmap (being imported)
    pub source: BeatmapSetRef,
    /// The existing beatmap (already present)
    pub existing: BeatmapSetRef,
    /// How the duplicate was detected
    pub match_type: MatchType,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f32,
}

/// Reference to a beatmap set
#[derive(Debug, Clone)]
pub struct BeatmapSetRef {
    pub set_id: Option<i32>,
    pub title: String,
    pub artist: String,
    pub creator: String,
    pub hash: Option<String>,
}

impl From<&BeatmapSet> for BeatmapSetRef {
    fn from(set: &BeatmapSet) -> Self {
        let metadata = set.metadata();
        Self {
            set_id: set.id,
            title: metadata.map(|m| m.title.clone()).unwrap_or_default(),
            artist: metadata.map(|m| m.artist.clone()).unwrap_or_default(),
            creator: metadata.map(|m| m.creator.clone()).unwrap_or_default(),
            hash: set.beatmaps.first().map(|b| b.md5_hash.clone()),
        }
    }
}

/// How a duplicate was detected
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchType {
    /// Exact MD5 hash match
    ExactHash,
    /// Same online beatmap set ID
    SameSetId,
    /// Same online beatmap ID
    SameBeatmapId,
    /// Title + Artist + Creator match
    Metadata,
    /// Partial/fuzzy match
    Similar(u8), // Similarity percentage
}

/// Detector for finding duplicate beatmaps
pub struct DuplicateDetector {
    strategy: DuplicateStrategy,
}

impl DuplicateDetector {
    /// Create a new detector with the given strategy
    pub fn new(strategy: DuplicateStrategy) -> Self {
        Self { strategy }
    }

    /// Check if a beatmap set already exists in the target index
    pub fn find_duplicate(
        &self,
        source: &BeatmapSet,
        existing_sets: &[BeatmapSet],
    ) -> Option<DuplicateInfo> {
        match self.strategy {
            DuplicateStrategy::ByHash => self.find_by_hash(source, existing_sets),
            DuplicateStrategy::BySetId => self.find_by_set_id(source, existing_sets),
            DuplicateStrategy::ByMetadata => self.find_by_metadata(source, existing_sets),
            DuplicateStrategy::Composite => self.find_composite(source, existing_sets),
        }
    }

    /// Find duplicates by MD5 hash
    fn find_by_hash(&self, source: &BeatmapSet, existing: &[BeatmapSet]) -> Option<DuplicateInfo> {
        for source_beatmap in &source.beatmaps {
            for existing_set in existing {
                for existing_beatmap in &existing_set.beatmaps {
                    if source_beatmap.md5_hash == existing_beatmap.md5_hash {
                        return Some(DuplicateInfo {
                            source: source.into(),
                            existing: existing_set.into(),
                            match_type: MatchType::ExactHash,
                            confidence: 1.0,
                        });
                    }
                }
            }
        }
        None
    }

    /// Find duplicates by beatmap set ID
    fn find_by_set_id(
        &self,
        source: &BeatmapSet,
        existing: &[BeatmapSet],
    ) -> Option<DuplicateInfo> {
        if let Some(source_id) = source.id {
            for existing_set in existing {
                if existing_set.id == Some(source_id) {
                    return Some(DuplicateInfo {
                        source: source.into(),
                        existing: existing_set.into(),
                        match_type: MatchType::SameSetId,
                        confidence: 0.95,
                    });
                }
            }
        }
        None
    }

    /// Find duplicates by metadata (title + artist + creator)
    fn find_by_metadata(
        &self,
        source: &BeatmapSet,
        existing: &[BeatmapSet],
    ) -> Option<DuplicateInfo> {
        let source_meta = source.metadata()?;

        for existing_set in existing {
            if let Some(existing_meta) = existing_set.metadata() {
                if source_meta.matches(existing_meta) {
                    return Some(DuplicateInfo {
                        source: source.into(),
                        existing: existing_set.into(),
                        match_type: MatchType::Metadata,
                        confidence: 0.8,
                    });
                }
            }
        }
        None
    }

    /// Composite detection: try all methods
    fn find_composite(
        &self,
        source: &BeatmapSet,
        existing: &[BeatmapSet],
    ) -> Option<DuplicateInfo> {
        // Try in order of confidence
        self.find_by_hash(source, existing)
            .or_else(|| self.find_by_set_id(source, existing))
            .or_else(|| self.find_by_metadata(source, existing))
    }

    /// Find all duplicates in a list of beatmaps to import
    pub fn find_all_duplicates(
        &self,
        sources: &[BeatmapSet],
        existing: &[BeatmapSet],
    ) -> Vec<DuplicateInfo> {
        sources
            .iter()
            .filter_map(|source| self.find_duplicate(source, existing))
            .collect()
    }
}

/// Pre-built index for O(1) duplicate lookups
/// This is MUCH faster than the linear scan for large collections
pub struct DuplicateIndex {
    /// Set IDs that exist in the target
    set_ids: HashSet<i32>,
    /// MD5 hashes that exist in the target
    md5_hashes: HashSet<String>,
    /// Metadata key (lowercase title|artist|creator) -> set index
    metadata_keys: HashSet<String>,
}

impl DuplicateIndex {
    /// Build an index from existing beatmap sets
    /// This is O(n) but only needs to be done once
    pub fn build(existing: &[BeatmapSet]) -> Self {
        let mut set_ids = HashSet::with_capacity(existing.len());
        let mut md5_hashes = HashSet::with_capacity(existing.len() * 5); // estimate 5 diffs per set
        let mut metadata_keys = HashSet::with_capacity(existing.len());

        for set in existing {
            // Index by set ID
            if let Some(id) = set.id {
                set_ids.insert(id);
            }

            // Index by MD5 hashes of all difficulties
            for beatmap in &set.beatmaps {
                if !beatmap.md5_hash.is_empty() {
                    md5_hashes.insert(beatmap.md5_hash.clone());
                }
            }

            // Index by metadata
            if let Some(meta) = set.metadata() {
                let key = format!(
                    "{}|{}|{}",
                    meta.title.to_lowercase(),
                    meta.artist.to_lowercase(),
                    meta.creator.to_lowercase()
                );
                metadata_keys.insert(key);
            }
        }

        Self {
            set_ids,
            md5_hashes,
            metadata_keys,
        }
    }

    /// Check if a set ID exists (O(1))
    #[inline]
    pub fn has_set_id(&self, id: i32) -> bool {
        self.set_ids.contains(&id)
    }

    /// Check if any MD5 hash from the source exists (O(k) where k = difficulties)
    #[inline]
    pub fn has_any_hash(&self, source: &BeatmapSet) -> bool {
        source
            .beatmaps
            .iter()
            .any(|b| self.md5_hashes.contains(&b.md5_hash))
    }

    /// Check if metadata matches (O(1))
    #[inline]
    pub fn has_metadata(&self, source: &BeatmapSet) -> bool {
        if let Some(meta) = source.metadata() {
            let key = format!(
                "{}|{}|{}",
                meta.title.to_lowercase(),
                meta.artist.to_lowercase(),
                meta.creator.to_lowercase()
            );
            self.metadata_keys.contains(&key)
        } else {
            false
        }
    }

    /// Fast duplicate check - returns true if this is likely a duplicate
    /// Uses O(1) lookups instead of O(n) scans
    #[inline]
    pub fn is_duplicate(&self, source: &BeatmapSet, strategy: DuplicateStrategy) -> bool {
        match strategy {
            DuplicateStrategy::ByHash => self.has_any_hash(source),
            DuplicateStrategy::BySetId => source.id.is_some_and(|id| self.has_set_id(id)),
            DuplicateStrategy::ByMetadata => self.has_metadata(source),
            DuplicateStrategy::Composite => {
                self.has_any_hash(source)
                    || source.id.is_some_and(|id| self.has_set_id(id))
                    || self.has_metadata(source)
            }
        }
    }

    /// Check if set ID exists in target
    #[inline]
    pub fn exists_by_id(&self, id: i32) -> bool {
        self.set_ids.contains(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beatmap::{BeatmapInfo, BeatmapMetadata};

    fn make_set(id: Option<i32>, title: &str, artist: &str, creator: &str) -> BeatmapSet {
        let mut set = BeatmapSet::new();
        set.id = id;
        set.beatmaps.push(BeatmapInfo {
            metadata: BeatmapMetadata {
                title: title.to_string(),
                artist: artist.to_string(),
                creator: creator.to_string(),
                beatmap_set_id: id,
                ..Default::default()
            },
            md5_hash: format!("hash_{}_{}_{}", title, artist, creator),
            ..Default::default()
        });
        set
    }

    #[test]
    fn test_find_by_set_id() {
        let detector = DuplicateDetector::new(DuplicateStrategy::BySetId);

        let source = make_set(Some(123), "Test", "Artist", "Creator");
        let existing = vec![make_set(Some(123), "Test", "Artist", "Creator")];

        let dup = detector.find_duplicate(&source, &existing);
        assert!(dup.is_some());
        assert_eq!(dup.unwrap().match_type, MatchType::SameSetId);
    }

    #[test]
    fn test_find_by_metadata() {
        let detector = DuplicateDetector::new(DuplicateStrategy::ByMetadata);

        let source = make_set(None, "Test Song", "Artist", "Mapper");
        let existing = vec![make_set(None, "test song", "artist", "mapper")];

        let dup = detector.find_duplicate(&source, &existing);
        assert!(dup.is_some());
        assert_eq!(dup.unwrap().match_type, MatchType::Metadata);
    }
}
