//! Refuses writes under the auto-detected osu!stable and osu!lazer folders unless
//! the process opted in with `--allow-live`.

use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use super::{detect_lazer_path, detect_stable_path};
use crate::error::{Error, Result};

/// The live install folders a write must stay out of.
#[derive(Debug, Clone, Default)]
pub struct LiveRoots {
    pub stable: Option<PathBuf>,
    pub lazer: Option<PathBuf>,
}

impl LiveRoots {
    pub fn detect() -> Self {
        Self {
            stable: detect_stable_path(),
            lazer: detect_lazer_path(),
        }
    }

    /// Returns the live root that contains `dest`, if any.
    pub fn containing(&self, dest: &Path) -> Option<&Path> {
        let dest = resolve(dest);
        [&self.stable, &self.lazer]
            .into_iter()
            .flatten()
            .find(|root| is_under(&dest, &resolve(root)))
            .map(PathBuf::as_path)
    }

    pub fn check(&self, dest: &Path) -> Result<()> {
        match self.containing(dest) {
            Some(root) => Err(Error::LiveWriteRefused {
                dest: dest.to_path_buf(),
                root: root.to_path_buf(),
            }),
            None => Ok(()),
        }
    }
}

static ALLOW_LIVE: AtomicBool = AtomicBool::new(false);
static DETECTED: OnceLock<LiveRoots> = OnceLock::new();

pub fn set_allow_live(allow: bool) {
    ALLOW_LIVE.store(allow, Ordering::SeqCst);
}

/// Entry-point check for every operation that writes into a game folder.
pub fn check_write(dest: &Path) -> Result<()> {
    if ALLOW_LIVE.load(Ordering::SeqCst) {
        return Ok(());
    }
    DETECTED.get_or_init(LiveRoots::detect).check(dest)
}

/// True when `path` equals `root` or lies inside it, compared component by component.
pub fn is_under(path: &Path, root: &Path) -> bool {
    let mut path = path.components();
    root.components().all(|r| path.next() == Some(r))
}

/// Canonicalizes the longest existing ancestor and re-appends the rest, so a
/// destination that does not exist yet still resolves through junctions and `..`.
fn resolve(path: &Path) -> PathBuf {
    let mut tail = Vec::new();
    let mut current = path;
    loop {
        if let Ok(canonical) = current.canonicalize() {
            return tail
                .iter()
                .rev()
                .fold(canonical, |acc: PathBuf, part| acc.join(part));
        }
        match (current.parent(), current.components().next_back()) {
            (Some(parent), Some(Component::Normal(part))) => {
                tail.push(part.to_os_string());
                current = parent;
            }
            _ => return path.to_path_buf(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn roots(dir: &TempDir) -> LiveRoots {
        let stable = dir.path().join("osu!");
        let lazer = dir.path().join("osu!lazer");
        std::fs::create_dir_all(stable.join("Songs")).unwrap();
        std::fs::create_dir_all(lazer.join("files")).unwrap();
        LiveRoots {
            stable: Some(stable),
            lazer: Some(lazer),
        }
    }

    #[test]
    fn refuses_write_under_live_path() {
        let dir = TempDir::new().unwrap();
        let roots = roots(&dir);

        let songs = dir.path().join("osu!").join("Songs").join("123 A - B");
        let err = roots.check(&songs).unwrap_err();
        assert!(matches!(
            &err,
            Error::LiveWriteRefused { root, .. } if root == &dir.path().join("osu!")
        ));

        let blob = dir.path().join("osu!lazer").join("files").join("ab");
        assert_eq!(
            roots.containing(&blob),
            Some(dir.path().join("osu!lazer").as_path())
        );

        let dotdot = dir.path().join("sandbox").join("..").join("osu!");
        assert_eq!(
            roots.containing(&dotdot),
            Some(dir.path().join("osu!").as_path())
        );
    }

    #[test]
    fn allows_write_under_sandbox() {
        let dir = TempDir::new().unwrap();
        let roots = roots(&dir);

        for dest in [
            dir.path()
                .join("osu-sync-sandbox")
                .join("stable")
                .join("Songs"),
            dir.path().join("osu!lazer-copy").join("files"),
            dir.path().join("osu"),
        ] {
            assert!(roots.check(&dest).is_ok(), "{} was refused", dest.display());
        }
    }

    #[test]
    fn is_under_compares_components_not_strings() {
        assert!(is_under(Path::new("D:/osu!/Songs"), Path::new("D:/osu!")));
        assert!(is_under(Path::new("D:/osu!"), Path::new("D:/osu!")));
        assert!(!is_under(Path::new("D:/osu!lazer"), Path::new("D:/osu!")));
        assert!(!is_under(
            Path::new("D:/osu-sync-sandbox/stable"),
            Path::new("D:/osu!")
        ));
        assert!(!is_under(Path::new("D:/"), Path::new("D:/osu!")));
    }
}
