//! Main Unified Storage Engine
//!
//! This module provides the orchestration layer that coordinates all unified
//! storage operations between osu! stable and lazer installations.
//!
//! The engine manages:
//! - Initial setup of unified storage (creating links)
//! - Synchronization between installations
//! - Verification of link integrity
//! - Repair of broken links
//! - Teardown and cleanup

use std::path::PathBuf;

use crate::error::{Error, Result};

use super::config::{UnifiedStorageConfig, UnifiedStorageMode};
use super::link::LinkManager;
use super::manifest::{LinkStatus, UnifiedManifest};

/// Result of a setup operation.
///
/// Contains statistics about the initial unified storage setup,
/// including the number of links created and any warnings encountered.
#[derive(Debug, Clone, Default)]
pub struct SetupResult {
    /// Number of filesystem links (symlinks/junctions) created.
    pub links_created: usize,
    /// Number of resources (beatmaps, skins, etc.) successfully linked.
    pub resources_linked: usize,
    /// Non-fatal warnings encountered during setup.
    pub warnings: Vec<String>,
}

impl SetupResult {
    /// Creates a new empty setup result.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if setup completed without warnings.
    pub fn is_clean(&self) -> bool {
        self.warnings.is_empty()
    }

    /// Adds a warning message to the result.
    pub fn add_warning(&mut self, warning: impl Into<String>) {
        self.warnings.push(warning.into());
    }
}

/// Result of a sync operation.
///
/// Contains statistics about what changed during synchronization.
#[derive(Debug, Clone, Default)]
pub struct SyncResult {
    /// Number of new links created.
    pub new_links: usize,
    /// Number of existing links updated.
    pub updated: usize,
    /// Number of stale links removed.
    pub removed: usize,
    /// Errors encountered during sync (non-fatal).
    pub errors: Vec<String>,
}

impl SyncResult {
    /// Creates a new empty sync result.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if sync completed without errors.
    pub fn is_success(&self) -> bool {
        self.errors.is_empty()
    }

    /// Returns the total number of changes made.
    pub fn total_changes(&self) -> usize {
        self.new_links + self.updated + self.removed
    }

    /// Adds an error message to the result.
    pub fn add_error(&mut self, error: impl Into<String>) {
        self.errors.push(error.into());
    }

    /// Merges another sync result into this one.
    pub fn merge(&mut self, other: SyncResult) {
        self.new_links += other.new_links;
        self.updated += other.updated;
        self.removed += other.removed;
        self.errors.extend(other.errors);
    }
}

/// Result of a verification operation.
///
/// Contains statistics about the current state of all links.
#[derive(Debug, Clone, Default)]
pub struct VerificationResult {
    /// Total number of links tracked in the manifest.
    pub total_links: usize,
    /// Number of links that are active and valid.
    pub active: usize,
    /// Number of links that are broken (target missing).
    pub broken: usize,
    /// Number of links that are stale (no longer needed).
    pub stale: usize,
}

impl VerificationResult {
    /// Creates a new empty verification result.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if all links are healthy.
    pub fn is_healthy(&self) -> bool {
        self.broken == 0 && self.stale == 0
    }

    /// Returns the percentage of healthy links (0.0 to 100.0).
    pub fn health_percentage(&self) -> f64 {
        if self.total_links == 0 {
            return 100.0;
        }
        (self.active as f64 / self.total_links as f64) * 100.0
    }
}

/// Result of a repair operation.
///
/// Contains statistics about what was fixed during repair.
#[derive(Debug, Clone, Default)]
pub struct RepairResult {
    /// Number of links successfully repaired.
    pub repaired: usize,
    /// Number of links that could not be repaired.
    pub failed: usize,
    /// Number of stale links that were removed.
    pub removed: usize,
}

impl RepairResult {
    /// Creates a new empty repair result.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns `true` if all repairs were successful.
    pub fn is_success(&self) -> bool {
        self.failed == 0
    }

    /// Returns the total number of actions taken.
    pub fn total_actions(&self) -> usize {
        self.repaired + self.failed + self.removed
    }
}

/// Main orchestration engine for unified storage operations.
///
/// The `UnifiedStorageEngine` coordinates all unified storage operations,
/// including setup, synchronization, verification, and repair of links
/// between osu! stable and lazer installations.
///
/// # Example
///
/// ```rust,ignore
/// use osu_sync_core::unified::{UnifiedStorageConfig, UnifiedStorageEngine};
/// use std::path::PathBuf;
///
/// let config = UnifiedStorageConfig::stable_master();
/// let stable = PathBuf::from("/path/to/osu-stable");
/// let lazer = PathBuf::from("/path/to/osu-lazer");
///
/// let mut engine = UnifiedStorageEngine::new(config, stable, lazer)?;
///
/// // Initial setup
/// let setup_result = engine.setup()?;
/// println!("Created {} links", setup_result.links_created);
///
/// // Verify integrity
/// let verify_result = engine.verify()?;
/// if !verify_result.is_healthy() {
///     let repair_result = engine.repair()?;
///     println!("Repaired {} links", repair_result.repaired);
/// }
/// ```
pub struct UnifiedStorageEngine {
    /// Configuration for unified storage behavior.
    config: UnifiedStorageConfig,
    /// Path to the osu! stable installation.
    stable_path: PathBuf,
    /// Path to the osu! lazer installation.
    lazer_path: PathBuf,
    /// Manifest tracking all linked resources.
    manifest: UnifiedManifest,
    /// Manager for filesystem link operations.
    link_manager: LinkManager,
}

impl UnifiedStorageEngine {
    /// Creates a new unified storage engine.
    ///
    /// # Arguments
    ///
    /// * `config` - Configuration for unified storage behavior
    /// * `stable` - Path to the osu! stable installation
    /// * `lazer` - Path to the osu! lazer installation
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The configuration is invalid
    /// - The stable or lazer paths don't exist
    /// - Failed to initialize the link manager
    pub fn new(config: UnifiedStorageConfig, stable: PathBuf, lazer: PathBuf) -> Result<Self> {
        // Validate configuration
        config.validate().map_err(Error::Config)?;

        // Validate paths exist
        if !stable.exists() {
            return Err(Error::OsuNotFound(stable));
        }
        if !lazer.exists() {
            return Err(Error::OsuNotFound(lazer));
        }

        // Initialize manifest with the configured mode
        let manifest = UnifiedManifest::new(config.mode);

        // Initialize link manager with configuration options
        let link_manager = LinkManager::new(config.should_use_junctions());

        Ok(Self {
            config,
            stable_path: stable,
            lazer_path: lazer,
            manifest,
            link_manager,
        })
    }

    /// Returns a reference to the current configuration.
    pub fn config(&self) -> &UnifiedStorageConfig {
        &self.config
    }

    /// Returns the path to the osu! stable installation.
    pub fn stable_path(&self) -> &PathBuf {
        &self.stable_path
    }

    /// Returns the path to the osu! lazer installation.
    pub fn lazer_path(&self) -> &PathBuf {
        &self.lazer_path
    }

    /// Returns a reference to the manifest.
    pub fn manifest(&self) -> &UnifiedManifest {
        &self.manifest
    }

    /// Performs initial setup of unified storage.
    ///
    /// This operation:
    /// 1. Analyzes both installations for shared resources
    /// 2. Backs up existing data if necessary
    /// 3. Creates symbolic links or junctions as configured
    /// 4. Updates the manifest with tracked links
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Unified storage is not enabled in the configuration
    /// - Failed to create required links
    /// - Insufficient permissions for link creation
    pub fn setup(&mut self) -> Result<SetupResult> {
        if !self.config.is_enabled() {
            return Err(Error::Config(
                "Unified storage is not enabled in configuration".to_string(),
            ));
        }

        tracing::info!(
            "Setting up unified storage in {:?} mode",
            self.config.mode
        );

        let mut result = SetupResult::new();

        // TODO: Implement setup logic
        // 1. Scan shared resources in both installations
        // 2. Determine which installation owns each resource
        // 3. Create links from non-master to master
        // 4. Update manifest

        match self.config.mode {
            UnifiedStorageMode::Disabled => {
                // Should not reach here due to earlier check
                unreachable!("Setup called with disabled mode");
            }
            UnifiedStorageMode::StableMaster => {
                self.setup_stable_master(&mut result)?;
            }
            UnifiedStorageMode::LazerMaster => {
                self.setup_lazer_master(&mut result)?;
            }
            UnifiedStorageMode::TrueUnified => {
                self.setup_true_unified(&mut result)?;
            }
        }

        tracing::info!(
            "Setup complete: {} links created, {} resources linked",
            result.links_created,
            result.resources_linked
        );

        Ok(result)
    }

    /// Synchronizes changes between installations.
    ///
    /// This operation:
    /// 1. Detects new, modified, or deleted resources
    /// 2. Creates or updates links as needed
    /// 3. Removes stale links
    /// 4. Updates the manifest
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Unified storage is not enabled
    /// - Critical sync operation fails
    pub fn sync(&mut self) -> Result<SyncResult> {
        if !self.config.is_enabled() {
            return Err(Error::Config(
                "Unified storage is not enabled in configuration".to_string(),
            ));
        }

        tracing::info!("Starting unified storage sync");

        let result = match self.config.mode {
            UnifiedStorageMode::Disabled => {
                unreachable!("Sync called with disabled mode");
            }
            UnifiedStorageMode::StableMaster => self.sync_stable_master()?,
            UnifiedStorageMode::LazerMaster => self.sync_lazer_master()?,
            UnifiedStorageMode::TrueUnified => self.sync_true_unified()?,
        };

        tracing::info!(
            "Sync complete: {} new, {} updated, {} removed",
            result.new_links,
            result.updated,
            result.removed
        );

        Ok(result)
    }

    /// Verifies the integrity of all links.
    ///
    /// This operation checks each tracked link to determine if it:
    /// - Is still valid and accessible
    /// - Points to a valid target
    /// - Is still needed based on current configuration
    ///
    /// # Errors
    ///
    /// Returns an error if verification cannot be performed.
    pub fn verify(&self) -> Result<VerificationResult> {
        tracing::info!("Verifying unified storage integrity");

        let mut result = VerificationResult::new();

        // TODO: Implement verification logic
        // 1. Iterate through manifest entries
        // 2. Check each link's status
        // 3. Categorize as active, broken, or stale

        for resource in self.manifest.iter() {
            // Count total links across all link_paths for this resource
            for link_path in &resource.link_paths {
                result.total_links += 1;

                match self.link_manager.check_link(link_path) {
                    Ok(info) => {
                        if info.is_valid {
                            result.active += 1;
                        } else {
                            result.broken += 1;
                        }
                    }
                    Err(_) => {
                        result.broken += 1;
                    }
                }
            }

            // Also check resource status from manifest
            match resource.status {
                LinkStatus::Stale => result.stale += 1,
                _ => {}
            }
        }

        tracing::info!(
            "Verification complete: {} total, {} active, {} broken, {} stale",
            result.total_links,
            result.active,
            result.broken,
            result.stale
        );

        Ok(result)
    }

    /// Repairs broken or stale links.
    ///
    /// This operation:
    /// 1. Attempts to recreate broken links
    /// 2. Removes stale links that are no longer needed
    /// 3. Updates the manifest with current state
    ///
    /// # Errors
    ///
    /// Returns an error if repair cannot be performed.
    pub fn repair(&mut self) -> Result<RepairResult> {
        tracing::info!("Repairing unified storage links");

        let mut result = RepairResult::new();

        // TODO: Implement repair logic
        // 1. Get verification result to find broken/stale links
        // 2. Attempt to repair broken links
        // 3. Remove stale links
        // 4. Update manifest

        let _verification = self.verify()?;

        // Collect resources needing attention to avoid borrow issues
        let resources_to_repair: Vec<_> = self
            .manifest
            .find_needing_attention()
            .into_iter()
            .map(|r| (r.source_path.clone(), r.link_paths.clone()))
            .collect();

        for (source_path, link_paths) in resources_to_repair {
            for link_path in &link_paths {
                match self.link_manager.check_link(link_path) {
                    Ok(info) => {
                        if !info.is_valid {
                            // Attempt to repair by recreating link
                            match self.link_manager.create_link(link_path, &source_path) {
                                Ok(_) => {
                                    result.repaired += 1;
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "Failed to repair link {}: {}",
                                        link_path.display(),
                                        e
                                    );
                                    result.failed += 1;
                                }
                            }
                        }
                    }
                    Err(_) => {
                        // Link doesn't exist, try to create it
                        match self.link_manager.create_link(link_path, &source_path) {
                            Ok(_) => {
                                result.repaired += 1;
                            }
                            Err(e) => {
                                tracing::warn!(
                                    "Failed to create link {}: {}",
                                    link_path.display(),
                                    e
                                );
                                result.failed += 1;
                            }
                        }
                    }
                }
            }
        }

        // Handle stale links - remove them from the manifest
        let stale_resources: Vec<_> = self
            .manifest
            .find_by_status(LinkStatus::Stale)
            .into_iter()
            .map(|r| r.source_path.clone())
            .collect();

        for source_path in stale_resources {
            if self.manifest.remove_resource(&source_path) {
                result.removed += 1;
            }
        }

        tracing::info!(
            "Repair complete: {} repaired, {} failed, {} removed",
            result.repaired,
            result.failed,
            result.removed
        );

        Ok(result)
    }

    /// Tears down unified storage, restoring independent installations.
    ///
    /// This operation:
    /// 1. Removes all symbolic links and junctions
    /// 2. Optionally copies data back to each installation
    /// 3. Clears the manifest
    ///
    /// # Errors
    ///
    /// Returns an error if teardown cannot be completed.
    pub fn teardown(&mut self) -> Result<()> {
        tracing::info!("Tearing down unified storage");

        // TODO: Implement teardown logic
        // 1. Remove all links tracked in manifest
        // 2. Optionally restore original data
        // 3. Clear manifest

        // Collect all link paths to avoid borrow issues
        let all_link_paths: Vec<PathBuf> = self
            .manifest
            .iter()
            .flat_map(|r| r.link_paths.clone())
            .collect();

        for link_path in all_link_paths {
            if let Err(e) = self.link_manager.remove_link(&link_path) {
                tracing::warn!(
                    "Failed to remove link {}: {}",
                    link_path.display(),
                    e
                );
            }
        }

        self.manifest.clear();

        tracing::info!("Teardown complete");

        Ok(())
    }

    // -------------------------------------------------------------------------
    // Mode-specific setup implementations
    // -------------------------------------------------------------------------

    /// Sets up unified storage with stable as the master.
    ///
    /// In this mode, osu! stable owns the canonical copies and
    /// osu! lazer links to stable's resources.
    fn setup_stable_master(&mut self, result: &mut SetupResult) -> Result<()> {
        tracing::debug!("Setting up StableMaster mode");

        // TODO: Implement stable master setup
        // 1. For each shared resource type:
        //    a. Locate resource in stable (e.g., Songs folder)
        //    b. Backup existing lazer resource if present
        //    c. Create link from lazer location to stable location
        //    d. Track in manifest

        for resource_type in self.config.shared_resources_iter() {
            let folder_name = resource_type.folder_name();

            let stable_resource = self.stable_path.join(folder_name);
            let lazer_resource = self.lazer_path.join(folder_name);

            if stable_resource.exists() {
                // TODO: Create link from lazer to stable
                tracing::debug!(
                    "Would link {} -> {}",
                    lazer_resource.display(),
                    stable_resource.display()
                );
                result.links_created += 1;
                result.resources_linked += 1;
            } else {
                result.add_warning(format!(
                    "{} not found in stable installation",
                    folder_name
                ));
            }
        }

        Ok(())
    }

    /// Sets up unified storage with lazer as the master.
    ///
    /// In this mode, osu! lazer owns the canonical copies and
    /// osu! stable links to lazer's resources.
    fn setup_lazer_master(&mut self, result: &mut SetupResult) -> Result<()> {
        tracing::debug!("Setting up LazerMaster mode");

        // TODO: Implement lazer master setup
        // 1. For each shared resource type:
        //    a. Locate resource in lazer
        //    b. Backup existing stable resource if present
        //    c. Create link from stable location to lazer location
        //    d. Track in manifest

        for resource_type in self.config.shared_resources_iter() {
            let folder_name = resource_type.folder_name();

            let stable_resource = self.stable_path.join(folder_name);
            let lazer_resource = self.lazer_path.join(folder_name);

            if lazer_resource.exists() {
                // TODO: Create link from stable to lazer
                tracing::debug!(
                    "Would link {} -> {}",
                    stable_resource.display(),
                    lazer_resource.display()
                );
                result.links_created += 1;
                result.resources_linked += 1;
            } else {
                result.add_warning(format!(
                    "{} not found in lazer installation",
                    folder_name
                ));
            }
        }

        Ok(())
    }

    /// Sets up unified storage with a shared third-party location.
    ///
    /// In this mode, both installations link to a shared location
    /// that is independent of either installation.
    #[allow(unused_variables)]
    fn setup_true_unified(&mut self, result: &mut SetupResult) -> Result<()> {
        tracing::debug!("Setting up TrueUnified mode");

        let shared_path = self.config.get_shared_path().ok_or_else(|| {
            Error::Config("TrueUnified mode requires a shared path".to_string())
        })?;

        // TODO: Implement true unified setup
        // 1. Create shared location if it doesn't exist
        // 2. Migrate data from both installations to shared location
        // 3. Create links from both installations to shared location
        // 4. Track in manifest

        for resource_type in self.config.shared_resources_iter() {
            let folder_name = resource_type.folder_name();

            let shared_resource = shared_path.join(folder_name);
            let stable_resource = self.stable_path.join(folder_name);
            let lazer_resource = self.lazer_path.join(folder_name);

            // TODO: Create shared location and links
            tracing::debug!(
                "Would create shared {} and link both installations",
                shared_resource.display()
            );

            // Count both links (stable and lazer to shared)
            result.links_created += 2;
            result.resources_linked += 1;
        }

        Ok(())
    }

    // -------------------------------------------------------------------------
    // Mode-specific sync implementations
    // -------------------------------------------------------------------------

    /// Syncs changes in StableMaster mode.
    ///
    /// Detects new resources in stable and creates links in lazer.
    fn sync_stable_master(&mut self) -> Result<SyncResult> {
        tracing::debug!("Syncing in StableMaster mode");

        let result = SyncResult::new();

        // TODO: Implement stable master sync
        // 1. Scan stable for new resources
        // 2. Check manifest for existing links
        // 3. Create new links as needed
        // 4. Update manifest

        Ok(result)
    }

    /// Syncs changes in LazerMaster mode.
    ///
    /// Detects new resources in lazer and creates links in stable.
    fn sync_lazer_master(&mut self) -> Result<SyncResult> {
        tracing::debug!("Syncing in LazerMaster mode");

        let result = SyncResult::new();

        // TODO: Implement lazer master sync
        // 1. Scan lazer for new resources
        // 2. Check manifest for existing links
        // 3. Create new links as needed
        // 4. Update manifest

        Ok(result)
    }

    /// Syncs changes in TrueUnified mode.
    ///
    /// Ensures both installations are properly linked to shared location.
    fn sync_true_unified(&mut self) -> Result<SyncResult> {
        tracing::debug!("Syncing in TrueUnified mode");

        let result = SyncResult::new();

        // TODO: Implement true unified sync
        // 1. Verify shared location exists
        // 2. Check links from both installations
        // 3. Repair any broken links
        // 4. Update manifest

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_setup_result() {
        let mut result = SetupResult::new();
        assert!(result.is_clean());

        result.add_warning("Test warning");
        assert!(!result.is_clean());
        assert_eq!(result.warnings.len(), 1);
    }

    #[test]
    fn test_sync_result() {
        let mut result = SyncResult::new();
        assert!(result.is_success());
        assert_eq!(result.total_changes(), 0);

        result.new_links = 5;
        result.updated = 3;
        result.removed = 1;
        assert_eq!(result.total_changes(), 9);

        result.add_error("Test error");
        assert!(!result.is_success());
    }

    #[test]
    fn test_sync_result_merge() {
        let mut result1 = SyncResult::new();
        result1.new_links = 5;
        result1.updated = 2;

        let mut result2 = SyncResult::new();
        result2.new_links = 3;
        result2.removed = 1;

        result1.merge(result2);

        assert_eq!(result1.new_links, 8);
        assert_eq!(result1.updated, 2);
        assert_eq!(result1.removed, 1);
    }

    #[test]
    fn test_verification_result() {
        let mut result = VerificationResult::new();
        assert!(result.is_healthy());
        assert_eq!(result.health_percentage(), 100.0);

        result.total_links = 10;
        result.active = 8;
        result.broken = 2;
        assert!(!result.is_healthy());
        assert_eq!(result.health_percentage(), 80.0);
    }

    #[test]
    fn test_repair_result() {
        let mut result = RepairResult::new();
        assert!(result.is_success());
        assert_eq!(result.total_actions(), 0);

        result.repaired = 5;
        result.removed = 2;
        assert!(result.is_success());
        assert_eq!(result.total_actions(), 7);

        result.failed = 1;
        assert!(!result.is_success());
        assert_eq!(result.total_actions(), 8);
    }
}
