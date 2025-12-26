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

/// Detect osu!lazer data directory
pub fn detect_lazer_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        // Windows: %APPDATA%/osu
        if let Some(appdata) = dirs::data_dir() {
            let path = appdata.join("osu");
            if path.exists() && path.join("client.realm").exists() {
                return Some(path);
            }
        }
        // Also check %LOCALAPPDATA%/osu for some installations
        if let Some(local) = dirs::data_local_dir() {
            let path = local.join("osu");
            if path.exists() && path.join("client.realm").exists() {
                return Some(path);
            }
        }

        // Scan all drives for portable lazer installations
        for drive in get_available_drives() {
            // Check common portable locations
            let candidates = [
                drive.join("osu"),
                drive.join("osu!lazer"),
                drive.join("Games").join("osu"),
                drive.join("Games").join("osu!lazer"),
            ];

            for path in candidates {
                if path.exists() && path.join("client.realm").exists() {
                    return Some(path);
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        // Linux: ~/.local/share/osu
        if let Some(data) = dirs::data_local_dir() {
            let path = data.join("osu");
            if path.exists() && path.join("client.realm").exists() {
                return Some(path);
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        // macOS: ~/Library/Application Support/osu
        if let Some(data) = dirs::data_dir() {
            let path = data.join("osu");
            if path.exists() && path.join("client.realm").exists() {
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
        // First check %LOCALAPPDATA%/osu! (common location)
        if let Some(local) = dirs::data_local_dir() {
            let osu_path = local.join("osu!");
            if osu_path.exists() && osu_path.join("Songs").exists() {
                return Some(osu_path);
            }
        }

        // Scan all available drives for osu! installations
        for drive in get_available_drives() {
            // Check common installation patterns on each drive
            let candidates = [
                drive.join("osu!"),
                drive.join("osu"),
                drive.join("Games").join("osu!"),
                drive.join("Games").join("osu"),
                drive.join("Program Files").join("osu!"),
                drive.join("Program Files (x86)").join("osu!"),
            ];

            for path in candidates {
                if path.exists() && path.join("Songs").exists() {
                    return Some(path);
                }
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        // osu!stable on Linux typically runs through Wine
        // Check common Wine prefixes
        if let Some(home) = dirs::home_dir() {
            let wine_paths = [
                home.join(".wine/drive_c/osu!"),
                home.join(".local/share/osu-wine/osu!"),
                home.join("Games/osu!"),
            ];

            for path in wine_paths {
                if path.exists() && path.join("Songs").exists() {
                    return Some(path);
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        // osu!stable on macOS runs through Wine/CrossOver
        if let Some(home) = dirs::home_dir() {
            let candidates = [
                home.join("Library/Application Support/osu-wine/osu!"),
                home.join(".wine/drive_c/osu!"),
            ];

            for path in candidates {
                if path.exists() && path.join("Songs").exists() {
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
