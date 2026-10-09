//! Refuses writes under the live osu!stable and osu!lazer folders unless the
//! process opted in with `--allow-live`. The live folders are the auto-detected
//! installs, every lazer data folder the default locations lead to, and the paths
//! saved in the config file.

use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use same_file::Handle;
use serde::Deserialize;

use super::paths::{lazer_default_dirs, lazer_live_dirs};
use super::{detect_lazer_path, detect_stable_path, lazer_path_overridden, Config};
use crate::error::{Error, Result};
use crate::sync::SyncDirection;

/// One stable folder and one lazer data folder, either of which may be unknown.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct InstallPaths {
    #[serde(rename = "stable_path")]
    pub stable: Option<PathBuf>,
    #[serde(rename = "lazer_path")]
    pub lazer: Option<PathBuf>,
}

impl InstallPaths {
    fn iter(&self) -> impl Iterator<Item = &PathBuf> {
        [&self.stable, &self.lazer].into_iter().flatten()
    }
}

/// The install paths saved in an osu-sync config file, and that file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedPaths {
    pub file: PathBuf,
    pub paths: InstallPaths,
}

impl SavedPaths {
    /// Reads the paths saved in a config file, ignoring every other field.
    pub fn read(file: &Path) -> Self {
        let paths = std::fs::read_to_string(file)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default();
        Self {
            file: file.to_path_buf(),
            paths,
        }
    }
}

/// Why a folder counts as live, which decides what a refusal tells the user to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RootOrigin {
    /// Found by install detection.
    Detected,
    /// Saved as an install path in this config file.
    Saved(PathBuf),
}

impl RootOrigin {
    pub(crate) fn refusal(&self, root: &Path) -> String {
        match self {
            Self::Detected => format!(
                "it is inside the live install {}. Pass --stable-path and --lazer-path to a sandbox, or --allow-live to write to the live install.",
                root.display()
            ),
            Self::Saved(file) => format!(
                "it is inside {}, which is saved as an install path in {}. Remove it from that file, or pass --allow-live to write to it.",
                root.display(),
                file.display()
            ),
        }
    }
}

/// What a stable-to-lazer sync does with the .osz files it wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LazerLaunch {
    /// Start the installed osu!lazer with each file, which imports into its own data folder.
    Launch,
    /// Leave the files in the import folder.
    Stage(StageReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageReason {
    /// The game would import into its own data folder, not the overridden one.
    LazerPathOverridden,
    /// No live lazer folder is known, so the game's import target is unknown.
    NoLiveLazer,
}

/// The live install folders a write must stay out of.
#[derive(Debug)]
pub struct LiveRoots {
    detected: InstallPaths,
    roots: Vec<Root>,
}

#[derive(Debug)]
struct Root {
    path: PathBuf,
    origin: RootOrigin,
    resolved: PathBuf,
    handle: Option<Handle>,
}

impl LiveRoots {
    /// `lazer_dirs` are the lazer data folders the default locations lead to, which
    /// can include a default folder that redirects elsewhere through storage.ini.
    pub fn new(
        detected: InstallPaths,
        lazer_dirs: Vec<PathBuf>,
        saved: Option<SavedPaths>,
    ) -> Self {
        let found = detected
            .iter()
            .chain(&lazer_dirs)
            .map(|path| (path, RootOrigin::Detected));
        let saved = saved.iter().flat_map(|saved| {
            saved
                .paths
                .iter()
                .map(|path| (path, RootOrigin::Saved(saved.file.clone())))
        });
        let mut roots: Vec<Root> = Vec::new();
        for (path, origin) in found.chain(saved) {
            if roots.iter().any(|r| &r.path == path) {
                continue;
            }
            roots.push(Root {
                path: path.clone(),
                origin,
                resolved: resolve(path)
                    .or_else(|| std::path::absolute(path).ok())
                    .unwrap_or_else(|| path.clone()),
                handle: Handle::from_path(path).ok(),
            });
        }
        Self { detected, roots }
    }

    pub fn detect() -> Self {
        let detected = InstallPaths {
            stable: detect_stable_path(),
            lazer: detect_lazer_path(),
        };
        let lazer_dirs = lazer_default_dirs()
            .iter()
            .flat_map(|default| lazer_live_dirs(default))
            .collect();
        let saved = Config::config_path().map(|file| SavedPaths::read(&file));
        Self::new(detected, lazer_dirs, saved)
    }

    pub fn check(&self, dest: &Path) -> Result<()> {
        let resolved = resolve(dest).ok_or_else(|| Error::LiveWriteUnresolved {
            dest: dest.to_path_buf(),
        })?;
        let root = self
            .roots
            .iter()
            .find(|root| is_under(&resolved, &root.resolved))
            .or_else(|| self.root_by_identity(&resolved));
        match root {
            Some(root) => Err(Error::LiveWriteRefused {
                dest: dest.to_path_buf(),
                root: root.path.clone(),
                origin: root.origin.clone(),
            }),
            None => Ok(()),
        }
    }

    /// Finds a root that is the same directory as an existing ancestor of `dest`. This
    /// catches a live folder reached through a UNC share, a mapped drive or subst.
    fn root_by_identity(&self, dest: &Path) -> Option<&Root> {
        dest.ancestors()
            .filter_map(|ancestor| Handle::from_path(ancestor).ok())
            .find_map(|ancestor| {
                self.roots
                    .iter()
                    .find(|root| root.handle.as_ref() == Some(&ancestor))
            })
    }

    /// Starting osu!lazer imports into the folder its storage.ini names, which is the
    /// detected lazer root, so a launch counts as a write there.
    pub fn lazer_launch(&self, lazer_overridden: bool, allow_live: bool) -> Result<LazerLaunch> {
        if lazer_overridden {
            return Ok(LazerLaunch::Stage(StageReason::LazerPathOverridden));
        }
        if allow_live {
            return Ok(LazerLaunch::Launch);
        }
        match &self.detected.lazer {
            Some(root) => Err(Error::LiveWriteRefused {
                dest: root.clone(),
                root: root.clone(),
                origin: RootOrigin::Detected,
            }),
            None => Ok(LazerLaunch::Stage(StageReason::NoLiveLazer)),
        }
    }
}

static ALLOW_LIVE: AtomicBool = AtomicBool::new(false);
static DETECTED: OnceLock<LiveRoots> = OnceLock::new();

#[cfg(test)]
thread_local! {
    static TEST_ROOTS: std::cell::RefCell<Option<LiveRoots>> = const { std::cell::RefCell::new(None) };
    static TEST_LAZER_OVERRIDDEN: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Makes `check_write` on this thread use `roots` instead of the detected installs.
#[cfg(test)]
pub(crate) fn set_test_roots(roots: LiveRoots) {
    TEST_ROOTS.with(|r| *r.borrow_mut() = Some(roots));
}

/// Makes this thread act as if `--lazer-path` were given, without touching the
/// process-wide overrides other tests share.
#[cfg(test)]
pub(crate) fn set_test_lazer_overridden() {
    TEST_LAZER_OVERRIDDEN.with(|o| o.set(true));
}

fn lazer_overridden() -> bool {
    #[cfg(test)]
    if TEST_LAZER_OVERRIDDEN.with(|o| o.get()) {
        return true;
    }
    lazer_path_overridden()
}

fn with_roots<T>(f: impl FnOnce(&LiveRoots) -> T) -> T {
    #[cfg(test)]
    if TEST_ROOTS.with(|r| r.borrow().is_some()) {
        return TEST_ROOTS.with(|r| f(r.borrow().as_ref().unwrap()));
    }
    f(DETECTED.get_or_init(LiveRoots::detect))
}

pub fn set_allow_live(allow: bool) {
    ALLOW_LIVE.store(allow, Ordering::SeqCst);
}

fn allow_live() -> bool {
    ALLOW_LIVE.load(Ordering::SeqCst)
}

/// Entry-point check for every operation that writes into a game folder.
pub fn check_write(dest: &Path) -> Result<()> {
    if allow_live() {
        return Ok(());
    }
    with_roots(|roots| roots.check(dest))
}

/// Decides whether a stable-to-lazer sync may start osu!lazer to import.
pub fn lazer_launch() -> Result<LazerLaunch> {
    with_roots(|roots| roots.lazer_launch(lazer_overridden(), allow_live()))
}

/// Checks the folders a sync in `direction` writes to, and the game launch an
/// import into lazer ends with.
pub fn check_sync(direction: SyncDirection, config: &Config) -> Result<()> {
    let links = match (config.lazer_files_path(), config.stable_songs_path()) {
        (Some(files), Some(songs)) => crate::unified::same_volume(&files, &songs).unwrap_or(true),
        _ => true,
    };
    for target in sync_write_targets(direction, config, links) {
        check_write(&target)?;
    }
    if direction.syncs_from_stable() {
        lazer_launch()?;
    }
    Ok(())
}

/// The folders a sync in `direction` writes to. Lazer to stable hard-links each
/// asset to its lazer file when both are on one volume, and a link shares the
/// file, so the lazer store counts as written to.
fn sync_write_targets(direction: SyncDirection, config: &Config, links: bool) -> Vec<PathBuf> {
    let mut targets = Vec::new();
    if direction.syncs_from_lazer() {
        targets.extend(config.stable_songs_path());
        if links {
            targets.extend(config.lazer_files_path());
        }
    }
    if direction.syncs_from_stable() {
        targets.extend(config.lazer_path.clone());
    }
    targets
}

/// True when `path` equals `root` or lies inside it, compared component by component.
pub fn is_under(path: &Path, root: &Path) -> bool {
    let mut path = path.components();
    root.components().all(|r| path.next() == Some(r))
}

/// Canonicalizes the longest existing ancestor and re-appends the rest, so a
/// destination that does not exist yet still resolves through junctions. Returns
/// `None` when a `..` or other non-name component sits in the part that does not
/// exist, since what it points at cannot be known.
fn resolve(path: &Path) -> Option<PathBuf> {
    let path = std::path::absolute(path).ok()?;
    let mut tail = Vec::new();
    let mut current = path.as_path();
    loop {
        if let Ok(canonical) = current.canonicalize() {
            return Some(
                tail.iter()
                    .rev()
                    .fold(canonical, |acc: PathBuf, part| acc.join(part)),
            );
        }
        match (current.parent(), current.components().next_back()) {
            (Some(parent), Some(Component::Normal(part))) => {
                tail.push(part.to_os_string());
                current = parent;
            }
            _ => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn installs(dir: &Path) -> InstallPaths {
        let stable = dir.join("osu!");
        let lazer = dir.join("osu!lazer");
        std::fs::create_dir_all(stable.join("Songs")).unwrap();
        std::fs::create_dir_all(lazer.join("files")).unwrap();
        InstallPaths {
            stable: Some(stable),
            lazer: Some(lazer),
        }
    }

    fn refused_root(roots: &LiveRoots, dest: &Path) -> Option<PathBuf> {
        match roots.check(dest) {
            Err(Error::LiveWriteRefused { root, .. }) => Some(root),
            Ok(()) => None,
            Err(e) => panic!("unexpected error for {}: {e}", dest.display()),
        }
    }

    #[test]
    fn refuses_write_under_live_path() {
        let dir = TempDir::new().unwrap();
        let roots = LiveRoots::new(installs(dir.path()), Vec::new(), None);

        let songs = dir.path().join("osu!").join("Songs").join("123 A - B");
        assert_eq!(refused_root(&roots, &songs), Some(dir.path().join("osu!")));

        let blob = dir.path().join("osu!lazer").join("files").join("ab");
        assert_eq!(
            refused_root(&roots, &blob),
            Some(dir.path().join("osu!lazer"))
        );

        let dotdot = dir.path().join("sandbox").join("..").join("osu!");
        assert_eq!(refused_root(&roots, &dotdot), Some(dir.path().join("osu!")));

        let missing_dotdot = dir
            .path()
            .join("osu!")
            .join("newA")
            .join("newB")
            .join("..")
            .join("x");
        assert_eq!(
            refused_root(&roots, &missing_dotdot),
            Some(dir.path().join("osu!"))
        );
    }

    #[test]
    fn allows_write_under_sandbox() {
        let dir = TempDir::new().unwrap();
        let roots = LiveRoots::new(installs(dir.path()), Vec::new(), None);

        for dest in [
            dir.path()
                .join("osu-sync-sandbox")
                .join("stable")
                .join("Songs"),
            dir.path().join("osu!lazer-copy").join("files"),
            dir.path().join("osu"),
        ] {
            assert_eq!(refused_root(&roots, &dest), None, "{}", dest.display());
        }
    }

    #[cfg(windows)]
    #[test]
    fn refuses_unresolvable_dotdot() {
        let dir = TempDir::new().unwrap();
        let roots = LiveRoots::new(installs(dir.path()), Vec::new(), None);

        let verbatim = format!(
            r"{}\sandbox\newA\newB\..\x",
            dir.path().canonicalize().unwrap().display()
        );
        let err = roots.check(Path::new(&verbatim)).unwrap_err();
        assert_eq!(
            err.to_string(),
            format!(
                "Refusing to write {verbatim}: the path cannot be resolved to check it against the live installs"
            )
        );
    }

    #[cfg(windows)]
    #[test]
    fn refuses_live_path_reached_through_admin_share() {
        let base = Path::new(r"D:\osu-sync-sandbox");
        if std::fs::create_dir_all(base).is_err() {
            return;
        }
        let dir = TempDir::new_in(base).unwrap();
        let roots = LiveRoots::new(installs(dir.path()), Vec::new(), None);

        let local = dir.path().join("osu!").join("Songs").join("1 A - B");
        let share = format!(
            r"\\localhost\D$\{}",
            local.strip_prefix(r"D:\").unwrap().display()
        );
        if !Path::new(&share).parent().unwrap().exists() {
            eprintln!("skipped: {share} is not reachable");
            return;
        }

        assert_eq!(
            refused_root(&roots, Path::new(&share)),
            Some(dir.path().join("osu!"))
        );
        let sandbox_share = format!(
            r"\\localhost\D$\{}\sandbox\Songs",
            dir.path().strip_prefix(r"D:\").unwrap().display()
        );
        assert_eq!(refused_root(&roots, Path::new(&sandbox_share)), None);
    }

    #[test]
    fn treats_saved_config_paths_as_live() {
        let dir = TempDir::new().unwrap();
        let saved_stable = dir.path().join("elsewhere").join("osu!");
        let saved_lazer = dir.path().join("elsewhere").join("lazer-data");
        std::fs::create_dir_all(saved_stable.join("Songs")).unwrap();
        let config_file = dir.path().join("config.json");
        std::fs::write(
            &config_file,
            serde_json::json!({
                "stable_path": saved_stable,
                "lazer_path": saved_lazer,
                "theme": "Mocha",
            })
            .to_string(),
        )
        .unwrap();

        let saved = SavedPaths::read(&config_file);
        assert_eq!(
            saved.paths,
            InstallPaths {
                stable: Some(saved_stable.clone()),
                lazer: Some(saved_lazer.clone()),
            }
        );

        let roots = LiveRoots::new(installs(dir.path()), Vec::new(), Some(saved));
        let songs = saved_stable.join("Songs").join("1 A - B");
        assert_eq!(refused_root(&roots, &songs), Some(saved_stable.clone()));
        assert_eq!(
            refused_root(&roots, &saved_lazer.join("files")),
            Some(saved_lazer)
        );
        assert_eq!(refused_root(&roots, &dir.path().join("sandbox")), None);
        assert_eq!(
            SavedPaths::read(&dir.path().join("missing.json")).paths,
            InstallPaths::default()
        );

        assert_eq!(
            roots.check(&songs).unwrap_err().to_string(),
            format!(
                "Refusing to write {}: it is inside {}, which is saved as an install path in {}. Remove it from that file, or pass --allow-live to write to it.",
                songs.display(),
                saved_stable.display(),
                config_file.display()
            )
        );
        let detected_stable = dir.path().join("osu!");
        let detected_songs = detected_stable.join("Songs");
        assert_eq!(
            roots.check(&detected_songs).unwrap_err().to_string(),
            format!(
                "Refusing to write {}: it is inside the live install {}. Pass --stable-path and --lazer-path to a sandbox, or --allow-live to write to the live install.",
                detected_songs.display(),
                detected_stable.display()
            )
        );
    }

    #[test]
    fn treats_redirecting_lazer_default_as_live() {
        let dir = TempDir::new().unwrap();
        let default = dir.path().join("Roaming").join("osu");
        let target = dir.path().join("osu!lazer");
        for lazer in [&default, &target] {
            std::fs::create_dir_all(lazer.join("files")).unwrap();
            std::fs::write(lazer.join("client.realm"), b"realm").unwrap();
        }
        std::fs::write(
            default.join("storage.ini"),
            format!("FullPath = {}\r\n", target.display()),
        )
        .unwrap();

        let lazer_dirs = lazer_live_dirs(&default);
        assert_eq!(lazer_dirs, vec![target.clone(), default.clone()]);

        let detected = InstallPaths {
            stable: None,
            lazer: Some(target.clone()),
        };
        let roots = LiveRoots::new(detected, lazer_dirs, None);
        assert_eq!(
            refused_root(&roots, &default.join("import").join("1 A - B.osz")),
            Some(default.clone())
        );
        assert_eq!(
            refused_root(&roots, &default.join("files").join("ab")),
            Some(default.clone())
        );
        assert_eq!(
            refused_root(&roots, &target.join("files").join("ab")),
            Some(target.clone())
        );
        let config_dir = dir.path().join("Roaming").join("osu-sync");
        assert_eq!(refused_root(&roots, &config_dir.join("config.json")), None);

        assert_eq!(lazer_live_dirs(&target), vec![target.clone()]);
    }

    fn config(stable: PathBuf, lazer: PathBuf) -> Config {
        Config {
            stable_path: Some(stable),
            lazer_path: Some(lazer),
            ..Config::default()
        }
    }

    #[test]
    fn l2s_writes_to_the_lazer_store_when_it_links() {
        let stable = PathBuf::from(r"D:\osu-sync-sandbox\p4fix1-unit\stable");
        let lazer = PathBuf::from(r"D:\osu!lazer");
        let config = config(stable.clone(), lazer.clone());

        assert_eq!(
            sync_write_targets(SyncDirection::LazerToStable, &config, true),
            vec![stable.join("Songs"), lazer.join("files")]
        );
        assert_eq!(
            sync_write_targets(SyncDirection::LazerToStable, &config, false),
            vec![stable.join("Songs")]
        );
        assert_eq!(
            sync_write_targets(SyncDirection::StableToLazer, &config, true),
            vec![lazer]
        );
    }

    #[test]
    fn l2s_from_a_live_lazer_store_is_refused() {
        let dir = TempDir::new().unwrap();
        set_test_roots(LiveRoots::new(installs(dir.path()), Vec::new(), None));
        let stable = dir.path().join("sandbox").join("stable");
        std::fs::create_dir_all(stable.join("Songs")).unwrap();

        let live = config(stable.clone(), dir.path().join("osu!lazer"));
        match check_sync(SyncDirection::LazerToStable, &live) {
            Err(Error::LiveWriteRefused { root, .. }) => {
                assert_eq!(root, dir.path().join("osu!lazer"))
            }
            other => panic!("expected a refusal, got {other:?}"),
        }

        let sandbox_lazer = dir.path().join("sandbox").join("lazer");
        std::fs::create_dir_all(sandbox_lazer.join("files")).unwrap();
        let sandbox = config(stable, sandbox_lazer);
        assert!(check_sync(SyncDirection::LazerToStable, &sandbox).is_ok());
    }

    #[test]
    fn lazer_launch_counts_as_a_live_write() {
        let dir = TempDir::new().unwrap();
        let live = installs(dir.path());
        let lazer_root = live.lazer.clone().unwrap();
        let roots = LiveRoots::new(live, Vec::new(), None);

        assert_eq!(
            roots.lazer_launch(true, false).unwrap(),
            LazerLaunch::Stage(StageReason::LazerPathOverridden)
        );
        assert_eq!(
            roots.lazer_launch(true, true).unwrap(),
            LazerLaunch::Stage(StageReason::LazerPathOverridden)
        );
        assert_eq!(
            roots.lazer_launch(false, true).unwrap(),
            LazerLaunch::Launch
        );
        assert!(matches!(
            roots.lazer_launch(false, false),
            Err(Error::LiveWriteRefused { root, .. }) if root == lazer_root
        ));

        let no_lazer = LiveRoots::new(InstallPaths::default(), Vec::new(), None);
        assert_eq!(
            no_lazer.lazer_launch(false, false).unwrap(),
            LazerLaunch::Stage(StageReason::NoLiveLazer)
        );
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
