# Changelog

> Last updated: 2025-12-28

All notable changes to osu-sync will be documented in this file.

## [Unreleased]

### Added

#### Beatmap Selection in Dry Run Preview
- **Checkbox selection**: Each beatmap set now has a checkbox (`[x]`/`[ ]`) in the Dry Run Preview screen
- **Keyboard shortcuts**:
  - `Space`: Toggle selection on current item
  - `Ctrl+A`: Select all importable beatmaps
  - `Ctrl+D`: Deselect all beatmaps
  - `Ctrl+I`: Invert selection (toggle all)
  - `/`: Enter filter mode to search by title/artist/set ID
  - `Esc` (in filter mode): Exit filter mode and clear filter
- **Filter functionality**: Type to filter visible beatmaps while preserving selection state
- **Selection statistics**: Shows count of selected beatmaps in real-time
- **Default behavior**: All beatmaps with "Import" action are selected by default

#### Auto-Scan on Startup
- Application now automatically scans for osu! installations when launched
- No longer requires manually selecting "Scan Installations" from the menu
- Menu item renamed from "Scan Installations" to "Rescan Installations"

#### Improved Path Detection
- Detection no longer relies on specific folder names (e.g., `osu!` or `osu!lazer`)
- Uses signature file detection to identify installations:
  - **osu!stable**: Looks for `Songs` folder + (`osu!.exe` OR `osu!.db` OR `collection.db` OR `scores.db`)
  - **osu!lazer**: Looks for `client.realm` file
- Scans all drive roots and common game directories
- Works even if osu! folder is renamed or in non-standard location

### Fixed

#### Build Fixes
- Fixed `to_string_lossy()` being called on `&str` instead of `OsStr` in game detection
- Fixed "cannot move out of type implementing Drop trait" error in `GameLaunchDetector`
- Added missing `unified_storage` field to Config initialization
- Made `start_scan()` method public for auto-scan functionality

### Changed

- Main menu now has 11 items (added "Unified Storage" placeholder)
- DryRunPreview state now tracks: `checked_items`, `filter_text`, `filter_mode`

---

## Development Notes

### Testing the Beatmap Selection Feature

1. Launch osu-sync: `target/release/osu-sync.exe`
2. Wait for auto-scan to complete (shows detected installations)
3. Press `Enter` to go to Main Menu
4. Select "Sync Beatmaps" and press `Enter`
5. Choose a sync direction (e.g., Stable to Lazer)
6. Press `d` for Dry Run
7. In the Dry Run Preview:
   - Use `Up`/`Down` to navigate
   - Use `Space` to toggle individual items
   - Use `Ctrl+A` to select all
   - Use `Ctrl+D` to deselect all
   - Use `/` to filter by name
   - Press `Enter` to sync selected beatmaps

### Path Detection Test

Run the detection example:
```bash
cargo run --example detect_paths
```

Expected output:
```
Detecting osu!stable...
  FOUND: D:\osu!
Detecting osu!lazer...
  FOUND: C:\Users\<user>\AppData\Roaming\osu
```
