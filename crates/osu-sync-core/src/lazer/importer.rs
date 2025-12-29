//! Import beatmaps into osu!lazer
//!
//! This module provides two import methods:
//! 1. **Direct import**: Launch lazer with .osz file for immediate import
//! 2. **Batch import**: Place .osz files in import folder, then launch lazer once
//!
//! Direct import is preferred for small batches as beatmaps appear instantly.
//! Batch import is more efficient for large syncs.

use crate::beatmap::BeatmapSet;
use crate::error::{Error, Result};
use crate::parser::create_osz_from_set;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

/// Windows flag to prevent console window from appearing
#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Result of an import operation
#[derive(Debug, Clone)]
pub struct ImportResult {
    /// Path to the .osz file created
    pub osz_path: PathBuf,
    /// Whether lazer was launched to import it
    pub lazer_triggered: bool,
}

/// Importer for adding beatmaps to osu!lazer
pub struct LazerImporter {
    /// Path to lazer data directory
    data_path: PathBuf,
    /// Path to import folder
    import_path: PathBuf,
    /// Path to lazer executable (if found)
    lazer_exe: Option<PathBuf>,
    /// Whether to trigger lazer import immediately
    trigger_import: bool,
    /// Accumulated .osz files for batch import
    pending_imports: Vec<PathBuf>,
}

impl LazerImporter {
    /// Create a new importer for the given lazer data directory
    pub fn new(lazer_data_path: &Path) -> Self {
        let lazer_exe = Self::find_lazer_executable(lazer_data_path);
        if lazer_exe.is_some() {
            tracing::info!("Found lazer executable for direct import");
        } else {
            tracing::info!("Lazer executable not found, will use import folder method");
        }

        Self {
            data_path: lazer_data_path.to_path_buf(),
            import_path: lazer_data_path.join("import"),
            lazer_exe,
            trigger_import: true,
            pending_imports: Vec::new(),
        }
    }

    /// Disable automatic import triggering (for batch mode)
    pub fn batch_mode(mut self) -> Self {
        self.trigger_import = false;
        self
    }

    /// Find the osu!lazer executable
    fn find_lazer_executable(data_path: &Path) -> Option<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            // On Windows, lazer is typically installed via the installer
            // Check common locations

            // 1. Check LocalAppData (default install location)
            if let Some(local_app_data) = dirs::data_local_dir() {
                let paths = [
                    // Standard installer location with version subfolder
                    local_app_data.join("osulazer").join("current").join("osu!.exe"),
                    local_app_data.join("osulazer").join("osu!.exe"),
                    local_app_data.join("osu!lazer").join("osu!.exe"),
                    local_app_data.join("Programs").join("osu!lazer").join("osu!.exe"),
                ];
                for path in &paths {
                    if path.exists() {
                        return Some(path.clone());
                    }
                }
            }

            // 2. Check Program Files
            if let Ok(program_files) = std::env::var("ProgramFiles") {
                let path = PathBuf::from(&program_files).join("osu!lazer").join("osu!.exe");
                if path.exists() {
                    return Some(path);
                }
            }

            // 3. Check relative to data path (portable installs)
            // Data is in AppData/Roaming/osu, exe might be nearby
            if let Some(parent) = data_path.parent() {
                let path = parent.join("osu!.exe");
                if path.exists() {
                    return Some(path);
                }
            }

            // 4. Check if osu! is in PATH
            if let Ok(output) = Command::new("where").arg("osu!.exe").output() {
                if output.status.success() {
                    if let Ok(path_str) = String::from_utf8(output.stdout) {
                        let path = PathBuf::from(path_str.trim());
                        if path.exists() {
                            return Some(path);
                        }
                    }
                }
            }
        }

        #[cfg(target_os = "linux")]
        {
            // On Linux, check common locations
            if let Some(home) = dirs::home_dir() {
                let paths = [
                    home.join(".local/bin/osu!"),
                    home.join("Applications/osu!.AppImage"),
                    PathBuf::from("/usr/bin/osu-lazer"),
                    PathBuf::from("/usr/local/bin/osu-lazer"),
                ];
                for path in &paths {
                    if path.exists() {
                        return Some(path.clone());
                    }
                }
            }

            // Check if available in PATH
            if let Ok(output) = Command::new("which").arg("osu-lazer").output() {
                if output.status.success() {
                    if let Ok(path_str) = String::from_utf8(output.stdout) {
                        return Some(PathBuf::from(path_str.trim()));
                    }
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            let paths = [
                PathBuf::from("/Applications/osu!.app/Contents/MacOS/osu!"),
                PathBuf::from("/Applications/osu!lazer.app/Contents/MacOS/osu!"),
            ];
            for path in &paths {
                if path.exists() {
                    return Some(path.clone());
                }
            }
        }

        None
    }

    /// Set a custom lazer executable path
    pub fn with_lazer_exe(mut self, path: PathBuf) -> Self {
        if path.exists() {
            self.lazer_exe = Some(path);
        }
        self
    }

    /// Ensure the import directory exists
    pub fn ensure_import_dir(&self) -> Result<()> {
        if !self.import_path.exists() {
            fs::create_dir_all(&self.import_path)?;
        }
        Ok(())
    }

    /// Import a beatmap set
    ///
    /// Creates an .osz file and optionally triggers lazer to import it immediately.
    pub fn import_beatmap_set(
        &mut self,
        beatmap_set: &BeatmapSet,
        files: &[(String, Vec<u8>)],
    ) -> Result<ImportResult> {
        self.ensure_import_dir()?;

        // Generate filename (sanitize for filesystem)
        let generated_name = beatmap_set.generate_folder_name();
        let base_name = beatmap_set
            .folder_name
            .as_ref()
            .map(|s| s.as_str())
            .unwrap_or(&generated_name);
        
        let filename = format!("{}.osz", sanitize_filename(base_name));
        let osz_path = self.import_path.join(&filename);

        // Create the .osz file
        create_osz_from_set(beatmap_set, files, &osz_path)?;
        tracing::info!("Created {} for lazer import", osz_path.display());

        // Track for batch import
        self.pending_imports.push(osz_path.clone());

        // Trigger immediate import if enabled and we have an exe
        let lazer_triggered = if self.trigger_import {
            self.trigger_single_import(&osz_path)
        } else {
            false
        };

        Ok(ImportResult {
            osz_path,
            lazer_triggered,
        })
    }

    /// Trigger lazer to import a single .osz file
    ///
    /// On Windows, uses `raw_arg()` with quoted path to handle special characters
    /// like `!`, `[]`, `&` that would otherwise break command-line parsing.
    fn trigger_single_import(&self, osz_path: &Path) -> bool {
        let Some(ref lazer_exe) = self.lazer_exe else {
            return false;
        };

        #[cfg(target_os = "windows")]
        {
            // On Windows, use raw_arg with quoted path to handle special characters
            // This prevents issues with !, [], &, etc. in filenames
            let quoted_path = format!("\"{}\"", osz_path.display());
            match Command::new(lazer_exe)
                .raw_arg(&quoted_path)
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
            {
                Ok(_) => {
                    tracing::debug!("Lazer import triggered for: {}", osz_path.display());
                    return true;
                }
                Err(e) => {
                    tracing::warn!("Failed to launch lazer for import: {}", e);
                }
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            // On Linux/macOS, standard arg passing works fine
            match Command::new(lazer_exe).arg(osz_path).spawn() {
                Ok(_) => {
                    tracing::debug!("Lazer import triggered for: {}", osz_path.display());
                    return true;
                }
                Err(e) => {
                    tracing::warn!("Failed to launch lazer for import: {}", e);
                }
            }
        }

        false
    }

    /// Trigger lazer to process all pending imports
    ///
    /// On Windows: Launches lazer once per file with proper quoting to handle special characters.
    /// On Linux/macOS: Launches lazer with batch of files as arguments.
    pub fn trigger_batch_import(&self) -> Result<bool> {
        if self.pending_imports.is_empty() {
            return Ok(false);
        }

        let Some(ref lazer_exe) = self.lazer_exe else {
            tracing::warn!(
                "Lazer executable not found. {} .osz files are waiting in: {}",
                self.pending_imports.len(),
                self.import_path.display()
            );
            tracing::warn!("Please start osu!lazer manually to import them.");
            return Ok(false);
        };

        let total = self.pending_imports.len();
        tracing::info!("Triggering lazer to import {} beatmaps", total);

        #[cfg(target_os = "windows")]
        {
            // On Windows, launch each file individually to handle special characters
            // This is slower but reliable for filenames with !, [], &, etc.
            let mut success_count = 0;
            let mut fail_count = 0;

            for (i, osz_path) in self.pending_imports.iter().enumerate() {
                let quoted_path = format!("\"{}\"", osz_path.display());

                match Command::new(lazer_exe)
                    .raw_arg(&quoted_path)
                    .creation_flags(CREATE_NO_WINDOW)
                    .spawn()
                {
                    Ok(_) => {
                        success_count += 1;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to import {}: {}", osz_path.display(), e);
                        fail_count += 1;
                    }
                }

                // Progress logging every 50 files
                if (i + 1) % 50 == 0 {
                    tracing::info!("Import progress: {}/{} files sent to lazer", i + 1, total);
                }

                // Small delay to not overwhelm lazer (100ms between each)
                std::thread::sleep(std::time::Duration::from_millis(100));
            }

            if fail_count > 0 {
                tracing::warn!(
                    "Import complete: {} succeeded, {} failed",
                    success_count,
                    fail_count
                );
            } else {
                tracing::info!("All {} files sent to lazer for import", success_count);
            }

            return Ok(success_count > 0);
        }

        #[cfg(not(target_os = "windows"))]
        {
            // On Linux/macOS, batch approach works fine
            const BATCH_SIZE: usize = 50;

            let batches: Vec<_> = self.pending_imports.chunks(BATCH_SIZE).collect();
            let batch_count = batches.len();

            if batch_count > 1 {
                tracing::info!(
                    "Splitting into {} batches of up to {} files each",
                    batch_count,
                    BATCH_SIZE
                );
            }

            // Launch lazer for each batch
            for (batch_idx, batch) in batches.iter().enumerate() {
                match Command::new(lazer_exe)
                    .args(batch.iter().map(|p| p.as_os_str()))
                    .spawn()
                {
                    Ok(_) => {
                        tracing::info!(
                            "Batch {}/{}: {} files sent to lazer",
                            batch_idx + 1,
                            batch_count,
                            batch.len()
                        );
                    }
                    Err(e) => {
                        tracing::warn!("Batch {}/{} failed: {}", batch_idx + 1, batch_count, e);
                    }
                }

                // Wait between batches
                if batch_idx < batch_count - 1 {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                }
            }

            return Ok(true);
        }
    }

    /// Check if we can trigger imports (lazer exe found)
    pub fn can_trigger_import(&self) -> bool {
        self.lazer_exe.is_some()
    }

    /// Get the lazer executable path if found
    pub fn lazer_executable(&self) -> Option<&Path> {
        self.lazer_exe.as_deref()
    }

    /// Import an existing .osz file by copying to the import directory
    pub fn import_osz(&mut self, osz_path: &Path) -> Result<ImportResult> {
        self.ensure_import_dir()?;

        let filename = osz_path
            .file_name()
            .ok_or_else(|| Error::Other("Invalid .osz path".to_string()))?;

        let dest_path = self.import_path.join(filename);
        fs::copy(osz_path, &dest_path)?;

        tracing::info!("Copied {} to lazer import directory", dest_path.display());

        self.pending_imports.push(dest_path.clone());

        let lazer_triggered = if self.trigger_import {
            self.trigger_single_import(&dest_path)
        } else {
            false
        };

        Ok(ImportResult {
            osz_path: dest_path,
            lazer_triggered,
        })
    }

    /// Get the import directory path
    pub fn import_dir(&self) -> &Path {
        &self.import_path
    }

    /// Get the data directory path
    pub fn data_dir(&self) -> &Path {
        &self.data_path
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

    /// Get count of pending imports from this session
    pub fn pending_count(&self) -> usize {
        self.pending_imports.len()
    }

    /// Clear tracking of pending imports (after successful batch import)
    pub fn clear_pending(&mut self) {
        self.pending_imports.clear();
    }
}

/// Sanitize a filename for safe filesystem use
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("normal"), "normal");
        assert_eq!(sanitize_filename("a/b\\c:d"), "a_b_c_d");
        assert_eq!(sanitize_filename("test*file?"), "test_file_");
        assert_eq!(sanitize_filename("good - name"), "good - name");
    }
}
