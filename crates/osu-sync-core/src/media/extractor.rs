//! Media extraction implementation

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use md5::{Md5, Digest};

use crate::beatmap::BeatmapSet;
use crate::error::Result;
use crate::lazer::{LazerBeatmapSet, LazerFileStore};

use super::types::{
    ExtractionProgress, ExtractionProgressCallback, ExtractionResult, MediaType,
    OutputOrganization,
};

/// Extractor for audio and background files from beatmaps
pub struct MediaExtractor {
    output_dir: PathBuf,
    organization: OutputOrganization,
    media_type: MediaType,
    /// Track extracted file hashes to avoid duplicates
    extracted_hashes: HashSet<String>,
}

impl MediaExtractor {
    /// Create a new media extractor
    pub fn new(output_dir: impl AsRef<Path>) -> Self {
        Self {
            output_dir: output_dir.as_ref().to_path_buf(),
            organization: OutputOrganization::default(),
            media_type: MediaType::default(),
            extracted_hashes: HashSet::new(),
        }
    }

    /// Set the output organization mode
    pub fn with_organization(mut self, organization: OutputOrganization) -> Self {
        self.organization = organization;
        self
    }

    /// Set the media type to extract
    pub fn with_media_type(mut self, media_type: MediaType) -> Self {
        self.media_type = media_type;
        self
    }

    /// Extract media from osu!stable beatmap sets
    pub fn extract_from_stable(
        &mut self,
        songs_path: &Path,
        sets: &[BeatmapSet],
        progress_callback: Option<ExtractionProgressCallback>,
    ) -> Result<ExtractionResult> {
        let mut result = ExtractionResult::new();
        let total_sets = sets.len();

        for (idx, set) in sets.iter().enumerate() {
            let set_name = set
                .metadata()
                .map(|m| format!("{} - {}", m.artist, m.title))
                .unwrap_or_else(|| "Unknown".to_string());

            // Get the beatmap folder path
            let folder_path = if let Some(ref folder_name) = set.folder_name {
                songs_path.join(folder_name)
            } else {
                continue;
            };

            if !folder_path.exists() {
                continue;
            }

            // Extract audio files
            if self.should_extract_audio() {
                for beatmap in &set.beatmaps {
                    if beatmap.audio_file.is_empty() {
                        continue;
                    }

                    let audio_path = folder_path.join(&beatmap.audio_file);
                    if !audio_path.exists() {
                        continue;
                    }

                    if let Some(ref cb) = progress_callback {
                        cb(ExtractionProgress {
                            current_set: set_name.clone(),
                            current_file: beatmap.audio_file.clone(),
                            sets_processed: idx,
                            total_sets,
                            files_extracted: result.total_extracted(),
                            bytes_written: result.bytes_written,
                        });
                    }

                    match self.extract_file(&audio_path, &set_name, &beatmap.audio_file, true) {
                        Ok(Some(bytes)) => {
                            result.audio_extracted += 1;
                            result.bytes_written += bytes;
                        }
                        Ok(None) => {
                            result.duplicates_skipped += 1;
                        }
                        Err(e) => {
                            result.errors.push((audio_path.display().to_string(), e.to_string()));
                        }
                    }

                    // Only extract audio once per set (they all share the same audio)
                    break;
                }
            }

            // Extract background files
            if self.should_extract_backgrounds() {
                for beatmap in &set.beatmaps {
                    if let Some(ref bg_file) = beatmap.background_file {
                        let bg_path = folder_path.join(bg_file);
                        if !bg_path.exists() {
                            continue;
                        }

                        if let Some(ref cb) = progress_callback {
                            cb(ExtractionProgress {
                                current_set: set_name.clone(),
                                current_file: bg_file.clone(),
                                sets_processed: idx,
                                total_sets,
                                files_extracted: result.total_extracted(),
                                bytes_written: result.bytes_written,
                            });
                        }

                        match self.extract_file(&bg_path, &set_name, bg_file, false) {
                            Ok(Some(bytes)) => {
                                result.backgrounds_extracted += 1;
                                result.bytes_written += bytes;
                            }
                            Ok(None) => {
                                result.duplicates_skipped += 1;
                            }
                            Err(e) => {
                                result.errors.push((bg_path.display().to_string(), e.to_string()));
                            }
                        }

                        // Only extract background once per set
                        break;
                    }
                }
            }
        }

        Ok(result)
    }

    /// Extract media from osu!lazer beatmap sets
    pub fn extract_from_lazer(
        &mut self,
        file_store: &LazerFileStore,
        sets: &[LazerBeatmapSet],
        progress_callback: Option<ExtractionProgressCallback>,
    ) -> Result<ExtractionResult> {
        let mut result = ExtractionResult::new();
        let total_sets = sets.len();

        for (idx, set) in sets.iter().enumerate() {
            let set_name = set
                .beatmaps
                .first()
                .map(|b| format!("{} - {}", b.metadata.artist, b.metadata.title))
                .unwrap_or_else(|| "Unknown".to_string());

            // Find audio and background files from the file list
            let mut audio_extracted = false;
            let mut bg_extracted = false;

            for file in &set.files {
                let is_audio = Self::is_audio_file(&file.filename);
                let is_background = Self::is_image_file(&file.filename);

                if is_audio && self.should_extract_audio() && !audio_extracted {
                    if let Some(ref cb) = progress_callback {
                        cb(ExtractionProgress {
                            current_set: set_name.clone(),
                            current_file: file.filename.clone(),
                            sets_processed: idx,
                            total_sets,
                            files_extracted: result.total_extracted(),
                            bytes_written: result.bytes_written,
                        });
                    }

                    match self.extract_lazer_file(file_store, &file.hash, &set_name, &file.filename, true) {
                        Ok(Some(bytes)) => {
                            result.audio_extracted += 1;
                            result.bytes_written += bytes;
                            audio_extracted = true;
                        }
                        Ok(None) => {
                            result.duplicates_skipped += 1;
                        }
                        Err(e) => {
                            result.errors.push((file.filename.clone(), e.to_string()));
                        }
                    }
                }

                if is_background && self.should_extract_backgrounds() && !bg_extracted {
                    if let Some(ref cb) = progress_callback {
                        cb(ExtractionProgress {
                            current_set: set_name.clone(),
                            current_file: file.filename.clone(),
                            sets_processed: idx,
                            total_sets,
                            files_extracted: result.total_extracted(),
                            bytes_written: result.bytes_written,
                        });
                    }

                    match self.extract_lazer_file(file_store, &file.hash, &set_name, &file.filename, false) {
                        Ok(Some(bytes)) => {
                            result.backgrounds_extracted += 1;
                            result.bytes_written += bytes;
                            bg_extracted = true;
                        }
                        Ok(None) => {
                            result.duplicates_skipped += 1;
                        }
                        Err(e) => {
                            result.errors.push((file.filename.clone(), e.to_string()));
                        }
                    }
                }
            }
        }

        Ok(result)
    }

    /// Extract a file from stable (filesystem)
    fn extract_file(
        &mut self,
        source_path: &Path,
        set_name: &str,
        filename: &str,
        is_audio: bool,
    ) -> Result<Option<u64>> {
        // Read file and compute hash for deduplication
        let mut file = File::open(source_path)?;
        let mut content = Vec::new();
        file.read_to_end(&mut content)?;

        let hash = format!("{:x}", Md5::digest(&content));
        if self.extracted_hashes.contains(&hash) {
            return Ok(None);
        }

        // Determine output path
        let output_path = self.get_output_path(set_name, filename, is_audio);

        // Ensure parent directory exists
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Write file
        let mut output_file = File::create(&output_path)?;
        output_file.write_all(&content)?;

        self.extracted_hashes.insert(hash);
        Ok(Some(content.len() as u64))
    }

    /// Extract a file from lazer (file store)
    fn extract_lazer_file(
        &mut self,
        file_store: &LazerFileStore,
        hash: &str,
        set_name: &str,
        filename: &str,
        is_audio: bool,
    ) -> Result<Option<u64>> {
        // Check for duplicates using the lazer hash
        if self.extracted_hashes.contains(hash) {
            return Ok(None);
        }

        // Read from file store
        let content = file_store.read(hash)?;

        // Determine output path
        let output_path = self.get_output_path(set_name, filename, is_audio);

        // Ensure parent directory exists
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Write file
        let mut output_file = File::create(&output_path)?;
        output_file.write_all(&content)?;

        self.extracted_hashes.insert(hash.to_string());
        Ok(Some(content.len() as u64))
    }

    /// Get the output path based on organization mode
    fn get_output_path(&self, set_name: &str, filename: &str, is_audio: bool) -> PathBuf {
        let sanitized_name = sanitize_filename(set_name);
        let ext = Path::new(filename)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        match self.organization {
            OutputOrganization::Flat => {
                // Use set name as filename for audio
                if is_audio {
                    self.output_dir.join(format!("{}.{}", sanitized_name, ext))
                } else {
                    self.output_dir.join(format!("{}_bg.{}", sanitized_name, ext))
                }
            }
            OutputOrganization::ByArtist => {
                // Extract artist from set name (format: "Artist - Title")
                let artist = set_name.split(" - ").next().unwrap_or("Unknown");
                let sanitized_artist = sanitize_filename(artist);
                let subdir = self.output_dir.join(&sanitized_artist);

                if is_audio {
                    subdir.join(format!("{}.{}", sanitized_name, ext))
                } else {
                    subdir.join(format!("{}_bg.{}", sanitized_name, ext))
                }
            }
            OutputOrganization::ByBeatmap => {
                let subdir = self.output_dir.join(&sanitized_name);
                subdir.join(filename)
            }
        }
    }

    fn should_extract_audio(&self) -> bool {
        matches!(self.media_type, MediaType::Audio | MediaType::Both)
    }

    fn should_extract_backgrounds(&self) -> bool {
        matches!(self.media_type, MediaType::Backgrounds | MediaType::Both)
    }

    fn is_audio_file(filename: &str) -> bool {
        let lower = filename.to_lowercase();
        lower.ends_with(".mp3") || lower.ends_with(".ogg") || lower.ends_with(".wav")
    }

    fn is_image_file(filename: &str) -> bool {
        let lower = filename.to_lowercase();
        lower.ends_with(".jpg") || lower.ends_with(".jpeg") || lower.ends_with(".png")
    }
}

/// Sanitize a filename by removing invalid characters
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("normal_name"), "normal_name");
        assert_eq!(sanitize_filename("path/with/slashes"), "path_with_slashes");
        assert_eq!(sanitize_filename("file:name"), "file_name");
        assert_eq!(sanitize_filename("file*name?"), "file_name_");
        assert_eq!(sanitize_filename("file<>|name"), "file___name");
        assert_eq!(sanitize_filename("\"quoted\""), "_quoted_");
        assert_eq!(sanitize_filename("Artist\\Song"), "Artist_Song");
    }

    #[test]
    fn test_is_audio_file() {
        assert!(MediaExtractor::is_audio_file("song.mp3"));
        assert!(MediaExtractor::is_audio_file("song.MP3"));
        assert!(MediaExtractor::is_audio_file("song.ogg"));
        assert!(MediaExtractor::is_audio_file("song.OGG"));
        assert!(MediaExtractor::is_audio_file("song.wav"));
        assert!(MediaExtractor::is_audio_file("song.WAV"));
        assert!(!MediaExtractor::is_audio_file("image.jpg"));
        assert!(!MediaExtractor::is_audio_file("image.png"));
        assert!(!MediaExtractor::is_audio_file("document.txt"));
    }

    #[test]
    fn test_is_image_file() {
        assert!(MediaExtractor::is_image_file("bg.jpg"));
        assert!(MediaExtractor::is_image_file("bg.JPG"));
        assert!(MediaExtractor::is_image_file("bg.jpeg"));
        assert!(MediaExtractor::is_image_file("bg.JPEG"));
        assert!(MediaExtractor::is_image_file("bg.png"));
        assert!(MediaExtractor::is_image_file("bg.PNG"));
        assert!(!MediaExtractor::is_image_file("song.mp3"));
        assert!(!MediaExtractor::is_image_file("file.osu"));
    }

    #[test]
    fn test_should_extract_audio() {
        let extractor_audio = MediaExtractor::new("/tmp").with_media_type(MediaType::Audio);
        assert!(extractor_audio.should_extract_audio());
        assert!(!extractor_audio.should_extract_backgrounds());

        let extractor_bg = MediaExtractor::new("/tmp").with_media_type(MediaType::Backgrounds);
        assert!(!extractor_bg.should_extract_audio());
        assert!(extractor_bg.should_extract_backgrounds());

        let extractor_both = MediaExtractor::new("/tmp").with_media_type(MediaType::Both);
        assert!(extractor_both.should_extract_audio());
        assert!(extractor_both.should_extract_backgrounds());
    }

    #[test]
    fn test_get_output_path_flat() {
        let extractor = MediaExtractor::new("/output").with_organization(OutputOrganization::Flat);

        let audio_path = extractor.get_output_path("Artist - Song", "audio.mp3", true);
        assert!(audio_path.to_string_lossy().contains("Artist - Song.mp3"));

        let bg_path = extractor.get_output_path("Artist - Song", "bg.jpg", false);
        assert!(bg_path.to_string_lossy().contains("Artist - Song_bg.jpg"));
    }

    #[test]
    fn test_get_output_path_by_artist() {
        let extractor = MediaExtractor::new("/output").with_organization(OutputOrganization::ByArtist);

        let audio_path = extractor.get_output_path("TestArtist - TestSong", "audio.mp3", true);
        assert!(audio_path.to_string_lossy().contains("TestArtist"));
    }

    #[test]
    fn test_get_output_path_by_beatmap() {
        let extractor = MediaExtractor::new("/output").with_organization(OutputOrganization::ByBeatmap);

        let audio_path = extractor.get_output_path("Artist - Song", "audio.mp3", true);
        assert!(audio_path.to_string_lossy().contains("Artist - Song"));
        assert!(audio_path.to_string_lossy().contains("audio.mp3"));
    }

    #[test]
    fn test_builder_pattern() {
        let extractor = MediaExtractor::new("/output")
            .with_organization(OutputOrganization::ByArtist)
            .with_media_type(MediaType::Audio);

        assert!(extractor.should_extract_audio());
        assert!(!extractor.should_extract_backgrounds());
    }

    #[test]
    fn test_extract_from_stable_empty_sets() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut extractor = MediaExtractor::new(temp_dir.path());

        let result = extractor.extract_from_stable(temp_dir.path(), &[], None).unwrap();
        assert_eq!(result.audio_extracted, 0);
        assert_eq!(result.backgrounds_extracted, 0);
        assert_eq!(result.duplicates_skipped, 0);
    }
}
