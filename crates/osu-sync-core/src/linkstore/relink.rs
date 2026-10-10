//! Turns stable files that are plain copies of lazer blobs into hard links to those blobs.
//!
//! A relink hard-links the blob to a temp name in the stable file's folder, then renames
//! the temp over the stable file. The stable file only ever goes away through that
//! rename, so a failure or a kill at any point leaves either the original or the link.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Read};
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
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
    /// The blob named by the content hash has another size or other bytes, so it is not trusted.
    BlobMismatch,
    /// The stable file already is the blob.
    AlreadyLinked,
    /// The blob already has the most hard links the filesystem allows.
    LinkLimit,
    /// Songs and the lazer store are on different volumes.
    CrossVolume,
    /// The stable file is open elsewhere or read-only.
    Locked,
    /// The stable file changed between hashing and replacing, or no longer has the
    /// content its cached hash says. Its cache entry is dropped.
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

/// Threads for a relink run: the requested count, or a quarter of the `logical` CPUs,
/// at least 1 and at most 4, so a relink leaves the machine usable.
pub fn relink_threads(requested: Option<NonZeroUsize>, logical: usize) -> usize {
    requested.map_or((logical / 4).clamp(1, 4), NonZeroUsize::get)
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
    threads: usize,
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
            threads: relink_threads(None, logical_cpus()),
            link: |src, dst| fs::hard_link(src, dst),
            same_volume,
        }
    }

    /// Runs on `requested` threads instead of the default from [`relink_threads`].
    pub fn threads(mut self, requested: Option<NonZeroUsize>) -> Self {
        self.threads = relink_threads(requested, logical_cpus());
        self
    }

    /// The hash cache file for `songs` in the per-user osu-sync cache folder.
    pub fn default_cache(songs: &Path) -> Option<PathBuf> {
        crate::config::scan_cache::default_root()
            .map(|root| crate::config::scan_cache::file_for(&root, "relink", songs, "bin"))
    }

    /// Relinks every stable file whose content is a lazer blob. One file failing never
    /// stops the run; it lands in `errors`. The caller checks that stable is closed.
    ///
    /// The run uses its own pool of [`Relinker::threads`] threads, and on Windows puts the
    /// process in background mode (lower CPU, disk and memory priority) until it returns.
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

        let _background = Background::enter();
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(self.threads)
            .thread_name(|i| format!("osu-sync-relink-{i}"))
            .build()
            .map_err(|e| Error::Other(format!("could not start the relink threads: {e}")))?;
        let candidates = self.walk(&mut report);
        let cache = self
            .cache
            .as_deref()
            .map(HashCache::load)
            .unwrap_or_default();
        let mut next = HashCache {
            version: CACHE_VERSION,
            entries: HashMap::new(),
        };
        let limited = Mutex::new(HashSet::new());
        let mut checked_dirs = HashSet::new();
        let total = candidates.len();
        let mut done = 0;
        // Each batch is hashed right before it is replaced, so the byte compare in `verify`
        // reads stable files that are still in the page cache.
        for batch in folder_batches(candidates, |(stable, _)| stable.parent()) {
            let walked: usize = batch.iter().map(Vec::len).sum();
            let batch: Vec<Vec<Candidate>> = pool.install(|| {
                batch
                    .into_par_iter()
                    .map(|folder| {
                        folder
                            .into_par_iter()
                            .filter_map(|(stable, rel)| self.inspect(stable, rel, &cache))
                            .collect()
                    })
                    .collect()
            });
            // A shard folder under `files` can be a junction into a live store, so each
            // blob folder is guarded on its own before any temp is linked from it. This runs
            // on the caller's thread, where the guard's roots are set.
            for c in batch.iter().flatten() {
                if let Found::Blob {
                    blob,
                    already_linked: false,
                } = &c.found
                {
                    if let Some(dir) = blob.parent() {
                        if !checked_dirs.contains(dir) {
                            live_guard::check_write(dir)?;
                            checked_dirs.insert(dir.to_path_buf());
                        }
                    }
                }
            }

            let settled: Vec<Vec<(Candidate, Settled)>> = pool.install(|| {
                batch
                    .into_par_iter()
                    .map(|folder| {
                        folder
                            .into_iter()
                            .map(|c| {
                                let settled = self.settle(&c, &limited);
                                (c, settled)
                            })
                            .collect()
                    })
                    .collect()
            });
            for (c, settled) in settled.into_iter().flatten() {
                if c.hashed {
                    report.hashed_files += 1;
                    report.hashed_bytes += c.size;
                }
                let mut stat = (c.size, c.mtime);
                let mut forget = false;
                match settled {
                    Settled::Error(e) => report.errors.push(e),
                    Settled::Skip(reason, note) => {
                        forget = reason == RelinkSkip::Changed;
                        report.skip(reason);
                        report.notes.extend(note);
                    }
                    Settled::Relinked { after, reclaimed } => {
                        report.relinked += 1;
                        if reclaimed {
                            report.bytes_reclaimed += c.size;
                        }
                        stat = after.unwrap_or(stat);
                    }
                }
                if let (Some(key), false) = (c.stable.to_str(), c.sha.is_empty() || forget) {
                    next.entries.insert(
                        key.to_string(),
                        CachedHash {
                            size: stat.0,
                            mtime: stat.1,
                            sha: c.sha.clone(),
                        },
                    );
                }
            }
            done += walked;
            progress(done, total);
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
        // An entry that is not a valid hash is a miss, so it is rehashed and replaced.
        let cached = cache
            .get(&c.stable, c.size, c.mtime)
            .filter(|sha| BlobHash::parse(sha).is_some());
        if let Some(sha) = cached {
            c.sha = sha.to_string();
        } else {
            match sha256(&c.stable) {
                Ok(sha) => {
                    c.sha = sha;
                    c.hashed = true;
                }
                Err(e) if is_locked(&e) => {
                    let (reason, note) = locked(&c.stable, &e);
                    c.found = Found::Skip(reason, note);
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
                Err(e) if is_locked(&e) => {
                    let (reason, note) = locked(&c.stable, &e);
                    Found::Skip(reason, note)
                }
                Err(e) => Found::Error(format!("{}: {e}", c.stable.display())),
            },
        };
        Some(c)
    }

    /// Settles one inspected candidate: replaces it when its plan says so. Runs in
    /// parallel across folders, so a blob that hit the link limit is shared through `limited`.
    fn settle(&self, c: &Candidate, limited: &Mutex<HashSet<PathBuf>>) -> Settled {
        match &c.found {
            Found::Error(e) => Settled::Error(e.clone()),
            Found::Skip(reason, note) => Settled::Skip(*reason, note.clone()),
            Found::Blob {
                blob,
                already_linked,
            } => {
                let limit_hit = limited.lock().is_ok_and(|set| set.contains(blob));
                let plan = Relink {
                    stable: c.stable.clone(),
                    blob: blob.clone(),
                    decision: decide(&c.rel, true, *already_linked, limit_hit),
                };
                match self.apply(&plan, c, limited) {
                    Ok(Ok(reclaimed)) => Settled::Relinked {
                        after: fs::metadata(&c.stable)
                            .ok()
                            .map(|meta| (meta.len(), mtime_key(&meta))),
                        reclaimed,
                    },
                    Ok(Err((reason, note))) => Settled::Skip(reason, note),
                    Err(e) => Settled::Error(format!("{}: {e}", c.stable.display())),
                }
            }
        }
    }

    /// Replaces the stable file with a link to its blob when the plan says so. Returns
    /// whether the replaced file was its data's only link, so its space was freed, or the
    /// skip reason and note when it leaves the file as it is.
    fn apply(
        &self,
        plan: &Relink,
        c: &Candidate,
        limited: &Mutex<HashSet<PathBuf>>,
    ) -> io::Result<std::result::Result<bool, Left>> {
        if let Decision::Skip(reason) = plan.decision {
            return Ok(Err((reason, None)));
        }
        // Held from the stat check through the rename, so no writer can change the file
        // after its bytes are compared, and one that has it open already makes this a skip.
        let mut held = match hold(&plan.stable) {
            Ok(file) => file,
            Err(e) if is_locked(&e) => return Ok(Err(locked(&plan.stable, &e))),
            Err(e) => return Err(e),
        };
        let meta = held.metadata()?;
        if meta.len() != c.size || mtime_key(&meta) != c.mtime {
            return Ok(Err((RelinkSkip::Changed, None)));
        }
        let links = match verify(&mut held, plan, c)? {
            Ok(links) => links,
            Err(left) => return Ok(Err(left)),
        };
        let folder = plan
            .stable
            .parent()
            .ok_or_else(|| io::Error::other("stable file has no folder"))?;
        let temp = match self.link_temp(&plan.blob, folder) {
            Ok(temp) => temp,
            Err(e) => {
                return match classify_hard_link_error(&e) {
                    HardLinkFailure::LinkLimit => {
                        if let Ok(mut set) = limited.lock() {
                            set.insert(plan.blob.clone());
                        }
                        Ok(Err((RelinkSkip::LinkLimit, None)))
                    }
                    HardLinkFailure::CrossVolume => Ok(Err((RelinkSkip::CrossVolume, None))),
                    HardLinkFailure::Other => Err(e),
                }
            }
        };
        let renamed = fs::rename(&temp, &plan.stable);
        drop(held);
        match renamed {
            Ok(()) => Ok(Ok(links == 1)),
            Err(e) => {
                let _ = fs::remove_file(&temp);
                if is_locked(&e) {
                    Ok(Err(locked(&plan.stable, &e)))
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

/// What happened to one candidate in the replace phase.
enum Settled {
    Error(String),
    Skip(RelinkSkip, Option<String>),
    /// Replaced. `after` is the size and mtime of the link now in its place; `reclaimed`
    /// says the replaced file was its data's only link.
    Relinked {
        after: Option<(u64, (u64, u32))>,
        reclaimed: bool,
    },
}

/// Why a file stays as it is, with a note for the report.
type Left = (RelinkSkip, Option<String>);

/// Folders per parallel batch; progress is reported after each batch.
const FOLDERS_PER_BATCH: usize = 256;

/// Splits items in walk order into runs of one folder, then into batches. A run stays
/// on one thread; `link_temp` claims temp names atomically, so two runs of a folder may share it.
fn folder_batches<T>(items: Vec<T>, folder_of: impl Fn(&T) -> Option<&Path>) -> Vec<Vec<Vec<T>>> {
    let mut folders: Vec<Vec<T>> = Vec::new();
    for item in items {
        match folders.last_mut() {
            Some(folder) if folder_of(&folder[0]) == folder_of(&item) => folder.push(item),
            _ => folders.push(vec![item]),
        }
    }
    let mut batches = Vec::new();
    let mut rest = folders.into_iter().peekable();
    while rest.peek().is_some() {
        batches.push(rest.by_ref().take(FOLDERS_PER_BATCH).collect());
    }
    batches
}

fn logical_cpus() -> usize {
    std::thread::available_parallelism().map_or(1, NonZeroUsize::get)
}

/// Windows background mode for the process while at least one relink runs. The mode is
/// per process, so concurrent runs share it: the first begins it, the last ends it and
/// restores the priority class, which ending the mode resets to normal.
struct Background;

/// Runs in progress, and the priority class to restore when this process began the mode.
static BACKGROUND: Mutex<(usize, Option<u32>)> = Mutex::new((0, None));

impl Background {
    fn enter() -> Self {
        let mut state = BACKGROUND.lock().unwrap_or_else(PoisonError::into_inner);
        if state.0 == 0 {
            state.1 = begin_background();
        }
        state.0 += 1;
        Self
    }
}

impl Drop for Background {
    fn drop(&mut self) {
        let mut state = BACKGROUND.lock().unwrap_or_else(PoisonError::into_inner);
        state.0 -= 1;
        if state.0 == 0 {
            if let Some(class) = state.1.take() {
                end_background(class);
            }
        }
    }
}

/// Begins background mode and returns the priority class to restore, or `None` when the
/// mode did not begin (it was on already, or this is not Windows).
fn begin_background() -> Option<u32> {
    #[cfg(windows)]
    {
        use windows::Win32::System::Threading::{
            GetCurrentProcess, GetPriorityClass, SetPriorityClass, PROCESS_MODE_BACKGROUND_BEGIN,
        };
        // SAFETY: the pseudo handle of the current process is always valid.
        unsafe {
            let class = GetPriorityClass(GetCurrentProcess());
            SetPriorityClass(GetCurrentProcess(), PROCESS_MODE_BACKGROUND_BEGIN)
                .ok()
                .map(|()| class)
        }
    }
    #[cfg(not(windows))]
    {
        None
    }
}

fn end_background(class: u32) {
    #[cfg(windows)]
    {
        use windows::Win32::System::Threading::{
            GetCurrentProcess, SetPriorityClass, PROCESS_CREATION_FLAGS,
            PROCESS_MODE_BACKGROUND_END,
        };
        // SAFETY: the pseudo handle of the current process is always valid.
        unsafe {
            let _ = SetPriorityClass(GetCurrentProcess(), PROCESS_MODE_BACKGROUND_END);
            if class != 0 {
                let _ = SetPriorityClass(GetCurrentProcess(), PROCESS_CREATION_FLAGS(class));
            }
        }
    }
    #[cfg(not(windows))]
    let _ = class;
}

fn locked(path: &Path, e: &io::Error) -> (RelinkSkip, Option<String>) {
    (
        RelinkSkip::Locked,
        Some(format!(
            "left {} as it is: it is in use or read-only ({e})",
            path.display()
        )),
    )
}

/// Opens the stable file for reading and keeps writers out while the handle is open. On
/// Windows it shares read and delete but not write: a writer that has the file open makes
/// the open fail, no new writer can open it, and the rename over it still works.
fn hold(path: &Path) -> io::Result<File> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows::Win32::Storage::FileSystem::{FILE_SHARE_DELETE, FILE_SHARE_READ};
        options.share_mode(FILE_SHARE_READ.0 | FILE_SHARE_DELETE.0);
    }
    options.open(path)
}

/// Reads the held stable file and its blob side by side right before the replace, so
/// neither a blob with wrong bytes nor a stale cached hash can put other bytes in the
/// stable file. Returns the stable file's hard-link count when both hold the same bytes,
/// or why the file stays as it is.
fn verify(
    stable: &mut File,
    plan: &Relink,
    c: &Candidate,
) -> io::Result<std::result::Result<u32, Left>> {
    let mut blob = File::open(&plan.blob)?;
    let same = same_bytes(stable, &mut blob, c.size)?;
    let links = link_count(stable)?;
    if same {
        return Ok(Ok(links));
    }
    if sha256(&plan.stable)? != c.sha {
        return Ok(Err((RelinkSkip::Changed, None)));
    }
    Ok(Err((
        RelinkSkip::BlobMismatch,
        Some(format!(
            "{} has the content hash that names {}, but their bytes differ",
            plan.stable.display(),
            plan.blob.display()
        )),
    )))
}

/// Hard links to the data of an open file.
fn link_count(file: &File) -> io::Result<u32> {
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: `file` keeps the handle open for the call and `info` is a valid out pointer.
        unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle() as isize), &mut info) }
            .map_err(io::Error::other)?;
        Ok(info.nNumberOfLinks)
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Ok(u32::try_from(file.metadata()?.nlink()).unwrap_or(u32::MAX))
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = file;
        Ok(1)
    }
}

/// True when both readers hold the same bytes to the end. Stops at the first difference.
fn same_bytes(a: &mut impl Read, b: &mut impl Read, size: u64) -> io::Result<bool> {
    let chunk = usize::try_from(size).map_or(1 << 20, |n| n.clamp(1 << 12, 1 << 20));
    let (mut x, mut y) = (vec![0u8; chunk], vec![0u8; chunk]);
    loop {
        let n = read_full(a, &mut x)?;
        if read_full(b, &mut y)? != n || x[..n] != y[..n] {
            return Ok(false);
        }
        if n == 0 {
            return Ok(true);
        }
    }
}

/// Reads until `buf` is full or the reader ends; returns the bytes read.
fn read_full(input: &mut impl Read, buf: &mut [u8]) -> io::Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        match input.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(n)
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
    fn link_limit_reached_midway_across_parallel_folders_is_exact() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static LINKS_LEFT: AtomicUsize = AtomicUsize::new(5);
        let fx = Fixture::new();
        let blob = fx.blob(AUDIO);
        let folders = FOLDERS_PER_BATCH + 44;
        let mut stables = Vec::new();
        for i in 0..folders {
            stables.push(fx.stable(&format!("{i} A - B/audio.mp3"), AUDIO));
            stables.push(fx.stable(&format!("{i} A - B/hit.wav"), AUDIO));
        }
        let mut r = fx.relinker();
        r.link = |src, dst| match LINKS_LEFT
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
        {
            Ok(_) => fs::hard_link(src, dst),
            Err(_) => Err(io::Error::from_raw_os_error(LINK_LIMIT_OS_ERROR)),
        };
        let mut calls = Vec::new();

        let report = r.run(&mut |done, total| calls.push((done, total))).unwrap();

        let total = 2 * folders;
        assert_eq!(report.relinked, 5);
        assert_eq!(
            report.skipped,
            skipped(&[(RelinkSkip::LinkLimit, total - 5)])
        );
        assert_eq!(report.errors, Vec::<String>::new());
        assert_eq!(report.bytes_reclaimed, 5 * AUDIO.len() as u64);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls.last(), Some(&(total, total)));
        let linked = stables.iter().filter(|s| same(s, &blob)).count();
        assert_eq!(linked, 5);
        for stable in &stables {
            assert_eq!(fs::read(stable).unwrap(), AUDIO);
        }
        for i in 0..folders {
            assert_eq!(
                names(&fx.songs.join(format!("{i} A - B"))),
                vec!["audio.mp3", "hit.wav"]
            );
        }
    }

    #[test]
    fn each_batch_is_hashed_right_before_it_is_replaced() {
        use std::sync::{Once, OnceLock};
        static LATE: OnceLock<PathBuf> = OnceLock::new();
        static REWRITE: Once = Once::new();
        const NEW: &[u8] = b"rewritten while the first batch was replaced";
        let fx = Fixture::new();
        let blob = fx.blob(AUDIO);
        let folders = FOLDERS_PER_BATCH + 1;
        for i in 0..folders {
            fx.stable(&format!("{i:04} A - B/audio.mp3"), AUDIO);
        }
        let late = fx.songs.join(format!("{FOLDERS_PER_BATCH:04} A - B"));
        LATE.set(late.join("audio.mp3")).unwrap();
        let mut r = fx.relinker();
        r.link = |src, dst| {
            REWRITE.call_once(|| fs::write(LATE.get().unwrap(), NEW).unwrap());
            fs::hard_link(src, dst)
        };

        let report = run(&r);

        // Hashed in its own batch, the rewritten file has no blob. Hashed with the first
        // batch, it would have been found changed at the replace.
        assert_eq!(report.relinked, FOLDERS_PER_BATCH);
        assert_eq!(report.skipped, skipped(&[(RelinkSkip::NoBlob, 1)]));
        assert_eq!(report.errors, Vec::<String>::new());
        assert_eq!(report.hashed_files, folders);
        assert_eq!(fs::read(late.join("audio.mp3")).unwrap(), NEW);
        assert!(!same(&late.join("audio.mp3"), &blob));
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

    const AUDIO_SHA256: &str = "27bcb9e8840262151723ad63edf27cbab63d0e4c45d6283ba1c853d210f2e58d";

    /// Hard links of the file at `path`.
    fn links(path: &Path) -> u32 {
        link_count(&File::open(path).unwrap()).unwrap()
    }

    #[test]
    fn same_bytes_reads_both_to_the_end() {
        let big = vec![7u8; (1 << 20) + 5];
        let mut late = big.clone();
        late[(1 << 20) + 4] = 8;
        let size = big.len() as u64;
        assert!(same_bytes(&mut &big[..], &mut &big[..], size).unwrap());
        assert!(!same_bytes(&mut &big[..], &mut &late[..], size).unwrap());
        assert!(!same_bytes(&mut &big[..], &mut &big[..big.len() - 1], size).unwrap());
        assert!(same_bytes(&mut &b""[..], &mut &b""[..], 0).unwrap());
        assert!(!same_bytes(&mut &b""[..], &mut &b"x"[..], 0).unwrap());
    }

    #[test]
    fn same_size_blob_with_other_bytes_is_never_linked() {
        let fx = Fixture::new();
        let blob = BlobHash::parse(AUDIO_SHA256).unwrap().path_in(&fx.files);
        fs::create_dir_all(blob.parent().unwrap()).unwrap();
        let mut tampered = AUDIO.to_vec();
        tampered[0] ^= 0xff;
        fs::write(&blob, &tampered).unwrap();
        let stable = fx.stable("1 A - B/audio.mp3", AUDIO);

        let report = run(&fx.relinker());

        assert_eq!(report.relinked, 0);
        assert_eq!(report.bytes_reclaimed, 0);
        assert_eq!(report.skipped, skipped(&[(RelinkSkip::BlobMismatch, 1)]));
        assert_eq!(report.errors, Vec::<String>::new());
        assert_eq!(
            report.notes,
            [format!(
                "{} has the content hash that names {}, but their bytes differ",
                stable.display(),
                blob.display()
            )]
        );
        assert_eq!(sha256(&stable).unwrap(), AUDIO_SHA256);
        assert_eq!(links(&stable), 1);
        assert_eq!(links(&blob), 1);
        assert_eq!(names(stable.parent().unwrap()), ["audio.mp3"]);
    }

    #[test]
    fn stale_cache_entry_never_replaces_new_content() {
        let fx = Fixture::new();
        let a: &[u8] = b"AAAAAAAAAAAAAAAAAAAA";
        let b: &[u8] = b"BBBBBBBBBBBBBBBBBBBB";
        let stable = fx.stable("1 A - B/audio.mp3", a);
        let first = run(&fx.relinker());
        assert_eq!(first.skipped, skipped(&[(RelinkSkip::NoBlob, 1)]));
        // New content at the same size and mtime, so the cache entry still matches.
        let mtime = fs::metadata(&stable).unwrap().modified().unwrap();
        fs::write(&stable, b).unwrap();
        File::options()
            .write(true)
            .open(&stable)
            .unwrap()
            .set_modified(mtime)
            .unwrap();
        let blob = fx.blob(a);

        let second = run(&fx.relinker());

        assert_eq!(second.relinked, 0);
        assert_eq!(second.hashed_files, 0);
        assert_eq!(second.skipped, skipped(&[(RelinkSkip::Changed, 1)]));
        assert_eq!(second.errors, Vec::<String>::new());
        assert_eq!(fs::read(&stable).unwrap(), b);
        assert_eq!(links(&stable), 1);
        assert!(!same(&stable, &blob));

        // The stale entry was dropped, so the next run hashes the new content.
        let third = run(&fx.relinker());
        assert_eq!(third.hashed_files, 1);
        assert_eq!(third.relinked, 0);
        assert_eq!(third.skipped, skipped(&[(RelinkSkip::NoBlob, 1)]));
        assert_eq!(fs::read(&stable).unwrap(), b);
    }

    #[cfg(windows)]
    fn junction(link: &Path, target: &Path) {
        let out = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(target)
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
    }

    #[cfg(windows)]
    #[test]
    fn blob_shard_junctioned_into_live_lazer_is_refused() {
        let fx = Fixture::new();
        let live = fx.dir.path().join("live-lazer");
        let live_files = live.join("files");
        let live_blob = BlobHash::parse(AUDIO_SHA256).unwrap().path_in(&live_files);
        fs::create_dir_all(live_blob.parent().unwrap()).unwrap();
        fs::write(&live_blob, AUDIO).unwrap();
        set_test_roots(LiveRoots::new(
            InstallPaths {
                stable: None,
                lazer: Some(live.clone()),
            },
            Vec::new(),
            None,
        ));
        fs::create_dir_all(&fx.files).unwrap();
        junction(&fx.files.join("2"), &live_files.join("2"));
        let stable = fx.stable("1 A - B/audio.mp3", AUDIO);
        let mut r = fx.relinker();
        r.link = |_, _| panic!("a refused run must not link");

        match r.run(&mut |_, _| panic!("no progress")) {
            Err(Error::LiveWriteRefused { root, .. }) => assert_eq!(root, live),
            other => panic!("expected a refusal, got {other:?}"),
        }
        assert_eq!(links(&stable), 1);
        assert_eq!(links(&live_blob), 1);
        assert_eq!(names(stable.parent().unwrap()), ["audio.mp3"]);
        assert_eq!(sha256(&stable).unwrap(), AUDIO_SHA256);
        assert!(!fx.cache().exists());
    }

    #[test]
    fn unparseable_cached_hash_is_dropped_and_rehashed() {
        let fx = Fixture::new();
        let stable = fx.stable("1 A - B/audio.mp3", AUDIO);
        let meta = fs::metadata(&stable).unwrap();
        let key = stable.to_str().unwrap().to_string();
        let mut seeded = HashCache {
            version: CACHE_VERSION,
            entries: HashMap::new(),
        };
        seeded.entries.insert(
            key.clone(),
            CachedHash {
                size: meta.len(),
                mtime: mtime_key(&meta),
                sha: "zz".to_string(),
            },
        );
        fs::create_dir_all(fx.cache().parent().unwrap()).unwrap();
        seeded.save(&fx.cache()).unwrap();

        let first = run(&fx.relinker());
        assert_eq!(first.errors, Vec::<String>::new());
        assert_eq!((first.hashed_files, first.relinked), (1, 0));
        assert_eq!(first.skipped, skipped(&[(RelinkSkip::NoBlob, 1)]));
        assert_eq!(HashCache::load(&fx.cache()).entries[&key].sha, AUDIO_SHA256);

        let second = run(&fx.relinker());
        assert_eq!(second.errors, Vec::<String>::new());
        assert_eq!((second.hashed_files, second.relinked), (0, 0));
        assert_eq!(second.skipped, skipped(&[(RelinkSkip::NoBlob, 1)]));
    }

    #[test]
    fn bytes_reclaimed_counts_only_files_with_one_link() {
        let fx = Fixture::new();
        let blob = fx.blob(AUDIO);
        let shared = fx.stable("1 A - B/audio.mp3", AUDIO);
        let outside = fx.dir.path().join("backup-audio.mp3");
        fs::hard_link(&shared, &outside).unwrap();
        let sole = fx.stable("2 C - D/audio.mp3", AUDIO);

        let report = run(&fx.relinker());

        assert_eq!(report.errors, Vec::<String>::new());
        assert_eq!(report.relinked, 2);
        assert_eq!(report.bytes_reclaimed, 20);
        assert!(same(&shared, &blob) && same(&sole, &blob));
        assert_eq!(links(&outside), 1);
        assert_eq!(fs::read(&outside).unwrap(), AUDIO);
    }

    #[test]
    fn default_relink_threads_are_a_quarter_of_the_cpus_from_1_to_4() {
        let n = NonZeroUsize::new;
        assert_eq!(relink_threads(None, 32), 4);
        assert_eq!(relink_threads(None, 16), 4);
        assert_eq!(relink_threads(None, 12), 3);
        assert_eq!(relink_threads(None, 8), 2);
        assert_eq!(relink_threads(None, 3), 1);
        assert_eq!(relink_threads(None, 1), 1);
        assert_eq!(relink_threads(None, 0), 1);
        assert_eq!(relink_threads(n(32), 32), 32);
        assert_eq!(relink_threads(n(1), 32), 1);
    }

    #[test]
    fn relink_runs_in_its_own_pool_of_the_requested_size() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEEN: AtomicUsize = AtomicUsize::new(0);
        let fx = Fixture::new();
        let blob = fx.blob(AUDIO);
        let stable = fx.stable("1 A - B/audio.mp3", AUDIO);
        let mut r = fx.relinker().threads(NonZeroUsize::new(3));
        r.link = |src, dst| {
            SEEN.store(rayon::current_num_threads(), Ordering::SeqCst);
            fs::hard_link(src, dst)
        };

        let report = run(&r);

        assert_eq!(report.relinked, 1);
        assert_eq!(SEEN.load(Ordering::SeqCst), 3);
        assert!(same(&stable, &blob));
    }

    #[cfg(windows)]
    fn memory_priority() -> u32 {
        use windows::Win32::System::Threading::{
            GetCurrentProcess, GetProcessInformation, ProcessMemoryPriority,
            MEMORY_PRIORITY_INFORMATION,
        };
        let mut info = MEMORY_PRIORITY_INFORMATION::default();
        // SAFETY: `info` is a valid out pointer of the size passed.
        unsafe {
            GetProcessInformation(
                GetCurrentProcess(),
                ProcessMemoryPriority,
                (&mut info as *mut MEMORY_PRIORITY_INFORMATION).cast(),
                std::mem::size_of::<MEMORY_PRIORITY_INFORMATION>() as u32,
            )
        }
        .unwrap();
        info.MemoryPriority.0
    }

    /// Background mode shows as memory priority 1 (very low) instead of 5 (normal).
    #[cfg(windows)]
    #[test]
    fn relink_runs_in_background_mode() {
        use std::sync::atomic::{AtomicU32, Ordering};
        static SEEN: AtomicU32 = AtomicU32::new(0);
        let fx = Fixture::new();
        fx.blob(AUDIO);
        fx.stable("1 A - B/audio.mp3", AUDIO);
        let mut r = fx.relinker();
        r.link = |src, dst| {
            SEEN.store(memory_priority(), Ordering::SeqCst);
            fs::hard_link(src, dst)
        };

        assert_eq!(run(&r).relinked, 1);
        assert_eq!(SEEN.load(Ordering::SeqCst), 1);
    }

    /// A third-party write after the byte compare used to land in the replaced file and be
    /// lost. The held handle keeps writers out until the rename, so the write fails instead.
    #[cfg(windows)]
    #[test]
    fn a_write_between_the_compare_and_the_rename_is_refused() {
        use std::sync::OnceLock;
        static TARGET: OnceLock<PathBuf> = OnceLock::new();
        static WRITES: Mutex<Vec<Option<i32>>> = Mutex::new(Vec::new());
        let fx = Fixture::new();
        let blob = fx.blob(AUDIO);
        let stable = fx.stable("1 A - B/audio.mp3", AUDIO);
        TARGET.set(stable.clone()).unwrap();
        let mut r = fx.relinker();
        r.link = |src, dst| {
            let write = fs::write(TARGET.get().unwrap(), b"USER EDIT 20 BYTES!!");
            WRITES
                .lock()
                .unwrap()
                .push(write.err().and_then(|e| e.raw_os_error()));
            fs::hard_link(src, dst)
        };

        let report = run(&r);

        assert_eq!(*WRITES.lock().unwrap(), [Some(32)]);
        assert_eq!(report.relinked, 1);
        assert_eq!(report.skipped, skipped(&[]));
        assert_eq!(fs::read(&stable).unwrap(), AUDIO);
        assert!(same(&stable, &blob));
    }

    /// A writer that has the file open, whether or not it shares delete, makes the file a
    /// locked skip, and its later writes land in the stable file, not in orphaned data.
    #[cfg(windows)]
    #[test]
    fn a_file_open_for_writing_is_left_as_it_is() {
        use std::io::Write;
        use std::os::windows::fs::OpenOptionsExt;
        for share in [1 | 2 | 4, 1 | 2] {
            let fx = Fixture::new();
            let blob = fx.blob(AUDIO);
            let stable = fx.stable("1 A - B/audio.mp3", AUDIO);
            let mut writer = File::options()
                .write(true)
                .share_mode(share)
                .open(&stable)
                .unwrap();

            let report = run(&fx.relinker());
            writer.write_all(b"X").unwrap();
            drop(writer);

            assert_eq!(report.relinked, 0, "share {share}");
            assert_eq!(report.skipped, skipped(&[(RelinkSkip::Locked, 1)]));
            assert_eq!(report.errors, Vec::<String>::new());
            assert!(
                report.notes[0].starts_with(&format!(
                    "left {} as it is: it is in use or read-only (The process cannot access the file because it is being used by another process. (os error 32))",
                    stable.display()
                )),
                "{}",
                report.notes[0]
            );
            assert_eq!(fs::read(&stable).unwrap(), b"XD3 not really audio");
            assert_eq!(fs::read(&blob).unwrap(), AUDIO);
            assert_eq!(links(&stable), 1);
            assert_eq!(names(stable.parent().unwrap()), ["audio.mp3"]);
        }
    }
}
