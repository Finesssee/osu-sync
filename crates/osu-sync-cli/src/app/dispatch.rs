use crossterm::event::KeyEvent;

use crate::event;

use super::{App, AppState};

#[derive(Debug, Clone, Copy)]
enum ScreenKey {
    MainMenu { selected: usize },
    Scanning,
    SyncConfig { selected: usize },
    Syncing,
    Duplicate { selected: usize, apply_to_all: bool },
    SyncComplete,
    Config { selected: usize },
    Statistics,
    CollectionConfig { selected: usize },
    CollectionSync,
    CollectionSummary,
    DryRunPreview,
    BackupConfig { selected: usize },
    BackupProgress,
    BackupComplete,
    RestoreConfig { selected: usize },
    RestoreConfirm { selected: usize },
    RestoreProgress,
    RestoreComplete,
    MediaConfig { selected: usize },
    MediaProgress,
    MediaComplete,
    ReplayConfig { selected: usize },
    ReplayProgress,
    ReplayComplete,
    UnifiedConfig,
    UnifiedSetup,
    UnifiedStatus,
}

impl App {
    /// Handle a keyboard event
    pub fn handle_key(&mut self, key: KeyEvent) {
        // Global quit handling
        if event::is_quit(&key) && matches!(self.state, AppState::MainMenu { .. }) {
            self.should_quit = true;
            return;
        }

        // Handle help screen - any key closes it
        if let AppState::Help { previous_state } = &self.state {
            self.state = *previous_state.clone();
            return;
        }

        // Global help key handling (? or h) - available from most screens
        // Skip if we're in filter mode (typing in search box)
        let in_filter_mode = matches!(
            &self.state,
            AppState::DryRunPreview {
                filter_mode: true,
                ..
            }
        );
        if event::is_help(&key) && self.can_show_help() && !in_filter_mode {
            self.show_help();
            return;
        }

        // Copy values needed from state first to avoid borrow conflicts
        let state_key = match &self.state {
            AppState::MainMenu { selected } => Some(ScreenKey::MainMenu {
                selected: *selected,
            }),
            AppState::Scanning { .. } => Some(ScreenKey::Scanning),
            AppState::SyncConfig { selected, .. } => Some(ScreenKey::SyncConfig {
                selected: *selected,
            }),
            AppState::Syncing { .. } => Some(ScreenKey::Syncing),
            AppState::DuplicateDialog {
                selected,
                apply_to_all,
                ..
            } => Some(ScreenKey::Duplicate {
                selected: *selected,
                apply_to_all: *apply_to_all,
            }),
            AppState::SyncComplete { .. } => Some(ScreenKey::SyncComplete),
            AppState::Config { selected, .. } => Some(ScreenKey::Config {
                selected: *selected,
            }),
            AppState::Statistics { .. } => Some(ScreenKey::Statistics),
            AppState::CollectionConfig { selected, .. } => Some(ScreenKey::CollectionConfig {
                selected: *selected,
            }),
            AppState::CollectionSync { .. } => Some(ScreenKey::CollectionSync),
            AppState::CollectionSummary { .. } => Some(ScreenKey::CollectionSummary),
            AppState::DryRunPreview { .. } => Some(ScreenKey::DryRunPreview),
            AppState::BackupConfig { selected, .. } => Some(ScreenKey::BackupConfig {
                selected: *selected,
            }),
            AppState::BackupProgress { .. } => Some(ScreenKey::BackupProgress),
            AppState::BackupComplete { .. } => Some(ScreenKey::BackupComplete),
            AppState::RestoreConfig { selected, .. } => Some(ScreenKey::RestoreConfig {
                selected: *selected,
            }),
            AppState::RestoreConfirm { selected, .. } => Some(ScreenKey::RestoreConfirm {
                selected: *selected,
            }),
            AppState::RestoreProgress { .. } => Some(ScreenKey::RestoreProgress),
            AppState::RestoreComplete { .. } => Some(ScreenKey::RestoreComplete),
            AppState::MediaConfig { selected, .. } => Some(ScreenKey::MediaConfig {
                selected: *selected,
            }),
            AppState::MediaProgress { .. } => Some(ScreenKey::MediaProgress),
            AppState::MediaComplete { .. } => Some(ScreenKey::MediaComplete),
            AppState::ReplayConfig { selected, .. } => Some(ScreenKey::ReplayConfig {
                selected: *selected,
            }),
            AppState::ReplayProgress { .. } => Some(ScreenKey::ReplayProgress),
            AppState::ReplayComplete { .. } => Some(ScreenKey::ReplayComplete),
            AppState::UnifiedConfig { .. } => Some(ScreenKey::UnifiedConfig),
            AppState::UnifiedSetup { .. } => Some(ScreenKey::UnifiedSetup),
            AppState::UnifiedStatus { .. } => Some(ScreenKey::UnifiedStatus),
            AppState::Help { .. } => None, // Already handled above
            AppState::Exiting => None,
        };

        // Delegate to current screen handler
        if let Some(state_key) = state_key {
            match state_key {
                ScreenKey::MainMenu { selected } => self.handle_main_menu_key(key, selected),
                ScreenKey::Scanning => self.handle_scanning_key(key),
                ScreenKey::SyncConfig { selected } => self.handle_sync_config_key(key, selected),
                ScreenKey::Syncing => self.handle_syncing_key(key),
                ScreenKey::Duplicate {
                    selected,
                    apply_to_all,
                } => self.handle_duplicate_dialog_key(key, selected, apply_to_all),
                ScreenKey::SyncComplete => self.handle_sync_complete_key(key),
                ScreenKey::Config { selected } => self.handle_config_key(key, selected),
                ScreenKey::Statistics => self.handle_statistics_key(key),
                ScreenKey::CollectionConfig { selected } => {
                    self.handle_collection_config_key(key, selected)
                }
                ScreenKey::CollectionSync => self.handle_collection_sync_key(key),
                ScreenKey::CollectionSummary => self.handle_collection_summary_key(key),
                ScreenKey::DryRunPreview => self.handle_dry_run_preview_key(key),
                ScreenKey::BackupConfig { selected } => {
                    self.handle_backup_config_key(key, selected)
                }
                ScreenKey::BackupProgress => self.handle_backup_progress_key(key),
                ScreenKey::BackupComplete => self.handle_backup_complete_key(key),
                ScreenKey::RestoreConfig { selected } => {
                    self.handle_restore_config_key(key, selected)
                }
                ScreenKey::RestoreConfirm { selected } => {
                    self.handle_restore_confirm_key(key, selected)
                }
                ScreenKey::RestoreProgress => self.handle_restore_progress_key(key),
                ScreenKey::RestoreComplete => self.handle_restore_complete_key(key),
                ScreenKey::MediaConfig { selected } => self.handle_media_config_key(key, selected),
                ScreenKey::MediaProgress => self.handle_media_progress_key(key),
                ScreenKey::MediaComplete => self.handle_media_complete_key(key),
                ScreenKey::ReplayConfig { selected } => {
                    self.handle_replay_config_key(key, selected)
                }
                ScreenKey::ReplayProgress => self.handle_replay_progress_key(key),
                ScreenKey::ReplayComplete => self.handle_replay_complete_key(key),
                ScreenKey::UnifiedConfig => self.handle_unified_config_key(key),
                ScreenKey::UnifiedSetup => self.handle_unified_setup_key(key),
                ScreenKey::UnifiedStatus => self.handle_unified_status_key(key),
            }
        }
    }
}
