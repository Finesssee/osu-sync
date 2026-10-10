//! Hard-link helpers for the linked store.
//!
//! The linked store makes stable's beatmap assets hard links to the blobs in
//! lazer's `files` folder. These helpers classify a failed link, check that two
//! paths share a volume, and rename without replacing.

use std::io;
use std::path::{Path, PathBuf};

/// Why `fs::hard_link` failed, when the caller can recover by copying instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HardLinkFailure {
    /// The blob already has the most links the filesystem allows (1023 extra on NTFS).
    LinkLimit,
    /// Source and destination are on different volumes.
    CrossVolume,
    /// Anything else, which copying would not fix.
    Other,
}

/// The raw OS error a hard link returns at the link limit on this platform.
#[cfg(windows)]
pub const LINK_LIMIT_OS_ERROR: i32 = 1142; // ERROR_TOO_MANY_LINKS
/// The raw OS error a hard link returns across volumes on this platform.
#[cfg(windows)]
pub const CROSS_VOLUME_OS_ERROR: i32 = 17; // ERROR_NOT_SAME_DEVICE
/// The raw OS error a hard link returns at the link limit on this platform.
#[cfg(not(windows))]
pub const LINK_LIMIT_OS_ERROR: i32 = 31; // EMLINK
/// The raw OS error a hard link returns across volumes on this platform.
#[cfg(not(windows))]
pub const CROSS_VOLUME_OS_ERROR: i32 = 18; // EXDEV

/// Classifies a failed `fs::hard_link` by its raw OS error.
pub fn classify_hard_link_error(error: &io::Error) -> HardLinkFailure {
    match error.raw_os_error() {
        Some(LINK_LIMIT_OS_ERROR) => HardLinkFailure::LinkLimit,
        Some(CROSS_VOLUME_OS_ERROR) => HardLinkFailure::CrossVolume,
        _ => HardLinkFailure::Other,
    }
}

/// True when `a` and `b` live on the same volume, so a hard link between them can work.
/// A path that does not exist yet is judged by its nearest existing ancestor.
pub fn same_volume(a: &Path, b: &Path) -> io::Result<bool> {
    Ok(volume_id(&nearest_existing(a)?)? == volume_id(&nearest_existing(b)?)?)
}

fn nearest_existing(path: &Path) -> io::Result<PathBuf> {
    let path = std::path::absolute(path)?;
    path.ancestors()
        .find(|p| p.exists())
        .map(Path::to_path_buf)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, path.display().to_string()))
}

#[cfg(windows)]
fn volume_id(path: &Path) -> io::Result<String> {
    windows_impl::volume_path_name(path).map(|v| v.to_lowercase())
}

#[cfg(unix)]
fn volume_id(path: &Path) -> io::Result<String> {
    use std::os::unix::fs::MetadataExt;
    Ok(std::fs::metadata(path)?.dev().to_string())
}

#[cfg(not(any(windows, unix)))]
fn volume_id(_path: &Path) -> io::Result<String> {
    Ok(String::new())
}

/// Renames `from` to `to`, failing with `AlreadyExists` instead of replacing `to`.
pub fn rename_no_replace(from: &Path, to: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        windows_impl::move_file_no_replace(from, to)
    }
    #[cfg(not(windows))]
    {
        std::fs::hard_link(from, to)?;
        std::fs::remove_file(from)
    }
}

#[cfg(windows)]
mod windows_impl {
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    fn wide(path: &Path) -> Vec<u16> {
        path.as_os_str().encode_wide().chain(Some(0)).collect()
    }

    fn io_error(error: windows::core::Error) -> io::Error {
        let code = error.code().0 as u32;
        if code & 0xFFFF_0000 == 0x8007_0000 {
            io::Error::from_raw_os_error((code & 0xFFFF) as i32)
        } else {
            io::Error::other(error)
        }
    }

    pub fn volume_path_name(path: &Path) -> io::Result<String> {
        let path = wide(path);
        let mut out = vec![0u16; 1024];
        unsafe {
            windows::Win32::Storage::FileSystem::GetVolumePathNameW(
                windows::core::PCWSTR(path.as_ptr()),
                &mut out,
            )
        }
        .map_err(io_error)?;
        let len = out.iter().position(|&c| c == 0).unwrap_or(out.len());
        Ok(String::from_utf16_lossy(&out[..len]))
    }

    pub fn move_file_no_replace(from: &Path, to: &Path) -> io::Result<()> {
        let (from, to) = (wide(from), wide(to));
        unsafe {
            windows::Win32::Storage::FileSystem::MoveFileExW(
                windows::core::PCWSTR(from.as_ptr()),
                windows::core::PCWSTR(to.as_ptr()),
                windows::Win32::Storage::FileSystem::MOVE_FILE_FLAGS(0),
            )
        }
        .map_err(io_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn hard_link_errors_are_classified_by_os_code() {
        let classify = |code| classify_hard_link_error(&io::Error::from_raw_os_error(code));
        assert_eq!(classify(LINK_LIMIT_OS_ERROR), HardLinkFailure::LinkLimit);
        assert_eq!(
            classify(CROSS_VOLUME_OS_ERROR),
            HardLinkFailure::CrossVolume
        );
        assert_eq!(classify(5), HardLinkFailure::Other);
    }

    #[test]
    fn rename_no_replace_keeps_an_existing_destination() {
        let dir = TempDir::new().unwrap();
        let from = dir.path().join("from.part");
        let to = dir.path().join("to.osu");
        fs::write(&from, b"new").unwrap();
        fs::write(&to, b"old").unwrap();

        assert!(rename_no_replace(&from, &to).is_err());
        assert_eq!(fs::read(&to).unwrap(), b"old");

        let free = dir.path().join("free.osu");
        rename_no_replace(&from, &free).unwrap();
        assert_eq!(fs::read(&free).unwrap(), b"new");
        assert!(!from.exists());
    }

    #[test]
    fn paths_in_one_temp_dir_share_a_volume() {
        let dir = TempDir::new().unwrap();
        assert!(same_volume(dir.path(), &dir.path().join("not/yet/created")).unwrap());
    }

    #[cfg(windows)]
    #[test]
    fn ntfs_link_limit_is_classified() {
        let dir = TempDir::new().unwrap();
        let blob = dir.path().join("blob");
        fs::write(&blob, b"x").unwrap();
        let mut n = 0;
        let error = loop {
            match fs::hard_link(&blob, dir.path().join(format!("l{n}"))) {
                Ok(()) => n += 1,
                Err(e) => break e,
            }
        };
        assert_eq!(n, 1023);
        assert_eq!(classify_hard_link_error(&error), HardLinkFailure::LinkLimit);
    }
}
