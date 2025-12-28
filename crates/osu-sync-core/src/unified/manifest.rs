//! Manifest tracking types for the Unified Storage feature.
//!
//! This module provides manifest management for tracking linked resources
//! between osu! stable and lazer installations. The manifest persists state
//! to disk and allows efficient synchronization by tracking resource status.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use super::config::{SharedResourceType, UnifiedStorageMode};
use crate::error::{Error, Result};

/// Current manifest format version.
const MANIFEST_VERSION: u32 = 1;

/// Manifest filename.
const MANIFEST_FILENAME: &str = "unified-manifest.json";

/// Status of a linked resource.
///
/// Tracks the current state of a symlink/junction between installations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkStatus {
    /// Link is active and functional.
    Active,
    /// Link target no longer exists or link is broken.
    Broken,
    /// Link exists but content may be out of sync.
    Stale,
    /// Link creation is pending (not yet created).
    Pending,
}

impl LinkStatus {
    /// Returns `true` if the link is in a healthy state.
    #[inline]
    pub fn is_healthy(&self) -> bool {
        matches!(self, Self::Active)
    }

    /// Returns `true` if the link needs attention (repair or creation).
    #[inline]
    pub fn needs_attention(&self) -> bool {
        matches!(self, Self::Broken | Self::Stale | Self::Pending)
    }

    /// Returns a human-readable description of the status.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Active => "Active and functional",
            Self::Broken => "Link is broken",
            Self::Stale => "Content may be out of sync",
            Self::Pending => "Awaiting link creation",
        }
    }
}

/// A linked resource tracked by the manifest.
///
/// Represents a single resource (file or directory) that is shared between
/// osu! stable and lazer installations through symlinks or junctions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedResource {
    /// The type of resource (beatmaps, skins, etc.).
    pub resource_type: SharedResourceType,
    /// The canonical source path (in the master installation).
    pub source_path: PathBuf,
    /// Paths where this resource is linked to.
    pub link_paths: Vec<PathBuf>,
    /// SHA-256 hash of the content (for change detection).
    pub content_hash: Option<String>,
    /// Last modification timestamp.
    pub modified_at: DateTime<Utc>,
    /// Current status of the link.
    pub status: LinkStatus,
}

impl LinkedResource {
    /// Creates a new linked resource with pending status.
    pub fn new(
        resource_type: SharedResourceType,
        source_path: PathBuf,
        link_paths: Vec<PathBuf>,
    ) -> Self {
        Self {
            resource_type,
            source_path,
            link_paths,
            content_hash: None,
            modified_at: Utc::now(),
            status: LinkStatus::Pending,
        }
    }

    /// Creates a new active linked resource.
    pub fn active(
        resource_type: SharedResourceType,
        source_path: PathBuf,
        link_paths: Vec<PathBuf>,
        content_hash: Option<String>,
    ) -> Self {
        Self {
            resource_type,
            source_path,
            link_paths,
            content_hash,
            modified_at: Utc::now(),
            status: LinkStatus::Active,
        }
    }

    /// Returns `true` if this resource matches the given source path.
    pub fn matches_source(&self, path: &Path) -> bool {
        self.source_path == path
    }

    /// Returns `true` if any of the link paths match the given path.
    pub fn has_link_path(&self, path: &Path) -> bool {
        self.link_paths.iter().any(|p| p == path)
    }

    /// Adds a new link path to this resource.
    pub fn add_link_path(&mut self, path: PathBuf) {
        if !self.link_paths.contains(&path) {
            self.link_paths.push(path);
            self.modified_at = Utc::now();
        }
    }

    /// Removes a link path from this resource.
    pub fn remove_link_path(&mut self, path: &Path) -> bool {
        let initial_len = self.link_paths.len();
        self.link_paths.retain(|p| p != path);
        if self.link_paths.len() != initial_len {
            self.modified_at = Utc::now();
            true
        } else {
            false
        }
    }

    /// Updates the status of this resource.
    pub fn set_status(&mut self, status: LinkStatus) {
        self.status = status;
        self.modified_at = Utc::now();
    }

    /// Updates the content hash of this resource.
    pub fn set_content_hash(&mut self, hash: Option<String>) {
        self.content_hash = hash;
        self.modified_at = Utc::now();
    }

    /// Marks the resource as active.
    pub fn mark_active(&mut self) {
        self.set_status(LinkStatus::Active);
    }

    /// Marks the resource as broken.
    pub fn mark_broken(&mut self) {
        self.set_status(LinkStatus::Broken);
    }

    /// Marks the resource as stale.
    pub fn mark_stale(&mut self) {
        self.set_status(LinkStatus::Stale);
    }

    /// Returns the primary link path (first in the list).
    ///
    /// This is a convenience accessor for cases where only one link is expected.
    pub fn link_path(&self) -> Option<&PathBuf> {
        self.link_paths.first()
    }

    /// Returns the target path (alias for source_path for API compatibility).
    pub fn target_path(&self) -> &PathBuf {
        &self.source_path
    }
}

/// The unified storage manifest.
///
/// Tracks all linked resources and their states. The manifest is persisted
/// to disk to maintain state across application restarts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedManifest {
    /// Manifest format version for future compatibility.
    pub version: u32,
    /// The storage mode when this manifest was created.
    pub mode: UnifiedStorageMode,
    /// When the manifest was first created.
    pub created_at: DateTime<Utc>,
    /// When the manifest was last updated.
    pub updated_at: DateTime<Utc>,
    /// All tracked linked resources.
    pub resources: Vec<LinkedResource>,
}

impl Default for UnifiedManifest {
    fn default() -> Self {
        Self::new(UnifiedStorageMode::Disabled)
    }
}

impl UnifiedManifest {
    /// Creates a new empty manifest with the specified mode.
    pub fn new(mode: UnifiedStorageMode) -> Self {
        let now = Utc::now();
        Self {
            version: MANIFEST_VERSION,
            mode,
            created_at: now,
            updated_at: now,
            resources: Vec::new(),
        }
    }

    /// Returns the path where the manifest should be stored.
    ///
    /// - Windows: `%APPDATA%/osu-sync/unified-manifest.json`
    /// - Linux/macOS: `~/.config/osu-sync/unified-manifest.json`
    pub fn manifest_path() -> Result<PathBuf> {
        let config_dir = dirs::config_dir().ok_or_else(|| {
            Error::ManifestError("Could not determine config directory".to_string())
        })?;

        Ok(config_dir.join("osu-sync").join(MANIFEST_FILENAME))
    }

    /// Returns the directory where the manifest is stored.
    pub fn manifest_dir() -> Result<PathBuf> {
        let config_dir = dirs::config_dir().ok_or_else(|| {
            Error::ManifestError("Could not determine config directory".to_string())
        })?;

        Ok(config_dir.join("osu-sync"))
    }

    /// Loads the manifest from the default location.
    ///
    /// Returns a new empty manifest if the file doesn't exist.
    pub fn load() -> Result<Self> {
        let path = Self::manifest_path()?;
        Self::load_from(&path)
    }

    /// Loads the manifest from a specific path.
    ///
    /// Returns a new empty manifest if the file doesn't exist.
    pub fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(path).map_err(|e| {
            Error::ManifestError(format!("Failed to read manifest file: {}", e))
        })?;

        let manifest: Self = serde_json::from_str(&content).map_err(|e| {
            Error::ManifestError(format!("Failed to parse manifest: {}", e))
        })?;

        // Check version compatibility
        if manifest.version > MANIFEST_VERSION {
            return Err(Error::ManifestError(format!(
                "Manifest version {} is newer than supported version {}",
                manifest.version, MANIFEST_VERSION
            )));
        }

        Ok(manifest)
    }

    /// Saves the manifest to the default location.
    pub fn save(&mut self) -> Result<()> {
        let path = Self::manifest_path()?;
        self.save_to(&path)
    }

    /// Saves the manifest to a specific path.
    pub fn save_to(&mut self, path: &Path) -> Result<()> {
        self.updated_at = Utc::now();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                Error::ManifestError(format!("Failed to create manifest directory: {}", e))
            })?;
        }

        let content = serde_json::to_string_pretty(self).map_err(|e| {
            Error::ManifestError(format!("Failed to serialize manifest: {}", e))
        })?;

        std::fs::write(path, content).map_err(|e| {
            Error::ManifestError(format!("Failed to write manifest file: {}", e))
        })?;

        Ok(())
    }

    /// Adds a new resource to the manifest.
    ///
    /// If a resource with the same source path already exists, it will be updated.
    pub fn add_resource(&mut self, resource: LinkedResource) {
        // Check if resource with same source already exists
        if let Some(existing) = self.find_by_source_mut(&resource.source_path) {
            *existing = resource;
        } else {
            self.resources.push(resource);
        }
        self.updated_at = Utc::now();
    }

    /// Removes a resource by its source path.
    ///
    /// Returns `true` if a resource was removed.
    pub fn remove_resource(&mut self, source_path: &Path) -> bool {
        let initial_len = self.resources.len();
        self.resources.retain(|r| r.source_path != source_path);
        if self.resources.len() != initial_len {
            self.updated_at = Utc::now();
            true
        } else {
            false
        }
    }

    /// Removes all resources of a specific type.
    ///
    /// Returns the number of resources removed.
    pub fn remove_resources_by_type(&mut self, resource_type: SharedResourceType) -> usize {
        let initial_len = self.resources.len();
        self.resources.retain(|r| r.resource_type != resource_type);
        let removed = initial_len - self.resources.len();
        if removed > 0 {
            self.updated_at = Utc::now();
        }
        removed
    }

    /// Updates the status of a resource by its source path.
    ///
    /// Returns `true` if the resource was found and updated.
    pub fn update_status(&mut self, source_path: &Path, status: LinkStatus) -> bool {
        if let Some(resource) = self.find_by_source_mut(source_path) {
            resource.set_status(status);
            self.updated_at = Utc::now();
            true
        } else {
            false
        }
    }

    /// Finds a resource by its source path.
    pub fn find_by_source(&self, source_path: &Path) -> Option<&LinkedResource> {
        self.resources.iter().find(|r| r.source_path == source_path)
    }

    /// Finds a resource by its source path (mutable).
    pub fn find_by_source_mut(&mut self, source_path: &Path) -> Option<&mut LinkedResource> {
        self.resources.iter_mut().find(|r| r.source_path == source_path)
    }

    /// Finds a resource by any of its link paths.
    pub fn find_by_link_path(&self, link_path: &Path) -> Option<&LinkedResource> {
        self.resources.iter().find(|r| r.has_link_path(link_path))
    }

    /// Finds a resource by any of its link paths (mutable).
    pub fn find_by_link_path_mut(&mut self, link_path: &Path) -> Option<&mut LinkedResource> {
        self.resources.iter_mut().find(|r| r.has_link_path(link_path))
    }

    /// Finds all resources of a specific type.
    pub fn find_by_type(&self, resource_type: SharedResourceType) -> Vec<&LinkedResource> {
        self.resources
            .iter()
            .filter(|r| r.resource_type == resource_type)
            .collect()
    }

    /// Finds all resources with a specific status.
    pub fn find_by_status(&self, status: LinkStatus) -> Vec<&LinkedResource> {
        self.resources.iter().filter(|r| r.status == status).collect()
    }

    /// Returns all resources that need attention (broken, stale, or pending).
    pub fn find_needing_attention(&self) -> Vec<&LinkedResource> {
        self.resources.iter().filter(|r| r.status.needs_attention()).collect()
    }

    /// Returns the total number of tracked resources.
    #[inline]
    pub fn resource_count(&self) -> usize {
        self.resources.len()
    }

    /// Returns the number of resources with a specific status.
    pub fn count_by_status(&self, status: LinkStatus) -> usize {
        self.resources.iter().filter(|r| r.status == status).count()
    }

    /// Returns the number of resources of a specific type.
    pub fn count_by_type(&self, resource_type: SharedResourceType) -> usize {
        self.resources
            .iter()
            .filter(|r| r.resource_type == resource_type)
            .count()
    }

    /// Returns `true` if the manifest has no resources.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.resources.is_empty()
    }

    /// Returns an iterator over all resources.
    pub fn iter(&self) -> impl Iterator<Item = &LinkedResource> {
        self.resources.iter()
    }

    /// Returns a slice of all resources.
    ///
    /// This is useful when you need direct access to the resources slice
    /// rather than an iterator.
    pub fn resources(&self) -> &[LinkedResource] {
        &self.resources
    }

    /// Returns a mutable iterator over all resources.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut LinkedResource> {
        self.resources.iter_mut()
    }

    /// Clears all resources from the manifest.
    pub fn clear(&mut self) {
        self.resources.clear();
        self.updated_at = Utc::now();
    }

    /// Updates the storage mode.
    pub fn set_mode(&mut self, mode: UnifiedStorageMode) {
        self.mode = mode;
        self.updated_at = Utc::now();
    }

    /// Marks all resources as stale.
    ///
    /// Useful when re-verifying all links after a restart or mode change.
    pub fn mark_all_stale(&mut self) {
        for resource in &mut self.resources {
            resource.status = LinkStatus::Stale;
        }
        self.updated_at = Utc::now();
    }

    /// Returns a summary of resource statuses.
    pub fn status_summary(&self) -> ManifestSummary {
        ManifestSummary {
            total: self.resources.len(),
            active: self.count_by_status(LinkStatus::Active),
            broken: self.count_by_status(LinkStatus::Broken),
            stale: self.count_by_status(LinkStatus::Stale),
            pending: self.count_by_status(LinkStatus::Pending),
        }
    }
}

/// Summary of manifest resource statuses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ManifestSummary {
    /// Total number of resources.
    pub total: usize,
    /// Number of active resources.
    pub active: usize,
    /// Number of broken resources.
    pub broken: usize,
    /// Number of stale resources.
    pub stale: usize,
    /// Number of pending resources.
    pub pending: usize,
}

impl ManifestSummary {
    /// Returns `true` if all resources are healthy (active).
    #[inline]
    pub fn is_healthy(&self) -> bool {
        self.total == self.active
    }

    /// Returns the number of resources needing attention.
    #[inline]
    pub fn needs_attention(&self) -> usize {
        self.broken + self.stale + self.pending
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn create_test_resource(name: &str) -> LinkedResource {
        LinkedResource::new(
            SharedResourceType::Beatmaps,
            PathBuf::from(format!("/source/{}", name)),
            vec![PathBuf::from(format!("/link/{}", name))],
        )
    }

    #[test]
    fn test_link_status() {
        assert!(LinkStatus::Active.is_healthy());
        assert!(!LinkStatus::Broken.is_healthy());
        assert!(!LinkStatus::Stale.is_healthy());
        assert!(!LinkStatus::Pending.is_healthy());

        assert!(!LinkStatus::Active.needs_attention());
        assert!(LinkStatus::Broken.needs_attention());
        assert!(LinkStatus::Stale.needs_attention());
        assert!(LinkStatus::Pending.needs_attention());
    }

    #[test]
    fn test_linked_resource_creation() {
        let resource = LinkedResource::new(
            SharedResourceType::Skins,
            PathBuf::from("/source/skin"),
            vec![PathBuf::from("/link/skin")],
        );

        assert_eq!(resource.resource_type, SharedResourceType::Skins);
        assert_eq!(resource.status, LinkStatus::Pending);
        assert!(resource.content_hash.is_none());
    }

    #[test]
    fn test_linked_resource_active() {
        let resource = LinkedResource::active(
            SharedResourceType::Beatmaps,
            PathBuf::from("/source/beatmap"),
            vec![PathBuf::from("/link/beatmap")],
            Some("abc123".to_string()),
        );

        assert_eq!(resource.status, LinkStatus::Active);
        assert_eq!(resource.content_hash, Some("abc123".to_string()));
    }

    #[test]
    fn test_linked_resource_path_operations() {
        let mut resource = create_test_resource("test");

        assert!(resource.matches_source(Path::new("/source/test")));
        assert!(!resource.matches_source(Path::new("/source/other")));

        assert!(resource.has_link_path(Path::new("/link/test")));
        assert!(!resource.has_link_path(Path::new("/link/other")));

        resource.add_link_path(PathBuf::from("/link/other"));
        assert!(resource.has_link_path(Path::new("/link/other")));

        assert!(resource.remove_link_path(Path::new("/link/other")));
        assert!(!resource.has_link_path(Path::new("/link/other")));
    }

    #[test]
    fn test_manifest_creation() {
        let manifest = UnifiedManifest::new(UnifiedStorageMode::StableMaster);

        assert_eq!(manifest.version, MANIFEST_VERSION);
        assert_eq!(manifest.mode, UnifiedStorageMode::StableMaster);
        assert!(manifest.is_empty());
    }

    #[test]
    fn test_manifest_add_remove_resource() {
        let mut manifest = UnifiedManifest::default();

        let resource = create_test_resource("beatmap1");
        manifest.add_resource(resource);
        assert_eq!(manifest.resource_count(), 1);

        let resource = create_test_resource("beatmap2");
        manifest.add_resource(resource);
        assert_eq!(manifest.resource_count(), 2);

        assert!(manifest.remove_resource(Path::new("/source/beatmap1")));
        assert_eq!(manifest.resource_count(), 1);

        assert!(!manifest.remove_resource(Path::new("/source/nonexistent")));
        assert_eq!(manifest.resource_count(), 1);
    }

    #[test]
    fn test_manifest_find_operations() {
        let mut manifest = UnifiedManifest::default();

        let mut resource1 = create_test_resource("beatmap1");
        resource1.status = LinkStatus::Active;
        manifest.add_resource(resource1);

        let mut resource2 = LinkedResource::new(
            SharedResourceType::Skins,
            PathBuf::from("/source/skin1"),
            vec![PathBuf::from("/link/skin1")],
        );
        resource2.status = LinkStatus::Broken;
        manifest.add_resource(resource2);

        // Find by source
        assert!(manifest.find_by_source(Path::new("/source/beatmap1")).is_some());
        assert!(manifest.find_by_source(Path::new("/source/nonexistent")).is_none());

        // Find by link path
        assert!(manifest.find_by_link_path(Path::new("/link/beatmap1")).is_some());
        assert!(manifest.find_by_link_path(Path::new("/link/nonexistent")).is_none());

        // Find by type
        let beatmaps = manifest.find_by_type(SharedResourceType::Beatmaps);
        assert_eq!(beatmaps.len(), 1);

        let skins = manifest.find_by_type(SharedResourceType::Skins);
        assert_eq!(skins.len(), 1);

        // Find by status
        let active = manifest.find_by_status(LinkStatus::Active);
        assert_eq!(active.len(), 1);

        let broken = manifest.find_by_status(LinkStatus::Broken);
        assert_eq!(broken.len(), 1);

        // Find needing attention
        let attention = manifest.find_needing_attention();
        assert_eq!(attention.len(), 1);
    }

    #[test]
    fn test_manifest_update_status() {
        let mut manifest = UnifiedManifest::default();
        let resource = create_test_resource("beatmap1");
        manifest.add_resource(resource);

        assert!(manifest.update_status(Path::new("/source/beatmap1"), LinkStatus::Active));
        assert_eq!(
            manifest.find_by_source(Path::new("/source/beatmap1")).unwrap().status,
            LinkStatus::Active
        );

        assert!(!manifest.update_status(Path::new("/source/nonexistent"), LinkStatus::Broken));
    }

    #[test]
    fn test_manifest_summary() {
        let mut manifest = UnifiedManifest::default();

        let mut r1 = create_test_resource("r1");
        r1.status = LinkStatus::Active;
        manifest.add_resource(r1);

        let mut r2 = create_test_resource("r2");
        r2.status = LinkStatus::Active;
        manifest.add_resource(r2);

        let mut r3 = create_test_resource("r3");
        r3.status = LinkStatus::Broken;
        manifest.add_resource(r3);

        let mut r4 = create_test_resource("r4");
        r4.status = LinkStatus::Pending;
        manifest.add_resource(r4);

        let summary = manifest.status_summary();
        assert_eq!(summary.total, 4);
        assert_eq!(summary.active, 2);
        assert_eq!(summary.broken, 1);
        assert_eq!(summary.pending, 1);
        assert_eq!(summary.stale, 0);
        assert!(!summary.is_healthy());
        assert_eq!(summary.needs_attention(), 2);
    }

    #[test]
    fn test_manifest_save_load() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("test-manifest.json");

        let mut manifest = UnifiedManifest::new(UnifiedStorageMode::LazerMaster);
        let mut resource = create_test_resource("beatmap1");
        resource.status = LinkStatus::Active;
        manifest.add_resource(resource);

        manifest.save_to(&manifest_path).unwrap();
        assert!(manifest_path.exists());

        let loaded = UnifiedManifest::load_from(&manifest_path).unwrap();
        assert_eq!(loaded.mode, UnifiedStorageMode::LazerMaster);
        assert_eq!(loaded.resource_count(), 1);
        assert!(loaded.find_by_source(Path::new("/source/beatmap1")).is_some());
    }

    #[test]
    fn test_manifest_load_nonexistent() {
        let result = UnifiedManifest::load_from(Path::new("/nonexistent/manifest.json"));
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_manifest_mark_all_stale() {
        let mut manifest = UnifiedManifest::default();

        let mut r1 = create_test_resource("r1");
        r1.status = LinkStatus::Active;
        manifest.add_resource(r1);

        let mut r2 = create_test_resource("r2");
        r2.status = LinkStatus::Active;
        manifest.add_resource(r2);

        manifest.mark_all_stale();

        for resource in manifest.iter() {
            assert_eq!(resource.status, LinkStatus::Stale);
        }
    }

    #[test]
    fn test_manifest_clear() {
        let mut manifest = UnifiedManifest::default();
        manifest.add_resource(create_test_resource("r1"));
        manifest.add_resource(create_test_resource("r2"));
        assert_eq!(manifest.resource_count(), 2);

        manifest.clear();
        assert!(manifest.is_empty());
    }

    #[test]
    fn test_manifest_remove_by_type() {
        let mut manifest = UnifiedManifest::default();

        manifest.add_resource(LinkedResource::new(
            SharedResourceType::Beatmaps,
            PathBuf::from("/source/b1"),
            vec![],
        ));
        manifest.add_resource(LinkedResource::new(
            SharedResourceType::Beatmaps,
            PathBuf::from("/source/b2"),
            vec![],
        ));
        manifest.add_resource(LinkedResource::new(
            SharedResourceType::Skins,
            PathBuf::from("/source/s1"),
            vec![],
        ));

        let removed = manifest.remove_resources_by_type(SharedResourceType::Beatmaps);
        assert_eq!(removed, 2);
        assert_eq!(manifest.resource_count(), 1);
        assert_eq!(manifest.count_by_type(SharedResourceType::Skins), 1);
    }
}
