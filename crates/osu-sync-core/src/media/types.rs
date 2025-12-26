//! Types for media extraction

use serde::{Deserialize, Serialize};

/// How to organize extracted files
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum OutputOrganization {
    /// All files in a single directory
    #[default]
    Flat,
    /// Organize by artist name (Artist/filename)
    ByArtist,
    /// Organize by beatmap (Artist - Title/filename)
    ByBeatmap,
}

/// Type of media to extract
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MediaType {
    /// Extract only audio files
    Audio,
    /// Extract only background images
    Backgrounds,
    /// Extract both audio and backgrounds
    #[default]
    Both,
}

/// Source installation for extraction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ExtractionSource {
    /// Extract from osu!stable
    #[default]
    Stable,
    /// Extract from osu!lazer
    Lazer,
    /// Extract from both installations
    Both,
}

/// Result of a media extraction operation
#[derive(Debug, Clone, Default)]
pub struct ExtractionResult {
    /// Number of audio files extracted
    pub audio_extracted: usize,
    /// Number of background images extracted
    pub backgrounds_extracted: usize,
    /// Number of files skipped due to duplicates
    pub duplicates_skipped: usize,
    /// Total bytes written
    pub bytes_written: u64,
    /// Errors encountered (path, error message)
    pub errors: Vec<(String, String)>,
}

impl ExtractionResult {
    /// Create a new empty result
    pub fn new() -> Self {
        Self::default()
    }

    /// Get total files extracted
    pub fn total_extracted(&self) -> usize {
        self.audio_extracted + self.backgrounds_extracted
    }

    /// Check if there were any errors
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

/// Progress callback for extraction
pub type ExtractionProgressCallback = Box<dyn Fn(ExtractionProgress) + Send + Sync>;

/// Progress information during extraction
#[derive(Debug, Clone)]
pub struct ExtractionProgress {
    /// Current beatmap set being processed
    pub current_set: String,
    /// Current file being processed
    pub current_file: String,
    /// Number of sets processed
    pub sets_processed: usize,
    /// Total sets to process
    pub total_sets: usize,
    /// Files extracted so far
    pub files_extracted: usize,
    /// Bytes written so far
    pub bytes_written: u64,
}

impl ExtractionProgress {
    /// Get progress percentage (0.0 to 100.0)
    pub fn percentage(&self) -> f32 {
        if self.total_sets == 0 {
            0.0
        } else {
            (self.sets_processed as f32 / self.total_sets as f32) * 100.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_organization_default() {
        assert_eq!(OutputOrganization::default(), OutputOrganization::Flat);
    }

    #[test]
    fn test_media_type_default() {
        assert_eq!(MediaType::default(), MediaType::Both);
    }

    #[test]
    fn test_extraction_source_default() {
        assert_eq!(ExtractionSource::default(), ExtractionSource::Stable);
    }

    #[test]
    fn test_extraction_result_new() {
        let result = ExtractionResult::new();
        assert_eq!(result.audio_extracted, 0);
        assert_eq!(result.backgrounds_extracted, 0);
        assert_eq!(result.duplicates_skipped, 0);
        assert_eq!(result.bytes_written, 0);
        assert!(result.errors.is_empty());
    }

    #[test]
    fn test_extraction_result_total_extracted() {
        let mut result = ExtractionResult::new();
        result.audio_extracted = 10;
        result.backgrounds_extracted = 5;
        assert_eq!(result.total_extracted(), 15);
    }

    #[test]
    fn test_extraction_result_has_errors() {
        let mut result = ExtractionResult::new();
        assert!(!result.has_errors());

        result.errors.push(("file.mp3".to_string(), "error".to_string()));
        assert!(result.has_errors());
    }

    #[test]
    fn test_extraction_progress_percentage() {
        let progress = ExtractionProgress {
            current_set: "test".to_string(),
            current_file: "audio.mp3".to_string(),
            sets_processed: 25,
            total_sets: 100,
            files_extracted: 10,
            bytes_written: 1024,
        };
        assert!((progress.percentage() - 25.0).abs() < 0.001);

        let progress_empty = ExtractionProgress {
            current_set: "test".to_string(),
            current_file: "".to_string(),
            sets_processed: 0,
            total_sets: 0,
            files_extracted: 0,
            bytes_written: 0,
        };
        assert!((progress_empty.percentage() - 0.0).abs() < 0.001);

        let progress_complete = ExtractionProgress {
            current_set: "test".to_string(),
            current_file: "".to_string(),
            sets_processed: 50,
            total_sets: 50,
            files_extracted: 100,
            bytes_written: 5000,
        };
        assert!((progress_complete.percentage() - 100.0).abs() < 0.001);
    }
}
