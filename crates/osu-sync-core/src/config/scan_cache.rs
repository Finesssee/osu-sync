//! Scan cache locations.
//!
//! Scan caches live in a per-user cache dir (`%LOCALAPPDATA%\osu-sync` on Windows), one file per
//! install, named `<kind>-<first 16 hex of blake3(canonical install path)>.<ext>`. A scan never
//! writes into a game folder.

use std::path::{Path, PathBuf};

/// The per-user cache dir, or `None` when the platform has none (caching is then skipped).
pub fn default_root() -> Option<PathBuf> {
    dirs::cache_dir().map(|dir| dir.join("osu-sync"))
}

/// The cache file for `install` under `root`.
pub fn file_for(root: &Path, kind: &str, install: &Path, ext: &str) -> PathBuf {
    let canonical = install
        .canonicalize()
        .unwrap_or_else(|_| install.to_path_buf());
    let hash = blake3::hash(canonical.to_string_lossy().as_bytes());
    root.join(format!("{kind}-{}.{ext}", &hash.to_hex()[..16]))
}

/// Write `bytes` to `path`, creating the cache dir first.
pub fn write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn file_for_keys_by_install_path() {
        let root = Path::new("C:/cache/osu-sync");
        let a = file_for(root, "lazer", Path::new("Z:/no/such/install-a"), "json");
        let b = file_for(root, "lazer", Path::new("Z:/no/such/install-b"), "json");

        assert_eq!(a.parent(), Some(root));
        let name = a.file_name().unwrap().to_str().unwrap();
        assert!(name.starts_with("lazer-") && name.ends_with(".json"));
        assert_eq!(name.len(), "lazer-".len() + 16 + ".json".len());
        assert_ne!(a, b);
        assert_eq!(
            a,
            file_for(root, "lazer", Path::new("Z:/no/such/install-a"), "json")
        );
    }

    #[test]
    fn write_creates_cache_dir() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("osu-sync").join("stable-0000.bin");

        write(&path, b"abc").unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), b"abc");
    }
}
