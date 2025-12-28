//! Import beatmaps into osu!lazer
//!
//! Since Realm database writing is complex, we use lazer's auto-import feature:
//! Place .osz files in the `import` directory and lazer will process them.

use crate::beatmap::BeatmapSet;
use crate::error::Result;
use crate::parser::create_osz_from_set;
use std::fs;
use std::path::{Path, PathBuf};

/// Importer for adding beatmaps to osu!lazer
pub struct LazerImporter {
    import_path: PathBuf,
}

impl LazerImporter {
    /// Create a new importer for the given lazer data directory
    pub fn new(lazer_data_path: &Path) -> Self {
        Self {
            import_path: lazer_data_path.join("import"),
        }
    }

    /// Ensure the import directory exists
    pub fn ensure_import_dir(&self) -> Result<()> {
        if !self.import_path.exists() {
            fs::create_dir_all(&self.import_path)?;
        }
        Ok(())
    }

    /// Import a beatmap set by creating an .osz in the import directory
    ///
    /// Lazer monitors this directory and auto-imports any .osz files
    pub fn import_beatmap_set(
        &self,
        beatmap_set: &BeatmapSet,
        files: &[(String, Vec<u8>)],
    ) -> Result<PathBuf> {
        self.ensure_import_dir()?;

        // Generate filename
        let filename = format!(
            "{}.osz",
            beatmap_set
                .folder_name
                .as_ref()
                .map(|s| s.as_str())
                .unwrap_or(&beatmap_set.generate_folder_name())
        );

        let osz_path = self.import_path.join(&filename);

        // Create the .osz file
        create_osz_from_set(beatmap_set, files, &osz_path)?;

        tracing::info!("Created {} for lazer import", osz_path.display());

        Ok(osz_path)
    }

    /// Import an existing .osz file by copying to the import directory
    pub fn import_osz(&self, osz_path: &Path) -> Result<PathBuf> {
        self.ensure_import_dir()?;

        let filename = osz_path
            .file_name()
            .ok_or_else(|| crate::error::Error::Other("Invalid .osz path".to_string()))?;

        let dest_path = self.import_path.join(filename);

        fs::copy(osz_path, &dest_path)?;

        tracing::info!("Copied {} to lazer import directory", dest_path.display());

        Ok(dest_path)
    }

    /// Import multiple .osz files
    pub fn import_multiple_osz(&self, osz_paths: &[PathBuf]) -> Vec<Result<PathBuf>> {
        osz_paths.iter().map(|p| self.import_osz(p)).collect()
    }

    /// Get the import directory path
    pub fn import_dir(&self) -> &Path {
        &self.import_path
    }

    /// List pending imports in the import directory
    pub fn list_pending(&self) -> Result<Vec<PathBuf>> {
        if !self.import_path.exists() {
            return Ok(Vec::new());
        }

        let entries: Vec<_> = fs::read_dir(&self.import_path)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext.eq_ignore_ascii_case("osz"))
                    .unwrap_or(false)
            })
            .map(|e| e.path())
            .collect();

        Ok(entries)
    }
}
