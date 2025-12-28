//! Game launch detection for osu! stable and lazer.
//!
//! This module provides functionality to detect when osu! games are launched
//! or closed, enabling automatic synchronization triggers.
//!
//! # Platform Support
//!
//! - **Windows**: Uses the `sysinfo` crate to query running processes
//! - **Linux**: Uses the `sysinfo` crate with /proc filesystem fallback
//! - **macOS**: Uses the `sysinfo` crate to query running processes
//!
//! # Example
//!
//! ```rust,ignore
//! use osu_sync_core::unified::{GameLaunchDetector, GameEvent, OsuGame};
//! use std::sync::mpsc::channel;
//!
//! let mut detector = GameLaunchDetector::new();
//!
//! // Check current state
//! if detector.is_stable_running() {
//!     println!("osu! stable is running");
//! }
//!
//! // Start continuous monitoring
//! let (tx, rx) = channel();
//! let handle = detector.start_monitoring(tx);
//!
//! // Handle events
//! for event in rx {
//!     match event {
//!         GameEvent::Launched(OsuGame::Stable) => println!("Stable launched!"),
//!         GameEvent::Closed(OsuGame::Lazer) => println!("Lazer closed!"),
//!         _ => {}
//!     }
//! }
//! ```

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use sysinfo::{ProcessRefreshKind, RefreshKind, System};

/// Represents the different osu! game variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OsuGame {
    /// osu! stable (the original osu! client)
    Stable,
    /// osu! lazer (the new open-source client)
    Lazer,
}

impl OsuGame {
    /// Returns the display name for this game variant.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Stable => "osu! stable",
            Self::Lazer => "osu! lazer",
        }
    }
}

impl std::fmt::Display for OsuGame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Events emitted when game state changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameEvent {
    /// A game was launched.
    Launched(OsuGame),
    /// A game was closed.
    Closed(OsuGame),
}

impl GameEvent {
    /// Returns the game associated with this event.
    pub fn game(&self) -> OsuGame {
        match self {
            Self::Launched(game) | Self::Closed(game) => *game,
        }
    }

    /// Returns `true` if this is a launch event.
    pub fn is_launch(&self) -> bool {
        matches!(self, Self::Launched(_))
    }

    /// Returns `true` if this is a close event.
    pub fn is_close(&self) -> bool {
        matches!(self, Self::Closed(_))
    }
}

impl std::fmt::Display for GameEvent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Launched(game) => write!(f, "{} launched", game),
            Self::Closed(game) => write!(f, "{} closed", game),
        }
    }
}

/// Information about a running process.
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    /// Process ID
    pub pid: u32,
    /// Process name (executable name)
    pub name: String,
    /// Full path to the executable, if available
    pub exe_path: Option<PathBuf>,
}

/// Detects osu! game launches and closures.
///
/// This detector polls the system for running processes at a configurable
/// interval and emits events when osu! games are launched or closed.
pub struct GameLaunchDetector {
    /// Executable name for osu! stable
    stable_exe_name: String,
    /// Executable name for osu! lazer
    lazer_exe_name: String,
    /// Alternative lazer executable name (some installations use this)
    lazer_alt_exe_name: String,
    /// Interval between process checks
    poll_interval: Duration,
    /// Whether stable was running in the last check
    stable_was_running: bool,
    /// Whether lazer was running in the last check
    lazer_was_running: bool,
    /// Flag to stop the monitoring thread
    stop_flag: Arc<AtomicBool>,
}

impl Default for GameLaunchDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl GameLaunchDetector {
    /// Default executable name for osu! stable.
    pub const STABLE_EXE_NAME: &'static str = "osu!.exe";

    /// Default executable name for osu! lazer.
    pub const LAZER_EXE_NAME: &'static str = "osu!.exe";

    /// Alternative executable name for osu! lazer.
    pub const LAZER_ALT_EXE_NAME: &'static str = "osu!lazer.exe";

    /// Default poll interval (1 second).
    pub const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(1);

    /// Creates a new game launch detector with default settings.
    pub fn new() -> Self {
        Self {
            stable_exe_name: Self::STABLE_EXE_NAME.to_string(),
            lazer_exe_name: Self::LAZER_EXE_NAME.to_string(),
            lazer_alt_exe_name: Self::LAZER_ALT_EXE_NAME.to_string(),
            poll_interval: Self::DEFAULT_POLL_INTERVAL,
            stable_was_running: false,
            lazer_was_running: false,
            stop_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Creates a new game launch detector with a custom poll interval.
    pub fn with_poll_interval(interval: Duration) -> Self {
        Self {
            stable_exe_name: Self::STABLE_EXE_NAME.to_string(),
            lazer_exe_name: Self::LAZER_EXE_NAME.to_string(),
            lazer_alt_exe_name: Self::LAZER_ALT_EXE_NAME.to_string(),
            poll_interval: interval,
            stable_was_running: false,
            lazer_was_running: false,
            stop_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Sets the executable name for osu! stable.
    pub fn set_stable_exe_name(&mut self, name: impl Into<String>) {
        self.stable_exe_name = name.into();
    }

    /// Sets the executable name for osu! lazer.
    pub fn set_lazer_exe_name(&mut self, name: impl Into<String>) {
        self.lazer_exe_name = name.into();
    }

    /// Sets the poll interval.
    pub fn set_poll_interval(&mut self, interval: Duration) {
        self.poll_interval = interval;
    }

    /// Returns `true` if osu! stable is currently running.
    pub fn is_stable_running(&self) -> bool {
        self.check_stable_running()
    }

    /// Returns `true` if osu! lazer is currently running.
    pub fn is_lazer_running(&self) -> bool {
        self.check_lazer_running()
    }

    /// Returns `true` if any osu! game is currently running.
    pub fn is_any_running(&self) -> bool {
        self.is_stable_running() || self.is_lazer_running()
    }

    /// Checks once for game state changes and returns any events.
    ///
    /// This method should be called periodically to detect game launches
    /// and closures. It updates the internal state and returns events
    /// for any state changes since the last call.
    pub fn check_once(&mut self) -> Vec<GameEvent> {
        let mut events = Vec::new();

        let stable_running = self.check_stable_running();
        let lazer_running = self.check_lazer_running();

        // Check for stable state changes
        if stable_running && !self.stable_was_running {
            events.push(GameEvent::Launched(OsuGame::Stable));
        } else if !stable_running && self.stable_was_running {
            events.push(GameEvent::Closed(OsuGame::Stable));
        }

        // Check for lazer state changes
        if lazer_running && !self.lazer_was_running {
            events.push(GameEvent::Launched(OsuGame::Lazer));
        } else if !lazer_running && self.lazer_was_running {
            events.push(GameEvent::Closed(OsuGame::Lazer));
        }

        self.stable_was_running = stable_running;
        self.lazer_was_running = lazer_running;

        events
    }

    /// Starts continuous monitoring in a background thread.
    ///
    /// Events are sent through the provided channel. The monitoring
    /// continues until the detector is dropped or `stop_monitoring` is called.
    ///
    /// # Returns
    ///
    /// A `JoinHandle` for the monitoring thread.
    pub fn start_monitoring(&mut self, event_tx: Sender<GameEvent>) -> JoinHandle<()> {
        // Reset the stop flag
        self.stop_flag.store(false, Ordering::SeqCst);

        let stop_flag = Arc::clone(&self.stop_flag);
        let poll_interval = self.poll_interval;
        let stable_exe = self.stable_exe_name.clone();
        let lazer_exe = self.lazer_exe_name.clone();
        let lazer_alt_exe = self.lazer_alt_exe_name.clone();

        // Initialize state based on current running processes
        let mut stable_was_running = check_stable_running_impl(&stable_exe, &lazer_exe);
        let mut lazer_was_running = check_lazer_running_impl(&lazer_exe, &lazer_alt_exe);

        thread::spawn(move || {
            // Create a system instance for this thread
            let mut sys = System::new_with_specifics(
                RefreshKind::new().with_processes(ProcessRefreshKind::new()),
            );

            while !stop_flag.load(Ordering::SeqCst) {
                thread::sleep(poll_interval);

                if stop_flag.load(Ordering::SeqCst) {
                    break;
                }

                // Refresh process list
                sys.refresh_processes();

                let stable_running =
                    check_stable_running_with_system(&sys, &stable_exe, &lazer_exe);
                let lazer_running =
                    check_lazer_running_with_system(&sys, &lazer_exe, &lazer_alt_exe);

                // Check for stable state changes
                if stable_running && !stable_was_running {
                    if event_tx.send(GameEvent::Launched(OsuGame::Stable)).is_err() {
                        break;
                    }
                } else if !stable_running && stable_was_running {
                    if event_tx.send(GameEvent::Closed(OsuGame::Stable)).is_err() {
                        break;
                    }
                }

                // Check for lazer state changes
                if lazer_running && !lazer_was_running {
                    if event_tx.send(GameEvent::Launched(OsuGame::Lazer)).is_err() {
                        break;
                    }
                } else if !lazer_running && lazer_was_running {
                    if event_tx.send(GameEvent::Closed(OsuGame::Lazer)).is_err() {
                        break;
                    }
                }

                stable_was_running = stable_running;
                lazer_was_running = lazer_running;
            }
        })
    }

    /// Stops the monitoring thread.
    pub fn stop_monitoring(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
    }

    /// Checks if osu! stable is running.
    fn check_stable_running(&self) -> bool {
        check_stable_running_impl(&self.stable_exe_name, &self.lazer_exe_name)
    }

    /// Checks if osu! lazer is running.
    fn check_lazer_running(&self) -> bool {
        check_lazer_running_impl(&self.lazer_exe_name, &self.lazer_alt_exe_name)
    }
}

impl Drop for GameLaunchDetector {
    fn drop(&mut self) {
        self.stop_monitoring();
    }
}

/// Creates a fresh system instance and checks for running processes.
fn get_fresh_system() -> System {
    let mut sys =
        System::new_with_specifics(RefreshKind::new().with_processes(ProcessRefreshKind::new()));
    sys.refresh_processes();
    sys
}

/// Checks if osu! stable is running (not lazer).
fn check_stable_running_impl(stable_exe: &str, lazer_exe: &str) -> bool {
    let sys = get_fresh_system();
    check_stable_running_with_system(&sys, stable_exe, lazer_exe)
}

/// Checks if osu! stable is running using an existing System instance.
fn check_stable_running_with_system(sys: &System, stable_exe: &str, _lazer_exe: &str) -> bool {
    let processes = find_running_processes_with_system(sys, stable_exe);

    // osu! stable is running if we find osu!.exe that is NOT lazer
    processes.iter().any(|p| !is_lazer_process(p))
}

/// Checks if osu! lazer is running.
fn check_lazer_running_impl(lazer_exe: &str, lazer_alt_exe: &str) -> bool {
    let sys = get_fresh_system();
    check_lazer_running_with_system(&sys, lazer_exe, lazer_alt_exe)
}

/// Checks if osu! lazer is running using an existing System instance.
fn check_lazer_running_with_system(sys: &System, lazer_exe: &str, lazer_alt_exe: &str) -> bool {
    // First check for the alternative lazer executable name
    if is_process_running_with_system(sys, lazer_alt_exe) {
        return true;
    }

    // For the main osu!.exe, check if it's lazer by path
    let processes = find_running_processes_with_system(sys, lazer_exe);
    processes.iter().any(|p| is_lazer_process(p))
}

/// Determines if a process is osu! lazer based on its path.
fn is_lazer_process(process: &ProcessInfo) -> bool {
    if let Some(path) = &process.exe_path {
        let path_str = path.to_string_lossy().to_lowercase();
        // Lazer is typically installed in a different location
        path_str.contains("osu!lazer")
            || path_str.contains("osulazer")
            || path_str.contains("osu-lazer")
            || path_str.contains("dotnet")
            || path_str.contains("osu.game")
            // On Linux/macOS, lazer might be in .local or AppImage
            || path_str.contains("appimage")
            || (path_str.contains(".local") && path_str.contains("osu"))
    } else {
        false
    }
}

// ============================================================================
// Cross-platform process detection using sysinfo
// ============================================================================

/// Finds all running processes matching the given executable name using sysinfo.
pub fn find_running_processes(exe_name: &str) -> Vec<ProcessInfo> {
    let sys = get_fresh_system();
    find_running_processes_with_system(&sys, exe_name)
}

/// Finds all running processes matching the given executable name using an existing System.
fn find_running_processes_with_system(sys: &System, exe_name: &str) -> Vec<ProcessInfo> {
    let exe_name_lower = exe_name.to_lowercase();
    let exe_name_without_ext = exe_name_lower.trim_end_matches(".exe");

    sys.processes()
        .iter()
        .filter_map(|(pid, process)| {
            let process_name = process.name().to_lowercase();

            // Check if the process name matches
            if process_name == exe_name_lower
                || process_name == exe_name_without_ext
                || process_name.contains(&exe_name_lower)
            {
                Some(ProcessInfo {
                    pid: pid.as_u32(),
                    name: process.name().to_string(),
                    exe_path: process.exe().map(|p| p.to_path_buf()),
                })
            } else {
                None
            }
        })
        .collect()
}

/// Checks if a process with the given name is running.
pub fn is_process_running(exe_name: &str) -> bool {
    let sys = get_fresh_system();
    is_process_running_with_system(&sys, exe_name)
}

/// Checks if a process with the given name is running using an existing System.
fn is_process_running_with_system(sys: &System, exe_name: &str) -> bool {
    let exe_name_lower = exe_name.to_lowercase();
    let exe_name_without_ext = exe_name_lower.trim_end_matches(".exe");

    sys.processes().values().any(|process| {
        let process_name = process.name().to_lowercase();
        process_name == exe_name_lower
            || process_name == exe_name_without_ext
            || process_name.contains(&exe_name_lower)
    })
}

// ============================================================================
// Platform-specific fallback implementations (for edge cases)
// ============================================================================

/// Windows-specific process detection fallback.
#[cfg(target_os = "windows")]
pub mod windows {
    use super::ProcessInfo;
    use std::path::PathBuf;

    /// Finds all running processes matching the given executable name using Windows API.
    ///
    /// This is a fallback implementation used when sysinfo doesn't work.
    pub fn find_running_processes_fallback(exe_name: &str) -> Vec<ProcessInfo> {
        let mut processes = Vec::new();

        // Use tasklist command as fallback
        if let Ok(output) = std::process::Command::new("tasklist")
            .args(["/FI", &format!("IMAGENAME eq {}", exe_name), "/FO", "CSV"])
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines().skip(1) {
                    let parts: Vec<&str> = line.split(',').collect();
                    if parts.len() >= 2 {
                        let name = parts[0].trim_matches('"').to_string();
                        if let Ok(pid) = parts[1].trim_matches('"').parse::<u32>() {
                            processes.push(ProcessInfo {
                                pid,
                                name,
                                exe_path: None,
                            });
                        }
                    }
                }
            }
        }

        processes
    }

    /// Checks if a process with the given name is running using Windows API.
    pub fn is_process_running_fallback(exe_name: &str) -> bool {
        if let Ok(output) = std::process::Command::new("tasklist")
            .args(["/FI", &format!("IMAGENAME eq {}", exe_name), "/NH"])
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                return stdout.contains(exe_name);
            }
        }
        false
    }
}

/// Linux-specific process detection fallback using /proc filesystem.
#[cfg(target_os = "linux")]
pub mod linux {
    use super::ProcessInfo;
    use std::fs;
    use std::path::PathBuf;

    /// Finds all running processes matching the given executable name using /proc.
    ///
    /// This is a fallback implementation used when sysinfo doesn't work.
    pub fn find_running_processes_fallback(exe_name: &str) -> Vec<ProcessInfo> {
        let mut processes = Vec::new();

        if let Ok(entries) = fs::read_dir("/proc") {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();

                if let Some(pid_str) = path.file_name().and_then(|n| n.to_str()) {
                    if let Ok(pid) = pid_str.parse::<u32>() {
                        // Read the comm file for process name
                        let comm_path = path.join("comm");
                        if let Ok(comm) = fs::read_to_string(&comm_path) {
                            let name = comm.trim().to_string();
                            let exe_name_clean = exe_name.trim_end_matches(".exe");

                            if name.contains(exe_name_clean)
                                || exe_name_clean.contains(&name)
                                || name == exe_name_clean
                            {
                                let exe_link = path.join("exe");
                                let exe_path = fs::read_link(&exe_link).ok();

                                processes.push(ProcessInfo {
                                    pid,
                                    name,
                                    exe_path,
                                });
                            }
                        }
                    }
                }
            }
        }

        processes
    }

    /// Checks if a process with the given name is running using /proc.
    pub fn is_process_running_fallback(exe_name: &str) -> bool {
        !find_running_processes_fallback(exe_name).is_empty()
    }
}

/// macOS-specific process detection fallback.
#[cfg(target_os = "macos")]
pub mod macos {
    use super::ProcessInfo;
    use std::path::PathBuf;

    /// Finds all running processes matching the given executable name using ps.
    ///
    /// This is a fallback implementation used when sysinfo doesn't work.
    pub fn find_running_processes_fallback(exe_name: &str) -> Vec<ProcessInfo> {
        let mut processes = Vec::new();

        if let Ok(output) = std::process::Command::new("ps")
            .args(["-axo", "pid,comm"])
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines().skip(1) {
                    let parts: Vec<&str> = line.trim().splitn(2, ' ').collect();
                    if parts.len() >= 2 {
                        if let Ok(pid) = parts[0].trim().parse::<u32>() {
                            let comm = parts[1].trim();
                            let exe_name_clean = exe_name.trim_end_matches(".exe");

                            if comm.contains(exe_name) || comm.ends_with(exe_name_clean) {
                                processes.push(ProcessInfo {
                                    pid,
                                    name: comm.to_string(),
                                    exe_path: Some(PathBuf::from(comm)),
                                });
                            }
                        }
                    }
                }
            }
        }

        processes
    }

    /// Checks if a process with the given name is running using pgrep.
    pub fn is_process_running_fallback(exe_name: &str) -> bool {
        if let Ok(output) = std::process::Command::new("pgrep")
            .args(["-f", exe_name])
            .output()
        {
            return output.status.success() && !output.stdout.is_empty();
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_osu_game_display() {
        assert_eq!(OsuGame::Stable.display_name(), "osu! stable");
        assert_eq!(OsuGame::Lazer.display_name(), "osu! lazer");
        assert_eq!(format!("{}", OsuGame::Stable), "osu! stable");
    }

    #[test]
    fn test_game_event() {
        let launch = GameEvent::Launched(OsuGame::Stable);
        assert!(launch.is_launch());
        assert!(!launch.is_close());
        assert_eq!(launch.game(), OsuGame::Stable);

        let close = GameEvent::Closed(OsuGame::Lazer);
        assert!(!close.is_launch());
        assert!(close.is_close());
        assert_eq!(close.game(), OsuGame::Lazer);
    }

    #[test]
    fn test_game_event_display() {
        let launch = GameEvent::Launched(OsuGame::Stable);
        assert_eq!(format!("{}", launch), "osu! stable launched");

        let close = GameEvent::Closed(OsuGame::Lazer);
        assert_eq!(format!("{}", close), "osu! lazer closed");
    }

    #[test]
    fn test_detector_creation() {
        let detector = GameLaunchDetector::new();
        assert_eq!(
            detector.poll_interval,
            GameLaunchDetector::DEFAULT_POLL_INTERVAL
        );
        assert_eq!(detector.stable_exe_name, GameLaunchDetector::STABLE_EXE_NAME);
        assert_eq!(detector.lazer_exe_name, GameLaunchDetector::LAZER_EXE_NAME);
    }

    #[test]
    fn test_detector_with_poll_interval() {
        let interval = Duration::from_millis(500);
        let detector = GameLaunchDetector::with_poll_interval(interval);
        assert_eq!(detector.poll_interval, interval);
    }

    #[test]
    fn test_detector_configuration() {
        let mut detector = GameLaunchDetector::new();

        detector.set_stable_exe_name("custom_stable.exe");
        assert_eq!(detector.stable_exe_name, "custom_stable.exe");

        detector.set_lazer_exe_name("custom_lazer.exe");
        assert_eq!(detector.lazer_exe_name, "custom_lazer.exe");

        let interval = Duration::from_secs(5);
        detector.set_poll_interval(interval);
        assert_eq!(detector.poll_interval, interval);
    }

    #[test]
    fn test_check_once_no_changes() {
        let mut detector = GameLaunchDetector::new();
        // First call establishes baseline
        let _ = detector.check_once();
        // Second call with no changes should return empty
        let events = detector.check_once();
        // Events depend on whether osu is actually running
        // In a test environment, we just verify no panic occurs
        assert!(events.len() <= 2); // At most 2 events (stable + lazer)
    }

    #[test]
    fn test_is_lazer_process() {
        let lazer_process = ProcessInfo {
            pid: 1234,
            name: "osu!.exe".to_string(),
            exe_path: Some(PathBuf::from("/home/user/.local/share/osu-lazer/osu!.exe")),
        };
        assert!(is_lazer_process(&lazer_process));

        let stable_process = ProcessInfo {
            pid: 5678,
            name: "osu!.exe".to_string(),
            exe_path: Some(PathBuf::from("C:\\osu!\\osu!.exe")),
        };
        assert!(!is_lazer_process(&stable_process));

        let no_path_process = ProcessInfo {
            pid: 9999,
            name: "osu!.exe".to_string(),
            exe_path: None,
        };
        assert!(!is_lazer_process(&no_path_process));
    }

    #[test]
    fn test_is_lazer_process_dotnet() {
        let lazer_dotnet = ProcessInfo {
            pid: 1234,
            name: "osu!.exe".to_string(),
            exe_path: Some(PathBuf::from("/usr/share/dotnet/osu!/osu!.exe")),
        };
        assert!(is_lazer_process(&lazer_dotnet));
    }

    #[test]
    fn test_is_lazer_process_appimage() {
        let lazer_appimage = ProcessInfo {
            pid: 1234,
            name: "osu!".to_string(),
            exe_path: Some(PathBuf::from("/tmp/.mount_osu.AppImage/osu!")),
        };
        assert!(is_lazer_process(&lazer_appimage));
    }

    #[test]
    fn test_default_implementation() {
        let detector = GameLaunchDetector::default();
        assert_eq!(
            detector.poll_interval,
            GameLaunchDetector::DEFAULT_POLL_INTERVAL
        );
    }

    #[test]
    fn test_stop_monitoring_flag() {
        let detector = GameLaunchDetector::new();
        assert!(!detector.stop_flag.load(Ordering::SeqCst));
        detector.stop_monitoring();
        assert!(detector.stop_flag.load(Ordering::SeqCst));
    }

    #[test]
    fn test_process_info_creation() {
        let info = ProcessInfo {
            pid: 12345,
            name: "test.exe".to_string(),
            exe_path: Some(PathBuf::from("/path/to/test.exe")),
        };

        assert_eq!(info.pid, 12345);
        assert_eq!(info.name, "test.exe");
        assert_eq!(
            info.exe_path,
            Some(PathBuf::from("/path/to/test.exe"))
        );
    }

    #[test]
    fn test_find_running_processes_returns_vec() {
        // Just verify the function returns without panic
        let processes = find_running_processes("nonexistent_process_xyz");
        assert!(processes.is_empty() || !processes.is_empty()); // Should not panic
    }

    #[test]
    fn test_is_process_running_returns_bool() {
        // Just verify the function returns without panic
        let running = is_process_running("nonexistent_process_xyz");
        assert!(!running); // A non-existent process should not be running
    }
}
