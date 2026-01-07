//! Skip list for permanently skipping beatmaps during sync

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

/// A list of beatmaps to permanently skip during sync
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkipList {
    /// Beatmap set IDs to skip
    #[serde(default)]
    pub set_ids: HashSet<i32>,
    /// Folder names to skip (for beatmaps without online IDs)
    #[serde(default)]
    pub folder_names: HashSet<String>,
}

impl SkipList {
    /// Create a new empty skip list
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the path to the skip list file
    fn file_path() -> Option<PathBuf> {
        dirs::config_dir().map(|p| p.join("osu-sync").join("skip_list.json"))
    }

    /// Load the skip list from disk
    pub fn load() -> std::io::Result<Self> {
        let path = Self::file_path().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "Config directory not found")
        })?;

        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&path)?;
        serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Save the skip list to disk
    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::file_path().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "Config directory not found")
        })?;

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(&path, content)
    }

    /// Add a beatmap set ID to the skip list
    pub fn add_set_id(&mut self, set_id: i32) {
        self.set_ids.insert(set_id);
    }

    /// Add a folder name to the skip list
    pub fn add_folder_name(&mut self, folder_name: impl Into<String>) {
        self.folder_names.insert(folder_name.into());
    }

    /// Remove a beatmap set ID from the skip list
    pub fn remove_set_id(&mut self, set_id: i32) -> bool {
        self.set_ids.remove(&set_id)
    }

    /// Remove a folder name from the skip list
    pub fn remove_folder_name(&mut self, folder_name: &str) -> bool {
        self.folder_names.remove(folder_name)
    }

    /// Check if a beatmap set ID should be skipped
    pub fn should_skip_by_id(&self, set_id: i32) -> bool {
        self.set_ids.contains(&set_id)
    }

    /// Check if a folder name should be skipped
    pub fn should_skip_by_folder(&self, folder_name: &str) -> bool {
        self.folder_names.contains(folder_name)
    }

    /// Check if a beatmap should be skipped by either ID or folder name
    pub fn should_skip(&self, set_id: Option<i32>, folder_name: Option<&str>) -> bool {
        if let Some(id) = set_id {
            if self.should_skip_by_id(id) {
                return true;
            }
        }
        if let Some(name) = folder_name {
            if self.should_skip_by_folder(name) {
                return true;
            }
        }
        false
    }

    /// Get the total number of items in the skip list
    pub fn len(&self) -> usize {
        self.set_ids.len() + self.folder_names.len()
    }

    /// Check if the skip list is empty
    pub fn is_empty(&self) -> bool {
        self.set_ids.is_empty() && self.folder_names.is_empty()
    }

    /// Clear all items from the skip list
    pub fn clear(&mut self) {
        self.set_ids.clear();
        self.folder_names.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_skip_list_basic() {
        let mut list = SkipList::new();
        assert!(list.is_empty());

        // Add by ID
        list.add_set_id(123);
        assert!(list.should_skip_by_id(123));
        assert!(!list.should_skip_by_id(456));

        // Add by folder name
        list.add_folder_name("123 Artist - Title");
        assert!(list.should_skip_by_folder("123 Artist - Title"));
        assert!(!list.should_skip_by_folder("Other Folder"));

        assert_eq!(list.len(), 2);

        // Test combined check
        assert!(list.should_skip(Some(123), None));
        assert!(list.should_skip(None, Some("123 Artist - Title")));
        assert!(list.should_skip(Some(999), Some("123 Artist - Title")));
        assert!(!list.should_skip(Some(999), Some("Unknown")));
    }

    #[test]
    fn test_skip_list_remove() {
        let mut list = SkipList::new();
        list.add_set_id(123);
        list.add_folder_name("test folder");

        assert!(list.remove_set_id(123));
        assert!(!list.should_skip_by_id(123));

        assert!(list.remove_folder_name("test folder"));
        assert!(!list.should_skip_by_folder("test folder"));

        assert!(list.is_empty());
    }
}
