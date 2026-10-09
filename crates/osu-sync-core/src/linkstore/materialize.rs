use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Utc};
use md5::Md5;
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

use crate::beatmap::folder_name;
use crate::config::live_guard;
use crate::error::{Error, Result};
use crate::lazer::LazerBeatmapSet;
use crate::unified::{classify_hard_link_error, rename_no_replace, same_volume, HardLinkFailure};

/// Longest full path osu!stable opens: `MAX_PATH` (260) minus the 12 characters
/// Windows keeps free for an 8.3 file name.
pub const STABLE_PATH_LIMIT: usize = 248;

/// A space and eight characters of the set's realm ID, added when another set owns the folder name.
const SUFFIX_LEN: usize = 9;

const TEMP_PREFIX: &str = "osu-sync_tmp_";
const TEMP_SUFFIX: &str = ".part";

/// SHA-256 of a blob in lazer's store, 64 lowercase hex characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BlobHash(String);

impl BlobHash {
    pub fn parse(hash: &str) -> Option<Self> {
        (hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()))
            .then(|| Self(hash.to_ascii_lowercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Where lazer keeps the blob: `files/a/ab/abcd...`.
    pub fn path_in(&self, files: &Path) -> PathBuf {
        files.join(&self.0[..1]).join(&self.0[..2]).join(&self.0)
    }
}

/// How one file reaches the Songs folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum How {
    Link,
    Copy,
    Skip,
}

/// Decides how a file at `rel` (relative to the set folder) is placed.
///
/// `.osz2` packages and `Data/e` payloads are encrypted osu!stable leftovers that
/// stable cannot read from a plain folder, so they are skipped.
pub fn how(rel: &str, same_volume: bool, link_limit_hit: bool) -> How {
    let lower = rel.replace('\\', "/").to_lowercase();
    if lower.ends_with(".osz2") || lower.starts_with("data/e/") {
        How::Skip
    } else if lower.ends_with(".osu") || lower.ends_with(".osb") {
        How::Copy
    } else if same_volume && !link_limit_hit {
        How::Link
    } else {
        How::Copy
    }
}

/// One file of a set: its path inside the set folder, its blob, and how it gets there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    pub dest: PathBuf,
    pub blob: BlobHash,
    pub how: How,
}

/// A set that passed every set-level check, ready to be written.
#[derive(Debug, Clone)]
pub struct PlannedSet {
    pub folder: String,
    pub date_added: DateTime<Utc>,
    /// `.osu` files come last, so a folder stable can see is always complete.
    pub placements: Vec<Placement>,
    /// MD5 realm expects for each `.osu` blob it references.
    pub osu_md5: HashMap<BlobHash, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SkipReason {
    #[error("the set has no .osu files")]
    NoOsuFiles,
    #[error("already in stable as {folder} (md5 {md5})")]
    AlreadyInStable { md5: String, folder: String },
    #[error("{filename} already exists in {folder}")]
    DuplicateOsuFilename { filename: String, folder: String },
    #[error("{a} and {b} differ only in case or normalization")]
    CaseCollision { a: String, b: String },
    #[error("unsafe filename {filename:?}")]
    UnsafeFilename { filename: String },
    #[error("the path needs {needed} characters, stable allows {limit}")]
    PathBudget { needed: usize, limit: usize },
    #[error("folder {folder} belongs to another set")]
    FolderTaken { folder: String },
    #[error("{filename} is missing from the lazer store")]
    MissingBlob { filename: String },
    #[error("{filename} has md5 {actual}, realm expects {expected}")]
    Md5Mismatch {
        filename: String,
        expected: String,
        actual: String,
    },
    #[error("{filename} already exists with different content")]
    ExistingFileDiffers { filename: String },
}

/// Files placed for one set. `present` files were already in place from an earlier run.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SetCounts {
    pub linked: usize,
    pub copied: usize,
    pub present: usize,
    pub link_limit_copies: usize,
    pub skipped: usize,
}

impl SetCounts {
    pub fn created(&self) -> usize {
        self.linked + self.copied
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetOutcome {
    Materialized(SetCounts),
    Skipped(SkipReason),
    Failed(String),
}

#[derive(Debug, Clone)]
pub struct SetReport {
    pub id: String,
    pub online_id: Option<i32>,
    pub folder: String,
    pub outcome: SetOutcome,
}

/// A set's plan without writing anything, for dry runs.
#[derive(Debug, Clone)]
pub struct SetPlan {
    pub id: String,
    pub online_id: Option<i32>,
    pub folder: String,
    pub result: std::result::Result<PlannedSet, SkipReason>,
}

#[derive(Debug, Clone, Default)]
pub struct MaterializeReport {
    pub sets: Vec<SetReport>,
    /// Lazer's store and Songs are on different volumes, so every file was copied.
    pub cross_volume: bool,
    /// Temporary files left by an interrupted run and removed by this one.
    pub temps_removed: usize,
}

impl MaterializeReport {
    pub fn totals(&self) -> SetCounts {
        let mut total = SetCounts::default();
        for set in &self.sets {
            if let SetOutcome::Materialized(c) = &set.outcome {
                total.linked += c.linked;
                total.copied += c.copied;
                total.present += c.present;
                total.link_limit_copies += c.link_limit_copies;
                total.skipped += c.skipped;
            }
        }
        total
    }

    pub fn skipped(&self) -> impl Iterator<Item = (&SetReport, &SkipReason)> {
        self.sets.iter().filter_map(|s| match &s.outcome {
            SetOutcome::Skipped(reason) => Some((s, reason)),
            _ => None,
        })
    }

    pub fn failed(&self) -> impl Iterator<Item = (&SetReport, &str)> {
        self.sets.iter().filter_map(|s| match &s.outcome {
            SetOutcome::Failed(message) => Some((s, message.as_str())),
            _ => None,
        })
    }
}

/// What stable already holds: folders with their `.osu` names, and beatmap MD5s.
/// Keys are NFC and lowercase, the way stable and Windows compare names.
#[derive(Debug, Default)]
pub struct StableClaims {
    folders: HashMap<String, HashSet<String>>,
    osu_names: HashMap<String, String>,
    md5s: HashMap<String, String>,
    temps: Vec<PathBuf>,
}

impl StableClaims {
    /// Lists every folder in Songs with its `.osu` files and any leftover temp files.
    pub fn from_songs(songs: &Path) -> io::Result<Self> {
        let mut claims = Self::default();
        let entries = match fs::read_dir(songs) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(claims),
            Err(e) => return Err(e),
        };
        for entry in entries {
            let path = entry?.path();
            if !path.is_dir() {
                continue;
            }
            let folder = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            claims.folders.entry(name_key(&folder)).or_default();
            for file in fs::read_dir(&path)? {
                let file = file?;
                let name = file.file_name().to_string_lossy().into_owned();
                if is_temp_name(&name) {
                    claims.temps.push(file.path());
                } else if name.to_lowercase().ends_with(".osu") {
                    claims.claim(&folder, Some(&name), None);
                }
            }
        }
        Ok(claims)
    }

    /// Records that `folder` holds a beatmap, as listed in Songs or in osu!.db.
    pub fn claim(&mut self, folder: &str, osu_filename: Option<&str>, md5: Option<&str>) {
        let keys = self.folders.entry(name_key(folder)).or_default();
        if let Some(name) = osu_filename.filter(|n| !n.is_empty()) {
            let key = name_key(name);
            keys.insert(key.clone());
            self.osu_names
                .entry(key)
                .or_insert_with(|| folder.to_string());
        }
        if let Some(md5) = md5.filter(|m| !m.is_empty()) {
            self.md5s
                .entry(md5.to_ascii_lowercase())
                .or_insert_with(|| folder.to_string());
        }
    }

    fn record(&mut self, set: &PlannedSet) {
        self.folders.entry(name_key(&set.folder)).or_default();
        for p in &set.placements {
            if is_top_level_osu(&p.dest) {
                let name = p.dest.to_string_lossy();
                let md5 = set.osu_md5.get(&p.blob).map(String::as_str);
                self.claim(&set.folder, Some(&name), md5);
            }
        }
    }

    /// True when the folder exists and holds a `.osu` file this set does not have.
    fn foreign(&self, folder: &str, ours: &HashSet<String>) -> bool {
        self.folders
            .get(&name_key(folder))
            .is_some_and(|keys| !keys.is_subset(ours))
    }

    /// Plans one set against what stable holds. Reads nothing from disk.
    pub fn plan(&self, set: &LazerBeatmapSet, songs: &Path) -> SetPlan {
        let meta = set.beatmaps.first().map(|b| &b.metadata);
        let base = clean_folder(&folder_name(set.online_id.filter(|id| *id > 0), meta));
        let result = self.plan_set(set, songs, &base);
        SetPlan {
            id: set.id.clone(),
            online_id: set.online_id,
            folder: result.as_ref().map(|p| p.folder.clone()).unwrap_or(base),
            result,
        }
    }

    fn plan_set(
        &self,
        set: &LazerBeatmapSet,
        songs: &Path,
        base: &str,
    ) -> std::result::Result<PlannedSet, SkipReason> {
        let mut seen: HashMap<String, &str> = HashMap::new();
        let mut placements = Vec::with_capacity(set.files.len());
        let mut osu_keys = HashMap::new();
        let mut longest = 0;
        for file in &set.files {
            let unsafe_name = || SkipReason::UnsafeFilename {
                filename: file.filename.clone(),
            };
            let parts: Vec<&str> = file.filename.split(['/', '\\']).collect();
            if !parts.iter().all(|p| safe_component(p)) {
                return Err(unsafe_name());
            }
            if parts.len() == 1 && is_temp_name(parts[0]) {
                return Err(unsafe_name());
            }
            let rel = parts.join("/");
            let key = name_key(&rel);
            if let Some(prev) = seen.insert(key.clone(), &file.filename) {
                return Err(SkipReason::CaseCollision {
                    a: prev.to_string(),
                    b: file.filename.clone(),
                });
            }
            let blob = BlobHash::parse(&file.hash).ok_or_else(|| SkipReason::MissingBlob {
                filename: file.filename.clone(),
            })?;
            longest = longest.max(len16(&rel));
            let dest: PathBuf = parts.iter().collect();
            if is_top_level_osu(&dest) {
                osu_keys.insert(key, file.filename.clone());
            }
            placements.push(Placement {
                how: how(&rel, true, false),
                dest,
                blob,
            });
        }
        if osu_keys.is_empty() {
            return Err(SkipReason::NoOsuFiles);
        }

        let osu_md5: HashMap<BlobHash, String> = set
            .beatmaps
            .iter()
            .filter(|b| !b.md5_hash.is_empty())
            .filter_map(|b| Some((BlobHash::parse(&b.hash)?, b.md5_hash.to_ascii_lowercase())))
            .collect();

        let fixed = len16(&songs.to_string_lossy()) + 2 + longest;
        let budget_error = |extra: usize| SkipReason::PathBudget {
            needed: fixed + extra,
            limit: STABLE_PATH_LIMIT,
        };
        let budget = STABLE_PATH_LIMIT
            .checked_sub(fixed)
            .ok_or_else(|| budget_error(1))?;
        let mut folder = fit(base, budget).ok_or_else(|| budget_error(1))?;
        let ours: HashSet<String> = osu_keys.keys().cloned().collect();
        if self.foreign(&folder, &ours) {
            let short = budget
                .checked_sub(SUFFIX_LEN)
                .and_then(|b| fit(base, b))
                .ok_or_else(|| budget_error(SUFFIX_LEN + 1))?;
            folder = format!("{short} {}", id_suffix(&set.id));
            if self.foreign(&folder, &ours) {
                return Err(SkipReason::FolderTaken { folder });
            }
        }

        let folder_key = name_key(&folder);
        for p in &placements {
            let Some(md5) = osu_md5.get(&p.blob) else {
                continue;
            };
            if let Some(owner) = self.md5s.get(md5) {
                if name_key(owner) != folder_key {
                    return Err(SkipReason::AlreadyInStable {
                        md5: md5.clone(),
                        folder: owner.clone(),
                    });
                }
            }
        }
        for (key, filename) in &osu_keys {
            if let Some(owner) = self.osu_names.get(key) {
                if name_key(owner) != folder_key {
                    return Err(SkipReason::DuplicateOsuFilename {
                        filename: filename.clone(),
                        folder: owner.clone(),
                    });
                }
            }
        }

        placements.sort_by_key(|p| is_osu(&p.dest));
        Ok(PlannedSet {
            folder,
            date_added: set.date_added,
            placements,
            osu_md5,
        })
    }
}

/// Writes lazer sets into a stable Songs folder.
pub struct Materializer {
    songs: PathBuf,
    files: PathBuf,
    link: fn(&Path, &Path) -> io::Result<()>,
}

impl Materializer {
    /// `files` is lazer's `files` folder, the root of the content-addressed store.
    pub fn new(songs: impl Into<PathBuf>, files: impl Into<PathBuf>) -> Self {
        Self {
            songs: songs.into(),
            files: files.into(),
            link: |src, dst| fs::hard_link(src, dst),
        }
    }

    /// Plans and checks every set without writing anything.
    pub fn preview(&self, sets: &[LazerBeatmapSet], claims: &mut StableClaims) -> Vec<SetPlan> {
        sets.iter()
            .map(|set| {
                let (plan, _) = self.check(set, claims);
                if let Ok(planned) = &plan.result {
                    claims.record(planned);
                }
                plan
            })
            .collect()
    }

    /// Materializes every set. A set is checked in full before anything is written for it.
    pub fn run(
        &self,
        sets: &[LazerBeatmapSet],
        claims: &mut StableClaims,
        progress: &mut dyn FnMut(usize, usize),
    ) -> Result<MaterializeReport> {
        live_guard::check_write(&self.songs)?;
        fs::create_dir_all(&self.songs)?;
        let mut report = MaterializeReport {
            cross_volume: !same_volume(&self.files, &self.songs)?,
            ..Default::default()
        };
        for temp in std::mem::take(&mut claims.temps) {
            fs::remove_file(&temp)?;
            report.temps_removed += 1;
        }

        let mut checked_dirs = HashSet::new();
        for (i, set) in sets.iter().enumerate() {
            let (plan, present) = self.check(set, claims);
            let outcome = match plan.result {
                Err(reason) => SetOutcome::Skipped(reason),
                Ok(planned) => {
                    claims.record(&planned);
                    match self.execute(
                        &planned,
                        &present,
                        &mut report.cross_volume,
                        &mut checked_dirs,
                    ) {
                        Ok(counts) => SetOutcome::Materialized(counts),
                        Err(Error::Io(e)) => SetOutcome::Failed(e.to_string()),
                        Err(e) => return Err(e),
                    }
                }
            };
            report.sets.push(SetReport {
                id: plan.id,
                online_id: plan.online_id,
                folder: plan.folder,
                outcome,
            });
            progress(i + 1, sets.len());
        }
        Ok(report)
    }

    fn check(&self, set: &LazerBeatmapSet, claims: &StableClaims) -> (SetPlan, Vec<bool>) {
        let mut plan = claims.plan(set, &self.songs);
        let mut present = Vec::new();
        if let Ok(planned) = &plan.result {
            match self.preflight(planned) {
                Ok(p) => present = p,
                Err(reason) => plan.result = Err(reason),
            }
        }
        (plan, present)
    }

    /// Checks blobs, `.osu` MD5s and files already in the folder. Returns which
    /// placements are already in place.
    fn preflight(&self, set: &PlannedSet) -> std::result::Result<Vec<bool>, SkipReason> {
        let folder = self.songs.join(&set.folder);
        set.placements
            .iter()
            .map(|p| {
                if p.how == How::Skip {
                    return Ok(false);
                }
                let filename = p.dest.to_string_lossy().replace('\\', "/");
                let blob = p.blob.path_in(&self.files);
                let missing = || SkipReason::MissingBlob {
                    filename: filename.clone(),
                };
                if !blob.is_file() {
                    return Err(missing());
                }
                if let Some(expected) = set.osu_md5.get(&p.blob) {
                    let (_, actual) = digests(&blob).map_err(|_| missing())?;
                    if &actual != expected {
                        return Err(SkipReason::Md5Mismatch {
                            filename,
                            expected: expected.clone(),
                            actual,
                        });
                    }
                }
                let dest = folder.join(&p.dest);
                if fs::symlink_metadata(&dest).is_err() {
                    return Ok(false);
                }
                if same_file::is_same_file(&dest, &blob).unwrap_or(false) {
                    return Ok(true);
                }
                match digests(&dest) {
                    Ok((sha, _)) if sha == p.blob.as_str() => Ok(true),
                    _ => Err(SkipReason::ExistingFileDiffers { filename }),
                }
            })
            .collect()
    }

    fn execute(
        &self,
        set: &PlannedSet,
        present: &[bool],
        cross_volume: &mut bool,
        checked_dirs: &mut HashSet<PathBuf>,
    ) -> Result<SetCounts> {
        let folder = self.songs.join(&set.folder);
        fs::create_dir_all(&folder)?;
        let date_added = SystemTime::from(set.date_added);
        let mut counts = SetCounts::default();
        for (p, &in_place) in set.placements.iter().zip(present) {
            if p.how == How::Skip {
                counts.skipped += 1;
                continue;
            }
            if in_place {
                counts.present += 1;
                continue;
            }
            let blob = p.blob.path_in(&self.files);
            let dest = folder.join(&p.dest);
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent)?;
            }
            if p.how == How::Link && !*cross_volume {
                if let Some(dir) = blob.parent() {
                    if !checked_dirs.contains(dir) {
                        live_guard::check_write(dir)?;
                        checked_dirs.insert(dir.to_path_buf());
                    }
                }
                match (self.link)(&blob, &dest) {
                    Ok(()) => {
                        counts.linked += 1;
                        continue;
                    }
                    Err(e) => match classify_hard_link_error(&e) {
                        HardLinkFailure::LinkLimit => counts.link_limit_copies += 1,
                        HardLinkFailure::CrossVolume => *cross_volume = true,
                        HardLinkFailure::Other => return Err(e.into()),
                    },
                }
            }
            let expected_md5 = set.osu_md5.get(&p.blob).map(String::as_str);
            let mtime = is_osu(&p.dest).then_some(date_added);
            copy_via_temp(&blob, &p.blob, &dest, &folder, expected_md5, mtime)?;
            counts.copied += 1;
        }
        Ok(counts)
    }
}

/// Copies a blob to a temp file in the set folder, verifies it, then moves it into
/// place without replacing anything. The temp file is removed if any step fails.
fn copy_via_temp(
    blob: &Path,
    hash: &BlobHash,
    dest: &Path,
    folder: &Path,
    expected_md5: Option<&str>,
    mtime: Option<SystemTime>,
) -> io::Result<()> {
    let (temp, out) = create_temp(folder)?;
    let result = write_temp(blob, hash, out, expected_md5, mtime)
        .and_then(|()| rename_no_replace(&temp, dest));
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn write_temp(
    blob: &Path,
    hash: &BlobHash,
    mut out: File,
    expected_md5: Option<&str>,
    mtime: Option<SystemTime>,
) -> io::Result<()> {
    let (sha, md5) = copy_hashing(blob, &mut out)?;
    if sha != hash.as_str() {
        return Err(invalid_data(format!(
            "{} copied as sha256 {sha}",
            blob.display()
        )));
    }
    if let Some(expected) = expected_md5 {
        if md5 != expected {
            return Err(invalid_data(format!(
                "{} copied as md5 {md5}, realm expects {expected}",
                blob.display()
            )));
        }
    }
    if let Some(mtime) = mtime {
        out.set_modified(mtime)?;
    }
    Ok(())
}

fn create_temp(folder: &Path) -> io::Result<(PathBuf, File)> {
    let mut n = 0u32;
    loop {
        let path = folder.join(format!("{TEMP_PREFIX}{n}{TEMP_SUFFIX}"));
        match File::options().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => n += 1,
            Err(e) => return Err(e),
        }
    }
}

fn copy_hashing(src: &Path, out: &mut File) -> io::Result<(String, String)> {
    let mut input = File::open(src)?;
    let mut sha = Sha256::new();
    let mut md5 = Md5::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        sha.update(&buf[..n]);
        md5.update(&buf[..n]);
        out.write_all(&buf[..n])?;
    }
    Ok((
        format!("{:x}", sha.finalize()),
        format!("{:x}", md5.finalize()),
    ))
}

/// SHA-256 and MD5 of a file, as lowercase hex.
fn digests(path: &Path) -> io::Result<(String, String)> {
    let mut input = File::open(path)?;
    let mut sha = Sha256::new();
    let mut md5 = Md5::new();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        sha.update(&buf[..n]);
        md5.update(&buf[..n]);
    }
    Ok((
        format!("{:x}", sha.finalize()),
        format!("{:x}", md5.finalize()),
    ))
}

fn invalid_data(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn name_key(name: &str) -> String {
    name.nfc().collect::<String>().to_lowercase()
}

fn len16(s: &str) -> usize {
    s.encode_utf16().count()
}

fn is_osu(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("osu"))
}

fn is_top_level_osu(path: &Path) -> bool {
    path.components().count() == 1 && is_osu(path)
}

fn is_temp_name(name: &str) -> bool {
    name.starts_with(TEMP_PREFIX) && name.ends_with(TEMP_SUFFIX)
}

/// Longest prefix of `base` within `budget` UTF-16 units, never splitting a
/// surrogate pair, without the trailing dots and spaces Windows drops.
fn fit(base: &str, budget: usize) -> Option<String> {
    let mut used = 0;
    let cut: String = base
        .chars()
        .take_while(|c| {
            used += c.len_utf16();
            used <= budget
        })
        .collect();
    let trimmed = cut.trim_end_matches(['.', ' ']);
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn clean_folder(name: &str) -> String {
    let trimmed = name.trim_start_matches(' ').trim_end_matches(['.', ' ']);
    if trimmed.is_empty() {
        "Unknown Beatmap".to_string()
    } else {
        trimmed.to_string()
    }
}

fn id_suffix(id: &str) -> String {
    id.chars()
        .filter(char::is_ascii_alphanumeric)
        .take(SUFFIX_LEN - 1)
        .collect::<String>()
        .to_ascii_lowercase()
}

/// A path component Windows stores under exactly this name.
fn safe_component(c: &str) -> bool {
    !c.is_empty()
        && c != "."
        && c != ".."
        && !c.ends_with(['.', ' '])
        && !c
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, '<' | '>' | ':' | '"' | '|' | '?' | '*'))
        && !is_reserved_device(c)
}

fn is_reserved_device(c: &str) -> bool {
    let stem = c
        .split('.')
        .next()
        .unwrap_or(c)
        .trim_end()
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((stem.starts_with("COM") || stem.starts_with("LPT"))
            && stem.len() == 4
            && stem.as_bytes()[3].is_ascii_digit()
            && stem.as_bytes()[3] != b'0')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::beatmap::{BeatmapDifficulty, BeatmapMetadata, GameMode};
    use crate::lazer::{LazerBeatmapInfo, LazerNamedFile};
    use crate::unified::LINK_LIMIT_OS_ERROR;
    use chrono::TimeZone;

    const OSU: &[u8] = b"osu file format v14\n\n[General]\nAudioFilename: audio.mp3\n";
    const OSU_MD5: &str = "27ce3bbb14207aeb035570fd0f1c0b9f";
    const OSU_NAME: &str = "Artist - Title (Mapper) [Easy].osu";
    const SET_ID: &str = "1d0c5b0e-7a2f-4c11-9e3b-2f6a8d4c0b17";

    struct Fixture {
        _dir: tempfile::TempDir,
        files: PathBuf,
        songs: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let files = dir.path().join("lazer").join("files");
            let songs = dir.path().join("stable").join("Songs");
            fs::create_dir_all(&files).unwrap();
            Self {
                _dir: dir,
                files,
                songs,
            }
        }

        fn blob(&self, content: &[u8]) -> String {
            let hash = format!("{:x}", Sha256::digest(content));
            let path = BlobHash::parse(&hash).unwrap().path_in(&self.files);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, content).unwrap();
            hash
        }

        fn blob_path(&self, hash: &str) -> PathBuf {
            BlobHash::parse(hash).unwrap().path_in(&self.files)
        }

        fn materializer(&self) -> Materializer {
            Materializer::new(&self.songs, &self.files)
        }

        fn run(&self, m: &Materializer, sets: &[LazerBeatmapSet]) -> MaterializeReport {
            let mut claims = StableClaims::from_songs(&self.songs).unwrap();
            m.run(sets, &mut claims, &mut |_, _| {}).unwrap()
        }

        /// A set with one `.osu` and an audio file, both stored as blobs.
        fn basic_set(&self) -> LazerBeatmapSet {
            let osu = self.blob(OSU);
            let audio = self.blob(b"ID3 not really audio");
            set(
                SET_ID,
                Some(1001),
                "Artist",
                "Title",
                vec![(OSU_NAME, osu.clone()), ("audio.mp3", audio)],
                vec![(osu, OSU_MD5.to_string())],
            )
        }
    }

    fn date_added() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2024, 3, 5, 7, 7, 9).unwrap()
    }

    fn set(
        id: &str,
        online_id: Option<i32>,
        artist: &str,
        title: &str,
        files: Vec<(&str, String)>,
        beatmaps: Vec<(String, String)>,
    ) -> LazerBeatmapSet {
        let metadata = BeatmapMetadata {
            artist: artist.to_string(),
            title: title.to_string(),
            ..Default::default()
        };
        LazerBeatmapSet {
            id: id.to_string(),
            online_id,
            beatmaps: beatmaps
                .into_iter()
                .map(|(hash, md5_hash)| LazerBeatmapInfo {
                    id: format!("{id}-{hash}"),
                    online_id: None,
                    hash,
                    md5_hash,
                    metadata: metadata.clone(),
                    difficulty: BeatmapDifficulty::default(),
                    version: "Easy".to_string(),
                    mode: GameMode::Osu,
                    length_ms: 0,
                    bpm: 0.0,
                    star_rating: None,
                    ranked_status: None,
                })
                .collect(),
            files: files
                .into_iter()
                .map(|(filename, hash)| LazerNamedFile {
                    filename: filename.to_string(),
                    hash,
                })
                .collect(),
            date_added: date_added(),
        }
    }

    fn only_counts(report: &MaterializeReport) -> SetCounts {
        match &report.sets[..] {
            [SetReport {
                outcome: SetOutcome::Materialized(counts),
                ..
            }] => *counts,
            other => panic!("expected one materialized set, got {other:?}"),
        }
    }

    fn only_skip(report: &MaterializeReport) -> SkipReason {
        match &report.sets[..] {
            [SetReport {
                outcome: SetOutcome::Skipped(reason),
                ..
            }] => reason.clone(),
            other => panic!("expected one skipped set, got {other:?}"),
        }
    }

    #[test]
    fn how_follows_the_file_type_and_volume() {
        assert_eq!(how("a.osu", true, false), How::Copy);
        assert_eq!(how("A.OSB", true, false), How::Copy);
        assert_eq!(how("audio.mp3", true, false), How::Link);
        assert_eq!(how("sb/bg.png", true, false), How::Link);
        assert_eq!(how("audio.mp3", false, false), How::Copy);
        assert_eq!(how("audio.mp3", true, true), How::Copy);
        assert_eq!(how("pack.osz2", true, false), How::Skip);
        assert_eq!(how("Data/e/clip.dat", true, false), How::Skip);
        assert_eq!(how("Data\\e\\clip.dat", true, false), How::Skip);
    }

    #[test]
    fn osu_files_are_copied() {
        let fx = Fixture::new();
        let s = fx.basic_set();
        let report = fx.run(&fx.materializer(), &[s]);

        let dest = fx.songs.join("1001 Artist - Title").join(OSU_NAME);
        assert_eq!(fs::read(&dest).unwrap(), OSU);
        assert!(!same_file::is_same_file(&dest, fx.blob_path(&fx.blob(OSU))).unwrap());
        assert_eq!(
            only_counts(&report),
            SetCounts {
                linked: 1,
                copied: 1,
                ..Default::default()
            }
        );
    }

    #[test]
    fn assets_are_hard_linked() {
        let fx = Fixture::new();
        let s = fx.basic_set();
        fx.run(&fx.materializer(), &[s]);

        let dest = fx.songs.join("1001 Artist - Title").join("audio.mp3");
        let blob = fx.blob_path(&fx.blob(b"ID3 not really audio"));
        assert!(same_file::is_same_file(&dest, &blob).unwrap());
    }

    #[test]
    fn link_limit_falls_back_to_copy() {
        let fx = Fixture::new();
        let s = fx.basic_set();
        let mut m = fx.materializer();
        m.link = |_, _| Err(io::Error::from_raw_os_error(LINK_LIMIT_OS_ERROR));
        let report = fx.run(&m, &[s]);

        let dest = fx.songs.join("1001 Artist - Title").join("audio.mp3");
        let blob = fx.blob_path(&fx.blob(b"ID3 not really audio"));
        assert_eq!(fs::read(&dest).unwrap(), b"ID3 not really audio");
        assert!(!same_file::is_same_file(&dest, &blob).unwrap());
        assert_eq!(
            only_counts(&report),
            SetCounts {
                copied: 2,
                link_limit_copies: 1,
                ..Default::default()
            }
        );
        assert!(!report.cross_volume);
    }

    #[test]
    fn rerun_is_a_no_op() {
        let fx = Fixture::new();
        let s = fx.basic_set();
        let m = fx.materializer();
        fx.run(&m, std::slice::from_ref(&s));
        let report = fx.run(&m, &[s]);

        assert_eq!(
            only_counts(&report),
            SetCounts {
                present: 2,
                ..Default::default()
            }
        );
        let mut names: Vec<_> = fs::read_dir(fx.songs.join("1001 Artist - Title"))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(names, [OSU_NAME, "audio.mp3"]);
    }

    #[test]
    fn stale_temp_files_are_removed() {
        let fx = Fixture::new();
        let s = fx.basic_set();
        let folder = fx.songs.join("1001 Artist - Title");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("osu-sync_tmp_0.part"), b"half").unwrap();
        let report = fx.run(&fx.materializer(), &[s]);

        assert_eq!(report.temps_removed, 1);
        assert!(!folder.join("osu-sync_tmp_0.part").exists());
        assert_eq!(only_counts(&report).created(), 2);
    }

    #[test]
    fn colliding_names_get_hash_suffix() {
        let fx = Fixture::new();
        let s = fx.basic_set();
        let taken = fx.songs.join("1001 Artist - Title");
        fs::create_dir_all(&taken).unwrap();
        fs::write(taken.join("Someone Else [Hard].osu"), b"other").unwrap();
        let report = fx.run(&fx.materializer(), &[s]);

        assert_eq!(report.sets[0].folder, "1001 Artist - Title 1d0c5b0e");
        assert_eq!(
            fs::read(fx.songs.join("1001 Artist - Title 1d0c5b0e").join(OSU_NAME)).unwrap(),
            OSU
        );
        assert_eq!(
            fs::read(taken.join("Someone Else [Hard].osu")).unwrap(),
            b"other"
        );
    }

    #[test]
    fn backslash_title_is_sanitized() {
        let fx = Fixture::new();
        let mut s = fx.basic_set();
        for b in &mut s.beatmaps {
            b.metadata.artist = "Neko Hacker".to_string();
            b.metadata.title = r"Turkish March - Owata \(^o^)/".to_string();
        }
        let report = fx.run(&fx.materializer(), &[s]);

        assert_eq!(
            report.sets[0].folder,
            "1001 Neko Hacker - Turkish March - Owata _(^o^)_"
        );
        assert!(fx
            .songs
            .join("1001 Neko Hacker - Turkish March - Owata _(^o^)_")
            .join(OSU_NAME)
            .is_file());
    }

    #[test]
    fn osu_md5_matches_realm() {
        let fx = Fixture::new();
        let s = fx.basic_set();
        fx.run(&fx.materializer(), &[s]);

        let (_, md5) = digests(&fx.songs.join("1001 Artist - Title").join(OSU_NAME)).unwrap();
        assert_eq!(md5, "27ce3bbb14207aeb035570fd0f1c0b9f");
    }

    #[test]
    fn md5_mismatch_skips_set() {
        let fx = Fixture::new();
        let mut s = fx.basic_set();
        s.beatmaps[0].md5_hash = "00000000000000000000000000000000".to_string();
        let report = fx.run(&fx.materializer(), &[s]);

        assert_eq!(
            only_skip(&report),
            SkipReason::Md5Mismatch {
                filename: OSU_NAME.to_string(),
                expected: "00000000000000000000000000000000".to_string(),
                actual: "27ce3bbb14207aeb035570fd0f1c0b9f".to_string(),
            }
        );
        assert!(!fx.songs.join("1001 Artist - Title").exists());
    }

    #[test]
    fn duplicate_osu_filename_skips_set() {
        let fx = Fixture::new();
        let s = fx.basic_set();
        let mut claims = StableClaims::default();
        claims.claim(
            "Old Folder",
            Some("ARTIST - TITLE (Mapper) [EASY].osu"),
            None,
        );
        let report = fx
            .materializer()
            .run(&[s], &mut claims, &mut |_, _| {})
            .unwrap();

        assert_eq!(
            only_skip(&report),
            SkipReason::DuplicateOsuFilename {
                filename: OSU_NAME.to_string(),
                folder: "Old Folder".to_string(),
            }
        );
        assert!(!fx.songs.join("1001 Artist - Title").exists());
    }

    #[test]
    fn duplicate_osu_filename_within_one_run_skips_the_later_set() {
        let fx = Fixture::new();
        let first = fx.basic_set();
        let mut second = fx.basic_set();
        second.id = "99999999-0000-0000-0000-000000000000".to_string();
        second.online_id = Some(2002);
        second.beatmaps[0].md5_hash.clear();
        let report = fx.run(&fx.materializer(), &[first, second]);

        assert_eq!(report.sets[0].folder, "1001 Artist - Title");
        assert_eq!(
            report.sets[1].outcome,
            SetOutcome::Skipped(SkipReason::DuplicateOsuFilename {
                filename: OSU_NAME.to_string(),
                folder: "1001 Artist - Title".to_string(),
            })
        );
        assert!(!fx.songs.join("2002 Artist - Title").exists());
    }

    #[test]
    fn same_md5_in_stable_skips_set() {
        let fx = Fixture::new();
        let s = fx.basic_set();
        let mut claims = StableClaims::default();
        claims.claim("Old Folder", Some("renamed.osu"), Some(OSU_MD5));
        let plan = claims.plan(&s, &fx.songs);

        assert_eq!(
            plan.result.unwrap_err(),
            SkipReason::AlreadyInStable {
                md5: OSU_MD5.to_string(),
                folder: "Old Folder".to_string(),
            }
        );
    }

    #[test]
    fn names_differing_only_in_case_are_a_collision() {
        let fx = Fixture::new();
        let mut s = fx.basic_set();
        let bg = fx.blob(b"png");
        s.files.push(LazerNamedFile {
            filename: "BG.png".to_string(),
            hash: bg.clone(),
        });
        s.files.push(LazerNamedFile {
            filename: "bg.png".to_string(),
            hash: bg,
        });
        let plan = StableClaims::default().plan(&s, &fx.songs);

        assert_eq!(
            plan.result.unwrap_err(),
            SkipReason::CaseCollision {
                a: "BG.png".to_string(),
                b: "bg.png".to_string(),
            }
        );
    }

    #[test]
    fn path_budget_trims_folder() {
        let songs = Path::new(r"D:\osu!\Songs");
        let osu = "a".repeat(36) + ".osu";
        assert_eq!(len16(&osu), 40);
        let hash = "ab".repeat(32);
        let make = |title: &str| {
            set(
                SET_ID,
                Some(1),
                "A",
                title,
                vec![(osu.as_str(), hash.clone())],
                vec![(hash.clone(), String::new())],
            )
        };

        // 248 - 13 (Songs) - 2 (separators) - 40 (longest file) leaves 193 for the folder.
        let plan = StableClaims::default().plan(&make(&"x".repeat(300)), songs);
        assert_eq!(plan.folder, format!("1 A - {}", "x".repeat(187)));
        assert_eq!(len16(&plan.folder), 193);

        // Each emoji is two UTF-16 units; a pair is never split, so 192 fits.
        let plan = StableClaims::default().plan(&make(&"\u{1F600}".repeat(150)), songs);
        assert_eq!(plan.folder, format!("1 A - {}", "\u{1F600}".repeat(93)));
        assert_eq!(len16(&plan.folder), 192);

        let long = "b".repeat(230) + ".osu";
        let s = set(
            SET_ID,
            Some(1),
            "A",
            "T",
            vec![(long.as_str(), hash.clone())],
            vec![(hash, String::new())],
        );
        assert_eq!(
            StableClaims::default().plan(&s, songs).result.unwrap_err(),
            SkipReason::PathBudget {
                needed: 250,
                limit: 248,
            }
        );
    }

    #[test]
    fn control_chars_stripped() {
        let fx = Fixture::new();
        let mut s = fx.basic_set();
        for b in &mut s.beatmaps {
            b.metadata.title = "Tab\tBell\u{7}End".to_string();
        }
        let plan = StableClaims::default().plan(&s, &fx.songs);
        assert_eq!(plan.result.unwrap().folder, "1001 Artist - TabBellEnd");

        s.files.push(LazerNamedFile {
            filename: "bad\u{1}.png".to_string(),
            hash: fx.blob(b"png"),
        });
        assert_eq!(
            StableClaims::default()
                .plan(&s, &fx.songs)
                .result
                .unwrap_err(),
            SkipReason::UnsafeFilename {
                filename: "bad\u{1}.png".to_string(),
            }
        );
    }

    #[test]
    fn subfolder_files_created() {
        let fx = Fixture::new();
        let mut s = fx.basic_set();
        let bg = fx.blob(b"storyboard layer");
        s.files.push(LazerNamedFile {
            filename: "sb/layer/bg.png".to_string(),
            hash: bg.clone(),
        });
        let report = fx.run(&fx.materializer(), &[s]);

        let dest = fx
            .songs
            .join("1001 Artist - Title")
            .join("sb")
            .join("layer")
            .join("bg.png");
        assert!(same_file::is_same_file(&dest, fx.blob_path(&bg)).unwrap());
        assert_eq!(only_counts(&report).linked, 2);
    }

    #[test]
    fn osz2_and_osb_are_copied_or_skipped() {
        let fx = Fixture::new();
        let mut s = fx.basic_set();
        let osb = fx.blob(b"[Events]\n");
        for (name, hash) in [
            ("Artist - Title (Mapper).osb", osb.clone()),
            ("pack.osz2", fx.blob(b"osz2")),
            ("Data/e/clip.dat", fx.blob(b"enc")),
        ] {
            s.files.push(LazerNamedFile {
                filename: name.to_string(),
                hash,
            });
        }
        let report = fx.run(&fx.materializer(), &[s]);

        let folder = fx.songs.join("1001 Artist - Title");
        let osb_dest = folder.join("Artist - Title (Mapper).osb");
        assert_eq!(fs::read(&osb_dest).unwrap(), b"[Events]\n");
        assert!(!same_file::is_same_file(&osb_dest, fx.blob_path(&osb)).unwrap());
        assert!(!folder.join("pack.osz2").exists());
        assert!(!folder.join("Data").exists());
        assert_eq!(
            only_counts(&report),
            SetCounts {
                linked: 1,
                copied: 2,
                skipped: 2,
                ..Default::default()
            }
        );
    }

    #[test]
    fn osu_mtime_is_date_added() {
        let fx = Fixture::new();
        let s = fx.basic_set();
        fx.run(&fx.materializer(), &[s]);

        let modified = fs::metadata(fx.songs.join("1001 Artist - Title").join(OSU_NAME))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(
            DateTime::<Utc>::from(modified).to_rfc3339(),
            "2024-03-05T07:07:09+00:00"
        );
    }

    #[test]
    fn linked_file_attributes_untouched() {
        let fx = Fixture::new();
        let s = fx.basic_set();
        let blob = fx.blob_path(&fx.blob(b"ID3 not really audio"));
        let past = SystemTime::from(Utc.with_ymd_and_hms(2020, 1, 2, 3, 4, 5).unwrap());
        File::options()
            .write(true)
            .open(&blob)
            .unwrap()
            .set_modified(past)
            .unwrap();
        let mut readonly = fs::metadata(&blob).unwrap().permissions();
        readonly.set_readonly(true);
        fs::set_permissions(&blob, readonly).unwrap();

        fx.run(&fx.materializer(), &[s]);

        let after = fs::metadata(&blob).unwrap();
        assert_eq!(after.modified().unwrap(), past);
        assert!(after.permissions().readonly());
        let dest = fx.songs.join("1001 Artist - Title").join("audio.mp3");
        assert!(same_file::is_same_file(&dest, &blob).unwrap());

        let mut writable = after.permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        writable.set_readonly(false);
        fs::set_permissions(&blob, writable).unwrap();
    }

    #[test]
    fn missing_blob_skips_set() {
        let fx = Fixture::new();
        let mut s = fx.basic_set();
        s.files.push(LazerNamedFile {
            filename: "gone.png".to_string(),
            hash: "cd".repeat(32),
        });
        let report = fx.run(&fx.materializer(), &[s]);

        assert_eq!(
            only_skip(&report),
            SkipReason::MissingBlob {
                filename: "gone.png".to_string(),
            }
        );
        assert!(!fx.songs.join("1001 Artist - Title").exists());
    }

    #[test]
    fn existing_file_with_other_content_skips_set() {
        let fx = Fixture::new();
        let s = fx.basic_set();
        let folder = fx.songs.join("1001 Artist - Title");
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join("audio.mp3"), b"something else").unwrap();
        let report = fx.run(&fx.materializer(), &[s]);

        assert_eq!(
            only_skip(&report),
            SkipReason::ExistingFileDiffers {
                filename: "audio.mp3".to_string(),
            }
        );
        assert!(!folder.join(OSU_NAME).exists());
    }
}
