//! Export beatmaps from osu!lazer

use crate::error::Result;
use crate::lazer::{LazerBeatmapSet, LazerDatabase};
use crate::linkstore::{ensure_stable_closed, Materializer, SetReport, StableClaims};
use crate::parser::create_osz_from_set;
use std::ops::ControlFlow;
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

    /// Materializes a beatmap set as a folder in osu!stable's Songs, with assets
    /// hard-linked to the lazer store. Duplicates are checked against the Songs
    /// listing only, since no osu!.db is given.
    pub fn export_to_stable_folder(
        &self,
        lazer_set: &LazerBeatmapSet,
        songs_path: &Path,
    ) -> Result<SetReport> {
        ensure_stable_closed()?;
        let mut claims = StableClaims::from_songs(songs_path)?;
        let materializer = Materializer::new(songs_path, self.database.file_store().files_path());
        let mut report = materializer.run(&[lazer_set], &mut claims, &mut |_, _, _| {
            ControlFlow::Continue(())
        })?;
        Ok(report.sets.remove(0))
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
