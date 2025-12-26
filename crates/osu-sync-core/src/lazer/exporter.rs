//! Export beatmaps from osu!lazer

use crate::error::Result;
use crate::lazer::{LazerBeatmapSet, LazerDatabase};
use crate::parser::create_osz_from_set;
use std::path::{Path, PathBuf};

/// Exporter for extracting beatmaps from osu!lazer
pub struct LazerExporter {
    database: LazerDatabase,
}

impl LazerExporter {
    /// Create a new exporter for the given lazer database
    pub fn new(database: LazerDatabase) -> Self {
        Self { database }
    }

    /// Export a beatmap set to an .osz file
    pub fn export_to_osz(&self, lazer_set: &LazerBeatmapSet, output_dir: &Path) -> Result<PathBuf> {
        // Read all files from the file store
        let files = self.read_set_files(lazer_set)?;

        // Convert to common beatmap set
        let beatmap_set = self.database.to_beatmap_set(lazer_set);

        // Generate output path
        let folder_name = beatmap_set.generate_folder_name();
        let output_path = output_dir.join(format!("{}.osz", folder_name));

        // Create the .osz
        create_osz_from_set(&beatmap_set, &files, &output_path)?;

        Ok(output_path)
    }

    /// Read all files for a beatmap set from the file store
    pub fn read_set_files(&self, lazer_set: &LazerBeatmapSet) -> Result<Vec<(String, Vec<u8>)>> {
        let file_store = self.database.file_store();
        let mut files = Vec::new();

        for named_file in &lazer_set.files {
            let content = file_store.read(&named_file.hash)?;
            files.push((named_file.filename.clone(), content));
        }

        Ok(files)
    }

    /// Export a beatmap set directly to osu!stable format (folder with files)
    pub fn export_to_stable_folder(
        &self,
        lazer_set: &LazerBeatmapSet,
        songs_path: &Path,
    ) -> Result<PathBuf> {
        let files = self.read_set_files(lazer_set)?;
        let beatmap_set = self.database.to_beatmap_set(lazer_set);

        // Create folder in Songs directory
        let folder_name = beatmap_set.generate_folder_name();
        let folder_path = songs_path.join(&folder_name);

        std::fs::create_dir_all(&folder_path)?;

        // Write all files
        for (filename, content) in files {
            let file_path = folder_path.join(&filename);
            std::fs::write(file_path, content)?;
        }

        Ok(folder_path)
    }

    /// Export multiple beatmap sets
    pub fn export_multiple(
        &self,
        sets: &[LazerBeatmapSet],
        output_dir: &Path,
    ) -> Vec<Result<PathBuf>> {
        sets.iter()
            .map(|set| self.export_to_osz(set, output_dir))
            .collect()
    }
}
