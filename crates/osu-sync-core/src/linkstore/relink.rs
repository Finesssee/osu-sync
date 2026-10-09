//! Turns stable files that are plain copies of lazer blobs into hard links to those blobs.
//!
//! A relink hard-links the blob to a temp name in the stable file's folder, then renames
//! the temp over the stable file. The stable file only ever goes away through that
//! rename, so a failure or a kill at any point leaves either the original or the link.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::materialize::{is_same_file, is_temp_name, TEMP_PREFIX, TEMP_SUFFIX};
use super::{how, BlobHash, How};
use crate::config::live_guard;
use crate::error::{Error, Result};
use crate::unified::{classify_hard_link_error, same_volume, HardLinkFailure};

/// Why a stable file was left as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RelinkSkip {
    /// `.osu`, `.osb`, `.osz2`, `.osu_*` leftovers and `Data\e`: stable writes or
    /// decrypts these in place, so they never share a file with lazer.
    NeverRelinked,
    /// A file directly in Songs. Its temp would be a new Songs entry, which makes stable rescan.
    SongsRoot,
    /// No lazer blob has this content.
    NoBlob,
    /// The blob named by the content hash has another size, so it is not trusted.
    BlobMismatch,
    /// The stable file already is the blob.
    AlreadyLinked,
    /// The blob already has the most hard links the filesystem allows.
    LinkLimit,
    /// Songs and the lazer store are on different volumes.
    CrossVolume,
    /// The stable file is open elsewhere or read-only.
    Locked,
    /// The stable file changed between hashing and replacing.
    Changed,
}

/// What happens to one stable file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Relink,
    Skip(RelinkSkip),
}

/// One stable file, the blob with its content, and what happens to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relink {
    pub stable: PathBuf,
    pub blob: PathBuf,
    pub decision: Decision,
}

/// Decides what happens to the stable file at `rel` (relative to Songs).
///
/// The file rule is P4's [`how`]: only a file P4 would hard-link is ever relinked.
pub fn decide(
    rel: &Path,
    same_volume: bool,
    already_linked: bool,
    link_limit_hit: bool,
) -> Decision {
    let mut parts = rel.components();
    parts.next();
    let in_set = parts.as_path();
    if in_set.as_os_str().is_empty() {
        Decision::Skip(RelinkSkip::SongsRoot)
    } else if how(&in_set.to_string_lossy(), true, false) != How::Link {
        Decision::Skip(RelinkSkip::NeverRelinked)
    } else if !same_volume {
        Decision::Skip(RelinkSkip::CrossVolume)
    } else if already_linked {
        Decision::Skip(RelinkSkip::AlreadyLinked)
    } else if link_limit_hit {
        Decision::Skip(RelinkSkip::LinkLimit)
    } else {
        Decision::Relink
    }
}

/// What a relink run did.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize)]
pub struct RelinkReport {
    pub relinked: usize,
    /// Sum of the sizes of the replaced copies.
    pub bytes_reclaimed: u64,
    /// Files read and hashed this run; files the cache knew are not counted.
    pub hashed_files: usize,
    pub hashed_bytes: u64,
    pub skipped: BTreeMap<RelinkSkip, usize>,
    /// Temp files of an interrupted run, removed before anything else.
    pub temps_removed: usize,
    /// One line per file that failed for an unexpected reason.
    pub errors: Vec<String>,
    pub notes: Vec<String>,
}

impl RelinkReport {
    fn skip(&mut self, reason: RelinkSkip) {
        *self.skipped.entry(reason).or_default() += 1;
    }
}

/// SHA-256 of a file at a given size and modification time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CachedHash {
    size: u64,
    mtime: (u64, u32),
    sha: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct HashCache {
    version: u32,
    entries: HashMap<String, CachedHash>,
}

const CACHE_VERSION: u32 = 1;

impl HashCache {
    /// An unreadable or outdated cache counts as empty: it only saves work.
    fn load(path: &Path) -> Self {
        fs::read(path)
            .ok()
            .and_then(|bytes| bincode::deserialize::<HashCache>(&bytes).ok())
            .filter(|cache| cache.version == CACHE_VERSION)
            .unwrap_or_default()
    }

    /// Writes a temp file next to the cache and renames it into place.
    fn save(&self, path: &Path) -> io::Result<()> {
        let bytes = bincode::serialize(self).map_err(io::Error::other)?;
        let temp = path.with_extension("bin.tmp");
        crate::config::scan_cache::write(&temp, &bytes)?;
        fs::rename(&temp, path)
    }

    fn get(&self, path: &Path, size: u64, mtime: (u64, u32)) -> Option<&str> {
        let entry = self.entries.get(path.to_str()?)?;
        (entry.size == size && entry.mtime == mtime).then_some(entry.sha.as_str())
    }
}

/// A stable file that may be relinked: its stat when hashed and its blob.
#[derive(Debug)]
struct Candidate {
    stable: PathBuf,
    rel: PathBuf,
    size: u64,
    mtime: (u64, u32),
    sha: String,
    hashed: bool,
    found: Found,
}

#[derive(Debug)]
enum Found {
    Blob { blob: PathBuf, already_linked: bool },
    Skip(RelinkSkip, Option<String>),
    Error(String),
}

/// Relinks the stable copies in one Songs folder onto one lazer store.
pub struct Relinker {
    songs: PathBuf,
    files: PathBuf,
    cache: Option<PathBuf>,
    link: fn(&Path, &Path) -> io::Result<()>,
    same_volume: fn(&Path, &Path) -> io::Result<bool>,
}

impl Relinker {
    /// `files` is lazer's `files` folder. `cache` is the hash cache file, which must
    /// not be inside a game folder; `None` hashes every file on every run.
    pub fn new(
        songs: impl Into<PathBuf>,
        files: impl Into<PathBuf>,
        cache: Option<PathBuf>,
    ) -> Self {
        let songs = songs.into();
        Self {
            songs: std::path::absolute(&songs).unwrap_or(songs),
            files: files.into(),
            cache,
            link: |src, dst| fs::hard_link(src, dst),
            same_volume,
        }
    }

    /// The hash cache file for `songs` in the per-user osu-sync cache folder.
    pub fn default_cache(songs: &Path) -> Option<PathBuf> {
        crate::config::scan_cache::default_root()
            .map(|root| crate::config::scan_cache::file_for(&root, "relink", songs, "bin"))
    }

    /// Relinks every stable file whose content is a lazer blob. One file failing never
    /// stops the run; it lands in `errors`. The caller checks that stable is closed.
    pub fn run(&self, progress: &mut dyn FnMut(usize, usize)) -> Result<RelinkReport> {
        live_guard::check_write(&self.songs)?;
        live_guard::check_write(&self.files)?;
        if !self.songs.is_dir() {
            return Err(Error::Other(format!(
                "{} is not a folder",
                self.songs.display()
            )));
        }

        let mut report = RelinkReport::default();
        if !(self.same_volume)(&self.files, &self.songs)? {
            report.notes.push(format!(
                "{} and {} are on different volumes, so nothing was relinked",
                self.songs.display(),
                self.files.display()
            ));
            return Ok(report);
        }

        let candidates = self.walk(&mut report);
        let cache = self
            .cache
            .as_deref()
            .map(HashCache::load)
            .unwrap_or_default();
        let inspected: Vec<Candidate> = candidates
            .into_par_iter()
            .filter_map(|(stable, rel)| self.inspect(stable, rel, &cache))
            .collect();

        let mut next = HashCache {
            version: CACHE_VERSION,
            entries: HashMap::new(),
        };
        let mut limited: HashSet<PathBuf> = HashSet::new();
        let total = inspected.len();
        for (i, c) in inspected.into_iter().enumerate() {
            if c.hashed {
                report.hashed_files += 1;
                report.hashed_bytes += c.size;
            }
            let mut stat = (c.size, c.mtime);
            match &c.found {
                Found::Error(e) => report.errors.push(e.clone()),
                Found::Skip(reason, note) => {
                    report.skip(*reason);
                    report.notes.extend(note.clone());
                }
                Found::Blob {
                    blob,
                    already_linked,
                } => {
                    let plan = Relink {
                        stable: c.stable.clone(),
                        blob: blob.clone(),
                        decision: decide(&c.rel, true, *already_linked, limited.contains(blob)),
                    };
                    match self.apply(&plan, &c, &mut limited) {
                        Ok(None) => {
                            report.relinked += 1;
                            report.bytes_reclaimed += c.size;
                            if let Ok(meta) = fs::metadata(&c.stable) {
                                stat = (meta.len(), mtime_key(&meta));
                            }
                        }
                        Ok(Some((reason, note))) => {
                            report.skip(reason);
                            report.notes.extend(note);
                        }
                        Err(e) => report.errors.push(format!("{}: {e}", c.stable.display())),
                    }
                }
            }
            if let (Some(key), false) = (c.stable.to_str(), c.sha.is_empty()) {
                next.entries.insert(
                    key.to_string(),
                    CachedHash {
                        size: stat.0,
                        mtime: stat.1,
                        sha: c.sha.clone(),
                    },
                );
            }
            progress(i + 1, total);
        }

        let limit_hits = report.skipped.get(&RelinkSkip::LinkLimit).copied();
        if let Some(n) = limit_hits {
            report.notes.push(format!(
                "{n} files stayed copies because their lazer file reached the hard-link limit"
            ));
        }
        if let Some(path) = &self.cache {
            if let Err(e) = next.save(path) {
                report.notes.push(format!(
                    "could not save the hash cache {}: {e}",
                    path.display()
                ));
            }
        }
        Ok(report)
    }

    /// Lists the files under each set folder that may be relinked, removes temp files
    /// an interrupted run left, and counts the files the rules exclude without reading them.
    fn walk(&self, report: &mut RelinkReport) -> Vec<(PathBuf, PathBuf)> {
        let mut candidates = Vec::new();
        for entry in walkdir::WalkDir::new(&self.songs)
            .min_depth(1)
            .sort_by_file_name()
        {
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    report.errors.push(e.to_string());
                    continue;
                }
            };
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.into_path();
            let Ok(rel) = path.strip_prefix(&self.songs).map(Path::to_path_buf) else {
                continue;
            };
            let in_set = rel.components().count() > 1;
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
            if in_set && name.as_deref().is_some_and(is_temp_name) {
                match fs::remove_file(&path) {
                    Ok(()) => report.temps_removed += 1,
                    Err(e) => report.errors.push(format!("{}: {e}", path.display())),
                }
                continue;
            }
            match decide(&rel, true, false, false) {
                Decision::Skip(reason) => report.skip(reason),
                Decision::Relink => candidates.push((path, rel)),
            }
        }
        candidates
    }

    /// Stats and hashes one candidate (unless the cache knows it) and finds its blob.
    fn inspect(&self, stable: PathBuf, rel: PathBuf, cache: &HashCache) -> Option<Candidate> {
        let mut c = Candidate {
            stable,
            rel,
            size: 0,
            mtime: (0, 0),
            sha: String::new(),
            hashed: false,
            found: Found::Error(String::new()),
        };
        let meta = match fs::metadata(&c.stable) {
            Ok(meta) => meta,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return None,
            Err(e) => {
                c.found = Found::Error(format!("{}: {e}", c.stable.display()));
                return Some(c);
            }
        };
        c.size = meta.len();
        c.mtime = mtime_key(&meta);
        if let Some(sha) = cache.get(&c.stable, c.size, c.mtime) {
            c.sha = sha.to_string();
        } else {
            match sha256(&c.stable) {
                Ok(sha) => {
                    c.sha = sha;
                    c.hashed = true;
                }
                Err(e) if is_locked(&e) => {
                    c.found = locked(&c.stable, &e);
                    return Some(c);
                }
                Err(e) => {
                    c.found = Found::Error(format!("{}: {e}", c.stable.display()));
                    return Some(c);
                }
            }
        }
        let Some(hash) = BlobHash::parse(&c.sha) else {
            c.found = Found::Error(format!("{}: bad cached hash", c.stable.display()));
            return Some(c);
        };
        let blob = hash.path_in(&self.files);
        c.found = match fs::metadata(&blob) {
            Err(_) => Found::Skip(RelinkSkip::NoBlob, None),
            Ok(b) if b.len() != c.size => Found::Skip(
                RelinkSkip::BlobMismatch,
                Some(format!(
                    "{} is {} bytes but its content hash names {} of {} bytes",
                    c.stable.display(),
                    c.size,
                    blob.display(),
                    b.len()
                )),
            ),
            Ok(_) => match is_same_file(&c.stable, &blob) {
                Ok(already_linked) => Found::Blob {
                    blob,
                    already_linked,
                },
                Err(e) if is_locked(&e) => locked(&c.stable, &e),
                Err(e) => Found::Error(format!("{}: {e}", c.stable.display())),
            },
        };
        Some(c)
    }

    /// Replaces the stable file with a link to its blob when the plan says so.
    /// Returns the skip reason and note when it leaves the file as it is.
    fn apply(
        &self,
        plan: &Relink,
        c: &Candidate,
        limited: &mut HashSet<PathBuf>,
    ) -> io::Result<Option<(RelinkSkip, Option<String>)>> {
        if let Decision::Skip(reason) = plan.decision {
            return Ok(Some((reason, None)));
        }
        let meta = fs::metadata(&plan.stable)?;
        if meta.len() != c.size || mtime_key(&meta) != c.mtime {
            return Ok(Some((RelinkSkip::Changed, None)));
        }
        let folder = plan
            .stable
            .parent()
            .ok_or_else(|| io::Error::other("stable file has no folder"))?;
        let temp = match self.link_temp(&plan.blob, folder) {
            Ok(temp) => temp,
            Err(e) => {
                return match classify_hard_link_error(&e) {
                    HardLinkFailure::LinkLimit => {
                        limited.insert(plan.blob.clone());
                        Ok(Some((RelinkSkip::LinkLimit, None)))
                    }
                    HardLinkFailure::CrossVolume => Ok(Some((RelinkSkip::CrossVolume, None))),
                    HardLinkFailure::Other => Err(e),
                }
            }
        };
        match fs::rename(&temp, &plan.stable) {
            Ok(()) => Ok(None),
            Err(e) => {
                let _ = fs::remove_file(&temp);
                if is_locked(&e) {
                    match locked(&plan.stable, &e) {
                        Found::Skip(reason, note) => Ok(Some((reason, note))),
                        _ => Err(e),
                    }
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Hard-links `blob` to the first free temp name in `folder`.
    fn link_temp(&self, blob: &Path, folder: &Path) -> io::Result<PathBuf> {
        let mut n = 0u32;
        loop {
            let temp = folder.join(format!("{TEMP_PREFIX}{n}{TEMP_SUFFIX}"));
            match (self.link)(blob, &temp) {
                Ok(()) => return Ok(temp),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => n += 1,
                Err(e) => return Err(e),
            }
        }
    }
}

fn locked(path: &Path, e: &io::Error) -> Found {
    Found::Skip(
        RelinkSkip::Locked,
        Some(format!(
            "left {} as it is: it is in use or read-only ({e})",
            path.display()
        )),
    )
}

/// Sharing and lock violations, and access denied, which a read-only file gives on rename.
fn is_locked(e: &io::Error) -> bool {
    const ERROR_SHARING_VIOLATION: i32 = 32;
    const ERROR_LOCK_VIOLATION: i32 = 33;
    e.kind() == io::ErrorKind::PermissionDenied
        || (cfg!(windows)
            && matches!(
                e.raw_os_error(),
                Some(ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION)
            ))
}

fn mtime_key(meta: &fs::Metadata) -> (u64, u32) {
    meta.modified()
        .ok()
        .and_then(|t: SystemTime| t.duration_since(UNIX_EPOCH).ok())
        .map_or((0, 0), |d| (d.as_secs(), d.subsec_nanos()))
}

/// SHA-256 of a file as lowercase hex. Only SHA-256, since this reads every asset.
fn sha256(path: &Path) -> io::Result<String> {
    let mut input = File::open(path)?;
    let mut sha = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        sha.update(&buf[..n]);
    }
    Ok(format!("{:x}", sha.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::live_guard::InstallPaths;
    use crate::config::live_guard::{set_test_roots, LiveRoots};
    use crate::unified::LINK_LIMIT_OS_ERROR;

    const AUDIO: &[u8] = b"ID3 not really audio";
    const BG: &[u8] = b"\x89PNG not really a background";
    const OSU: &[u8] = b"osu file format v14\n\n[General]\nAudioFilename: audio.mp3\n";

    struct Fixture {
        dir: tempfile::TempDir,
        files: PathBuf,
        songs: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let files = dir.path().join("lazer").join("files");
            let songs = dir.path().join("stable").join("Songs");
            fs::create_dir_all(&files).unwrap();
            fs::create_dir_all(&songs).unwrap();
            Self { dir, files, songs }
        }

        fn blob(&self, content: &[u8]) -> PathBuf {
            let hash = format!("{:x}", Sha256::digest(content));
            let path = BlobHash::parse(&hash).unwrap().path_in(&self.files);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, content).unwrap();
            path
        }

        fn stable(&self, rel: &str, content: &[u8]) -> PathBuf {
            let path = rel
                .split('/')
                .fold(self.songs.clone(), |p, part| p.join(part));
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, content).unwrap();
            path
        }

        fn cache(&self) -> PathBuf {
            self.dir.path().join("cache").join("relink-test.bin")
        }

        fn relinker(&self) -> Relinker {
            Relinker::new(&self.songs, &self.files, Some(self.cache()))
        }
    }

    fn run(r: &Relinker) -> RelinkReport {
        r.run(&mut |_, _| {}).unwrap()
    }

    fn skipped(pairs: &[(RelinkSkip, usize)]) -> BTreeMap<RelinkSkip, usize> {
        pairs.iter().copied().collect()
    }

    fn same(a: &Path, b: &Path) -> bool {
        is_same_file(a, b).unwrap()
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn decision_follows_the_file_rule_volume_identity_and_limit() {
        let rel = Path::new("1 A - B").join("audio.mp3");
        assert_eq!(decide(&rel, true, false, false), Decision::Relink);
        assert_eq!(
            decide(&rel, false, false, false),
            Decision::Skip(RelinkSkip::CrossVolume)
        );
        assert_eq!(
            decide(&rel, true, true, false),
            Decision::Skip(RelinkSkip::AlreadyLinked)
        );
        assert_eq!(
            decide(&rel, true, false, true),
            Decision::Skip(RelinkSkip::LinkLimit)
        );
        for never in [
            "1 A - B/x [Easy].osu",
            "1 A - B/storyboard.OSB",
            "1 A - B/package.osz2",
            "1 A - B/Data/e/payload",
            "1 A - B/x [Easy].osu_0f3c",
        ] {
            assert_eq!(
                decide(Path::new(never), true, false, false),
                Decision::Skip(RelinkSkip::NeverRelinked),
                "{never}"
            );
        }
        assert_eq!(
            decide(Path::new("loose.mp3"), true, false, false),
            Decision::Skip(RelinkSkip::SongsRoot)
        );
        assert_eq!(
            decide(
                &Path::new("1 A - B").join("sb").join("bg.png"),
                true,
                false,
                false
            ),
            Decision::Relink
        );
    }

    #[test]
    fn relinks_matching_file() {
        let fx = Fixture::new();
        let blob = fx.blob(AUDIO);
        let stable = fx.stable("1 A - B/audio.mp3", AUDIO);

        let report = run(&fx.relinker());

        assert_eq!(report.relinked, 1);
        assert_eq!(report.bytes_reclaimed, 20);
        assert_eq!(report.hashed_files, 1);
        assert_eq!(report.hashed_bytes, 20);
        assert_eq!(report.skipped, skipped(&[]));
        assert_eq!(report.errors, Vec::<String>::new());
        assert!(same(&stable, &blob));
        assert_eq!(fs::read(&stable).unwrap(), AUDIO);
        assert_eq!(names(&fx.songs.join("1 A - B")), vec!["audio.mp3"]);
    }

    #[test]
    fn leaves_unmatched_file() {
        let fx = Fixture::new();
        fx.blob(AUDIO);
        let stable = fx.stable("1 A - B/audio.mp3", b"edited audio");

        let report = run(&fx.relinker());

        assert_eq!(report.relinked, 0);
        assert_eq!(report.bytes_reclaimed, 0);
        assert_eq!(report.skipped, skipped(&[(RelinkSkip::NoBlob, 1)]));
        assert_eq!(fs::read(&stable).unwrap(), b"edited audio");
        assert_eq!(names(&fx.songs.join("1 A - B")), vec!["audio.mp3"]);
    }

    #[test]
    fn skips_already_linked() {
        let fx = Fixture::new();
        let blob = fx.blob(AUDIO);
        let stable = fx.songs.join("1 A - B").join("audio.mp3");
        fs::create_dir_all(stable.parent().unwrap()).unwrap();
        fs::hard_link(&blob, &stable).unwrap();

        let report = Relinker::new(&fx.songs, &fx.files, None)
            .run(&mut |_, _| {})
            .unwrap();

        assert_eq!(report.relinked, 0);
        assert_eq!(report.hashed_files, 1);
        assert_eq!(report.skipped, skipped(&[(RelinkSkip::AlreadyLinked, 1)]));
        assert!(same(&stable, &blob));
    }

    #[test]
    fn never_touches_osu_files() {
        let fx = Fixture::new();
        fx.blob(OSU);
        fx.blob(AUDIO);
        let osu = fx.stable("1 A - B/A - B (M) [Easy].osu", OSU);
        let osb = fx.stable("1 A - B/A - B (M).osb", AUDIO);
        let leftover = fx.stable("1 A - B/A - B (M) [Easy].osu_1", AUDIO);
        let osz2 = fx.stable("1 A - B/A - B.osz2", AUDIO);
        let data_e = fx.stable("1 A - B/Data/e/1", AUDIO);
        let mut r = fx.relinker();
        r.link = |_, _| panic!("no file here may be linked");

        let report = run(&r);

        assert_eq!(report.relinked, 0);
        assert_eq!(report.hashed_files, 0);
        assert_eq!(report.skipped, skipped(&[(RelinkSkip::NeverRelinked, 5)]));
        assert_eq!(fs::read(&osu).unwrap(), OSU);
        for other in [&osb, &leftover, &osz2, &data_e] {
            assert_eq!(fs::read(other).unwrap(), AUDIO);
        }
        assert!(!same(&osu, &fx.blob(OSU)));
    }

    #[test]
    fn rerun_hashes_nothing() {
        let fx = Fixture::new();
        fx.blob(AUDIO);
        fx.blob(BG);
        fx.stable("1 A - B/audio.mp3", AUDIO);
        fx.stable("1 A - B/bg.png", BG);
        fx.stable("1 A - B/hitnormal.wav", b"not in lazer");

        let first = run(&fx.relinker());
        assert_eq!((first.relinked, first.hashed_files), (2, 3));

        let second = run(&fx.relinker());
        assert_eq!(second.relinked, 0);
        assert_eq!(second.hashed_files, 0);
        assert_eq!(second.hashed_bytes, 0);
        assert_eq!(
            second.skipped,
            skipped(&[(RelinkSkip::AlreadyLinked, 2), (RelinkSkip::NoBlob, 1)])
        );
    }

    #[test]
    fn changed_file_is_hashed_again() {
        let fx = Fixture::new();
        fx.blob(AUDIO);
        let stable = fx.stable("1 A - B/audio.mp3", b"first");
        run(&fx.relinker());
        fs::write(&stable, AUDIO).unwrap();

        let report = run(&fx.relinker());

        assert_eq!((report.relinked, report.hashed_files), (1, 1));
    }

    #[cfg(windows)]
    fn live_roots() {
        set_test_roots(LiveRoots::new(
            InstallPaths {
                stable: Some(PathBuf::from(r"D:\osu!")),
                lazer: Some(PathBuf::from(r"D:\osu!lazer")),
            },
            Vec::new(),
            None,
        ));
    }

    #[cfg(windows)]
    fn assert_refused(result: Result<RelinkReport>, expected_root: &str) {
        if !Path::new(r"D:\").exists() {
            assert!(matches!(result, Err(Error::LiveWriteUnresolved { .. })));
            return;
        }
        match result {
            Err(Error::LiveWriteRefused { root, .. }) => {
                assert_eq!(root, PathBuf::from(expected_root))
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[cfg(windows)]
    #[test]
    fn relink_refuses_live_stable_path() {
        live_roots();
        let fx = Fixture::new();
        let mut r = Relinker::new(r"D:\osu!\Songs", &fx.files, Some(fx.cache()));
        r.link = |_, _| panic!("a refused run must not link");

        assert_refused(r.run(&mut |_, _| panic!("no progress")), r"D:\osu!");
        assert!(!fx.cache().exists());
    }

    #[cfg(windows)]
    #[test]
    fn relink_refuses_live_lazer_source() {
        live_roots();
        let fx = Fixture::new();
        let stable = fx.stable("1 A - B/audio.mp3", AUDIO);
        let mut r = Relinker::new(&fx.songs, r"D:\osu!lazer\files", Some(fx.cache()));
        r.link = |_, _| panic!("a refused run must not link");

        assert_refused(r.run(&mut |_, _| panic!("no progress")), r"D:\osu!lazer");
        assert!(!fx.cache().exists());
        assert_eq!(fs::read(&stable).unwrap(), AUDIO);
    }

    #[test]
    fn refusal_comes_before_any_read() {
        let fx = Fixture::new();
        set_test_roots(LiveRoots::new(
            InstallPaths {
                stable: Some(fx.dir.path().join("stable")),
                lazer: None,
            },
            Vec::new(),
            None,
        ));
        fx.blob(AUDIO);
        let stable = fx.stable("1 A - B/audio.mp3", AUDIO);
        let mut r = fx.relinker();
        r.link = |_, _| panic!("a refused run must not link");

        match r.run(&mut |_, _| panic!("no progress")) {
            Err(Error::LiveWriteRefused { root, .. }) => {
                assert_eq!(root, fx.dir.path().join("stable"))
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
        assert!(!fx.cache().exists());
        assert!(!same(&stable, &fx.blob(AUDIO)));
    }

    fn lock_for_reading(path: &Path) -> Option<Box<dyn std::any::Any>> {
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            let held = fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(path)
                .unwrap();
            Some(Box::new(held))
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            struct Restore(PathBuf);
            impl Drop for Restore {
                fn drop(&mut self) {
                    let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o644));
                }
            }
            fs::set_permissions(path, fs::Permissions::from_mode(0o000)).unwrap();
            let restore = Restore(path.to_path_buf());
            File::open(path)
                .is_err()
                .then(|| Box::new(restore) as Box<dyn std::any::Any>)
        }
    }

    #[test]
    fn locked_file_is_skipped_and_reported() {
        let fx = Fixture::new();
        let audio_blob = fx.blob(AUDIO);
        let bg_blob = fx.blob(BG);
        let audio = fx.stable("1 A - B/audio.mp3", AUDIO);
        let bg = fx.stable("1 A - B/bg.png", BG);
        let Some(held) = lock_for_reading(&audio) else {
            eprintln!("skipped: this user can read a file without read permission");
            return;
        };

        let report = run(&fx.relinker());
        drop(held);

        assert_eq!(report.relinked, 1);
        assert_eq!(report.bytes_reclaimed, 28);
        assert_eq!(report.skipped, skipped(&[(RelinkSkip::Locked, 1)]));
        assert_eq!(report.errors, Vec::<String>::new());
        assert_eq!(report.notes.len(), 1);
        assert!(
            report.notes[0]
                .starts_with(&format!("left {} as it is: it is in use", audio.display())),
            "{}",
            report.notes[0]
        );
        assert!(same(&bg, &bg_blob));
        assert!(!same(&audio, &audio_blob));
        assert_eq!(fs::read(&audio).unwrap(), AUDIO);
    }

    #[test]
    fn read_only_file_is_skipped_and_keeps_its_attribute() {
        let fx = Fixture::new();
        let blob = fx.blob(AUDIO);
        let stable = fx.stable("1 A - B/audio.mp3", AUDIO);
        let mut perms = fs::metadata(&stable).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&stable, perms).unwrap();

        let report = run(&fx.relinker());

        let read_only = fs::metadata(&stable).unwrap().permissions().readonly();
        let mut perms = fs::metadata(&stable).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        perms.set_readonly(false);
        fs::set_permissions(&stable, perms).unwrap();
        if cfg!(windows) {
            assert_eq!(report.relinked, 0);
            assert_eq!(report.skipped, skipped(&[(RelinkSkip::Locked, 1)]));
            assert!(read_only);
            assert!(!same(&stable, &blob));
        } else {
            // A Unix rename replaces a read-only file; only the folder's mode matters.
            assert_eq!(report.relinked, 1);
        }
        assert_eq!(report.errors, Vec::<String>::new());
        assert_eq!(names(&fx.songs.join("1 A - B")), vec!["audio.mp3"]);
    }

    #[test]
    fn cross_volume_relinks_nothing() {
        let fx = Fixture::new();
        fx.blob(AUDIO);
        let stable = fx.stable("1 A - B/audio.mp3", AUDIO);
        fx.stable("2 C - D/audio.mp3", AUDIO);
        let mut r = fx.relinker();
        r.same_volume = |_, _| Ok(false);
        r.link = |_, _| panic!("nothing may be linked across volumes");

        let report = run(&r);

        assert_eq!(report.relinked, 0);
        assert_eq!(report.hashed_files, 0);
        assert_eq!(report.skipped, skipped(&[]));
        assert_eq!(
            report.notes,
            vec![format!(
                "{} and {} are on different volumes, so nothing was relinked",
                fx.songs.display(),
                fx.files.display()
            )]
        );
        assert_eq!(fs::read(&stable).unwrap(), AUDIO);
    }

    #[test]
    fn link_limit_keeps_the_copy_and_reports_it() {
        let fx = Fixture::new();
        let blob = fx.blob(AUDIO);
        let a = fx.stable("1 A - B/audio.mp3", AUDIO);
        let b = fx.stable("2 C - D/audio.mp3", AUDIO);
        let mut r = fx.relinker();
        r.link = |_, _| Err(io::Error::from_raw_os_error(LINK_LIMIT_OS_ERROR));

        let report = run(&r);

        assert_eq!(report.relinked, 0);
        assert_eq!(report.skipped, skipped(&[(RelinkSkip::LinkLimit, 2)]));
        assert_eq!(report.errors, Vec::<String>::new());
        assert_eq!(
            report.notes,
            vec!["2 files stayed copies because their lazer file reached the hard-link limit"]
        );
        for stable in [&a, &b] {
            assert_eq!(fs::read(stable).unwrap(), AUDIO);
            assert!(!same(stable, &blob));
        }
        assert_eq!(names(&fx.songs.join("1 A - B")), vec!["audio.mp3"]);
    }

    #[test]
    fn failed_link_leaves_original_and_no_temp() {
        let fx = Fixture::new();
        fx.blob(AUDIO);
        let stable = fx.stable("1 A - B/audio.mp3", AUDIO);
        let mut r = fx.relinker();
        r.link = |_, dst| {
            fs::write(dst, b"partial")?;
            fs::remove_file(dst)?;
            Err(io::Error::from_raw_os_error(1))
        };

        let report = run(&r);

        assert_eq!(report.relinked, 0);
        assert_eq!(report.errors.len(), 1);
        assert!(report.errors[0].starts_with(&stable.display().to_string()));
        assert_eq!(fs::read(&stable).unwrap(), AUDIO);
        assert_eq!(names(&fx.songs.join("1 A - B")), vec!["audio.mp3"]);
    }

    #[test]
    fn stale_temps_of_a_killed_run_are_removed() {
        let fx = Fixture::new();
        let blob = fx.blob(AUDIO);
        let stable = fx.stable("1 A - B/audio.mp3", AUDIO);
        fs::hard_link(&blob, fx.songs.join("1 A - B").join("osu-sync_tmp_0.part")).unwrap();
        fx.stable("osu-sync_tmp_0.part", b"not ours: at the Songs root");

        let report = run(&fx.relinker());

        assert_eq!(report.temps_removed, 1);
        assert_eq!(report.relinked, 1);
        assert_eq!(report.skipped, skipped(&[(RelinkSkip::SongsRoot, 1)]));
        assert!(same(&stable, &blob));
        assert_eq!(names(&fx.songs.join("1 A - B")), vec!["audio.mp3"]);
        assert!(fx.songs.join("osu-sync_tmp_0.part").exists());
    }

    #[test]
    fn mtime_of_the_blob_is_not_changed() {
        let fx = Fixture::new();
        let blob = fx.blob(AUDIO);
        let before = fs::metadata(&blob).unwrap().modified().unwrap();
        let stable = fx.stable("1 A - B/audio.mp3", AUDIO);
        File::options()
            .write(true)
            .open(&stable)
            .unwrap()
            .set_modified(UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000))
            .unwrap();

        run(&fx.relinker());

        assert_eq!(fs::metadata(&blob).unwrap().modified().unwrap(), before);
        assert_eq!(fs::metadata(&stable).unwrap().modified().unwrap(), before);
    }

    #[test]
    fn report_serializes_skip_reasons_as_names() {
        let mut report = RelinkReport::default();
        report.skip(RelinkSkip::NeverRelinked);
        report.skip(RelinkSkip::NoBlob);
        report.skip(RelinkSkip::NoBlob);
        let json = serde_json::to_value(&report).unwrap();
        assert_eq!(
            json["skipped"],
            serde_json::json!({"never_relinked": 1, "no_blob": 2})
        );
    }
}
