//! Runs the linked-store step when stable's Songs, lazer's `files` store or lazer's
//! realm changes.
//!
//! The watcher sleeps on the OS change feed for Songs and `files`. A change starts a
//! short quiet period; the step runs once changes stop for `quiet` (a second at most),
//! or at most `interval` after the first change. A step refused because osu!stable
//! runs is retried an interval later, and the watcher logs why.
//!
//! Lazer writes a new set's blobs as ordinary files, which the feed reports, then
//! commits the set to `client.realm` through a mapped view, which changes neither the
//! realm's modified time nor its size and raises no change event. Every commit rewrites
//! the realm's header, so while idle the watcher reads the realm stamp (size, modified
//! time and 24 header bytes) once per interval, and a stamp that differs from the one
//! the last step took right before reading the realm runs the step. Songs and `files`
//! changes arriving during a step or within a grace period after it may be the step's
//! own writes, so instead of a step each they run one confirming step after the grace.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use notify::{Event, RecursiveMode, Watcher};

use super::engine::{RealmStamp, StepReport, UnifiedStorageEngine};
use crate::error::{Error, Result};
use crate::linkstore::is_temp_name;

/// Timing for the watcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatchTiming {
    /// Longest wait from the first change to the step, and the retry delay while
    /// osu!stable runs.
    pub interval: Duration,
    /// The step runs once no change arrived for this long.
    pub quiet: Duration,
    /// Changes in Songs this soon after a step are the step's own.
    pub grace: Duration,
}

impl WatchTiming {
    pub fn from_interval_secs(secs: u64) -> Self {
        let interval = Duration::from_secs(secs.max(1));
        Self {
            interval,
            quiet: interval.min(Duration::from_secs(1)),
            grace: Duration::from_secs(2),
        }
    }
}

/// What the watcher did, for the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchEvent {
    /// A change arrived and a step is scheduled.
    Changed(PathBuf),
    /// The step waits because osu!stable is running.
    Deferred,
    /// Songs or `files` changed during the last step or just after it, so the step
    /// runs once more after the grace period.
    Recheck,
    Ran(StepReport),
    Failed(String),
}

impl fmt::Display for WatchEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Changed(path) => write!(f, "change in {}, sync scheduled", path.display()),
            Self::Deferred => f.write_str(
                "sync deferred: osu!stable is running and rewrites Songs and osu!.db while open; \
                 retrying after it closes",
            ),
            Self::Recheck => f.write_str(
                "Songs or lazer's files changed during the last sync; syncing once more to confirm",
            ),
            Self::Ran(report) => {
                f.write_str("sync done: ")?;
                for (label, value) in report.rows() {
                    write!(f, "{} {value}, ", label.to_lowercase())?;
                }
                write!(f, "errors {}", report.errors.len())
            }
            Self::Failed(e) => write!(f, "sync failed: {e}"),
        }
    }
}

/// Decides which changed paths should run the step.
#[derive(Debug, Clone)]
struct TriggerFilter {
    songs: PathBuf,
    files: PathBuf,
    realm: PathBuf,
}

/// Where a change happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChangeSource {
    /// Stable's Songs folder, which the step writes too.
    Songs,
    /// Lazer's `files` store, where lazer writes a new set's blobs.
    Library,
    /// Lazer's `client.realm`, which the step only reads.
    Realm,
}

impl TriggerFilter {
    fn new(songs: &Path, files: &Path, realm: &Path) -> Self {
        let abs = |p: &Path| std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
        Self {
            songs: abs(songs),
            files: abs(files),
            realm: abs(realm),
        }
    }

    /// The folders to watch: Songs and `files` recursively, and the folder holding
    /// `client.realm` on its own.
    fn watch_targets(&self) -> Vec<(PathBuf, RecursiveMode)> {
        let mut targets = vec![
            (self.songs.clone(), RecursiveMode::Recursive),
            (self.files.clone(), RecursiveMode::Recursive),
        ];
        if let Some(dir) = self.realm.parent() {
            targets.push((dir.to_path_buf(), RecursiveMode::NonRecursive));
        }
        targets
    }

    /// Where a change to `path` came from, or `None` when it should not run the step:
    /// osu-sync's temp files, `.tmp` files, realm lock and note files, and anything
    /// else outside Songs and `files`.
    fn classify(&self, path: &Path) -> Option<ChangeSource> {
        if path == self.realm {
            return Some(ChangeSource::Realm);
        }
        let source = if path.starts_with(&self.songs) {
            ChangeSource::Songs
        } else if path.starts_with(&self.files) {
            ChangeSource::Library
        } else {
            return None;
        };
        let name = path.file_name()?.to_string_lossy();
        let temp = is_temp_name(&name)
            || Path::new(name.as_ref())
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("tmp"));
        (!temp).then_some(source)
    }

    /// The path of a Songs or `files` change in `message`. A feed error or a rescan
    /// request may hide one, so it counts as a change to Songs.
    fn content_change(&self, message: &notify::Result<Event>) -> Option<PathBuf> {
        let event = match message {
            Ok(event) => event,
            Err(_) => return Some(self.songs.clone()),
        };
        if event.need_rescan() {
            return Some(self.songs.clone());
        }
        event
            .paths
            .iter()
            .find(|path| {
                matches!(
                    self.classify(path),
                    Some(ChangeSource::Songs | ChangeSource::Library)
                )
            })
            .cloned()
    }

    fn touches_realm(&self, message: &notify::Result<Event>) -> bool {
        message
            .as_ref()
            .is_ok_and(|event| event.paths.iter().any(|path| *path == self.realm))
    }
}

/// Shared between the change handler and the loop: whether a step runs, and whether
/// the handler dropped a Songs or `files` change while it ran.
#[derive(Debug, Default)]
struct StepGate {
    stepping: AtomicBool,
    missed: AtomicBool,
}

/// Checks the engine's folders once, then watches them and runs its step on changes
/// until the change feed fails. Returns only on error; the CLI stops it with Ctrl+C.
pub fn watch(
    engine: &UnifiedStorageEngine,
    timing: WatchTiming,
    log: &mut dyn FnMut(WatchEvent),
) -> Result<()> {
    engine.preflight()?;
    let filter = TriggerFilter::new(&engine.songs(), &engine.files(), &engine.realm());
    let (tx, rx) = std::sync::mpsc::channel();
    let gate = Arc::new(StepGate::default());
    let mut watcher = notify::recommended_watcher(feed(tx, gate.clone(), filter.clone()))
        .map_err(|e| Error::WatcherError(format!("could not start the watcher: {e}")))?;
    for (path, mode) in filter.watch_targets() {
        watcher
            .watch(&path, mode)
            .map_err(|e| Error::WatcherError(format!("could not watch {}: {e}", path.display())))?;
    }
    let realm = engine.realm();
    watch_loop(
        &rx,
        &filter,
        timing,
        &gate,
        &mut || engine.sync(&mut |_, _, _| {}),
        &|| RealmStamp::read(&realm),
        log,
    );
    Err(Error::WatcherError("the change feed closed".to_string()))
}

/// The change handler: passes events on to `tx`. While a step runs it drops them,
/// noting in the gate when a dropped one was a Songs or `files` change.
fn feed(
    tx: Sender<notify::Result<Event>>,
    gate: Arc<StepGate>,
    filter: TriggerFilter,
) -> impl FnMut(notify::Result<Event>) + Send + 'static {
    move |event| {
        if !gate.stepping.load(Ordering::Relaxed) {
            let _ = tx.send(event);
        } else if filter.content_change(&event).is_some() {
            gate.missed.store(true, Ordering::Relaxed);
        }
    }
}

/// The watcher's loop over a change feed, with the step and the realm stamp passed
/// in. Marks the gate stepping while the step runs. Runs one step at start, to catch
/// up, reads the realm stamp once per interval while idle, and returns when `rx` closes.
fn watch_loop(
    rx: &Receiver<notify::Result<Event>>,
    filter: &TriggerFilter,
    timing: WatchTiming,
    gate: &StepGate,
    step: &mut dyn FnMut() -> Result<StepReport>,
    realm_stamp: &dyn Fn() -> Option<RealmStamp>,
    log: &mut dyn FnMut(WatchEvent),
) {
    let mut pending = Some(Pending::by(Instant::now()));
    let mut grace_until: Option<Instant> = None;
    let mut last_realm: Option<RealmStamp> = None;

    loop {
        let message = match &pending {
            None => match rx.recv_timeout(timing.interval) {
                Ok(message) => Some(message),
                Err(RecvTimeoutError::Timeout) => {
                    if realm_committed(realm_stamp, last_realm) {
                        log(WatchEvent::Changed(filter.realm.clone()));
                        pending = Some(Pending::by(Instant::now()));
                    }
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => return,
            },
            Some(p) => {
                let wait = p.due.saturating_duration_since(Instant::now());
                if wait.is_zero() {
                    None
                } else {
                    match rx.recv_timeout(wait) {
                        Ok(message) => Some(message),
                        Err(RecvTimeoutError::Timeout) => None,
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
            }
        };

        if let Some(message) = message {
            let now = Instant::now();
            let grace = grace_until.filter(|until| now < *until);
            let changed = match (filter.content_change(&message), grace) {
                (Some(_), Some(until)) => {
                    pending.get_or_insert_with(|| {
                        log(WatchEvent::Recheck);
                        Pending::by(until)
                    });
                    None
                }
                (Some(path), None) => Some(path),
                (None, _) => (filter.touches_realm(&message)
                    && realm_committed(realm_stamp, last_realm))
                .then(|| filter.realm.clone()),
            };
            if let Some(path) = changed {
                let p = pending.get_or_insert_with(|| {
                    log(WatchEvent::Changed(path));
                    Pending::by(now + timing.interval)
                });
                p.due = (now + timing.quiet).min(p.deadline);
            }
            continue;
        }

        gate.stepping.store(true, Ordering::Relaxed);
        let result = step();
        gate.stepping.store(false, Ordering::Relaxed);
        let until = Instant::now() + timing.grace;
        grace_until = Some(until);
        pending = None;
        let retry = Pending::by(Instant::now() + timing.interval);
        match result {
            Ok(report) => {
                last_realm = report.realm_stamp.or(last_realm);
                log(WatchEvent::Ran(report));
                if realm_committed(realm_stamp, last_realm) {
                    pending = Some(retry);
                }
            }
            Err(Error::GameRunning { .. }) => {
                log(WatchEvent::Deferred);
                pending = Some(retry);
            }
            Err(e) => {
                last_realm = realm_stamp().or(last_realm);
                log(WatchEvent::Failed(e.to_string()));
            }
        }
        if gate.missed.swap(false, Ordering::Relaxed) {
            log(WatchEvent::Recheck);
            let at = pending.map_or(until, |p| p.deadline.min(until));
            pending = Some(Pending::by(at));
        }
    }
}

/// True when the realm can be read and its stamp differs from `last`. An unreadable
/// realm never counts as a commit.
fn realm_committed(
    realm_stamp: &dyn Fn() -> Option<RealmStamp>,
    last: Option<RealmStamp>,
) -> bool {
    realm_stamp().is_some_and(|now| Some(now) != last)
}

/// A scheduled step: it runs at `due`, which a change moves to `quiet` after it but
/// never past `deadline`.
struct Pending {
    due: Instant,
    deadline: Instant,
}

impl Pending {
    fn by(deadline: Instant) -> Self {
        Self {
            due: deadline,
            deadline,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{CreateKind, EventKind};
    use std::cell::{Cell, RefCell};
    use std::sync::mpsc::channel;
    use std::time::SystemTime;

    fn filter() -> TriggerFilter {
        TriggerFilter::new(
            Path::new(r"C:\s\Songs"),
            Path::new(r"C:\l\files"),
            Path::new(r"C:\l\client.realm"),
        )
    }

    fn fast() -> WatchTiming {
        WatchTiming {
            interval: Duration::from_millis(400),
            quiet: Duration::from_millis(50),
            grace: Duration::from_millis(100),
        }
    }

    fn event(path: &str) -> notify::Result<Event> {
        Ok(Event::new(EventKind::Create(CreateKind::File)).add_path(PathBuf::from(path)))
    }

    fn send(tx: &Sender<notify::Result<Event>>, path: &str) {
        tx.send(event(path)).unwrap();
    }

    /// A realm stamp whose header holds `commit`; size and modified time never change,
    /// as with a commit through a mapped view.
    fn stamp(commit: u64) -> Option<RealmStamp> {
        let mut header = [0u8; super::super::engine::REALM_HEADER_LEN];
        header[..8].copy_from_slice(&commit.to_le_bytes());
        Some(RealmStamp {
            modified: SystemTime::UNIX_EPOCH,
            len: 10,
            header,
        })
    }

    /// Runs the loop on `rx` with a gate of its own and no realm.
    fn run(
        rx: &Receiver<notify::Result<Event>>,
        step: &mut dyn FnMut() -> Result<StepReport>,
        log: &mut dyn FnMut(WatchEvent),
    ) {
        watch_loop(
            rx,
            &filter(),
            fast(),
            &StepGate::default(),
            step,
            &|| None,
            log,
        );
    }

    #[test]
    fn songs_blob_and_realm_changes_trigger_and_temp_files_do_not() {
        let f = filter();
        let classify = |p: &str| f.classify(Path::new(p));
        assert_eq!(
            classify(r"C:\s\Songs\1 A - B\audio.mp3"),
            Some(ChangeSource::Songs)
        );
        assert_eq!(classify(r"C:\l\client.realm"), Some(ChangeSource::Realm));
        assert_eq!(
            classify(r"C:\l\files\a\ab\abcdef"),
            Some(ChangeSource::Library)
        );
        assert_eq!(classify(r"C:\l\files\a\ab\abcdef.tmp"), None);
        assert_eq!(classify(r"C:\l\exports\x.osz"), None);
        assert_eq!(classify(r"C:\l\client.realm.lock"), None);
        assert_eq!(classify(r"C:\l\client.realm.note"), None);
        assert_eq!(
            classify(r"C:\s\Songs\1 A - B\osu-sync_tmp_audio.mp3.part"),
            None
        );
        assert_eq!(classify(r"C:\s\Songs\1 A - B\x.tmp"), None);
        assert_eq!(classify(r"C:\s\osu!.db"), None);
    }

    #[test]
    fn the_watcher_follows_songs_files_and_the_realm_folder() {
        let targets = filter().watch_targets();
        assert_eq!(
            targets,
            [
                (PathBuf::from(r"C:\s\Songs"), RecursiveMode::Recursive),
                (PathBuf::from(r"C:\l\files"), RecursiveMode::Recursive),
                (PathBuf::from(r"C:\l"), RecursiveMode::NonRecursive),
            ]
        );
    }

    #[test]
    fn the_interval_is_at_least_a_second_and_quiet_is_at_most_one() {
        let t = WatchTiming::from_interval_secs(5);
        assert_eq!(t.interval, Duration::from_secs(5));
        assert_eq!(t.quiet, Duration::from_secs(1));
        assert_eq!(
            WatchTiming::from_interval_secs(0).interval,
            Duration::from_secs(1)
        );
    }

    #[test]
    fn a_burst_of_changes_runs_one_step_after_the_catch_up_step() {
        let (tx, rx) = channel();
        let steps = Cell::new(0);
        let mut log = Vec::new();
        let feeder = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            for i in 0..5 {
                send(&tx, &format!(r"C:\s\Songs\{i} A - B\audio.mp3"));
            }
            std::thread::sleep(Duration::from_millis(600));
        });
        run(
            &rx,
            &mut || {
                steps.set(steps.get() + 1);
                Ok(StepReport::default())
            },
            &mut |e| log.push(e),
        );
        feeder.join().unwrap();
        assert_eq!(steps.get(), 2);
        assert_eq!(
            log.iter()
                .filter(|e| matches!(e, WatchEvent::Changed(_)))
                .count(),
            1
        );
    }

    #[test]
    fn a_blob_write_under_files_runs_a_step() {
        let (tx, rx) = channel();
        let steps = Cell::new(0);
        let mut log = Vec::new();
        let feeder = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            send(&tx, r"C:\l\files\0\00\new-blob");
            std::thread::sleep(Duration::from_millis(600));
        });
        run(
            &rx,
            &mut || {
                steps.set(steps.get() + 1);
                Ok(StepReport::default())
            },
            &mut |e| log.push(e),
        );
        feeder.join().unwrap();
        assert_eq!(steps.get(), 2);
        assert_eq!(
            log[1],
            WatchEvent::Changed(PathBuf::from(r"C:\l\files\0\00\new-blob"))
        );
    }

    #[test]
    fn a_header_only_realm_commit_with_no_change_event_runs_a_step() {
        let (tx, rx) = channel::<notify::Result<Event>>();
        let realm = Arc::new(std::sync::atomic::AtomicU64::new(1));
        let commit = realm.clone();
        let feeder = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            commit.store(2, Ordering::Relaxed);
            std::thread::sleep(Duration::from_millis(1200));
            drop(tx);
        });
        let seen = RefCell::new(Vec::new());
        watch_loop(
            &rx,
            &filter(),
            fast(),
            &StepGate::default(),
            &mut || {
                seen.borrow_mut().push(realm.load(Ordering::Relaxed));
                Ok(StepReport {
                    realm_stamp: stamp(realm.load(Ordering::Relaxed)),
                    ..StepReport::default()
                })
            },
            &|| stamp(realm.load(Ordering::Relaxed)),
            &mut |_| {},
        );
        feeder.join().unwrap();
        assert_eq!(seen.into_inner(), [1, 2]);
    }

    const RECHECK: &str =
        "Songs or lazer's files changed during the last sync; syncing once more to confirm";

    fn names(log: &[WatchEvent]) -> Vec<&'static str> {
        log.iter()
            .map(|e| match e {
                WatchEvent::Changed(_) => "changed",
                WatchEvent::Deferred => "deferred",
                WatchEvent::Recheck => "recheck",
                WatchEvent::Ran(_) => "ran",
                WatchEvent::Failed(_) => "failed",
            })
            .collect()
    }

    #[test]
    fn changes_just_after_a_step_run_one_confirming_step_after_the_grace() {
        let (tx, rx) = channel();
        let sender = RefCell::new(Some(tx));
        let steps = Cell::new(0);
        let mut log = Vec::new();
        run(
            &rx,
            &mut || {
                steps.set(steps.get() + 1);
                if let Some(tx) = sender.borrow_mut().take() {
                    send(&tx, r"C:\s\Songs\1 A - B\audio.mp3");
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_millis(20));
                        send(&tx, r"C:\l\files\0\00\blob");
                        std::thread::sleep(Duration::from_millis(300));
                    });
                }
                Ok(StepReport::default())
            },
            &mut |e| log.push(e),
        );
        assert_eq!(steps.get(), 2);
        assert_eq!(names(&log), ["ran", "recheck", "ran"]);
        assert_eq!(log[1].to_string(), RECHECK);
    }

    #[test]
    fn a_change_dropped_during_a_step_runs_one_confirming_step() {
        let (tx, rx) = channel();
        let gate = Arc::new(StepGate::default());
        let mut handler = Some(feed(tx.clone(), gate.clone(), filter()));
        let closer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(600));
            drop(tx);
        });
        let steps = Cell::new(0);
        let mut log = Vec::new();
        watch_loop(
            &rx,
            &filter(),
            fast(),
            &gate,
            &mut || {
                steps.set(steps.get() + 1);
                if let Some(mut handler) = handler.take() {
                    handler(event(r"C:\s\Songs\1 A - B\audio.mp3"));
                }
                Ok(StepReport::default())
            },
            &|| None,
            &mut |e| log.push(e),
        );
        closer.join().unwrap();
        assert_eq!(steps.get(), 2);
        assert_eq!(names(&log), ["ran", "recheck", "ran"]);
        assert!(!gate.missed.load(Ordering::Relaxed));
    }

    #[test]
    fn a_step_with_only_temp_and_realm_lock_changes_runs_once() {
        let (tx, rx) = channel();
        let gate = Arc::new(StepGate::default());
        let mut handler = Some(feed(tx.clone(), gate.clone(), filter()));
        let late = tx.clone();
        let closer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            send(&late, r"C:\s\Songs\1 A - B\osu-sync_tmp_bg.jpg.part");
            send(&late, r"C:\l\client.realm.lock");
            std::thread::sleep(Duration::from_millis(600));
        });
        drop(tx);
        let steps = Cell::new(0);
        let mut log = Vec::new();
        watch_loop(
            &rx,
            &filter(),
            fast(),
            &gate,
            &mut || {
                steps.set(steps.get() + 1);
                if let Some(mut handler) = handler.take() {
                    handler(event(r"C:\s\Songs\1 A - B\osu-sync_tmp_audio.mp3.part"));
                    handler(event(r"C:\l\client.realm.lock"));
                }
                Ok(StepReport::default())
            },
            &|| None,
            &mut |e| log.push(e),
        );
        closer.join().unwrap();
        assert_eq!(steps.get(), 1);
        assert_eq!(names(&log), ["ran"]);
    }

    #[test]
    fn the_feed_drops_events_while_a_step_runs_and_notes_library_changes() {
        let (tx, rx) = channel();
        let gate = Arc::new(StepGate::default());
        gate.stepping.store(true, Ordering::Relaxed);
        let mut handler = feed(tx, gate.clone(), filter());
        handler(event(r"C:\l\client.realm.lock"));
        assert_eq!(rx.try_recv().ok().map(|e| e.unwrap().paths), None);
        assert!(!gate.missed.load(Ordering::Relaxed));
        handler(event(r"C:\l\files\0\00\blob"));
        assert_eq!(rx.try_recv().ok().map(|e| e.unwrap().paths), None);
        assert!(gate.missed.load(Ordering::Relaxed));
        gate.stepping.store(false, Ordering::Relaxed);
        handler(event(r"C:\s\Songs\1 A - B\audio.mp3"));
        assert_eq!(
            rx.try_recv().ok().map(|e| e.unwrap().paths),
            Some(vec![PathBuf::from(r"C:\s\Songs\1 A - B\audio.mp3")])
        );
    }

    #[test]
    fn the_loop_marks_itself_stepping_only_while_the_step_runs() {
        let (tx, rx) = channel::<notify::Result<Event>>();
        drop(tx);
        let gate = StepGate::default();
        let during = Cell::new(false);
        watch_loop(
            &rx,
            &filter(),
            fast(),
            &gate,
            &mut || {
                during.set(gate.stepping.load(Ordering::Relaxed));
                Ok(StepReport::default())
            },
            &|| None,
            &mut |_| {},
        );
        assert!(during.get());
        assert!(!gate.stepping.load(Ordering::Relaxed));
    }

    #[test]
    fn a_step_refused_while_stable_runs_is_retried_and_logged() {
        let (tx, rx) = channel::<notify::Result<Event>>();
        let steps = Cell::new(0);
        let log = RefCell::new(Vec::new());
        let closer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(1500));
            drop(tx);
        });
        run(
            &rx,
            &mut || {
                steps.set(steps.get() + 1);
                if steps.get() <= 2 {
                    return Err(Error::GameRunning {
                        game: "osu!stable".to_string(),
                    });
                }
                Ok(StepReport::default())
            },
            &mut |e| log.borrow_mut().push(e.to_string()),
        );
        closer.join().unwrap();
        assert_eq!(steps.get(), 3);
        let log = log.into_inner();
        let deferred = "sync deferred: osu!stable is running and rewrites Songs and osu!.db \
                        while open; retrying after it closes";
        assert_eq!(log[..2], [deferred, deferred]);
        assert_eq!(
            log[2],
            "sync done: lazer sets 0, sets written 0, sets complete 0, sets skipped 0, \
             sets failed 0, files linked 0, files copied 0, relinked 0, bytes reclaimed 0 bytes, \
             errors 0"
        );
    }

    #[test]
    fn a_realm_commit_during_a_step_runs_another_step() {
        let (tx, rx) = channel::<notify::Result<Event>>();
        let steps = Cell::new(0);
        let realm = Cell::new(1);
        let closer = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(1200));
            drop(tx);
        });
        watch_loop(
            &rx,
            &filter(),
            fast(),
            &StepGate::default(),
            &mut || {
                steps.set(steps.get() + 1);
                let read = stamp(realm.get());
                if steps.get() == 1 {
                    realm.set(2);
                }
                Ok(StepReport {
                    realm_stamp: read,
                    ..StepReport::default()
                })
            },
            &|| stamp(realm.get()),
            &mut |_| {},
        );
        closer.join().unwrap();
        assert_eq!(steps.get(), 2);
    }

    #[test]
    fn a_realm_event_without_a_new_commit_is_ignored() {
        let (tx, rx) = channel();
        let steps = Cell::new(0);
        let feeder = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            send(&tx, r"C:\l\client.realm");
            std::thread::sleep(Duration::from_millis(500));
        });
        watch_loop(
            &rx,
            &filter(),
            fast(),
            &StepGate::default(),
            &mut || {
                steps.set(steps.get() + 1);
                Ok(StepReport {
                    realm_stamp: stamp(7),
                    ..StepReport::default()
                })
            },
            &|| stamp(7),
            &mut |_| {},
        );
        feeder.join().unwrap();
        assert_eq!(steps.get(), 1);
    }
}
