//! The linked-store step and its status.
//!
//! One step builds stable's Songs folder out of lazer's `files` store: the
//! materializer writes every lazer set stable lacks as hard links (`.osu` and `.osb`
//! files are copies), then the relinker turns stable files that are plain copies of
//! lazer blobs into links. Setup, the watcher and "sync now" all run this step, and
//! a rerun with nothing new changes nothing.

use std::num::NonZeroUsize;
use std::ops::ControlFlow;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use serde::Serialize;

use crate::config::{live_guard, Config};
use crate::error::{Error, Result};
use crate::lazer::{LazerBeatmapSet, LazerDatabase};
use crate::linkstore::{ensure_stable_closed, link_count_at, Materializer, Relinker, StableClaims};
use crate::sync::format_byte_count;

/// Shortest time between two progress reports within one phase.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(50);

/// The part of the step a progress report belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepPhase {
    /// Reading lazer's library through the realm export.
    Reading,
    /// Writing missing lazer sets into Songs.
    Materializing,
    /// Turning stable copies of lazer blobs into links.
    Relinking,
}

impl StepPhase {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Reading => "Reading lazer library",
            Self::Materializing => "Linking lazer sets into Songs",
            Self::Relinking => "Relinking stable copies",
        }
    }
}

/// What one linked-store step did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct StepReport {
    pub lazer_sets: usize,
    /// Sets that got at least one new file.
    pub sets_written: usize,
    /// Sets whose folder was already complete.
    pub sets_complete: usize,
    pub sets_skipped: usize,
    pub sets_failed: usize,
    /// New files made as hard links to lazer blobs.
    pub files_linked: usize,
    /// New files written as copies: `.osu`, `.osb`, and link fallbacks.
    pub files_copied: usize,
    /// Existing stable copies replaced by links.
    pub relinked: usize,
    pub bytes_reclaimed: u64,
    pub notes: Vec<String>,
    pub errors: Vec<String>,
    /// `client.realm`'s modified time and size right before the step read it.
    #[serde(skip)]
    pub(crate) realm_stamp: Option<FileStamp>,
}

impl StepReport {
    /// Files the step created or replaced; 0 means the step changed nothing.
    pub fn changed_files(&self) -> usize {
        self.files_linked + self.files_copied + self.relinked
    }

    /// The counts as label and value, in the order every front end shows them.
    pub fn rows(&self) -> Vec<(&'static str, String)> {
        vec![
            ("Lazer sets", self.lazer_sets.to_string()),
            ("Sets written", self.sets_written.to_string()),
            ("Sets complete", self.sets_complete.to_string()),
            ("Sets skipped", self.sets_skipped.to_string()),
            ("Sets failed", self.sets_failed.to_string()),
            ("Files linked", self.files_linked.to_string()),
            ("Files copied", self.files_copied.to_string()),
            ("Relinked", self.relinked.to_string()),
            ("Bytes reclaimed", format_byte_count(self.bytes_reclaimed)),
        ]
    }
}

/// A file's modified time and size, to tell whether it changed between two looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FileStamp {
    pub modified: SystemTime,
    pub len: u64,
}

impl FileStamp {
    pub(crate) fn of(path: &Path) -> Option<Self> {
        let meta = std::fs::metadata(path).ok()?;
        Some(Self {
            modified: meta.modified().ok()?,
            len: meta.len(),
        })
    }
}

/// How much of Songs shares its data with lazer's store.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct LinkedStoreStatus {
    /// Files with two or more hard links.
    pub linked_files: u64,
    /// Files with one link: `.osu` and `.osb` files, stable-only sets, fallbacks.
    pub copied_files: u64,
    /// Sum of the sizes of the linked files, stored once instead of twice.
    pub bytes_saved: u64,
    /// Files whose link count could not be read, such as locked ones.
    pub unreadable_files: u64,
}

impl LinkedStoreStatus {
    /// The counts as label and value; unreadable files only when there are some.
    pub fn rows(&self) -> Vec<(&'static str, String)> {
        let mut rows = vec![
            ("Linked files", self.linked_files.to_string()),
            ("Copied files", self.copied_files.to_string()),
            ("Bytes saved", format_byte_count(self.bytes_saved)),
        ];
        if self.unreadable_files > 0 {
            rows.push(("Unreadable files", self.unreadable_files.to_string()));
        }
        rows
    }
}

/// Runs the linked-store step between one stable and one lazer install.
pub struct UnifiedStorageEngine {
    stable: PathBuf,
    lazer: PathBuf,
    relink_threads: Option<NonZeroUsize>,
    relink_cache: Option<PathBuf>,
}

impl UnifiedStorageEngine {
    /// `stable` is the osu!stable folder (holding Songs and osu!.db), `lazer` the
    /// osu!lazer data folder (holding client.realm and files).
    pub fn new(stable: impl Into<PathBuf>, lazer: impl Into<PathBuf>) -> Self {
        let stable = stable.into();
        let relink_cache = Relinker::default_cache(&stable.join("Songs"));
        Self {
            stable,
            lazer: lazer.into(),
            relink_threads: None,
            relink_cache,
        }
    }

    /// The engine for the installs `config` names.
    pub fn from_config(config: &Config) -> Result<Self> {
        let stable = config.stable_path.clone().ok_or(Error::MissingPath {
            path_type: "osu!stable",
        })?;
        let lazer = config.lazer_path.clone().ok_or(Error::MissingPath {
            path_type: "osu!lazer",
        })?;
        Ok(Self::new(stable, lazer))
    }

    /// Threads for the relink part of the step; `None` uses relink's default cap.
    pub fn threads(mut self, threads: Option<NonZeroUsize>) -> Self {
        self.relink_threads = threads;
        self
    }

    /// Where relink keeps its hash cache; `None` hashes every candidate on every step.
    pub fn relink_cache(mut self, cache: Option<PathBuf>) -> Self {
        self.relink_cache = cache;
        self
    }

    pub fn songs(&self) -> PathBuf {
        self.stable.join("Songs")
    }

    pub fn files(&self) -> PathBuf {
        self.lazer.join("files")
    }

    pub fn realm(&self) -> PathBuf {
        self.lazer.join("client.realm")
    }

    /// Checks made before any write. Fails when Songs or lazer's `files` is a junction
    /// or symbolic link, which only an older unified storage mode makes, or when the
    /// live guard refuses either folder.
    pub(crate) fn preflight(&self) -> Result<()> {
        for (what, path) in [
            ("stable Songs folder", self.songs()),
            ("lazer files folder", self.files()),
        ] {
            refuse_link(what, &path)?;
        }
        live_guard::check_write(&self.songs())?;
        live_guard::check_write(&self.files())?;
        Ok(())
    }

    /// [`Self::preflight`], then a check that osu!stable is closed.
    fn ready(&self) -> Result<()> {
        self.preflight()?;
        ensure_stable_closed()
    }

    /// The step: reads lazer's library, then materializes and relinks. Waits for
    /// nothing; fails with [`Error::GameRunning`] while osu!stable runs.
    pub fn sync(&self, progress: &mut dyn FnMut(StepPhase, usize, usize)) -> Result<StepReport> {
        self.ready()?;
        progress(StepPhase::Reading, 0, 0);
        // Stamped before the export reads the realm, so a commit during the export
        // changes the stamp the watcher compares against and gets its own step.
        let realm_stamp = FileStamp::of(&self.realm());
        let db = LazerDatabase::open(&self.lazer)?;
        let mut report = self.step(db.sets(), progress)?;
        report.notes.extend(db.skipped().map(ToString::to_string));
        report.realm_stamp = realm_stamp;
        Ok(report)
    }

    /// The step for `sets` given directly instead of read from lazer's realm.
    pub fn sync_sets(
        &self,
        sets: &[LazerBeatmapSet],
        progress: &mut dyn FnMut(StepPhase, usize, usize),
    ) -> Result<StepReport> {
        self.ready()?;
        self.step(sets, progress)
    }

    fn step(
        &self,
        sets: &[LazerBeatmapSet],
        progress: &mut dyn FnMut(StepPhase, usize, usize),
    ) -> Result<StepReport> {
        let mut last: Option<(StepPhase, Instant)> = None;
        let mut progress = |phase: StepPhase, done: usize, total: usize| {
            let due = last.is_none_or(|(last_phase, at)| {
                last_phase != phase || done == total || at.elapsed() >= PROGRESS_INTERVAL
            });
            if due {
                last = Some((phase, Instant::now()));
                progress(phase, done, total);
            }
        };
        let (songs, files) = (self.songs(), self.files());

        let (mut claims, db_note) = StableClaims::from_install(&self.stable)?;
        let refs: Vec<&LazerBeatmapSet> = sets.iter().collect();
        let materialized =
            Materializer::new(&songs, &files).run(&refs, &mut claims, &mut |done, total, _| {
                progress(StepPhase::Materializing, done, total);
                ControlFlow::Continue(())
            })?;
        let tally = materialized.tally();
        let totals = materialized.totals();
        let mut report = StepReport {
            lazer_sets: sets.len(),
            sets_written: tally.written,
            sets_complete: tally.complete,
            sets_skipped: tally.skipped,
            sets_failed: tally.failed,
            files_linked: totals.linked,
            files_copied: totals.copied,
            ..StepReport::default()
        };
        let line = |(folder, reason): (String, String)| format!("{folder}: {reason}");
        report.notes.extend(db_note);
        report.notes.extend(tally.skips.into_iter().map(line));
        report.errors.extend(tally.errors.into_iter().map(line));
        report.notes.extend(materialized.notes(&songs));

        let relinked = Relinker::new(&songs, &files, self.relink_cache.clone())
            .threads(self.relink_threads)
            .run(&mut |done, total| progress(StepPhase::Relinking, done, total))?;
        report.relinked = relinked.relinked;
        report.bytes_reclaimed = relinked.bytes_reclaimed;
        report.notes.extend(relinked.notes);
        report.errors.extend(relinked.errors);
        Ok(report)
    }

    /// Counts linked and copied files in Songs. Reads only, so it runs while a game is open.
    pub fn status(&self) -> Result<LinkedStoreStatus> {
        let songs = self.songs();
        refuse_link("stable Songs folder", &songs)?;
        songs_status(&songs)
    }
}

/// Counts the files under `songs` by link count, without following links.
fn songs_status(songs: &Path) -> Result<LinkedStoreStatus> {
    let mut status = LinkedStoreStatus::default();
    if !songs.is_dir() {
        return Ok(status);
    }
    for entry in walkdir::WalkDir::new(songs).follow_links(false) {
        let entry = entry.map_err(|e| Error::Other(e.to_string()))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let len = entry.metadata().map(|m| m.len()).unwrap_or(0);
        match link_count_at(entry.path()) {
            Ok(n) if n >= 2 => {
                status.linked_files += 1;
                status.bytes_saved += len;
            }
            Ok(_) => status.copied_files += 1,
            Err(_) => status.unreadable_files += 1,
        }
    }
    Ok(status)
}

/// True when `path` is a junction or symbolic link rather than a real folder.
fn is_link(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())
}

fn refuse_link(what: &str, path: &Path) -> Result<()> {
    if is_link(path) {
        return Err(Error::UnifiedStorage(format!(
            "the {what} {} is a junction or symbolic link, which an older unified storage mode \
             makes. The linked store needs the real folder there. Restore it, then run setup again.",
            path.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn status_counts_links_copies_and_saved_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let blob = dir.path().join("blob");
        fs::write(&blob, b"0123456789").unwrap();
        let set = dir.path().join("Songs").join("1 A - B");
        fs::create_dir_all(&set).unwrap();
        fs::hard_link(&blob, set.join("audio.mp3")).unwrap();
        fs::write(set.join("A - B (m) [x].osu"), b"osu").unwrap();

        let status = songs_status(&dir.path().join("Songs")).unwrap();
        assert_eq!(
            status,
            LinkedStoreStatus {
                linked_files: 1,
                copied_files: 1,
                bytes_saved: 10,
                unreadable_files: 0,
            }
        );
    }

    #[test]
    fn a_missing_songs_folder_has_an_empty_status() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            songs_status(&dir.path().join("Songs")).unwrap(),
            LinkedStoreStatus::default()
        );
    }

    #[test]
    fn a_report_with_no_new_files_changed_nothing() {
        let report = StepReport {
            sets_complete: 3,
            ..StepReport::default()
        };
        assert_eq!(report.changed_files(), 0);
        let report = StepReport {
            files_linked: 2,
            files_copied: 1,
            relinked: 4,
            ..StepReport::default()
        };
        assert_eq!(report.changed_files(), 7);
    }
}
