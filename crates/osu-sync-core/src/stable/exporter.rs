//! Export beatmaps from osu!stable

use crate::beatmap::BeatmapSet;
use crate::error::Result;
use crate::parser::create_osz;
use std::fs;
use std::path::{Path, PathBuf};

/// Exporter for creating .osz files from osu!stable beatmaps
pub struct StableExporter {
    songs_path: PathBuf,
}

impl StableExporter {
    /// Create a new exporter for the given Songs folder
    pub fn new(songs_path: PathBuf) -> Self {
        Self { songs_path }
    }

    /// Export a beatmap set to an .osz file
    pub fn export_to_osz(&self, beatmap_set: &BeatmapSet, output_dir: &Path) -> Result<PathBuf> {
        let folder_name = beatmap_set
            .folder_name
            .as_ref()
            .ok_or_else(|| crate::error::Error::Other("Beatmap set has no folder name".to_string()))?;

        let source_dir = self.songs_path.join(folder_name);

        if !source_dir.exists() {
            return Err(crate::error::Error::BeatmapNotFound(folder_name.clone()));
        }

        // Create output directory if needed
        fs::create_dir_all(output_dir)?;

        // Generate output filename
        let output_name = format!("{}.osz", folder_name);
        let output_path = output_dir.join(&output_name);

        // Create the .osz archive
        create_osz(&source_dir, &output_path)
    }

    /// Export multiple beatmap sets to .osz files
    pub fn export_multiple(
        &self,
        beatmap_sets: &[BeatmapSet],
        output_dir: &Path,
    ) -> Vec<Result<PathBuf>> {
        beatmap_sets
            .iter()
            .map(|set| self.export_to_osz(set, output_dir))
            .collect()
    }

    /// Read all files from a beatmap set folder
    pub fn read_beatmap_files(&self, beatmap_set: &BeatmapSet) -> Result<Vec<(String, Vec<u8>)>> {
        let folder_name = beatmap_set
            .folder_name
            .as_ref()
            .ok_or_else(|| crate::error::Error::Other("Beatmap set has no folder name".to_string()))?;

        let source_dir = self.songs_path.join(folder_name);

        if !source_dir.exists() {
            return Err(crate::error::Error::BeatmapNotFound(folder_name.clone()));
        }

        let mut files = Vec::new();

        for entry in walkdir::WalkDir::new(&source_dir)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if path.is_file() {
                let content = fs::read(path)?;
                let filename = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                files.push((filename, content));
            }
        }

        Ok(files)
    }
}
