//! Import beatmaps into osu!stable

use crate::beatmap::BeatmapSet;
use crate::error::Result;
use crate::parser::extract_osz;
use std::fs;
use std::path::{Path, PathBuf};

/// Importer for adding beatmaps to osu!stable
pub struct StableImporter {
    songs_path: PathBuf,
}

/// Result of an import operation
#[derive(Debug, Clone)]
pub struct ImportResult {
    pub success: bool,
    pub folder_name: String,
    pub path: PathBuf,
    pub error: Option<String>,
}

impl StableImporter {
    /// Create a new importer for the given Songs folder
    pub fn new(songs_path: PathBuf) -> Self {
        Self { songs_path }
    }

    /// Import a beatmap set from an .osz file
    pub fn import_osz(&self, osz_path: &Path) -> Result<ImportResult> {
        // Create temporary directory for extraction
        let temp_dir = std::env::temp_dir().join(format!("osu-sync-{}", uuid_simple()));

        // Extract the .osz
        let beatmap_set = extract_osz(osz_path, &temp_dir)?;

        // Move to Songs folder
        let result = self.import_extracted(&temp_dir, &beatmap_set)?;

        // Cleanup temp directory
        let _ = fs::remove_dir_all(&temp_dir);

        Ok(result)
    }

    /// Import an already extracted beatmap set
    pub fn import_extracted(&self, source_dir: &Path, beatmap_set: &BeatmapSet) -> Result<ImportResult> {
        let folder_name = beatmap_set
            .folder_name
            .clone()
            .unwrap_or_else(|| beatmap_set.generate_folder_name());

        let dest_path = self.songs_path.join(&folder_name);

        // Check if folder already exists
        if dest_path.exists() {
            return Ok(ImportResult {
                success: false,
                folder_name,
                path: dest_path,
                error: Some("Folder already exists".to_string()),
            });
        }

        // Copy files
        copy_dir_recursive(source_dir, &dest_path)?;

        Ok(ImportResult {
            success: true,
            folder_name,
            path: dest_path,
            error: None,
        })
    }

    /// Import a beatmap set by copying files
    pub fn import_files(
        &self,
        files: &[(String, Vec<u8>)],
        beatmap_set: &BeatmapSet,
    ) -> Result<ImportResult> {
        let folder_name = beatmap_set
            .folder_name
            .clone()
            .unwrap_or_else(|| beatmap_set.generate_folder_name());

        let dest_path = self.songs_path.join(&folder_name);

        // Check if folder already exists
        if dest_path.exists() {
            return Ok(ImportResult {
                success: false,
                folder_name,
                path: dest_path,
                error: Some("Folder already exists".to_string()),
            });
        }

        // Create directory
        fs::create_dir_all(&dest_path)?;

        // Write all files
        for (filename, content) in files {
            let file_path = dest_path.join(filename);
            if let Some(parent) = file_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&file_path, content)?;
        }

        Ok(ImportResult {
            success: true,
            folder_name,
            path: dest_path,
            error: None,
        })
    }
}

/// Generate a simple UUID-like string
fn uuid_simple() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    format!("{:x}{:x}", duration.as_secs(), duration.subsec_nanos())
}

/// Recursively copy a directory
fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    if !dst.exists() {
        fs::create_dir_all(dst)?;
    }

    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }

    Ok(())
}
