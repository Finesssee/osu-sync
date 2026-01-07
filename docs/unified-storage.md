# Unified Storage Feature

## Overview

The Unified Storage feature allows users to combine osu!stable and osu!lazer installations into a shared folder structure using symlinks/junctions. This saves disk space and keeps both installations in sync automatically.

## User Requirements

| Requirement | Implementation |
|-------------|----------------|
| Source modes | Stable Master, Lazer Master, True Unified |
| Resources | Beatmaps, Skins, Replays, Screenshots, Exports, Backgrounds |
| Sync triggers | File watcher + Manual trigger + On game launch |
| Mechanism | Symlinks/Junctions (not file copying) |

## Architecture

### Core Module Structure

```
crates/osu-sync-core/src/unified/
├── mod.rs           # Module exports and documentation
├── config.rs        # Configuration types
├── manifest.rs      # Link tracking manifest
├── link.rs          # Platform-specific symlink/junction operations
├── engine.rs        # UnifiedStorageEngine orchestration
├── watcher.rs       # File system watcher (notify crate)
├── game_detect.rs   # Game launch detection
└── migration.rs     # Migration from separate to unified
```

### CLI Screens

```
crates/osu-sync-cli/src/screens/
├── unified_config.rs   # Configuration UI
├── unified_setup.rs    # Setup progress screen
└── unified_status.rs   # Status dashboard
```

## Storage Modes

### 1. Disabled (Default)
- Standard copy-based sync
- Files are duplicated between installations
- No symlinks or junctions created

### 2. Stable as Master
- Beatmaps live in stable's `Songs/` folder
- Lazer imports from stable via `.osz` files in import folder
- New stable beatmaps trigger lazer import automatically

### 3. Lazer as Master
- Beatmaps extracted from lazer's hash store to shared folder
- Junctions created from stable's `Songs/` to shared folder
- New lazer beatmaps get extracted and linked

### 4. True Unified
- New shared folder structure created:
  ```
  /shared/osu-unified/
  ├── beatmaps/{SetID} Artist - Title/
  ├── skins/SkinName/
  ├── replays/
  └── screenshots/
  ```
- Stable's `Songs/` becomes junction to `shared/beatmaps/`
- Lazer uses import folder approach or hash-links

## Windows Symlink Strategy

The implementation uses a cascading approach for maximum compatibility:

1. **Prefer NTFS Junctions** (directories) - No admin rights needed
2. **Try Symlinks** (files) - May require Developer Mode
3. **Request Elevation** (UAC prompt) - If symlinks fail
4. **Fallback to Copy** - With warning if user declines elevation

```rust
pub enum LinkCapability {
    Full,           // Can create all link types
    JunctionsOnly,  // Can only create junctions (Windows non-admin)
    None,           // Cannot create any links
}
```

## Key Data Structures

### UnifiedStorageConfig

```rust
pub struct UnifiedStorageConfig {
    pub mode: UnifiedStorageMode,
    pub shared_path: Option<PathBuf>,
    pub shared_resources: HashSet<SharedResourceType>,
    pub triggers: SyncTriggers,
    pub use_junctions: bool,
    pub track_manifest: bool,
}
```

### SyncTriggers

```rust
pub struct SyncTriggers {
    pub file_watcher: bool,      // Background monitoring
    pub on_game_launch: bool,    // Sync when game starts
    pub manual: bool,            // Manual trigger only
    pub watcher_interval_secs: u64,
}
```

### UnifiedManifest

Tracks all created links for verification and repair:

```rust
pub struct LinkedResource {
    pub resource_type: SharedResourceType,
    pub source_path: PathBuf,
    pub link_paths: Vec<PathBuf>,
    pub content_hash: Option<String>,
    pub status: LinkStatus,  // Active, Broken, Stale, Pending
}
```

## File Watcher

Uses the `notify` crate for cross-platform file watching:

- Debounces events to avoid duplicate processing
- Filters temporary files (.tmp, .partial, etc.)
- Configurable polling interval
- Events: Created, Modified, Deleted, Renamed

## Game Launch Detection

Cross-platform process detection:

| Platform | Method |
|----------|--------|
| Windows | Win32 ProcessStatus APIs via `sysinfo` |
| Linux | `/proc` filesystem |
| macOS | `sysinfo` crate |

Detects:
- `osu!.exe` (stable)
- `osu!lazer.exe` or `osu!.exe` in lazer directory

## Migration System

### Migration Steps

1. `CheckPrerequisites` - Verify games closed, disk space, permissions
2. `CreateSharedFolder` - Create unified directory structure
3. `BackupOriginal` - Create backup manifest
4. `CopyBeatmaps` - Copy files to shared location
5. `CreateJunctions` - Create symlinks/junctions
6. `UpdateManifest` - Record all links
7. `VerifyIntegrity` - Validate both games can access files
8. `CleanupBackups` - Remove temporary files

### Rollback Support

If migration fails at any step, the system can rollback:
- Remove created junctions/symlinks
- Restore original folder structure
- Clear migration manifest

## Dependencies Added

```toml
# File watching
notify = "6"
notify-debouncer-mini = "0.4"

# Process detection
sysinfo = "0.30"

# Windows-specific
[target.'cfg(windows)'.dependencies]
windows = { version = "0.54", features = [
    "Win32_System_ProcessStatus",
    "Win32_Foundation",
    "Win32_System_Threading",
    "Win32_Storage_FileSystem",
] }
```

## Error Handling

New error variants added:

```rust
#[error("Unified storage error: {0}")]
UnifiedStorage(String),

#[error("Failed to create symlink/junction from {source} to {link}: {message}")]
LinkCreation { source: PathBuf, link: PathBuf, message: String },

#[error("Symlink/junction is broken: {path}")]
BrokenLink { path: PathBuf },

#[error("Elevated privileges required for symlink creation")]
ElevationRequired,

#[error("Game is currently running: {game}")]
GameRunning { game: String },

#[error("Migration failed at step '{step}': {message}")]
MigrationFailed { step: String, message: String },

#[error("File watcher error: {0}")]
WatcherError(String),

#[error("Manifest error: {0}")]
ManifestError(String),
```

## UI Screens

### Configuration Screen
- Mode selection with radio buttons
- Shared folder path input (for TrueUnified)
- Resource type checkboxes
- Sync trigger toggles
- Apply/Cancel buttons

### Setup Progress Screen
- Current operation display
- Progress bar with percentage
- Bytes processed / total
- Log of completed steps
- Cancel button

### Status Dashboard
- Current mode display
- Link health gauge (active/broken/stale)
- Storage statistics (space used/saved)
- Recent sync events
- Quick actions: Verify, Repair, Sync Now, Configure

## Future Enhancements

1. **Headless CLI Commands** - Expose unified storage actions via CLI flags
2. **Cross-Platform Validation** - Expand automated tests for Linux/macOS
3. **Background Health Checks** - Periodic verify/repair scheduling

## Testing

Run tests with:
```bash
cargo test -p osu-sync-core unified
```

## Implementation Status

| Component | Status |
|-----------|--------|
| Core module structure | ✅ Complete |
| Configuration types | ✅ Complete |
| Manifest tracking | ✅ Complete |
| Link operations | ✅ Complete |
| Storage engine | ✅ Complete |
| File watcher | ✅ Complete |
| Game detection | ✅ Complete |
| Migration system | ✅ Complete |
| Config screen | ✅ Complete |
| Setup screen | ✅ Complete |
| Status screen | ✅ Complete |
| Main menu integration | ✅ Complete |
| Worker integration | ✅ Complete |
| Full app integration | ✅ Complete |
