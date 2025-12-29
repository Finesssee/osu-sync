//! TUI Snapshot capture for AI vision systems.
//!
//! Provides functionality to capture the current TUI state as text or structured JSON,
//! enabling AI systems to "see" and interact with the application.

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use serde::{Deserialize, Serialize};
use std::io;

use crate::app::{App, AppState};

/// Snapshot of TUI state for AI vision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiSnapshot {
    /// Current screen state name (e.g., "MainMenu", "SyncConfig")
    pub state: String,

    /// Rendered TUI buffer as text (120x30 default)
    pub buffer: String,

    /// Terminal dimensions
    pub width: u16,
    pub height: u16,

    /// State-specific structured data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_data: Option<StateData>,

    /// Timestamp of capture (ISO 8601)
    pub timestamp: String,
}

/// State-specific structured data for AI consumption
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum StateData {
    MainMenu {
        selected: usize,
        items: Vec<String>,
    },
    SyncConfig {
        selected: usize,
        stable_count: usize,
        lazer_count: usize,
        filter_panel_open: bool,
    },
    DryRunPreview {
        selected_item: usize,
        total_items: usize,
        checked_count: usize,
        filter_text: String,
        filter_mode: bool,
    },
    Statistics {
        tab: String,
        loading: bool,
        has_stats: bool,
    },
    Scanning {
        in_progress: bool,
        status_message: String,
    },
    Syncing {
        is_paused: bool,
        imported: usize,
        skipped: usize,
        failed: usize,
    },
    Config {
        selected: usize,
        stable_path: Option<String>,
        lazer_path: Option<String>,
        editing: bool,
    },
    CollectionConfig {
        selected: usize,
        collection_count: usize,
        loading: bool,
    },
    BackupConfig {
        selected: usize,
    },
    RestoreConfig {
        selected: usize,
        backup_count: usize,
        loading: bool,
    },
    MediaConfig {
        selected: usize,
        media_type: String,
    },
    ReplayConfig {
        selected: usize,
        replay_count: usize,
        loading: bool,
    },
    UnifiedConfig {
        selected: usize,
    },
    Help {
        previous_state: String,
    },
    Other {
        state_name: String,
    },
}

impl TuiSnapshot {
    /// Capture current TUI state using TestBackend
    ///
    /// Creates a fresh App instance and captures its initial state.
    /// For capturing a running app's state, use `capture_from_app`.
    pub fn capture(width: u16, height: u16) -> io::Result<Self> {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend)?;
        let app = App::new();

        terminal.draw(|frame| {
            crate::screens::render(frame, &app);
        })?;

        let buffer = Self::extract_buffer(&terminal);
        let state_name = Self::get_state_name(&app.state);
        let state_data = Self::extract_state_data(&app);

        Ok(Self {
            state: state_name,
            buffer,
            width,
            height,
            state_data: Some(state_data),
            timestamp: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Capture from an existing App instance (for live TUI capture)
    pub fn capture_from_app(app: &App, width: u16, height: u16) -> io::Result<Self> {
        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend)?;

        terminal.draw(|frame| {
            crate::screens::render(frame, app);
        })?;

        let buffer = Self::extract_buffer(&terminal);
        let state_name = Self::get_state_name(&app.state);
        let state_data = Self::extract_state_data(app);

        Ok(Self {
            state: state_name,
            buffer,
            width,
            height,
            state_data: Some(state_data),
            timestamp: chrono::Utc::now().to_rfc3339(),
        })
    }

    /// Extract the rendered buffer as a string
    fn extract_buffer(terminal: &Terminal<TestBackend>) -> String {
        let buffer = terminal.backend().buffer();
        let mut output = String::new();

        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                let cell = &buffer[(x, y)];
                output.push_str(cell.symbol());
            }
            output.push('\n');
        }

        output
    }

    /// Get the state name from AppState
    fn get_state_name(state: &AppState) -> String {
        match state {
            AppState::MainMenu { .. } => "MainMenu",
            AppState::Scanning { .. } => "Scanning",
            AppState::SyncConfig { .. } => "SyncConfig",
            AppState::Syncing { .. } => "Syncing",
            AppState::DuplicateDialog { .. } => "DuplicateDialog",
            AppState::SyncComplete { .. } => "SyncComplete",
            AppState::Config { .. } => "Config",
            AppState::Statistics { .. } => "Statistics",
            AppState::CollectionConfig { .. } => "CollectionConfig",
            AppState::CollectionSync { .. } => "CollectionSync",
            AppState::CollectionSummary { .. } => "CollectionSummary",
            AppState::DryRunPreview { .. } => "DryRunPreview",
            AppState::BackupConfig { .. } => "BackupConfig",
            AppState::BackupProgress { .. } => "BackupProgress",
            AppState::BackupComplete { .. } => "BackupComplete",
            AppState::RestoreConfig { .. } => "RestoreConfig",
            AppState::RestoreConfirm { .. } => "RestoreConfirm",
            AppState::RestoreProgress { .. } => "RestoreProgress",
            AppState::RestoreComplete { .. } => "RestoreComplete",
            AppState::MediaConfig { .. } => "MediaConfig",
            AppState::MediaProgress { .. } => "MediaProgress",
            AppState::MediaComplete { .. } => "MediaComplete",
            AppState::ReplayConfig { .. } => "ReplayConfig",
            AppState::ReplayProgress { .. } => "ReplayProgress",
            AppState::ReplayComplete { .. } => "ReplayComplete",
            AppState::Help { .. } => "Help",
            AppState::UnifiedConfig { .. } => "UnifiedConfig",
            AppState::UnifiedSetup { .. } => "UnifiedSetup",
            AppState::UnifiedStatus { .. } => "UnifiedStatus",
            AppState::Exiting => "Exiting",
        }
        .to_string()
    }

    /// Extract structured state data for AI consumption
    fn extract_state_data(app: &App) -> StateData {
        match &app.state {
            AppState::MainMenu { selected } => StateData::MainMenu {
                selected: *selected,
                items: vec![
                    "Sync Beatmaps".into(),
                    "Collection Sync".into(),
                    "Statistics".into(),
                    "Extract Media".into(),
                    "Export Replays".into(),
                    "Backup".into(),
                    "Restore".into(),
                    "Configuration".into(),
                    "Unified Storage".into(),
                    "Exit".into(),
                ],
            },
            AppState::SyncConfig {
                selected,
                stable_count,
                lazer_count,
                filter_panel_open,
                ..
            } => StateData::SyncConfig {
                selected: *selected,
                stable_count: *stable_count,
                lazer_count: *lazer_count,
                filter_panel_open: *filter_panel_open,
            },
            AppState::DryRunPreview {
                selected_item,
                result,
                checked_items,
                filter_text,
                filter_mode,
                ..
            } => StateData::DryRunPreview {
                selected_item: *selected_item,
                total_items: result.items.len(),
                checked_count: checked_items.len(),
                filter_text: filter_text.clone(),
                filter_mode: *filter_mode,
            },
            AppState::Statistics {
                stats,
                loading,
                tab,
                ..
            } => StateData::Statistics {
                tab: format!("{:?}", tab),
                loading: *loading,
                has_stats: stats.is_some(),
            },
            AppState::Scanning {
                in_progress,
                status_message,
                ..
            } => StateData::Scanning {
                in_progress: *in_progress,
                status_message: status_message.clone(),
            },
            AppState::Syncing { is_paused, stats, .. } => StateData::Syncing {
                is_paused: *is_paused,
                imported: stats.imported,
                skipped: stats.skipped,
                failed: stats.failed,
            },
            AppState::Config {
                selected,
                stable_path,
                lazer_path,
                editing,
                ..
            } => StateData::Config {
                selected: *selected,
                stable_path: stable_path.clone(),
                lazer_path: lazer_path.clone(),
                editing: editing.is_some(),
            },
            AppState::CollectionConfig {
                selected,
                collections,
                loading,
                ..
            } => StateData::CollectionConfig {
                selected: *selected,
                collection_count: collections.len(),
                loading: *loading,
            },
            AppState::BackupConfig { selected, .. } => StateData::BackupConfig {
                selected: *selected,
            },
            AppState::RestoreConfig {
                selected,
                backups,
                loading,
                ..
            } => StateData::RestoreConfig {
                selected: *selected,
                backup_count: backups.len(),
                loading: *loading,
            },
            AppState::MediaConfig {
                selected,
                media_type,
                ..
            } => StateData::MediaConfig {
                selected: *selected,
                media_type: format!("{:?}", media_type),
            },
            AppState::ReplayConfig {
                selected,
                replays,
                loading,
                ..
            } => StateData::ReplayConfig {
                selected: *selected,
                replay_count: replays.len(),
                loading: *loading,
            },
            AppState::UnifiedConfig { .. } => StateData::UnifiedConfig { selected: 0 },
            AppState::Help { previous_state } => StateData::Help {
                previous_state: Self::get_state_name(previous_state),
            },
            _ => StateData::Other {
                state_name: Self::get_state_name(&app.state),
            },
        }
    }

    /// Output as formatted text (for human/AI consumption)
    pub fn as_text(&self) -> String {
        format!(
            "State: {}\nSize: {}x{}\nTimestamp: {}\n---\n{}",
            self.state, self.width, self.height, self.timestamp, self.buffer
        )
    }

    /// Output as JSON string
    pub fn as_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Output as compact JSON (for MCP tools)
    pub fn as_json_compact(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_snapshot_capture() {
        let snapshot = TuiSnapshot::capture(120, 30).unwrap();
        assert_eq!(snapshot.state, "MainMenu");
        assert!(!snapshot.buffer.is_empty());
        assert_eq!(snapshot.width, 120);
        assert_eq!(snapshot.height, 30);
    }

    #[test]
    fn test_snapshot_json_serialization() {
        let snapshot = TuiSnapshot::capture(120, 30).unwrap();
        let json = snapshot.as_json().unwrap();
        assert!(json.contains("MainMenu"));
        assert!(json.contains("buffer"));
    }

    #[test]
    fn test_snapshot_text_output() {
        let snapshot = TuiSnapshot::capture(120, 30).unwrap();
        let text = snapshot.as_text();
        assert!(text.contains("State: MainMenu"));
        assert!(text.contains("Size: 120x30"));
    }

    #[test]
    fn test_state_data_mainmenu() {
        let app = App::new();
        let state_data = TuiSnapshot::extract_state_data(&app);
        match state_data {
            StateData::MainMenu { selected, items } => {
                assert_eq!(selected, 0);
                assert_eq!(items.len(), 10);
            }
            _ => panic!("Expected MainMenu state data"),
        }
    }
}
