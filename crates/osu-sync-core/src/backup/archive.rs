//! Archive creation and extraction for backups

use crate::error::{Error, Result};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use walkdir::WalkDir;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use super::{BackupPhase, BackupProgress, BackupProgressCallback, BackupTarget};

/// Create a backup archive from a source directory
pub fn create_backup_archive(
    source: &Path,
    dest: &Path,
    _target: BackupTarget,
    progress: Option<BackupProgressCallback>,
) -> Result<()> {
    // Notify scanning phase
    if let Some(ref cb) = progress {
        cb(BackupProgress {
            phase: BackupPhase::Scanning,
            files_processed: 0,
            total_files: None,
            bytes_written: 0,
            current_file: None,
        });
    }

    // Check if source is a file or directory
    let is_file = source.is_file();

    // Count total files for progress
    let total_files = if is_file {
        1
    } else if source.is_dir() {
        WalkDir::new(source)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .count()
    } else {
        return Err(Error::Other(format!(
            "Source path does not exist: {}",
            source.display()
        )));
    };

    // Create the zip file
    let file = File::create(dest)?;
    let mut zip = ZipWriter::new(file);

    // Set compression options - use Deflated for good compression
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(6)); // Balanced compression

    let mut files_processed = 0usize;
    let mut bytes_written = 0u64;

    // Notify archiving phase
    if let Some(ref cb) = progress {
        cb(BackupProgress {
            phase: BackupPhase::Archiving,
            files_processed: 0,
            total_files: Some(total_files),
            bytes_written: 0,
            current_file: None,
        });
    }

    if is_file {
        // Single file backup (e.g., collection.db, scores.db)
        let filename = source
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("backup");

        add_file_to_zip(&mut zip, source, filename, options)?;
        files_processed = 1;

        if let Some(ref cb) = progress {
            cb(BackupProgress {
                phase: BackupPhase::Archiving,
                files_processed,
                total_files: Some(total_files),
                bytes_written,
                current_file: Some(filename.to_string()),
            });
        }
    } else {
        // Directory backup
        let source_prefix = source;

        for entry in WalkDir::new(source) {
            let entry = entry.map_err(|e| Error::Other(e.to_string()))?;

            if entry.file_type().is_file() {
                let path = entry.path();
                let relative_path = path
                    .strip_prefix(source_prefix)
                    .unwrap_or(path)
                    .to_string_lossy()
                    .replace('\\', "/"); // Normalize path separators

                let file_size = add_file_to_zip(&mut zip, path, &relative_path, options)?;
                files_processed += 1;
                bytes_written += file_size;

                if let Some(ref cb) = progress {
                    cb(BackupProgress {
                        phase: BackupPhase::Archiving,
                        files_processed,
                        total_files: Some(total_files),
                        bytes_written,
                        current_file: Some(relative_path),
                    });
                }
            } else if entry.file_type().is_dir() && entry.path() != source {
                // Add directory entry
                let relative_path = entry
                    .path()
                    .strip_prefix(source_prefix)
                    .unwrap_or(entry.path())
                    .to_string_lossy()
                    .replace('\\', "/")
                    + "/";

                zip.add_directory(&relative_path, options)?;
            }
        }
    }

    // Notify finalizing phase
    if let Some(ref cb) = progress {
        cb(BackupProgress {
            phase: BackupPhase::Finalizing,
            files_processed,
            total_files: Some(total_files),
            bytes_written,
            current_file: None,
        });
    }

    // Finish the archive
    zip.finish()?;

    // Notify complete
    if let Some(ref cb) = progress {
        cb(BackupProgress {
            phase: BackupPhase::Complete,
            files_processed,
            total_files: Some(total_files),
            bytes_written,
            current_file: None,
        });
    }

    Ok(())
}

/// Add a file to a zip archive
fn add_file_to_zip<W: Write + std::io::Seek>(
    zip: &mut ZipWriter<W>,
    file_path: &Path,
    archive_path: &str,
    options: SimpleFileOptions,
) -> Result<u64> {
    let mut file = File::open(file_path)?;
    let metadata = file.metadata()?;
    let file_size = metadata.len();

    zip.start_file(archive_path, options)?;

    // Read and write in chunks
    let mut buffer = vec![0u8; 64 * 1024]; // 64KB buffer
    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        zip.write_all(&buffer[..bytes_read])?;
    }

    Ok(file_size)
}

/// Extract a backup archive to a destination directory
pub fn extract_backup_archive(
    archive_path: &Path,
    dest: &Path,
    progress: Option<BackupProgressCallback>,
) -> Result<()> {
    let file = File::open(archive_path)?;
    let mut archive = ZipArchive::new(file)?;

    let total_files = archive.len();

    // Notify scanning phase
    if let Some(ref cb) = progress {
        cb(BackupProgress {
            phase: BackupPhase::Scanning,
            files_processed: 0,
            total_files: Some(total_files),
            bytes_written: 0,
            current_file: None,
        });
    }

    // Create destination directory
    std::fs::create_dir_all(dest)?;

    let mut files_processed = 0usize;
    let mut bytes_written = 0u64;

    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let outpath = match file.enclosed_name() {
            Some(path) => dest.join(path),
            None => continue,
        };

        let filename = file.name().to_string();

        // Notify progress
        if let Some(ref cb) = progress {
            cb(BackupProgress {
                phase: BackupPhase::Archiving, // Reusing for extraction
                files_processed,
                total_files: Some(total_files),
                bytes_written,
                current_file: Some(filename.clone()),
            });
        }

        if file.is_dir() {
            std::fs::create_dir_all(&outpath)?;
        } else {
            // Create parent directories if needed
            if let Some(parent) = outpath.parent() {
                if !parent.exists() {
                    std::fs::create_dir_all(parent)?;
                }
            }

            // Extract file
            let mut outfile = File::create(&outpath)?;
            let bytes = std::io::copy(&mut file, &mut outfile)?;
            bytes_written += bytes;
        }

        files_processed += 1;
    }

    // Notify complete
    if let Some(ref cb) = progress {
        cb(BackupProgress {
            phase: BackupPhase::Complete,
            files_processed,
            total_files: Some(total_files),
            bytes_written,
            current_file: None,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_create_and_extract_backup() {
        let temp_dir = tempdir().unwrap();
        let source_dir = temp_dir.path().join("source");
        let backup_file = temp_dir.path().join("backup.zip");
        let restore_dir = temp_dir.path().join("restore");

        // Create test structure
        std::fs::create_dir_all(&source_dir).unwrap();
        let mut file = File::create(source_dir.join("test.txt")).unwrap();
        file.write_all(b"Hello, World!").unwrap();

        // Create backup
        create_backup_archive(&source_dir, &backup_file, BackupTarget::StableSongs, None).unwrap();
        assert!(backup_file.exists());

        // Extract backup
        extract_backup_archive(&backup_file, &restore_dir, None).unwrap();

        // Verify
        let restored_file = restore_dir.join("test.txt");
        assert!(restored_file.exists());
        let content = std::fs::read_to_string(restored_file).unwrap();
        assert_eq!(content, "Hello, World!");
    }
}
