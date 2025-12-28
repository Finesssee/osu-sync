//! Media extraction implementation

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use md5::{Digest, Md5};

use crate::beatmap::BeatmapSet;
use crate::error::Result;
use crate::lazer::{LazerBeatmapSet, LazerFileStore};

use super::types::{
    AudioFormat, AudioInfo, AudioMetadata, ExtractionProgress, ExtractionProgressCallback,
    ExtractionResult, ImageSizeCategory, MediaType, OutputOrganization,
};

/// Size of the sample to read for fast hashing (first 1KB)
const FAST_HASH_SAMPLE_SIZE: usize = 1024;

/// Extractor for audio and background files from beatmaps
pub struct MediaExtractor {
    output_dir: PathBuf,
    organization: OutputOrganization,
    media_type: MediaType,
    image_size_category: ImageSizeCategory,
    /// Whether to skip duplicate files
    skip_duplicates: bool,
    /// Whether to create metadata sidecar files for audio
    create_metadata: bool,
    /// Whether to embed ID3v1 tags in MP3 files
    embed_id3_tags: bool,
    /// Track extracted file hashes to avoid duplicates
    extracted_hashes: HashSet<String>,
    /// Track hashes of files already in output directory
    existing_hashes: HashSet<String>,
}

impl MediaExtractor {
    /// Create a new media extractor
    pub fn new(output_dir: impl AsRef<Path>) -> Self {
        Self {
            output_dir: output_dir.as_ref().to_path_buf(),
            organization: OutputOrganization::default(),
            media_type: MediaType::default(),
            image_size_category: ImageSizeCategory::default(),
            skip_duplicates: true, // Enabled by default
            create_metadata: false,
            embed_id3_tags: false,
            extracted_hashes: HashSet::new(),
            existing_hashes: HashSet::new(),
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

    /// Enable/disable metadata sidecar file creation
    ///
    /// When enabled, creates a .txt file alongside each audio file with:
    /// - Artist, Title, Source from beatmap
    /// - Beatmap set ID, difficulty name, mapper
    /// - Audio format, duration, bitrate info
    pub fn with_metadata(mut self, create_metadata: bool) -> Self {
        self.create_metadata = create_metadata;
        self
    }

    /// Enable/disable ID3v1 tag embedding for MP3 files
    ///
    /// When enabled, appends ID3v1 tags (128 bytes) to MP3 files with:
    /// - Title, Artist, Album (source)
    /// - Comment with mapper info
    pub fn with_id3_tags(mut self, embed_tags: bool) -> Self {
        self.embed_id3_tags = embed_tags;
        self
    }

    /// Set the image size category filter
    pub fn with_image_size_category(mut self, category: ImageSizeCategory) -> Self {
        self.image_size_category = category;
        self
    }

    /// Set whether to skip duplicate files
    pub fn with_skip_duplicates(mut self, skip: bool) -> Self {
        self.skip_duplicates = skip;
        self
    }

    /// Compute a fast hash for duplicate detection (first 1KB + file size)
    /// This is much faster than full MD5 for large files while still being effective
    fn compute_fast_hash(content: &[u8]) -> String {
        let size = content.len();
        let sample_size = size.min(FAST_HASH_SAMPLE_SIZE);
        let sample = &content[..sample_size];

        // Hash: first 1KB + file size for uniqueness
        let mut hasher = Md5::new();
        hasher.update(sample);
        hasher.update(size.to_le_bytes());
        format!("{:x}", hasher.finalize())
    }

    /// Compute a fast hash from a file path without reading entire file
    fn compute_fast_hash_from_file(path: &Path) -> Result<String> {
        let mut file = File::open(path)?;
        let metadata = file.metadata()?;
        let size = metadata.len();

        let mut sample = vec![0u8; (size as usize).min(FAST_HASH_SAMPLE_SIZE)];
        file.read_exact(&mut sample)?;

        let mut hasher = Md5::new();
        hasher.update(&sample);
        hasher.update(size.to_le_bytes());
        Ok(format!("{:x}", hasher.finalize()))
    }

    /// Scan existing files in output directory to build hash set for deduplication
    pub fn scan_existing_files(&mut self) -> Result<usize> {
        if !self.output_dir.exists() {
            return Ok(0);
        }

        let mut count = 0;
        self.scan_directory_recursive(&self.output_dir.clone(), &mut count)?;
        Ok(count)
    }

    fn scan_directory_recursive(&mut self, dir: &Path, count: &mut usize) -> Result<()> {
        if !dir.is_dir() {
            return Ok(());
        }

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                self.scan_directory_recursive(&path, count)?;
            } else if path.is_file() {
                // Check if it's a media file we care about
                let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if Self::is_audio_file(filename) || Self::is_image_file(filename) {
                    if let Ok(hash) = Self::compute_fast_hash_from_file(&path) {
                        self.existing_hashes.insert(hash);
                        *count += 1;
                    }
                }
            }
        }
        Ok(())
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
            let metadata = set.metadata();
            let set_name = metadata
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

                    // Build audio metadata from beatmap info
                    let audio_metadata = AudioMetadata {
                        artist: beatmap.metadata.artist.clone(),
                        title: beatmap.metadata.title.clone(),
                        source: beatmap.metadata.source.clone().unwrap_or_default(),
                        beatmap_set_id: beatmap.metadata.beatmap_set_id,
                        difficulty: Some(beatmap.version.clone()),
                        mapper: Some(beatmap.metadata.creator.clone()),
                        audio_info: None, // Will be filled during extraction
                    };

                    match self.extract_file_with_metadata(
                        &audio_path,
                        &set_name,
                        &beatmap.audio_file,
                        true,
                        Some(beatmap.length_ms),
                        audio_metadata,
                        &mut result,
                    ) {
                        Ok(Some(bytes)) => {
                            result.audio_extracted += 1;
                            result.unique_files += 1;
                            result.bytes_written += bytes;
                        }
                        Ok(None) => {
                            result.duplicates_skipped += 1;
                        }
                        Err(e) => {
                            result
                                .errors
                                .push((audio_path.display().to_string(), e.to_string()));
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
                                result.unique_files += 1;
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
            let first_beatmap = set.beatmaps.first();
            let set_name = first_beatmap
                .map(|b| format!("{} - {}", b.metadata.artist, b.metadata.title))
                .unwrap_or_else(|| "Unknown".to_string());

            // Find audio and background files from the file list
            let mut audio_extracted_flag = false;
            let mut bg_extracted = false;

            for file in &set.files {
                let is_audio = Self::is_audio_file(&file.filename);
                let is_background = Self::is_image_file(&file.filename);

                if is_audio && self.should_extract_audio() && !audio_extracted_flag {
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

                    // Build audio metadata from first beatmap
                    let audio_metadata = first_beatmap
                        .map(|b| AudioMetadata {
                            artist: b.metadata.artist.clone(),
                            title: b.metadata.title.clone(),
                            source: b.metadata.source.clone().unwrap_or_default(),
                            beatmap_set_id: b.metadata.beatmap_set_id,
                            difficulty: Some(b.version.clone()),
                            mapper: Some(b.metadata.creator.clone()),
                            audio_info: None,
                        })
                        .unwrap_or_default();

                    let duration_ms = first_beatmap.map(|b| b.length_ms);

                    match self.extract_lazer_file_with_metadata(
                        file_store,
                        &file.hash,
                        &set_name,
                        &file.filename,
                        true,
                        duration_ms,
                        audio_metadata,
                        &mut result,
                    ) {
                        Ok(Some(bytes)) => {
                            result.audio_extracted += 1;
                            result.unique_files += 1;
                            result.bytes_written += bytes;
                            audio_extracted_flag = true;
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

                    match self.extract_lazer_file(
                        file_store,
                        &file.hash,
                        &set_name,
                        &file.filename,
                        false,
                    ) {
                        Ok(Some(bytes)) => {
                            result.backgrounds_extracted += 1;
                            result.unique_files += 1;
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

    /// Extract a file from stable (filesystem) without metadata
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

        // Use fast hash (first 1KB + size) for speed
        let hash = Self::compute_fast_hash(&content);

        // Check if duplicate (either already extracted this session, or exists in output dir)
        if self.skip_duplicates {
            if self.extracted_hashes.contains(&hash) || self.existing_hashes.contains(&hash) {
                return Ok(None);
            }
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

    /// Extract a file from stable with metadata support
    fn extract_file_with_metadata(
        &mut self,
        source_path: &Path,
        set_name: &str,
        filename: &str,
        is_audio: bool,
        duration_ms: Option<u64>,
        mut metadata: AudioMetadata,
        result: &mut ExtractionResult,
    ) -> Result<Option<u64>> {
        // Read file and compute hash for deduplication
        let mut file = File::open(source_path)?;
        let mut content = Vec::new();
        file.read_to_end(&mut content)?;

        // Use fast hash (first 1KB + size) for speed
        let hash = Self::compute_fast_hash(&content);

        // Check if duplicate (either already extracted this session, or exists in output dir)
        if self.skip_duplicates {
            if self.extracted_hashes.contains(&hash) || self.existing_hashes.contains(&hash) {
                return Ok(None);
            }
        }

        // Build audio info
        let audio_info = AudioInfo::from_file_data(filename, &content, duration_ms);
        result.record_audio_info(&audio_info);
        metadata.audio_info = Some(audio_info.clone());

        // Determine output path
        let output_path = self.get_output_path(set_name, filename, is_audio);

        // Ensure parent directory exists
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // For MP3 files with ID3 embedding enabled, append ID3v1 tag
        let final_content = if self.embed_id3_tags
            && is_audio
            && audio_info.format == AudioFormat::Mp3
        {
            let mut new_content = content.clone();
            let id3_tag = metadata.to_id3v1_tag();
            new_content.extend_from_slice(&id3_tag);
            new_content
        } else {
            content.clone()
        };

        // Write file
        let mut output_file = File::create(&output_path)?;
        output_file.write_all(&final_content)?;

        // Create metadata sidecar file if enabled
        if self.create_metadata && is_audio {
            let sidecar_path = output_path.with_extension("txt");
            if let Ok(mut sidecar_file) = File::create(&sidecar_path) {
                let sidecar_content = metadata.to_sidecar_text();
                let _ = sidecar_file.write_all(sidecar_content.as_bytes());
                result.metadata_files_created += 1;
            }
        }

        self.extracted_hashes.insert(hash);
        Ok(Some(final_content.len() as u64))
    }

    /// Extract a file from lazer (file store) without metadata
    fn extract_lazer_file(
        &mut self,
        file_store: &LazerFileStore,
        lazer_hash: &str,
        set_name: &str,
        filename: &str,
        is_audio: bool,
    ) -> Result<Option<u64>> {
        // Read from file store first to compute fast hash
        let content = file_store.read(lazer_hash)?;

        // Use fast hash for deduplication (consistent with stable extraction)
        let hash = Self::compute_fast_hash(&content);

        // Check if duplicate (either already extracted this session, or exists in output dir)
        if self.skip_duplicates {
            if self.extracted_hashes.contains(&hash) || self.existing_hashes.contains(&hash) {
                return Ok(None);
            }
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

    /// Extract a file from lazer with metadata support
    #[allow(clippy::too_many_arguments)]
    fn extract_lazer_file_with_metadata(
        &mut self,
        file_store: &LazerFileStore,
        lazer_hash: &str,
        set_name: &str,
        filename: &str,
        is_audio: bool,
        duration_ms: Option<u64>,
        mut metadata: AudioMetadata,
        result: &mut ExtractionResult,
    ) -> Result<Option<u64>> {
        // Read from file store first to compute fast hash
        let content = file_store.read(lazer_hash)?;

        // Use fast hash for deduplication (consistent with stable extraction)
        let hash = Self::compute_fast_hash(&content);

        // Check if duplicate (either already extracted this session, or exists in output dir)
        if self.skip_duplicates {
            if self.extracted_hashes.contains(&hash) || self.existing_hashes.contains(&hash) {
                return Ok(None);
            }
        }

        // Build audio info
        let audio_info = AudioInfo::from_file_data(filename, &content, duration_ms);
        result.record_audio_info(&audio_info);
        metadata.audio_info = Some(audio_info.clone());

        // Determine output path
        let output_path = self.get_output_path(set_name, filename, is_audio);

        // Ensure parent directory exists
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // For MP3 files with ID3 embedding enabled, append ID3v1 tag
        let final_content = if self.embed_id3_tags
            && is_audio
            && audio_info.format == AudioFormat::Mp3
        {
            let mut new_content = content.clone();
            let id3_tag = metadata.to_id3v1_tag();
            new_content.extend_from_slice(&id3_tag);
            new_content
        } else {
            content.clone()
        };

        // Write file
        let mut output_file = File::create(&output_path)?;
        output_file.write_all(&final_content)?;

        // Create metadata sidecar file if enabled
        if self.create_metadata && is_audio {
            let sidecar_path = output_path.with_extension("txt");
            if let Ok(mut sidecar_file) = File::create(&sidecar_path) {
                let sidecar_content = metadata.to_sidecar_text();
                let _ = sidecar_file.write_all(sidecar_content.as_bytes());
                result.metadata_files_created += 1;
            }
        }

        self.extracted_hashes.insert(hash);
        Ok(Some(final_content.len() as u64))
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
                    self.output_dir
                        .join(format!("{}_bg.{}", sanitized_name, ext))
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
        let extractor =
            MediaExtractor::new("/output").with_organization(OutputOrganization::Flat);

        let audio_path = extractor.get_output_path("Artist - Song", "audio.mp3", true);
        assert!(audio_path.to_string_lossy().contains("Artist - Song.mp3"));

        let bg_path = extractor.get_output_path("Artist - Song", "bg.jpg", false);
        assert!(bg_path.to_string_lossy().contains("Artist - Song_bg.jpg"));
    }

    #[test]
    fn test_get_output_path_by_artist() {
        let extractor =
            MediaExtractor::new("/output").with_organization(OutputOrganization::ByArtist);

        let audio_path = extractor.get_output_path("TestArtist - TestSong", "audio.mp3", true);
        assert!(audio_path.to_string_lossy().contains("TestArtist"));
    }

    #[test]
    fn test_get_output_path_by_beatmap() {
        let extractor =
            MediaExtractor::new("/output").with_organization(OutputOrganization::ByBeatmap);

        let audio_path = extractor.get_output_path("Artist - Song", "audio.mp3", true);
        assert!(audio_path.to_string_lossy().contains("Artist - Song"));
        assert!(audio_path.to_string_lossy().contains("audio.mp3"));
    }

    #[test]
    fn test_builder_pattern() {
        let extractor = MediaExtractor::new("/output")
            .with_organization(OutputOrganization::ByArtist)
            .with_media_type(MediaType::Audio)
            .with_metadata(true)
            .with_id3_tags(true);

        assert!(extractor.should_extract_audio());
        assert!(!extractor.should_extract_backgrounds());
        assert!(extractor.create_metadata);
        assert!(extractor.embed_id3_tags);
    }

    #[test]
    fn test_extract_from_stable_empty_sets() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut extractor = MediaExtractor::new(temp_dir.path());

        let result = extractor
            .extract_from_stable(temp_dir.path(), &[], None)
            .unwrap();
        assert_eq!(result.audio_extracted, 0);
        assert_eq!(result.backgrounds_extracted, 0);
        assert_eq!(result.duplicates_skipped, 0);
    }

    #[test]
    fn test_metadata_builder_defaults() {
        let extractor = MediaExtractor::new("/output");
        assert!(!extractor.create_metadata);
        assert!(!extractor.embed_id3_tags);

        let extractor_with_meta = MediaExtractor::new("/output").with_metadata(true);
        assert!(extractor_with_meta.create_metadata);
        assert!(!extractor_with_meta.embed_id3_tags);

        let extractor_with_id3 = MediaExtractor::new("/output").with_id3_tags(true);
        assert!(!extractor_with_id3.create_metadata);
        assert!(extractor_with_id3.embed_id3_tags);
    }
}
