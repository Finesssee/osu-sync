//! File system watcher for unified storage.
//!
//! This module provides cross-platform file watching capabilities using the `notify` crate.
//! It monitors directories for changes and emits events that can trigger synchronization
//! operations between osu! stable and lazer installations.
//!
//! # Features
//!
//! - Cross-platform file watching using `notify::RecommendedWatcher`
//! - Event debouncing to prevent duplicate notifications
//! - Filtering for temporary files (.tmp, .partial, etc.)
//! - Support for watching multiple directories simultaneously
//!
//! # Example
//!
//! ```rust,ignore
//! use osu_sync_core::unified::{UnifiedWatcher, FileChangeEvent};
//! use std::path::Path;
//!
//! let (mut watcher, rx) = UnifiedWatcher::new()?;
//! watcher.watch(Path::new("/path/to/songs"))?;
//!
//! // Process events in a loop
//! while let Ok(event) = rx.recv() {
//!     match event {
//!         FileChangeEvent::Created { path, is_dir } => {
//!             println!("Created: {:?} (dir: {})", path, is_dir);
//!         }
//!         FileChangeEvent::Modified { path } => {
//!             println!("Modified: {:?}", path);
//!         }
//!         FileChangeEvent::Deleted { path } => {
//!             println!("Deleted: {:?}", path);
//!         }
//!         FileChangeEvent::Renamed { from, to } => {
//!             println!("Renamed: {:?} -> {:?}", from, to);
//!         }
//!     }
//! }
//! ```

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};

use crate::error::{Error, Result};

/// Default debounce duration in milliseconds.
const DEFAULT_DEBOUNCE_MS: u64 = 100;

/// File extensions that are considered temporary and should be ignored.
const TEMP_EXTENSIONS: &[&str] = &[
    "tmp",
    "temp",
    "partial",
    "crdownload",
    "part",
    "download",
    "swp",
    "lock",
    "bak",
];

/// File name patterns that indicate temporary or system files.
const TEMP_PATTERNS: &[&str] = &[
    ".tmp",
    ".temp",
    ".partial",
    "~$", // Office temp files
    ".~", // Backup files
    "Thumbs.db",
    ".DS_Store",
    "desktop.ini",
    ".git",
    ".svn",
];

/// Event representing a file system change.
///
/// These events are processed and deduplicated versions of the raw
/// `notify` events, suitable for triggering sync operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileChangeEvent {
    /// A file or directory was created.
    Created {
        /// Path to the created file or directory.
        path: PathBuf,
        /// Whether the created item is a directory.
        is_dir: bool,
    },
    /// A file was modified.
    Modified {
        /// Path to the modified file.
        path: PathBuf,
    },
    /// A file or directory was deleted.
    Deleted {
        /// Path to the deleted file or directory.
        path: PathBuf,
    },
    /// A file or directory was renamed.
    Renamed {
        /// Original path before renaming.
        from: PathBuf,
        /// New path after renaming.
        to: PathBuf,
    },
}

impl FileChangeEvent {
    /// Returns the primary path associated with this event.
    pub fn path(&self) -> &Path {
        match self {
            Self::Created { path, .. } => path,
            Self::Modified { path } => path,
            Self::Deleted { path } => path,
            Self::Renamed { to, .. } => to,
        }
    }

    /// Returns true if this event involves a directory.
    pub fn is_directory_event(&self) -> bool {
        match self {
            Self::Created { is_dir, .. } => *is_dir,
            _ => false,
        }
    }
}

/// Cross-platform file system watcher for unified storage.
///
/// This struct wraps `notify::RecommendedWatcher` to provide a simplified
/// interface for watching directories and receiving change events.
pub struct UnifiedWatcher {
    /// The underlying notify watcher.
    watcher: RecommendedWatcher,
    /// Internal sender for processed events (kept for watcher callback).
    #[allow(dead_code)]
    event_tx: Sender<FileChangeEvent>,
    /// List of currently watched paths.
    watched_paths: Vec<PathBuf>,
    /// Event handler for processing and filtering events.
    handler: WatcherEventHandler,
}

impl UnifiedWatcher {
    /// Creates a new unified watcher with default settings.
    ///
    /// Returns a tuple containing the watcher and a receiver for file change events.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying file system watcher cannot be created.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let (mut watcher, rx) = UnifiedWatcher::new()?;
    /// watcher.watch(Path::new("/path/to/watch"))?;
    /// ```
    pub fn new() -> Result<(Self, Receiver<FileChangeEvent>)> {
        Self::with_debounce(DEFAULT_DEBOUNCE_MS)
    }

    /// Creates a new unified watcher with custom debounce duration.
    ///
    /// # Arguments
    ///
    /// * `debounce_ms` - Debounce duration in milliseconds.
    ///
    /// # Errors
    ///
    /// Returns an error if the underlying file system watcher cannot be created.
    pub fn with_debounce(debounce_ms: u64) -> Result<(Self, Receiver<FileChangeEvent>)> {
        let (event_tx, event_rx) = channel();
        let (internal_tx, internal_rx) = channel::<Event>();

        // Create the watcher with a simple callback that forwards raw events
        let watcher = RecommendedWatcher::new(
            move |result: std::result::Result<Event, notify::Error>| {
                if let Ok(event) = result {
                    let _ = internal_tx.send(event);
                }
            },
            notify::Config::default(),
        )
        .map_err(|e| Error::Other(format!("Failed to create file watcher: {}", e)))?;

        let handler = WatcherEventHandler::new(debounce_ms);
        let handler_clone = handler.clone();
        let event_tx_clone = event_tx.clone();

        // Spawn a thread to process and debounce events
        std::thread::spawn(move || {
            let handler = handler_clone;
            let mut pending_events: HashMap<PathBuf, (EventKind, Instant)> = HashMap::new();
            let mut rename_from: Option<PathBuf> = None;
            let debounce_duration = Duration::from_millis(debounce_ms);

            loop {
                // Try to receive with a timeout for debounce processing
                match internal_rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(event) => {
                        // Process each path in the event
                        for path in event.paths {
                            // Skip if this path should be ignored
                            if handler.should_ignore(&path) {
                                continue;
                            }

                            match event.kind {
                                EventKind::Create(_) => {
                                    pending_events
                                        .insert(path.clone(), (event.kind, Instant::now()));
                                }
                                EventKind::Modify(_) => {
                                    // Only update if not already pending as Create
                                    if !matches!(
                                        pending_events.get(&path),
                                        Some((EventKind::Create(_), _))
                                    ) {
                                        pending_events
                                            .insert(path.clone(), (event.kind, Instant::now()));
                                    }
                                }
                                EventKind::Remove(_) => {
                                    // Remove cancels any pending create/modify
                                    pending_events.remove(&path);
                                    pending_events
                                        .insert(path.clone(), (event.kind, Instant::now()));
                                }
                                EventKind::Access(_) => {
                                    // Ignore access events
                                }
                                EventKind::Other => {
                                    // Handle rename events
                                    if rename_from.is_none() {
                                        rename_from = Some(path);
                                    } else if let Some(from) = rename_from.take() {
                                        // We have both parts of a rename
                                        let change_event =
                                            FileChangeEvent::Renamed { from, to: path };
                                        let _ = event_tx_clone.send(change_event);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        // Process debounced events
                        let now = Instant::now();
                        let mut to_emit = Vec::new();

                        pending_events.retain(|path, (kind, timestamp)| {
                            if now.duration_since(*timestamp) >= debounce_duration {
                                to_emit.push((path.clone(), *kind));
                                false
                            } else {
                                true
                            }
                        });

                        for (path, kind) in to_emit {
                            let change_event = match kind {
                                EventKind::Create(_) => FileChangeEvent::Created {
                                    is_dir: path.is_dir(),
                                    path,
                                },
                                EventKind::Modify(_) => FileChangeEvent::Modified { path },
                                EventKind::Remove(_) => FileChangeEvent::Deleted { path },
                                _ => continue,
                            };
                            let _ = event_tx_clone.send(change_event);
                        }

                        // Clear stale rename_from after timeout
                        if rename_from.is_some() {
                            rename_from = None;
                        }
                    }
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                        // Channel closed, exit the thread
                        break;
                    }
                }
            }
        });

        Ok((
            Self {
                watcher,
                event_tx,
                watched_paths: Vec::new(),
                handler,
            },
            event_rx,
        ))
    }

    /// Starts watching a directory for changes.
    ///
    /// The directory will be watched recursively, meaning all subdirectories
    /// and their contents will also be monitored.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the directory to watch.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The path does not exist
    /// - The path is not a directory
    /// - The watcher fails to register the path
    pub fn watch(&mut self, path: &Path) -> Result<()> {
        if !path.exists() {
            return Err(Error::Other(format!(
                "Path does not exist: {}",
                path.display()
            )));
        }

        if !path.is_dir() {
            return Err(Error::Other(format!(
                "Path is not a directory: {}",
                path.display()
            )));
        }

        self.watcher
            .watch(path, RecursiveMode::Recursive)
            .map_err(|e| Error::Other(format!("Failed to watch path: {}", e)))?;

        self.watched_paths.push(path.to_path_buf());
        Ok(())
    }

    /// Stops watching a directory.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the directory to stop watching.
    ///
    /// # Errors
    ///
    /// Returns an error if the watcher fails to unregister the path.
    pub fn unwatch(&mut self, path: &Path) -> Result<()> {
        self.watcher
            .unwatch(path)
            .map_err(|e| Error::Other(format!("Failed to unwatch path: {}", e)))?;

        self.watched_paths.retain(|p| p != path);
        Ok(())
    }

    /// Returns a slice of all currently watched paths.
    pub fn get_watched_paths(&self) -> &[PathBuf] {
        &self.watched_paths
    }

    /// Adds an ignore pattern to the event handler.
    ///
    /// Files matching any ignore pattern will not generate events.
    pub fn add_ignore_pattern(&mut self, pattern: &str) {
        self.handler.add_ignore_pattern(pattern);
    }

    /// Returns the number of currently watched directories.
    pub fn watched_count(&self) -> usize {
        self.watched_paths.len()
    }

    /// Returns true if any directories are being watched.
    pub fn is_watching(&self) -> bool {
        !self.watched_paths.is_empty()
    }
}

/// Handler for processing and filtering file system events.
///
/// This struct provides debouncing logic and filtering capabilities
/// to reduce noise from the file system watcher.
#[derive(Clone)]
pub struct WatcherEventHandler {
    /// Debounce duration in milliseconds.
    debounce_ms: u64,
    /// List of patterns to ignore (glob-like patterns).
    ignore_patterns: Vec<String>,
}

impl WatcherEventHandler {
    /// Creates a new event handler with the specified debounce duration.
    ///
    /// # Arguments
    ///
    /// * `debounce_ms` - Debounce duration in milliseconds.
    pub fn new(debounce_ms: u64) -> Self {
        Self {
            debounce_ms,
            ignore_patterns: Vec::new(),
        }
    }

    /// Adds a pattern to the ignore list.
    ///
    /// Patterns are matched against file names and paths. Simple glob-like
    /// matching is supported:
    /// - `*` matches any sequence of characters
    /// - Patterns are case-insensitive
    ///
    /// # Arguments
    ///
    /// * `pattern` - The pattern to add to the ignore list.
    pub fn add_ignore_pattern(&mut self, pattern: &str) {
        self.ignore_patterns.push(pattern.to_lowercase());
    }

    /// Removes a pattern from the ignore list.
    ///
    /// # Arguments
    ///
    /// * `pattern` - The pattern to remove.
    ///
    /// # Returns
    ///
    /// Returns true if the pattern was found and removed.
    pub fn remove_ignore_pattern(&mut self, pattern: &str) -> bool {
        let pattern_lower = pattern.to_lowercase();
        let initial_len = self.ignore_patterns.len();
        self.ignore_patterns.retain(|p| p != &pattern_lower);
        self.ignore_patterns.len() != initial_len
    }

    /// Returns true if the given path should be ignored.
    ///
    /// A path is ignored if:
    /// - It has a temporary file extension (.tmp, .partial, etc.)
    /// - It matches a known temporary file pattern
    /// - It matches any user-defined ignore pattern
    ///
    /// # Arguments
    ///
    /// * `path` - The path to check.
    pub fn should_ignore(&self, path: &Path) -> bool {
        // Get the file name
        let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
            return false;
        };
        let file_name_lower = file_name.to_lowercase();

        // Check for temporary file extensions
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if TEMP_EXTENSIONS.contains(&ext.to_lowercase().as_str()) {
                return true;
            }
        }

        // Check for known temporary file patterns
        for pattern in TEMP_PATTERNS {
            if file_name_lower.contains(&pattern.to_lowercase()) {
                return true;
            }
        }

        // Check user-defined ignore patterns
        for pattern in &self.ignore_patterns {
            if self.matches_pattern(&file_name_lower, pattern) {
                return true;
            }

            // Also check full path
            if let Some(path_str) = path.to_str() {
                if self.matches_pattern(&path_str.to_lowercase(), pattern) {
                    return true;
                }
            }
        }

        false
    }

    /// Performs simple glob-like pattern matching.
    fn matches_pattern(&self, text: &str, pattern: &str) -> bool {
        if pattern.contains('*') {
            // Simple glob matching
            let parts: Vec<&str> = pattern.split('*').collect();
            if parts.len() == 1 {
                return text == pattern;
            }

            let mut pos = 0;
            for (i, part) in parts.iter().enumerate() {
                if part.is_empty() {
                    continue;
                }

                if let Some(found_pos) = text[pos..].find(part) {
                    if i == 0 && found_pos != 0 {
                        // First part must match at start
                        return false;
                    }
                    pos += found_pos + part.len();
                } else {
                    return false;
                }
            }

            // Last part must match at end if not empty
            if let Some(last) = parts.last() {
                if !last.is_empty() && !text.ends_with(last) {
                    return false;
                }
            }

            true
        } else {
            text.contains(pattern)
        }
    }

    /// Processes a batch of raw notify events into filtered FileChangeEvents.
    ///
    /// This method consolidates multiple related events and filters out
    /// temporary files and ignored patterns.
    ///
    /// # Arguments
    ///
    /// * `events` - A vector of raw notify events to process.
    ///
    /// # Returns
    ///
    /// A vector of processed and filtered FileChangeEvents.
    pub fn process_events(&self, events: Vec<Event>) -> Vec<FileChangeEvent> {
        let mut result = Vec::new();
        let mut seen_paths: HashMap<PathBuf, EventKind> = HashMap::new();
        let mut rename_from: Option<PathBuf> = None;

        for event in events {
            for path in event.paths {
                // Skip ignored paths
                if self.should_ignore(&path) {
                    continue;
                }

                match event.kind {
                    EventKind::Create(_) => {
                        // Create overrides any previous event
                        seen_paths.insert(path, event.kind);
                    }
                    EventKind::Modify(_) => {
                        // Modify only if not already seen
                        seen_paths.entry(path).or_insert(event.kind);
                    }
                    EventKind::Remove(_) => {
                        // Remove overrides create (file created then deleted)
                        if matches!(seen_paths.get(&path), Some(EventKind::Create(_))) {
                            seen_paths.remove(&path);
                        } else {
                            seen_paths.insert(path, event.kind);
                        }
                    }
                    EventKind::Access(_) => {
                        // Ignore access events
                    }
                    EventKind::Other => {
                        // Handle rename events (platform-specific)
                        if rename_from.is_none() {
                            rename_from = Some(path);
                        } else if let Some(from) = rename_from.take() {
                            result.push(FileChangeEvent::Renamed { from, to: path });
                        }
                    }
                    _ => {}
                }
            }
        }

        // Convert seen paths to events
        for (path, kind) in seen_paths {
            let event = match kind {
                EventKind::Create(_) => FileChangeEvent::Created {
                    is_dir: path.is_dir(),
                    path,
                },
                EventKind::Modify(_) => FileChangeEvent::Modified { path },
                EventKind::Remove(_) => FileChangeEvent::Deleted { path },
                _ => continue,
            };
            result.push(event);
        }

        result
    }

    /// Returns the debounce duration in milliseconds.
    pub fn debounce_ms(&self) -> u64 {
        self.debounce_ms
    }

    /// Sets the debounce duration in milliseconds.
    pub fn set_debounce_ms(&mut self, debounce_ms: u64) {
        self.debounce_ms = debounce_ms;
    }

    /// Returns the number of ignore patterns.
    pub fn ignore_pattern_count(&self) -> usize {
        self.ignore_patterns.len()
    }

    /// Clears all ignore patterns.
    pub fn clear_ignore_patterns(&mut self) {
        self.ignore_patterns.clear();
    }
}

impl Default for WatcherEventHandler {
    fn default() -> Self {
        Self::new(DEFAULT_DEBOUNCE_MS)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_ignore_temp_files() {
        let handler = WatcherEventHandler::new(100);

        // Should ignore temp extensions
        assert!(handler.should_ignore(Path::new("/path/to/file.tmp")));
        assert!(handler.should_ignore(Path::new("/path/to/file.partial")));
        assert!(handler.should_ignore(Path::new("/path/to/file.crdownload")));

        // Should ignore known patterns
        assert!(handler.should_ignore(Path::new("/path/to/.DS_Store")));
        assert!(handler.should_ignore(Path::new("/path/to/Thumbs.db")));
        assert!(handler.should_ignore(Path::new("/path/to/~$document.docx")));

        // Should not ignore normal files
        assert!(!handler.should_ignore(Path::new("/path/to/beatmap.osu")));
        assert!(!handler.should_ignore(Path::new("/path/to/audio.mp3")));
        assert!(!handler.should_ignore(Path::new("/path/to/image.png")));
    }

    #[test]
    fn test_custom_ignore_patterns() {
        let mut handler = WatcherEventHandler::new(100);

        // Add custom pattern
        handler.add_ignore_pattern("*.osr");
        assert!(handler.should_ignore(Path::new("/path/to/replay.osr")));
        assert!(!handler.should_ignore(Path::new("/path/to/beatmap.osu")));

        // Remove pattern
        assert!(handler.remove_ignore_pattern("*.osr"));
        assert!(!handler.should_ignore(Path::new("/path/to/replay.osr")));
    }

    #[test]
    fn test_pattern_matching() {
        let handler = WatcherEventHandler::new(100);

        // Exact match
        assert!(handler.matches_pattern("file.txt", "file.txt"));

        // Contains
        assert!(handler.matches_pattern("myfile.txt", "file"));

        // Glob patterns
        assert!(handler.matches_pattern("file.txt", "*.txt"));
        assert!(handler.matches_pattern("file.osu", "*.osu"));
        assert!(!handler.matches_pattern("file.txt", "*.osu"));
    }

    #[test]
    fn test_file_change_event() {
        let created = FileChangeEvent::Created {
            path: PathBuf::from("/path/to/file"),
            is_dir: false,
        };
        assert_eq!(created.path(), Path::new("/path/to/file"));
        assert!(!created.is_directory_event());

        let created_dir = FileChangeEvent::Created {
            path: PathBuf::from("/path/to/dir"),
            is_dir: true,
        };
        assert!(created_dir.is_directory_event());

        let modified = FileChangeEvent::Modified {
            path: PathBuf::from("/path/to/file"),
        };
        assert_eq!(modified.path(), Path::new("/path/to/file"));
        assert!(!modified.is_directory_event());

        let renamed = FileChangeEvent::Renamed {
            from: PathBuf::from("/old/path"),
            to: PathBuf::from("/new/path"),
        };
        assert_eq!(renamed.path(), Path::new("/new/path"));
    }

    #[test]
    fn test_process_events_consolidation() {
        let handler = WatcherEventHandler::new(100);

        // Create followed by remove should cancel out
        let events = vec![
            Event {
                kind: EventKind::Create(notify::event::CreateKind::File),
                paths: vec![PathBuf::from("/path/to/file.txt")],
                attrs: Default::default(),
            },
            Event {
                kind: EventKind::Remove(notify::event::RemoveKind::File),
                paths: vec![PathBuf::from("/path/to/file.txt")],
                attrs: Default::default(),
            },
        ];

        let processed = handler.process_events(events);
        assert!(processed.is_empty());
    }

    #[test]
    fn test_process_events_filters_temp() {
        let handler = WatcherEventHandler::new(100);

        let events = vec![Event {
            kind: EventKind::Create(notify::event::CreateKind::File),
            paths: vec![PathBuf::from("/path/to/file.tmp")],
            attrs: Default::default(),
        }];

        let processed = handler.process_events(events);
        assert!(processed.is_empty());
    }

    #[test]
    fn test_handler_default() {
        let handler = WatcherEventHandler::default();
        assert_eq!(handler.debounce_ms(), DEFAULT_DEBOUNCE_MS);
        assert_eq!(handler.ignore_pattern_count(), 0);
    }

    #[test]
    fn test_handler_setters() {
        let mut handler = WatcherEventHandler::new(100);

        handler.set_debounce_ms(200);
        assert_eq!(handler.debounce_ms(), 200);

        handler.add_ignore_pattern("test");
        assert_eq!(handler.ignore_pattern_count(), 1);

        handler.clear_ignore_patterns();
        assert_eq!(handler.ignore_pattern_count(), 0);
    }
}
