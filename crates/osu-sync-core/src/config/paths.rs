//! Platform-specific path detection for osu! installations

use std::path::{Path, PathBuf};

/// Get all available drive letters on Windows
#[cfg(target_os = "windows")]
fn get_available_drives() -> Vec<PathBuf> {
    let mut drives = Vec::new();
    // Check drives A-Z
    for letter in b'A'..=b'Z' {
        let drive = format!("{}:\\", letter as char);
        let path = PathBuf::from(&drive);
        if path.exists() {
            drives.push(path);
        }
    }
    drives
}

/// Check if a path is a valid osu!stable installation
/// Looks for: Songs folder + (osu!.exe OR osu!.db OR collection.db)
fn is_stable_installation(path: &Path) -> bool {
    if !path.exists() || !path.is_dir() {
        return false;
    }

    let songs = path.join("Songs");
    if !songs.exists() || !songs.is_dir() {
        return false;
    }

    // Confirm it's actually osu! by checking for signature files
    path.join("osu!.exe").exists()
        || path.join("osu!.db").exists()
        || path.join("collection.db").exists()
        || path.join("scores.db").exists()
}

/// Check if a path is a valid osu!lazer data directory
/// Looks for: client.realm file
fn is_lazer_installation(path: &Path) -> bool {
    if !path.exists() || !path.is_dir() {
        return false;
    }

    path.join("client.realm").exists()
}

/// Read the custom data location lazer records in `storage.ini` (`FullPath = ...`)
/// when the user moves their data via Settings > Maintenance > Change location.
/// Parsed like osu-framework's ini reader: BOM tolerated, case-sensitive key,
/// last line wins. A relative value is ignored rather than resolved against
/// the process working directory.
fn read_storage_ini(dir: &Path) -> Option<PathBuf> {
    let content = std::fs::read_to_string(dir.join("storage.ini")).ok()?;
    let (_, value) = content
        .trim_start_matches('\u{feff}')
        .lines()
        .filter_map(|line| line.split_once('='))
        .filter(|(key, _)| key.trim() == "FullPath")
        .last()?;
    let path = PathBuf::from(value.trim());
    path.is_absolute().then_some(path)
}

/// Resolve lazer's data directory from its default location, following `storage.ini`.
/// A relocated install leaves a stale `client.realm` behind, so the redirect wins.
fn resolve_lazer_dir(default: &Path) -> Option<PathBuf> {
    if let Some(custom) = read_storage_ini(default) {
        if is_lazer_installation(&custom) {
            return Some(custom);
        }
    }
    is_lazer_installation(default).then(|| default.to_path_buf())
}

/// Scan a directory for osu! installations (non-recursive, checks immediate children)
#[cfg(target_os = "windows")]
fn scan_directory_for_stable(dir: &Path) -> Option<PathBuf> {
    if !dir.exists() || !dir.is_dir() {
        return None;
    }

    // First check if this directory itself is osu!
    if is_stable_installation(dir) {
        return Some(dir.to_path_buf());
    }

    // Then check immediate children
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && is_stable_installation(&path) {
                return Some(path);
            }
        }
    }

    None
}

/// Scan a directory for osu!lazer installations (non-recursive, checks immediate children)
#[cfg(target_os = "windows")]
fn scan_directory_for_lazer(dir: &Path) -> Option<PathBuf> {
    if !dir.exists() || !dir.is_dir() {
        return None;
    }

    // First check if this directory itself is lazer
    if is_lazer_installation(dir) {
        return Some(dir.to_path_buf());
    }

    // Then check immediate children
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && is_lazer_installation(&path) {
                return Some(path);
            }
        }
    }

    None
}

/// Detect osu!lazer data directory
pub fn detect_lazer_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        // Priority 1: Standard locations with known names
        if let Some(appdata) = dirs::data_dir() {
            if let Some(path) = resolve_lazer_dir(&appdata.join("osu")) {
                return Some(path);
            }
        }
        if let Some(local) = dirs::data_local_dir() {
            if let Some(path) = resolve_lazer_dir(&local.join("osu")) {
                return Some(path);
            }
        }

        // Priority 2: Scan common directories on all drives
        for drive in get_available_drives() {
            // Check common game directories (scans children too)
            let scan_dirs = [
                drive.clone(),
                drive.join("Games"),
                drive.join("Program Files"),
                drive.join("Program Files (x86)"),
            ];

            for dir in &scan_dirs {
                if let Some(path) = scan_directory_for_lazer(dir) {
                    return Some(path);
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(data) = dirs::data_local_dir() {
            if let Some(path) = resolve_lazer_dir(&data.join("osu")) {
                return Some(path);
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(data) = dirs::data_dir() {
            if let Some(path) = resolve_lazer_dir(&data.join("osu")) {
                return Some(path);
            }
        }
    }

    None
}

/// Detect osu!stable installation directory
pub fn detect_stable_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        // Priority 1: Standard location
        if let Some(local) = dirs::data_local_dir() {
            let osu_path = local.join("osu!");
            if is_stable_installation(&osu_path) {
                return Some(osu_path);
            }
        }

        // Priority 2: Scan common directories on all drives
        // This will find osu! even if the folder is renamed
        for drive in get_available_drives() {
            // Check common game directories (scans children too)
            let scan_dirs = [
                drive.clone(),
                drive.join("Games"),
                drive.join("Program Files"),
                drive.join("Program Files (x86)"),
            ];

            for dir in &scan_dirs {
                if let Some(path) = scan_directory_for_stable(dir) {
                    return Some(path);
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Some(home) = dirs::home_dir() {
            let wine_paths = [
                home.join(".wine/drive_c/osu!"),
                home.join(".local/share/osu-wine/osu!"),
                home.join("Games/osu!"),
            ];

            for path in wine_paths {
                if is_stable_installation(&path) {
                    return Some(path);
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = dirs::home_dir() {
            let candidates = [
                home.join("Library/Application Support/osu-wine/osu!"),
                home.join(".wine/drive_c/osu!"),
            ];

            for path in candidates {
                if is_stable_installation(&path) {
                    return Some(path);
                }
            }
        }
    }

    None
}

/// Validate that a path is a valid osu!stable installation
pub fn validate_stable_path(path: &Path) -> bool {
    path.exists() && path.join("Songs").is_dir()
}

/// Validate that a path is a valid osu!lazer data directory
pub fn validate_lazer_path(path: &Path) -> bool {
    path.exists() && path.join("client.realm").is_file() && path.join("files").is_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_paths() {
        // These tests just verify the functions run without panicking
        let _ = detect_lazer_path();
        let _ = detect_stable_path();
    }

    fn make_lazer_dir(path: &Path) {
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(path.join("client.realm"), b"").unwrap();
    }

    #[test]
    fn test_resolve_lazer_dir_follows_storage_ini() {
        let tmp = tempfile::tempdir().unwrap();
        let default = tmp.path().join("osu");
        let custom = tmp.path().join("osu!lazer");
        make_lazer_dir(&default); // stale realm left behind after relocation
        make_lazer_dir(&custom);
        std::fs::write(
            default.join("storage.ini"),
            format!("FullPath = {}\r\n", custom.display()),
        )
        .unwrap();

        assert_eq!(resolve_lazer_dir(&default), Some(custom));
    }

    fn write_storage_ini(dir: &Path, content: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("storage.ini"), content).unwrap();
    }

    #[test]
    fn test_resolve_lazer_dir_follows_redirect_without_default_realm() {
        let tmp = tempfile::tempdir().unwrap();
        let default = tmp.path().join("osu");
        let custom = tmp.path().join("osu!lazer");
        make_lazer_dir(&custom);
        write_storage_ini(&default, &format!("FullPath = {}\r\n", custom.display()));

        assert_eq!(resolve_lazer_dir(&default), Some(custom));
    }

    #[test]
    fn test_read_storage_ini_matches_lazer_parsing() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("osu");
        let a = tmp.path().join("a");
        let b = tmp.path().join("b");

        write_storage_ini(&dir, &format!("\u{feff}FullPath = {}\r\n", a.display()));
        assert_eq!(read_storage_ini(&dir), Some(a.clone()));

        write_storage_ini(
            &dir,
            &format!("FullPath = {}\nFullPath = {}\n", a.display(), b.display()),
        );
        assert_eq!(read_storage_ini(&dir), Some(b));

        write_storage_ini(&dir, &format!("FullPath = {}\nFullPath =\n", a.display()));
        assert_eq!(read_storage_ini(&dir), None);

        write_storage_ini(&dir, &format!("fullpath = {}\n", a.display()));
        assert_eq!(read_storage_ini(&dir), None);

        write_storage_ini(&dir, "FullPath = relative\\dir\n");
        assert_eq!(read_storage_ini(&dir), None);
    }

    #[test]
    fn test_resolve_lazer_dir_ignores_invalid_redirect() {
        let tmp = tempfile::tempdir().unwrap();
        let default = tmp.path().join("osu");
        make_lazer_dir(&default);
        std::fs::write(default.join("storage.ini"), "FullPath = Z:\\missing\n").unwrap();

        assert_eq!(resolve_lazer_dir(&default), Some(default.clone()));
    }

    #[test]
    fn test_resolve_lazer_dir_without_storage_ini() {
        let tmp = tempfile::tempdir().unwrap();
        let default = tmp.path().join("osu");
        assert_eq!(resolve_lazer_dir(&default), None);

        make_lazer_dir(&default);
        assert_eq!(resolve_lazer_dir(&default), Some(default.clone()));
    }
}
