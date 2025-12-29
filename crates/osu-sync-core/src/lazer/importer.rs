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
    fn trigger_single_import(&self, osz_path: &Path) -> bool {
        if let Some(ref lazer_exe) = self.lazer_exe {
            // Launch lazer with the .osz file - lazer will import and exit
            // Using --import flag if available, otherwise just pass the file
            match Command::new(lazer_exe)
                .arg(osz_path)
                .spawn()
            {
                Ok(mut child) => {
                    // Wait a short time to let lazer start processing
                    // Don't wait for full completion as lazer stays open
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    
                    // Check if process is still running (good sign)
                    match child.try_wait() {
                        Ok(Some(status)) => {
                            if status.success() {
                                tracing::info!("Lazer import triggered successfully");
                                return true;
                            } else {
                                tracing::warn!("Lazer exited with error: {:?}", status);
                            }
                        }
                        Ok(None) => {
                            // Still running, which is expected
                            tracing::info!("Lazer import triggered, processing...");
                            return true;
                        }
                        Err(e) => {
                            tracing::warn!("Failed to check lazer status: {}", e);
                        }
                    }
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
    /// Call this after batch imports to launch lazer with .osz files as arguments.
    /// Lazer will import all files passed as command-line arguments.
    pub fn trigger_batch_import(&self) -> Result<bool> {
        if self.pending_imports.is_empty() {
            return Ok(false);
        }

        if let Some(ref lazer_exe) = self.lazer_exe {
            let total = self.pending_imports.len();
            tracing::info!(
                "Triggering lazer to import {} beatmaps",
                total
            );

            // Windows has ~32767 char command line limit, but safer to use ~8000
            // Each path is roughly 80-150 chars, so batch ~50 files at a time
            const BATCH_SIZE: usize = 50;
            
            let batches: Vec<_> = self.pending_imports.chunks(BATCH_SIZE).collect();
            let batch_count = batches.len();
            
            if batch_count > 1 {
                tracing::info!(
                    "Splitting into {} batches of up to {} files each",
                    batch_count, BATCH_SIZE
                );
            }

            // Launch lazer with first batch - it will import those files
            // User can restart lazer for remaining batches, or we could launch multiple times
            let first_batch = &batches[0];
            
            match Command::new(lazer_exe)
                .args(first_batch.iter().map(|p| p.as_os_str()))
                .spawn()
            {
                Ok(_) => {
                    if batch_count > 1 {
                        let remaining = total - first_batch.len();
                        tracing::info!(
                            "Lazer launched with first {} files. {} files remain in import folder for next launch.",
                            first_batch.len(),
                            remaining
                        );
                        tracing::info!(
                            "Restart lazer to import remaining files from: {}",
                            self.import_path.display()
                        );
                    } else {
                        tracing::info!("Lazer launched with all {} files for import", total);
                    }
                    return Ok(true);
                }
                Err(e) => {
                    tracing::warn!("Failed to launch lazer: {}", e);
                    return Err(Error::Other(format!("Failed to launch lazer: {}", e)));
                }
            }
        }

        tracing::warn!(
            "Lazer executable not found. {} .osz files are waiting in: {}",
            self.pending_imports.len(),
            self.import_path.display()
        );
        tracing::warn!("Please start osu!lazer manually to import them.");

        Ok(false)
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
