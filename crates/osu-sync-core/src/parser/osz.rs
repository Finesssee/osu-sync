//! .osz archive handling

use crate::beatmap::{BeatmapFile, BeatmapSet};
use crate::error::{Error, Result};
use crate::parser::parse_osu_file;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use zip::write::FileOptions;
use zip::{ZipArchive, ZipWriter};

/// Extract an .osz archive to a destination directory
pub fn extract_osz(osz_path: &Path, dest: &Path) -> Result<BeatmapSet> {
    let file = File::open(osz_path)?;
    let mut archive = ZipArchive::new(file)?;

    // Create destination directory
    fs::create_dir_all(dest)?;

    let mut beatmap_set = BeatmapSet::new();
    let mut osu_files = Vec::new();

    // Extract all files
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let filename = file
            .enclosed_name()
            .ok_or_else(|| Error::InvalidOsz {
                reason: "Invalid file path in archive".to_string(),
            })?
            .to_path_buf();

        let dest_path = dest.join(&filename);

        // Create parent directories if needed
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Extract file
        let mut content = Vec::new();
        file.read_to_end(&mut content)?;

        // Calculate hash
        let hash = format!("{:x}", Sha256::digest(&content));

        // Write file
        let mut output = File::create(&dest_path)?;
        output.write_all(&content)?;

        // Track file info
        let filename_str = filename.to_string_lossy().to_string();
        beatmap_set.files.push(BeatmapFile {
            filename: filename_str.clone(),
            hash,
            size: content.len() as u64,
        });

        // Track .osu files for parsing
        if filename_str.to_lowercase().ends_with(".osu") {
            osu_files.push(dest_path);
        }
    }

    // Parse all .osu files
    for osu_path in osu_files {
        match parse_osu_file(&osu_path) {
            Ok(info) => {
                // Extract beatmap set ID if not already set
                if beatmap_set.id.is_none() {
                    beatmap_set.id = info.metadata.beatmap_set_id;
                }
                beatmap_set.beatmaps.push(info);
            }
            Err(e) => {
                tracing::warn!("Failed to parse {}: {}", osu_path.display(), e);
            }
        }
    }

    if beatmap_set.beatmaps.is_empty() {
        return Err(Error::InvalidOsz {
            reason: "No valid .osu files found in archive".to_string(),
        });
    }

    // Generate folder name
    beatmap_set.folder_name = Some(beatmap_set.generate_folder_name());

    Ok(beatmap_set)
}

/// Create an .osz archive from a beatmap set
pub fn create_osz(source_dir: &Path, dest_path: &Path) -> Result<PathBuf> {
    let file = File::create(dest_path)?;
    let mut zip = ZipWriter::new(file);

    let options = FileOptions::<()>::default()
        .compression_method(zip::CompressionMethod::Stored);

    // Walk through all files in the source directory
    for entry in walkdir::WalkDir::new(source_dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() {
            let relative_path = path
                .strip_prefix(source_dir)
                .map_err(|_| Error::Other("Failed to get relative path".to_string()))?;

            let name = relative_path.to_string_lossy();
            zip.start_file(name.as_ref(), options)?;

            let content = fs::read(path)?;
            zip.write_all(&content)?;
        }
    }

    zip.finish()?;
    Ok(dest_path.to_path_buf())
}

/// Create an .osz archive from a BeatmapSet with files already loaded
pub fn create_osz_from_set(_beatmap_set: &BeatmapSet, files: &[(String, Vec<u8>)], dest_path: &Path) -> Result<PathBuf> {
    let file = File::create(dest_path)?;
    let mut zip = ZipWriter::new(file);

    let options = FileOptions::<()>::default()
        .compression_method(zip::CompressionMethod::Stored);

    for (filename, content) in files {
        zip.start_file(filename.as_str(), options)?;
        zip.write_all(content)?;
    }

    zip.finish()?;
    Ok(dest_path.to_path_buf())
}

#[cfg(test)]
mod tests {
    // Integration tests would go here with actual .osz files
}
