//! osu!lazer hash-based file storage

use crate::error::{Error, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

/// Handler for osu!lazer's hash-based file storage
///
/// Files are stored at: `files/{hash[0]}/{hash[0..2]}/{hash}`
/// Where `hash` is the lowercase SHA-256 hex digest
pub struct LazerFileStore {
    files_path: PathBuf,
}

impl LazerFileStore {
    /// Create a new file store handler
    pub fn new(lazer_data_path: &Path) -> Self {
        Self {
            files_path: lazer_data_path.join("files"),
        }
    }

    /// Get the storage path for a given hash
    ///
    /// Path format: `files/{hash[0]}/{hash[0..2]}/{hash}`
    pub fn hash_to_path(&self, hash: &str) -> PathBuf {
        let hash = hash.to_lowercase();
        if hash.len() < 2 {
            return self.files_path.join(&hash);
        }

        self.files_path
            .join(&hash[0..1])
            .join(&hash[0..2])
            .join(&hash)
    }

    /// Check if a file exists in the store
    pub fn exists(&self, hash: &str) -> bool {
        self.hash_to_path(hash).exists()
    }

    /// Read a file by its hash
    pub fn read(&self, hash: &str) -> Result<Vec<u8>> {
        let path = self.hash_to_path(hash);
        if !path.exists() {
            return Err(Error::BeatmapNotFound(format!(
                "File with hash {} not found",
                hash
            )));
        }
        Ok(fs::read(path)?)
    }

    /// Read just the first N bytes of a file (for header detection)
    pub fn read_prefix(&self, hash: &str, len: usize) -> Result<Vec<u8>> {
        use std::io::Read;
        let path = self.hash_to_path(hash);
        if !path.exists() {
            return Err(Error::BeatmapNotFound(format!(
                "File with hash {} not found",
                hash
            )));
        }
        let mut file = fs::File::open(path)?;
        let mut buffer = vec![0u8; len];
        let bytes_read = file.read(&mut buffer)?;
        buffer.truncate(bytes_read);
        Ok(buffer)
    }

    /// Verify a file's hash matches its content
    pub fn verify(&self, hash: &str) -> Result<bool> {
        let content = self.read(hash)?;
        let actual_hash = format!("{:x}", Sha256::digest(&content));
        Ok(actual_hash == hash.to_lowercase())
    }

    /// Get all files in the store using parallel directory walking
    pub fn list_all(&self) -> Result<Vec<String>> {
        use rayon::prelude::*;

        if !self.files_path.exists() {
            return Ok(Vec::new());
        }

        // Collect first-level directories
        let dir1_entries: Vec<_> = fs::read_dir(&self.files_path)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .collect();

        // Process directories in parallel
        let hashes: Vec<String> = dir1_entries
            .par_iter()
            .flat_map(|dir1| {
                let mut local_hashes = Vec::new();
                if let Ok(dir2_iter) = fs::read_dir(dir1.path()) {
                    for dir2 in dir2_iter.filter_map(|e| e.ok()) {
                        if !dir2.path().is_dir() {
                            continue;
                        }
                        if let Ok(file_iter) = fs::read_dir(dir2.path()) {
                            for file in file_iter.filter_map(|e| e.ok()) {
                                if file.path().is_file() {
                                    if let Some(name) = file.file_name().to_str() {
                                        local_hashes.push(name.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
                local_hashes
            })
            .collect();

        Ok(hashes)
    }

    /// Calculate the SHA-256 hash of content
    pub fn calculate_hash(content: &[u8]) -> String {
        format!("{:x}", Sha256::digest(content))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_to_path() {
        let store = LazerFileStore::new(Path::new("/data/osu"));

        let hash = "a1b2c3d4e5f6789";
        let path = store.hash_to_path(hash);

        assert!(path.to_string_lossy().contains("a"));
        assert!(path.to_string_lossy().contains("a1"));
        assert!(path.ends_with(hash));
    }

    #[test]
    fn test_calculate_hash() {
        let content = b"test content";
        let hash = LazerFileStore::calculate_hash(content);
        assert_eq!(hash.len(), 64); // SHA-256 produces 64 hex characters
    }
}
