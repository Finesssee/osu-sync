//! Platform-specific path detection for osu! installations

use std::path::PathBuf;

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
fn is_stable_installation(path: &PathBuf) -> bool {
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
fn is_lazer_installation(path: &PathBuf) -> bool {
    if !path.exists() || !path.is_dir() {
        return false;
    }

    path.join("client.realm").exists()
}

/// Scan a directory for osu! installations (non-recursive, checks immediate children)
#[cfg(target_os = "windows")]
fn scan_directory_for_stable(dir: &PathBuf) -> Option<PathBuf> {
    if !dir.exists() || !dir.is_dir() {
        return None;
    }

    // First check if this directory itself is osu!
    if is_stable_installation(dir) {
        return Some(dir.clone());
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
fn scan_directory_for_lazer(dir: &PathBuf) -> Option<PathBuf> {
    if !dir.exists() || !dir.is_dir() {
        return None;
    }

    // First check if this directory itself is lazer
    if is_lazer_installation(dir) {
        return Some(dir.clone());
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
            let path = appdata.join("osu");
            if is_lazer_installation(&path) {
                return Some(path);
            }
        }
        if let Some(local) = dirs::data_local_dir() {
            let path = local.join("osu");
            if is_lazer_installation(&path) {
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
            let path = data.join("osu");
            if is_lazer_installation(&path) {
                return Some(path);
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(data) = dirs::data_dir() {
            let path = data.join("osu");
            if is_lazer_installation(&path) {
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
pub fn validate_stable_path(path: &PathBuf) -> bool {
    path.exists() && path.join("Songs").is_dir()
}

/// Validate that a path is a valid osu!lazer data directory
pub fn validate_lazer_path(path: &PathBuf) -> bool {
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
}
