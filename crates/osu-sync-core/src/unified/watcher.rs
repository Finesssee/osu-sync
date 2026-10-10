//! Runs the linked-store step when stable's Songs or lazer's realm changes.
//!
//! The watcher sleeps on the OS change feed and wakes only for a change, so an idle
//! library costs no CPU. A change starts a short quiet period; the step runs once
//! changes stop for `quiet` (a second at most), or at most `interval` after the first
//! change. A step refused because osu!stable runs is retried an interval later, and
//! the watcher logs why.
//!
//! Lazer commits a new set to `client.realm` after writing its blobs, so the watcher
//! follows the realm and not lazer's `files` store. The step never writes the realm,
//! so a realm change counts whenever its modified time or size differs from what
//! the last step stamped right before reading it. Changes arriving during a step are
//! dropped, and Songs changes within a grace period after it are the step's own; a
//! realm commit made meanwhile still differs from the stamp and gets its own step.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use notify::{Event, RecursiveMode, Watcher};

use super::engine::{FileStamp, StepReport, UnifiedStorageEngine};
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
    realm: PathBuf,
}

/// Where a change happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChangeSource {
    /// Stable's Songs folder, which the step writes too.
    Songs,
    /// Lazer's `client.realm`, which the step only reads.
    Realm,
}

impl TriggerFilter {
    fn new(songs: &Path, realm: &Path) -> Self {
        let abs = |p: &Path| std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
        Self {
            songs: abs(songs),
            realm: abs(realm),
        }
    }

    /// The folders to watch: Songs recursively, and the folder holding `client.realm`
    /// on its own.
    fn watch_targets(&self) -> Vec<(PathBuf, RecursiveMode)> {
        let mut targets = vec![(self.songs.clone(), RecursiveMode::Recursive)];
        if let Some(dir) = self.realm.parent() {
            targets.push((dir.to_path_buf(), RecursiveMode::NonRecursive));
        }
        targets
    }

    /// Where a change to `path` came from, or `None` when it should not run the step:
    /// osu-sync's temp files, `.tmp` files, realm lock and note files, and anything
    /// else outside Songs, lazer's blobs included.
    fn classify(&self, path: &Path) -> Option<ChangeSource> {
        if path == self.realm {
            return Some(ChangeSource::Realm);
        }
        if !path.starts_with(&self.songs) {
            return None;
        }
        let name = path.file_name()?.to_string_lossy();
        let temp = is_temp_name(&name)
            || Path::new(name.as_ref())
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("tmp"));
        (!temp).then_some(ChangeSource::Songs)
    }
}

/// Checks the engine's folders once, then watches them and runs its step on changes
/// until the change feed fails. Returns only on error; the CLI stops it with Ctrl+C.
pub fn watch(
    engine: &UnifiedStorageEngine,
    timing: WatchTiming,
    log: &mut dyn FnMut(WatchEvent),
) -> Result<()> {
    engine.preflight()?;
    let filter = TriggerFilter::new(&engine.songs(), &engine.realm());
    let (tx, rx) = std::sync::mpsc::channel();
    let stepping = Arc::new(AtomicBool::new(false));
    let mut watcher = notify::recommended_watcher(feed(tx, stepping.clone()))
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
        &stepping,
        &mut || engine.sync(&mut |_, _, _| {}),
        &|| FileStamp::of(&realm),
        log,
    );
    Err(Error::WatcherError("the change feed closed".to_string()))
}

/// The change handler: passes events on to `tx`, except while `stepping` is set.
fn feed(
    tx: Sender<notify::Result<Event>>,
    stepping: Arc<AtomicBool>,
) -> impl FnMut(notify::Result<Event>) + Send + 'static {
    move |event| {
        if !stepping.load(Ordering::Relaxed) {
            let _ = tx.send(event);
        }
    }
}

/// The watcher's loop over a change feed, with the step and the realm stamp passed
/// in. Sets `stepping` while the step runs. Runs one step at start, to catch up, and
/// returns when `rx` closes.
fn watch_loop(
    rx: &Receiver<notify::Result<Event>>,
    filter: &TriggerFilter,
    timing: WatchTiming,
    stepping: &AtomicBool,
    step: &mut dyn FnMut() -> Result<StepReport>,
    realm_stamp: &dyn Fn() -> Option<FileStamp>,
    log: &mut dyn FnMut(WatchEvent),
) {
    let mut pending = Some(Pending::by(Instant::now()));
    let mut grace_until: Option<Instant> = None;
    let mut last_realm: Option<FileStamp> = None;

    loop {
        let message = match &pending {
            None => match rx.recv() {
                Ok(message) => Some(message),
                Err(_) => return,
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
            let in_grace = grace_until.is_some_and(|until| Instant::now() < until);
            if let Some(path) = trigger(&message, filter, in_grace, last_realm, realm_stamp) {
                let now = Instant::now();
                let p = pending.get_or_insert_with(|| {
                    log(WatchEvent::Changed(path));
                    Pending::by(now + timing.interval)
                });
                p.due = (now + timing.quiet).min(p.deadline);
            }
            continue;
        }

        stepping.store(true, Ordering::Relaxed);
        let result = step();
        stepping.store(false, Ordering::Relaxed);
        grace_until = Some(Instant::now() + timing.grace);
        pending = None;
        let retry = Pending::by(Instant::now() + timing.interval);
        match result {
            Ok(report) => {
                let read = report.realm_stamp;
                log(WatchEvent::Ran(report));
                if read.is_some() {
                    last_realm = read;
                    if realm_stamp() != read {
                        pending = Some(retry);
                    }
                }
            }
            Err(Error::GameRunning { .. }) => {
                log(WatchEvent::Deferred);
                pending = Some(retry);
            }
            Err(e) => log(WatchEvent::Failed(e.to_string())),
        }
    }
}

/// The path that makes `message` run the step, if it should.
fn trigger(
    message: &notify::Result<Event>,
    filter: &TriggerFilter,
    in_grace: bool,
    last_realm: Option<FileStamp>,
    realm_stamp: &dyn Fn() -> Option<FileStamp>,
) -> Option<PathBuf> {
    let event = match message {
        Ok(event) => event,
        Err(_) => return Some(filter.songs.clone()),
    };
    if event.need_rescan() {
        return Some(filter.songs.clone());
    }
    event
        .paths
        .iter()
        .find(|path| match filter.classify(path) {
            Some(ChangeSource::Songs) => !in_grace,
            Some(ChangeSource::Realm) => last_realm.is_none() || realm_stamp() != last_realm,
            None => false,
        })
        .cloned()
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
        TriggerFilter::new(Path::new(r"C:\s\Songs"), Path::new(r"C:\l\client.realm"))
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

    fn stamp(secs: u64) -> Option<FileStamp> {
        Some(FileStamp {
            modified: SystemTime::UNIX_EPOCH + Duration::from_secs(secs),
            len: 10,
        })
    }

    /// Runs the loop on `rx` with no `stepping` flag to share and no realm.
    fn run(
        rx: &Receiver<notify::Result<Event>>,
        step: &mut dyn FnMut() -> Result<StepReport>,
        log: &mut dyn FnMut(WatchEvent),
    ) {
        watch_loop(
            rx,
            &filter(),
            fast(),
            &AtomicBool::new(false),
            step,
            &|| None,
            log,
        );
    }

    #[test]
    fn songs_and_realm_changes_trigger_and_blobs_and_temp_files_do_not() {
        let f = filter();
        let classify = |p: &str| f.classify(Path::new(p));
        assert_eq!(
            classify(r"C:\s\Songs\1 A - B\audio.mp3"),
            Some(ChangeSource::Songs)
        );
        assert_eq!(classify(r"C:\l\client.realm"), Some(ChangeSource::Realm));
        assert_eq!(classify(r"C:\l\files\a\ab\abcdef"), None);
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
    fn the_watcher_follows_songs_and_the_realm_folder_only() {
        let targets = filter().watch_targets();
        assert_eq!(
            targets,
            [
                (PathBuf::from(r"C:\s\Songs"), RecursiveMode::Recursive),
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
    fn a_blob_write_alone_runs_no_step_and_the_realm_commit_after_it_does() {
        let (tx, rx) = channel();
        let steps = Cell::new(0);
        let realm = Arc::new(std::sync::atomic::AtomicU64::new(1));
        let commit = realm.clone();
        let feeder = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(300));
            send(&tx, r"C:\l\files\0\00\new-blob");
            std::thread::sleep(Duration::from_millis(600));
            commit.store(2, Ordering::Relaxed);
            send(&tx, r"C:\l\client.realm");
            std::thread::sleep(Duration::from_millis(600));
        });
        let seen = RefCell::new(Vec::new());
        watch_loop(
            &rx,
            &filter(),
            fast(),
            &AtomicBool::new(false),
            &mut || {
                steps.set(steps.get() + 1);
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

    #[test]
    fn changes_during_and_just_after_a_step_are_its_own() {
        let (tx, rx) = channel();
        let sender = RefCell::new(Some(tx));
        let steps = Cell::new(0);
        run(
            &rx,
            &mut || {
                steps.set(steps.get() + 1);
                if let Some(tx) = sender.borrow_mut().take() {
                    send(&tx, r"C:\s\Songs\1 A - B\audio.mp3");
                    std::thread::spawn(move || {
                        std::thread::sleep(Duration::from_millis(20));
                        send(&tx, r"C:\s\Songs\1 A - B\bg.jpg");
                        std::thread::sleep(Duration::from_millis(300));
                    });
                }
                Ok(StepReport::default())
            },
            &mut |_| {},
        );
        assert_eq!(steps.get(), 1);
    }

    #[test]
    fn the_feed_drops_events_while_a_step_runs() {
        let (tx, rx) = channel();
        let stepping = Arc::new(AtomicBool::new(true));
        let mut handler = feed(tx, stepping.clone());
        handler(event(r"C:\s\Songs\1 A - B\audio.mp3"));
        assert!(rx.try_recv().is_err());
        stepping.store(false, Ordering::Relaxed);
        handler(event(r"C:\s\Songs\1 A - B\audio.mp3"));
        assert!(rx.try_recv().is_ok());
    }

    #[test]
    fn the_loop_marks_itself_stepping_only_while_the_step_runs() {
        let (tx, rx) = channel::<notify::Result<Event>>();
        drop(tx);
        let stepping = AtomicBool::new(false);
        let during = Cell::new(false);
        watch_loop(
            &rx,
            &filter(),
            fast(),
            &stepping,
            &mut || {
                during.set(stepping.load(Ordering::Relaxed));
                Ok(StepReport::default())
            },
            &|| None,
            &mut |_| {},
        );
        assert!(during.get());
        assert!(!stepping.load(Ordering::Relaxed));
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
            &AtomicBool::new(false),
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
            &AtomicBool::new(false),
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
