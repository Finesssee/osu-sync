//! Screen rendering and routing

mod activity_log;
mod backup;
mod collection_config;
mod collection_summary;
mod collection_sync;
mod config;
pub mod dry_run_preview;
mod duplicate_dialog;
mod help;
mod main_menu;
mod media;
mod replay;
mod restore;
mod scan;
mod statistics;
mod sync_config;
mod sync_progress;
mod sync_summary;
pub mod unified_config;
pub mod unified_setup;
pub mod unified_status;

use crate::app::{App, AppState};
use crate::widgets;
use ratatui::prelude::*;

/// Render the current screen based on app state
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Create main layout with header and footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Content
            Constraint::Length(3), // Footer
        ])
        .split(area);

    // Render header
    widgets::render_header(frame, chunks[0]);

    // Render footer based on current state
    let hints = get_hints(&app.state);
    widgets::render_footer(frame, chunks[2], &hints);

    // Render main content based on state
    match &app.state {
        AppState::MainMenu { selected } => {
            main_menu::render(frame, chunks[1], *selected, app);
        }
        AppState::Scanning {
            in_progress,
            stable_result,
            lazer_result,
            status_message,
        } => {
            scan::render(
                frame,
                chunks[1],
                *in_progress,
                stable_result,
                lazer_result,
                status_message,
            );
        }
        AppState::SyncConfig {
            selected,
            stable_count,
            lazer_count,
            filter,
            filter_panel_open,
            filter_field,
        } => {
            sync_config::render(
                frame,
                chunks[1],
                *selected,
                *stable_count,
                *lazer_count,
                filter,
                *filter_panel_open,
                *filter_field,
            );
        }
        AppState::Syncing {
            progress,
            logs,
            stats,
            is_paused,
        } => {
            sync_progress::render(frame, chunks[1], progress, logs, stats, *is_paused);
        }
        AppState::DuplicateDialog {
            info,
            selected,
            apply_to_all,
        } => {
            // Render dimmed syncing screen behind
            sync_progress::render(frame, chunks[1], &None, &[], &Default::default(), false);
            // Render modal on top
            duplicate_dialog::render(frame, area, info, *selected, *apply_to_all);
        }
        AppState::SyncComplete { result } => {
            sync_summary::render(frame, chunks[1], result);
        }
        AppState::Config {
            selected,
            stable_path,
            lazer_path,
            status_message,
            editing,
        } => {
            config::render(
                frame,
                chunks[1],
                *selected,
                stable_path,
                lazer_path,
                status_message,
                editing.as_deref(),
            );
        }
        AppState::Statistics {
            stats,
            loading,
            tab,
            status_message,
            export_state,
        } => {
            statistics::render(
                frame,
                chunks[1],
                stats,
                *loading,
                *tab,
                status_message,
                export_state,
            );
        }
        AppState::CollectionConfig {
            collections,
            selected,
            strategy,
            loading,
            status_message,
        } => {
            collection_config::render(
                frame,
                chunks[1],
                collections,
                *selected,
                *strategy,
                *loading,
                status_message,
            );
        }
        AppState::CollectionSync {
            progress,
            current_collection,
            logs,
        } => {
            collection_sync::render(frame, chunks[1], *progress, current_collection, logs);
        }
        AppState::CollectionSummary { result } => {
            collection_summary::render(frame, chunks[1], result);
        }
        AppState::DryRunPreview {
            result,
            direction,
            selected_item,
            scroll_offset,
            checked_items,
            filter_text,
            filter_mode,
        } => {
            dry_run_preview::render(
                frame,
                chunks[1],
                result,
                *direction,
                *selected_item,
                *scroll_offset,
                checked_items,
                filter_text,
                *filter_mode,
            );
        }
        AppState::BackupConfig {
            selected,
            status_message,
        } => {
            backup::render(frame, chunks[1], *selected, status_message);
        }
        AppState::BackupProgress { target, progress } => {
            backup::render_progress(frame, chunks[1], progress, *target);
        }
        AppState::BackupComplete {
            backup_path,
            size_bytes,
            is_incremental,
        } => {
            backup::render_complete_with_type(
                frame,
                chunks[1],
                &backup_path.display().to_string(),
                *size_bytes,
                *is_incremental,
            );
        }
        AppState::RestoreConfig {
            backups,
            selected,
            loading,
            status_message,
        } => {
            restore::render(
                frame,
                chunks[1],
                backups,
                *selected,
                *loading,
                status_message,
            );
        }
        AppState::RestoreConfirm {
            backup,
            dest_path,
            selected,
        } => {
            restore::render_confirm(
                frame,
                area,
                backup,
                &dest_path.display().to_string(),
                *selected,
            );
        }
        AppState::RestoreProgress {
            backup_name,
            progress,
        } => {
            restore::render_progress(frame, chunks[1], progress, backup_name);
        }
        AppState::RestoreComplete {
            dest_path,
            files_restored,
        } => {
            restore::render_complete(
                frame,
                chunks[1],
                &dest_path.display().to_string(),
                *files_restored,
            );
        }
        AppState::MediaConfig {
            selected,
            media_type,
            organization,
            output_path,
            skip_duplicates,
            include_metadata,
            status_message,
            ..
        } => {
            media::render_config(
                frame,
                chunks[1],
                *selected,
                *media_type,
                *organization,
                output_path,
                *skip_duplicates,
                *include_metadata,
                status_message,
            );
        }
        AppState::MediaProgress {
            progress,
            current_set,
        } => {
            media::render_progress(frame, chunks[1], progress, current_set);
        }
        AppState::MediaComplete { result } => {
            media::render_complete(frame, chunks[1], result);
        }
        AppState::ReplayConfig {
            selected,
            organization,
            output_path,
            replays,
            loading: _,
            status_message,
            filter,
            rename_pattern,
            filter_panel_open,
            filter_field,
        } => {
            let exportable = replays.iter().filter(|r| r.has_replay_file).count();
            replay::render_config(
                frame,
                chunks[1],
                *selected,
                *organization,
                output_path,
                exportable,
                status_message,
                filter,
                rename_pattern,
                *filter_panel_open,
                *filter_field,
            );
        }
        AppState::ReplayProgress {
            progress,
            current_replay,
        } => {
            replay::render_progress(frame, chunks[1], progress, current_replay);
        }
        AppState::ReplayComplete { result, stats } => {
            replay::render_complete(frame, chunks[1], result, stats);
        }
        AppState::UnifiedConfig { screen } => {
            unified_config::render(frame, chunks[1], screen);
        }
        AppState::UnifiedSetup { screen } => {
            screen.render(frame, chunks[1]);
        }
        AppState::UnifiedStatus { screen } => {
            unified_status::render(frame, chunks[1], screen);
        }
        AppState::Help { previous_state } => {
            // Render the previous screen behind the help modal
            render_state(frame, chunks[1], previous_state, app);
            // Render help modal on top
            help::render(frame, area);
        }
        AppState::Exiting => {}
    }
}

/// Render a specific state (used for rendering previous state behind modals)
fn render_state(frame: &mut Frame, area: Rect, state: &AppState, app: &App) {
    match state {
        AppState::MainMenu { selected } => {
            main_menu::render(frame, area, *selected, app);
        }
        AppState::Scanning {
            in_progress,
            stable_result,
            lazer_result,
            status_message,
        } => {
            scan::render(
                frame,
                area,
                *in_progress,
                stable_result,
                lazer_result,
                status_message,
            );
        }
        AppState::SyncConfig {
            selected,
            stable_count,
            lazer_count,
            filter,
            filter_panel_open,
            filter_field,
        } => {
            sync_config::render(
                frame,
                area,
                *selected,
                *stable_count,
                *lazer_count,
                filter,
                *filter_panel_open,
                *filter_field,
            );
        }
        AppState::Syncing {
            progress,
            logs,
            stats,
            is_paused,
        } => {
            sync_progress::render(frame, area, progress, logs, stats, *is_paused);
        }
        AppState::DuplicateDialog { .. } => {
            // Just render empty syncing screen behind duplicates
            sync_progress::render(frame, area, &None, &[], &Default::default(), false);
        }
        AppState::SyncComplete { result } => {
            sync_summary::render(frame, area, result);
        }
        AppState::Config {
            selected,
            stable_path,
            lazer_path,
            status_message,
            editing,
        } => {
            config::render(
                frame,
                area,
                *selected,
                stable_path,
                lazer_path,
                status_message,
                editing.as_deref(),
            );
        }
        AppState::Statistics {
            stats,
            loading,
            tab,
            status_message,
            export_state,
        } => {
            statistics::render(
                frame,
                area,
                stats,
                *loading,
                *tab,
                status_message,
                export_state,
            );
        }
        AppState::CollectionConfig {
            collections,
            selected,
            strategy,
            loading,
            status_message,
        } => {
            collection_config::render(
                frame,
                area,
                collections,
                *selected,
                *strategy,
                *loading,
                status_message,
            );
        }
        AppState::CollectionSync {
            progress,
            current_collection,
            logs,
        } => {
            collection_sync::render(frame, area, *progress, current_collection, logs);
        }
        AppState::CollectionSummary { result } => {
            collection_summary::render(frame, area, result);
        }
        AppState::DryRunPreview {
            result,
            direction,
            selected_item,
            scroll_offset,
            checked_items,
            filter_text,
            filter_mode,
        } => {
            dry_run_preview::render(
                frame,
                area,
                result,
                *direction,
                *selected_item,
                *scroll_offset,
                checked_items,
                filter_text,
                *filter_mode,
            );
        }
        AppState::BackupConfig {
            selected,
            status_message,
        } => {
            backup::render(frame, area, *selected, status_message);
        }
        AppState::BackupProgress { target, progress } => {
            backup::render_progress(frame, area, progress, *target);
        }
        AppState::BackupComplete {
            backup_path,
            size_bytes,
            is_incremental,
        } => {
            backup::render_complete_with_type(
                frame,
                area,
                &backup_path.display().to_string(),
                *size_bytes,
                *is_incremental,
            );
        }
        AppState::RestoreConfig {
            backups,
            selected,
            loading,
            status_message,
        } => {
            restore::render(frame, area, backups, *selected, *loading, status_message);
        }
        AppState::RestoreConfirm {
            backup,
            dest_path,
            selected,
        } => {
            restore::render_confirm(
                frame,
                area,
                backup,
                &dest_path.display().to_string(),
                *selected,
            );
        }
        AppState::RestoreProgress {
            backup_name,
            progress,
        } => {
            restore::render_progress(frame, area, progress, backup_name);
        }
        AppState::RestoreComplete {
            dest_path,
            files_restored,
        } => {
            restore::render_complete(
                frame,
                area,
                &dest_path.display().to_string(),
                *files_restored,
            );
        }
        AppState::MediaConfig {
            selected,
            media_type,
            organization,
            output_path,
            skip_duplicates,
            include_metadata,
            status_message,
            ..
        } => {
            media::render_config(
                frame,
                area,
                *selected,
                *media_type,
                *organization,
                output_path,
                *skip_duplicates,
                *include_metadata,
                status_message,
            );
        }
        AppState::MediaProgress {
            progress,
            current_set,
        } => {
            media::render_progress(frame, area, progress, current_set);
        }
        AppState::MediaComplete { result } => {
            media::render_complete(frame, area, result);
        }
        AppState::ReplayConfig {
            selected,
            organization,
            output_path,
            replays,
            loading: _,
            status_message,
            filter,
            rename_pattern,
            filter_panel_open,
            filter_field,
        } => {
            let exportable = replays.iter().filter(|r| r.has_replay_file).count();
            replay::render_config(
                frame,
                area,
                *selected,
                *organization,
                output_path,
                exportable,
                status_message,
                filter,
                rename_pattern,
                *filter_panel_open,
                *filter_field,
            );
        }
        AppState::ReplayProgress {
            progress,
            current_replay,
        } => {
            replay::render_progress(frame, area, progress, current_replay);
        }
        AppState::ReplayComplete { result, stats } => {
            replay::render_complete(frame, area, result, stats);
        }
        AppState::UnifiedConfig { screen } => {
            unified_config::render(frame, area, screen);
        }
        AppState::UnifiedSetup { screen } => {
            screen.render(frame, area);
        }
        AppState::UnifiedStatus { screen } => {
            unified_status::render(frame, area, screen);
        }
        AppState::Help { previous_state } => {
            // Recursively render the previous state
            render_state(frame, area, previous_state, app);
        }
        AppState::Exiting => {}
    }
}

/// Get keyboard hints for the current state
fn get_hints(state: &AppState) -> Vec<(&'static str, &'static str)> {
    match state {
        AppState::MainMenu { .. } => vec![("Enter", "Select"), ("j/k", "Navigate"), ("q", "Quit")],
        AppState::Scanning {
            in_progress: true, ..
        } => vec![("Esc", "Cancel")],
        AppState::Scanning {
            in_progress: false, ..
        } => vec![("Enter", "Continue"), ("r", "Rescan"), ("Esc", "Back")],
        AppState::SyncConfig {
            filter_panel_open: true,
            ..
        } => vec![("Space", "Toggle"), ("j/k", "Navigate"), ("Esc", "Close")],
        AppState::SyncConfig {
            filter_panel_open: false,
            ..
        } => vec![
            ("Enter", "Start Sync"),
            ("d", "Dry Run"),
            ("f", "Filters"),
            ("j/k", "Navigate"),
            ("Esc", "Back"),
        ],
        AppState::Syncing { is_paused, .. } => {
            if *is_paused {
                vec![("Space", "Resume"), ("Esc", "Cancel")]
            } else {
                vec![("Space", "Pause"), ("Esc", "Cancel")]
            }
        }
        AppState::DuplicateDialog { .. } => vec![
            ("Enter", "Confirm"),
            ("Space", "Toggle Apply All"),
            ("j/k", "Navigate"),
        ],
        AppState::SyncComplete { .. } => vec![("Enter", "Back to Menu")],
        AppState::Config {
            editing: Some(_), ..
        } => vec![("Enter", "Save"), ("Esc", "Cancel")],
        AppState::Config { editing: None, .. } => {
            vec![("Enter", "Edit"), ("d", "Auto-detect"), ("Esc", "Back")]
        }
        AppState::Statistics { loading: true, .. } => vec![("Esc", "Cancel")],
        AppState::Statistics {
            loading: false,
            export_state,
            ..
        } => {
            if export_state.dialog_open {
                vec![("Enter", "Export"), ("j/k", "Select"), ("Esc", "Cancel")]
            } else {
                vec![("Tab", "Next Tab"), ("e", "Export"), ("Esc", "Back")]
            }
        }
        AppState::CollectionConfig { loading: true, .. } => vec![("Esc", "Cancel")],
        AppState::CollectionConfig { loading: false, .. } => vec![
            ("Enter", "Sync / Toggle"),
            ("j/k", "Navigate"),
            ("Esc", "Back"),
        ],
        AppState::CollectionSync { .. } => vec![("Esc", "Cancel")],
        AppState::CollectionSummary { .. } => vec![("Enter", "Back to Menu")],
        AppState::DryRunPreview { result, filter_mode, checked_items, .. } => {
            if *filter_mode {
                vec![
                    ("Enter", "Apply"),
                    ("Esc", "Clear"),
                    ("", "Type to search..."),
                ]
            } else if result.has_imports() {
                if checked_items.is_empty() {
                    vec![
                        ("Enter", "Sync Current"),
                        ("Space", "Toggle"),
                        ("/", "Search"),
                        ("Ctrl+A", "Select All"),
                        ("Esc", "Back"),
                    ]
                } else {
                    vec![
                        ("Enter", "Sync Selected"),
                        ("Space", "Toggle"),
                        ("/", "Search"),
                        ("Ctrl+D", "Clear"),
                        ("Esc", "Back"),
                    ]
                }
            } else {
                vec![("Enter/Esc", "Back")]
            }
        }
        AppState::BackupConfig { .. } => vec![
            ("Enter", "Start Backup"),
            ("j/k", "Navigate"),
            ("Esc", "Back"),
        ],
        AppState::BackupProgress { .. } => vec![("Esc", "Cancel")],
        AppState::BackupComplete { .. } => vec![("Enter", "Back to Menu")],
        AppState::RestoreConfig { loading: true, .. } => vec![("Esc", "Cancel")],
        AppState::RestoreConfig { loading: false, .. } => {
            vec![("Enter", "Select"), ("j/k", "Navigate"), ("Esc", "Back")]
        }
        AppState::RestoreConfirm { .. } => vec![
            ("Enter", "Confirm"),
            ("Left/Right", "Select"),
            ("Esc", "Cancel"),
        ],
        AppState::RestoreProgress { .. } => vec![("Esc", "Cancel")],
        AppState::RestoreComplete { .. } => vec![("Enter", "Back to Menu")],
        AppState::MediaConfig { .. } => vec![
            ("Enter", "Toggle/Start"),
            ("j/k", "Navigate"),
            ("Esc", "Back"),
        ],
        AppState::MediaProgress { .. } => vec![("Esc", "Cancel")],
        AppState::MediaComplete { .. } => vec![("Enter", "Back to Menu")],
        AppState::ReplayConfig { loading: true, .. } => vec![("Esc", "Cancel")],
        AppState::ReplayConfig {
            loading: false,
            filter_panel_open: true,
            ..
        } => vec![("Space", "Toggle"), ("j/k", "Navigate"), ("Esc", "Close")],
        AppState::ReplayConfig {
            loading: false,
            filter_panel_open: false,
            ..
        } => vec![
            ("Enter", "Toggle/Start"),
            ("j/k", "Navigate"),
            ("Esc", "Back"),
        ],
        AppState::ReplayProgress { .. } => vec![("Esc", "Cancel")],
        AppState::ReplayComplete { .. } => vec![("Enter", "Back to Menu")],
        AppState::UnifiedConfig { .. } => vec![
            ("Enter", "Select/Toggle"),
            ("Tab", "Next Section"),
            ("j/k", "Navigate"),
            ("Esc", "Back"),
        ],
        AppState::UnifiedSetup { .. } => vec![("Esc", "Cancel")],
        AppState::UnifiedStatus { .. } => vec![
            ("Enter", "Action"),
            ("←/→", "Navigate"),
            ("Esc", "Back"),
        ],
        AppState::Help { .. } => vec![("Any key", "Close")],
        AppState::Exiting => vec![],
    }
}
