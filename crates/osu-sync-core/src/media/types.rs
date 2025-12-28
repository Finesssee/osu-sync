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

/// Image size category for extraction filtering
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ImageSizeCategory {
    /// Keep all images regardless of size
    #[default]
    All,
    /// Only large images (1280x720 or larger)
    LargeOnly,
    /// Only HD images (1920x1080 or larger)
    HdOnly,
    /// Custom minimum size
    Custom(u32, u32),
}

impl ImageSizeCategory {
    /// Check if an image meets the size requirements
    pub fn meets_requirements(&self, width: u32, height: u32) -> bool {
        match self {
            ImageSizeCategory::All => true,
            ImageSizeCategory::LargeOnly => width >= 1280 && height >= 720,
            ImageSizeCategory::HdOnly => width >= 1920 && height >= 1080,
            ImageSizeCategory::Custom(min_w, min_h) => width >= *min_w && height >= *min_h,
        }
    }
}

/// Audio file format
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum AudioFormat {
    #[default]
    Mp3,
    Ogg,
    Wav,
    Unknown,
}

impl AudioFormat {
    /// Detect format from filename extension
    pub fn from_filename(filename: &str) -> Self {
        let lower = filename.to_lowercase();
        if lower.ends_with(".mp3") {
            AudioFormat::Mp3
        } else if lower.ends_with(".ogg") {
            AudioFormat::Ogg
        } else if lower.ends_with(".wav") {
            AudioFormat::Wav
        } else {
            AudioFormat::Unknown
        }
    }
}

/// Audio file information
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AudioInfo {
    /// Audio format
    pub format: AudioFormat,
    /// File size in bytes
    pub file_size: u64,
    /// Duration in milliseconds (if known)
    pub duration_ms: Option<u64>,
    /// Estimated bitrate in kbps (if applicable)
    pub bitrate_kbps: Option<u32>,
}

impl AudioInfo {
    /// Build audio info from file data
    pub fn from_file_data(filename: &str, content: &[u8], duration_ms: Option<u64>) -> Self {
        let format = AudioFormat::from_filename(filename);
        let file_size = content.len() as u64;

        // Estimate bitrate if we have duration
        let bitrate_kbps = duration_ms.filter(|&d| d > 0).map(|d| {
            ((file_size * 8) / (d / 1000).max(1)) as u32 / 1000
        });

        Self {
            format,
            file_size,
            duration_ms,
            bitrate_kbps,
        }
    }
}

/// Audio metadata for ID3 tags and sidecar files
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AudioMetadata {
    /// Artist name
    pub artist: String,
    /// Song title
    pub title: String,
    /// Source (album/game/etc.)
    pub source: String,
    /// Beatmap set ID
    pub beatmap_set_id: Option<i32>,
    /// Difficulty name
    pub difficulty: Option<String>,
    /// Mapper name
    pub mapper: Option<String>,
    /// Audio info (format, duration, etc.)
    pub audio_info: Option<AudioInfo>,
}

impl AudioMetadata {
    /// Generate ID3v1 tag (128 bytes)
    pub fn to_id3v1_tag(&self) -> [u8; 128] {
        let mut tag = [0u8; 128];

        // TAG header
        tag[0..3].copy_from_slice(b"TAG");

        // Title (30 bytes)
        let title_bytes = self.title.as_bytes();
        let title_len = title_bytes.len().min(30);
        tag[3..3 + title_len].copy_from_slice(&title_bytes[..title_len]);

        // Artist (30 bytes)
        let artist_bytes = self.artist.as_bytes();
        let artist_len = artist_bytes.len().min(30);
        tag[33..33 + artist_len].copy_from_slice(&artist_bytes[..artist_len]);

        // Album (30 bytes) - use source
        let album_bytes = self.source.as_bytes();
        let album_len = album_bytes.len().min(30);
        tag[63..63 + album_len].copy_from_slice(&album_bytes[..album_len]);

        // Year (4 bytes) - leave empty

        // Comment (30 bytes) - use mapper info
        if let Some(ref mapper) = self.mapper {
            let comment = format!("Mapper: {}", mapper);
            let comment_bytes = comment.as_bytes();
            let comment_len = comment_bytes.len().min(30);
            tag[97..97 + comment_len].copy_from_slice(&comment_bytes[..comment_len]);
        }

        // Genre (1 byte) - 255 = unknown
        tag[127] = 255;

        tag
    }

    /// Generate sidecar text file content
    pub fn to_sidecar_text(&self) -> String {
        let mut lines = Vec::new();

        lines.push(format!("Artist: {}", self.artist));
        lines.push(format!("Title: {}", self.title));

        if !self.source.is_empty() {
            lines.push(format!("Source: {}", self.source));
        }

        if let Some(set_id) = self.beatmap_set_id {
            lines.push(format!("Beatmap Set ID: {}", set_id));
        }

        if let Some(ref diff) = self.difficulty {
            lines.push(format!("Difficulty: {}", diff));
        }

        if let Some(ref mapper) = self.mapper {
            lines.push(format!("Mapper: {}", mapper));
        }

        if let Some(ref info) = self.audio_info {
            lines.push(String::new());
            lines.push(format!("Format: {:?}", info.format));
            lines.push(format!("File Size: {} bytes", info.file_size));
            if let Some(duration) = info.duration_ms {
                let secs = duration / 1000;
                let mins = secs / 60;
                lines.push(format!("Duration: {}:{:02}", mins, secs % 60));
            }
            if let Some(bitrate) = info.bitrate_kbps {
                lines.push(format!("Bitrate: {} kbps", bitrate));
            }
        }

        lines.join("\n")
    }
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
    /// Number of unique files extracted
    pub unique_files: usize,
    /// Total bytes written
    pub bytes_written: u64,
    /// Number of metadata sidecar files created
    pub metadata_files_created: usize,
    /// Audio format breakdown
    pub audio_by_format: std::collections::HashMap<String, usize>,
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

    /// Record audio info for statistics
    pub fn record_audio_info(&mut self, info: &AudioInfo) {
        let format_str = format!("{:?}", info.format);
        *self.audio_by_format.entry(format_str).or_insert(0) += 1;
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
