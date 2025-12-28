//! TUI-based conflict resolver using channels

use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Mutex, RwLock};

use osu_sync_core::dedup::{DuplicateAction, DuplicateInfo, DuplicateResolution};
use osu_sync_core::sync::ConflictResolver;

use crate::app::AppMessage;

/// A conflict resolver that communicates with the TUI through channels
#[allow(dead_code)]
pub struct TuiResolver {
    /// Channel to send duplicate info to the UI
    ui_tx: Sender<AppMessage>,
    /// Channel to receive resolution from the UI (wrapped in Mutex for Sync)
    resolution_rx: Mutex<Receiver<DuplicateResolution>>,
    /// Cached "apply to all" resolution
    cached: RwLock<Option<DuplicateResolution>>,
}

impl TuiResolver {
    /// Create a new TUI resolver with the given channels
    #[allow(dead_code)]
    pub fn new(ui_tx: Sender<AppMessage>, resolution_rx: Receiver<DuplicateResolution>) -> Self {
        Self {
            ui_tx,
            resolution_rx: Mutex::new(resolution_rx),
            cached: RwLock::new(None),
        }
    }
}

impl ConflictResolver for TuiResolver {
    fn resolve(&self, duplicate: &DuplicateInfo) -> DuplicateResolution {
        // Check cache first
        if let Ok(guard) = self.cached.read() {
            if let Some(ref resolution) = *guard {
                if resolution.apply_to_all {
                    return resolution.clone();
                }
            }
        }

        // Send duplicate info to UI thread
        if self
            .ui_tx
            .send(AppMessage::DuplicateFound(duplicate.clone()))
            .is_err()
        {
            // Channel closed, default to skip
            return DuplicateResolution {
                action: DuplicateAction::Skip,
                apply_to_all: false,
            };
        }

        // Wait for resolution from UI (blocking)
        let resolution = {
            let rx = self.resolution_rx.lock().unwrap();
            match rx.recv() {
                Ok(res) => res,
                Err(_) => {
                    // Channel closed, default to skip
                    DuplicateResolution {
                        action: DuplicateAction::Skip,
                        apply_to_all: false,
                    }
                }
            }
        };

        // Cache if apply_to_all
        if resolution.apply_to_all {
            if let Ok(mut guard) = self.cached.write() {
                *guard = Some(resolution.clone());
            }
        }

        resolution
    }

    fn name(&self) -> &'static str {
        "tui"
    }
}

/// Simple auto resolver for when we don't want interactive resolution
#[allow(dead_code)]
pub struct AutoSkipResolver;

impl ConflictResolver for AutoSkipResolver {
    fn resolve(&self, _duplicate: &DuplicateInfo) -> DuplicateResolution {
        DuplicateResolution {
            action: DuplicateAction::Skip,
            apply_to_all: true,
        }
    }

    fn name(&self) -> &'static str {
        "auto-skip"
    }
}
