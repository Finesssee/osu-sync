//! Duplicate detection logic

use crate::beatmap::BeatmapSet;
use crate::dedup::DuplicateStrategy;

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
    pub fn find_duplicate<'a>(
        &self,
        source: &BeatmapSet,
        existing_sets: &'a [BeatmapSet],
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
    fn find_by_set_id(&self, source: &BeatmapSet, existing: &[BeatmapSet]) -> Option<DuplicateInfo> {
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
    fn find_by_metadata(&self, source: &BeatmapSet, existing: &[BeatmapSet]) -> Option<DuplicateInfo> {
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
    fn find_composite(&self, source: &BeatmapSet, existing: &[BeatmapSet]) -> Option<DuplicateInfo> {
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
