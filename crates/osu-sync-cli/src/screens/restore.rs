//! Restore screen for restoring osu! data backups

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Gauge, List, ListItem};

use crate::app::{PINK, SUBTLE, SUCCESS, WARNING, TEXT, ERROR};
use osu_sync_core::backup::{
    BackupInfo, BackupProgress, BackupVerificationResult, RestoreMode, RestorePreview,
    VerificationStatus,
};

/// Render the restore screen (backup list)
pub fn render(
    frame: &mut Frame,
    area: Rect,
    backups: &[BackupInfo],
    selected: usize,
    loading: bool,
    status_message: &str,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Min(0),    // Content
            Constraint::Length(2), // Status
        ])
        .split(area);

    // Title
    let title = Paragraph::new(Span::styled(
        "Restore Backup",
        Style::default().fg(PINK).bold(),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    if loading {
        // Loading state
        let loading_text = Paragraph::new(Span::styled(
            "Loading backups...",
            Style::default().fg(SUBTLE),
        ))
        .alignment(Alignment::Center);
        frame.render_widget(loading_text, chunks[1]);
    } else if backups.is_empty() {
        // No backups found
        let no_backups = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "No backups found",
                Style::default().fg(WARNING),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Create a backup first from the Backup screen",
                Style::default().fg(SUBTLE),
            )),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(no_backups, chunks[1]);
    } else {
        // Backup list
        let list_area = centered_rect(65, (backups.len() as u16).min(15) + 4, chunks[1]);
        let list_block = Block::default()
            .title(Span::styled(
                format!(" Available Backups ({}) ", backups.len()),
                Style::default().fg(PINK).bold(),
            ))
            .borders(Borders::ALL)
            .border_style(Style::default().fg(PINK))
            .border_type(ratatui::widgets::BorderType::Rounded);

        let list_inner = list_block.inner(list_area);
        frame.render_widget(list_block, list_area);

        let items: Vec<ListItem> = backups
            .iter()
            .enumerate()
            .map(|(i, backup)| {
                let is_selected = i == selected;

                let prefix = if is_selected { "> " } else { "  " };
                let icon = get_target_icon(&backup.target);
                let target_name = backup.target.label();
                let size = backup.size_display();
                let age = backup.age_display();

                let style = if is_selected {
                    Style::default().fg(Color::White).bold()
                } else {
                    Style::default().fg(TEXT)
                };

                let line = Line::from(vec![
                    Span::styled(prefix, if is_selected { Style::default().fg(PINK) } else { Style::default().fg(SUBTLE) }),
                    Span::styled(format!("{} ", icon), if is_selected { Style::default().fg(PINK) } else { Style::default().fg(SUBTLE) }),
                    Span::styled(format!("{:<25}", target_name), style),
                    Span::styled(format!("{:>10}", size), Style::default().fg(SUBTLE)),
                    Span::styled(format!("  {}", age), Style::default().fg(SUBTLE)),
                ]);

                ListItem::new(line)
            })
            .collect();

        let list = List::new(items);
        frame.render_widget(list, list_inner);
    }

    // Status message
    let status = Paragraph::new(Span::styled(
        status_message,
        Style::default().fg(SUBTLE),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(status, chunks[2]);
}

/// Render restore progress screen
pub fn render_progress(
    frame: &mut Frame,
    area: Rect,
    progress: &BackupProgress,
    backup_name: &str,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Length(5), // Progress info
            Constraint::Length(3), // Progress bar
            Constraint::Min(0),    // Details
        ])
        .split(area);

    // Title
    let title = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("Restoring: {}", backup_name),
            Style::default().fg(PINK).bold(),
        )),
    ])
    .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // Progress info
    let phase_text = format!("{}", progress.phase);
    let files_text = match progress.total_files {
        Some(total) => format!("{} / {} files", progress.files_processed, total),
        None => format!("{} files", progress.files_processed),
    };

    let info = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(&phase_text, Style::default().fg(TEXT))),
        Line::from(Span::styled(&files_text, Style::default().fg(SUBTLE))),
    ])
    .alignment(Alignment::Center);
    frame.render_widget(info, chunks[1]);

    // Progress bar
    let progress_percent = if let Some(total) = progress.total_files {
        if total > 0 {
            (progress.files_processed as f64 / total as f64 * 100.0) as u16
        } else {
            0
        }
    } else {
        0
    };

    let gauge_area = centered_rect(50, 3, chunks[2]);
    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(SUBTLE)))
        .gauge_style(Style::default().fg(PINK))
        .percent(progress_percent);
    frame.render_widget(gauge, gauge_area);

    // Current file
    if let Some(ref current_file) = progress.current_file {
        let truncated = truncate_path(current_file, 60);
        let current = Paragraph::new(Span::styled(
            truncated,
            Style::default().fg(SUBTLE),
        ))
        .alignment(Alignment::Center);
        frame.render_widget(current, chunks[3]);
    }
}

/// Render restore complete screen
pub fn render_complete(
    frame: &mut Frame,
    area: Rect,
    dest_path: &str,
    files_restored: usize,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),  // Title + status
            Constraint::Length(8),  // Results
            Constraint::Min(0),     // Spacer
        ])
        .split(area);

    // Title and status
    let title = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "Restore Complete",
            Style::default().fg(PINK).bold(),
        )),
        Line::from(Span::styled(
            "Backup restored successfully",
            Style::default().fg(SUCCESS),
        )),
    ])
    .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // Results panel
    let results_area = centered_rect(55, 6, chunks[1]);
    let results_block = Block::default()
        .title(" Restore Details ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(SUBTLE));

    let results_inner = results_block.inner(results_area);
    frame.render_widget(results_block, results_area);

    let path_display = truncate_path(dest_path, 45);

    let results = Paragraph::new(vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Restored to: ", Style::default().fg(SUBTLE)),
            Span::styled(path_display, Style::default().fg(TEXT)),
        ]),
        Line::from(vec![
            Span::styled("  Files:       ", Style::default().fg(SUBTLE)),
            Span::styled(format!("{}", files_restored), Style::default().fg(SUCCESS)),
        ]),
    ]);
    frame.render_widget(results, results_inner);
}

/// Render verification status
pub fn render_verification(
    frame: &mut Frame,
    area: Rect,
    backup: &BackupInfo,
    verification: Option<&BackupVerificationResult>,
    verifying: bool,
) {
    // Dim the background
    let dim_style = Style::default().bg(Color::Black);
    let dim = Paragraph::new("").style(dim_style);
    frame.render_widget(dim, area);

    // Dialog box
    let dialog_area = centered_rect(55, 16, area);
    let (title_color, title_text) = if verifying {
        (PINK, " Verifying Backup... ")
    } else if let Some(v) = verification {
        match v.status {
            VerificationStatus::Valid => (SUCCESS, " Backup Verified "),
            VerificationStatus::Warning => (WARNING, " Verification Warning "),
            VerificationStatus::Invalid => (ERROR, " Verification Failed "),
        }
    } else {
        (PINK, " Backup Verification ")
    };

    let dialog_block = Block::default()
        .title(Span::styled(title_text, Style::default().fg(title_color).bold()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(title_color))
        .border_type(ratatui::widgets::BorderType::Rounded);

    let dialog_inner = dialog_block.inner(dialog_area);
    frame.render_widget(dialog_block, dialog_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Backup info
            Constraint::Length(5), // Verification results
            Constraint::Length(4), // Issues
            Constraint::Length(2), // Footer
        ])
        .split(dialog_inner);

    // Backup info
    let info = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("  Backup: ", Style::default().fg(SUBTLE)),
            Span::styled(backup.target.label(), Style::default().fg(TEXT)),
        ]),
        Line::from(vec![
            Span::styled("  Size:   ", Style::default().fg(SUBTLE)),
            Span::styled(backup.size_display(), Style::default().fg(TEXT)),
        ]),
    ]);
    frame.render_widget(info, chunks[0]);

    if verifying {
        let loading = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled("Checking archive integrity...", Style::default().fg(SUBTLE))),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(loading, chunks[1]);
    } else if let Some(v) = verification {
        // Verification results
        let status_color = match v.status {
            VerificationStatus::Valid => SUCCESS,
            VerificationStatus::Warning => WARNING,
            VerificationStatus::Invalid => ERROR,
        };

        let results = Paragraph::new(vec![
            Line::from(vec![
                Span::styled("  Status:      ", Style::default().fg(SUBTLE)),
                Span::styled(format!("{}", v.status), Style::default().fg(status_color)),
            ]),
            Line::from(vec![
                Span::styled("  File count:  ", Style::default().fg(SUBTLE)),
                Span::styled(format!("{}", v.file_count), Style::default().fg(TEXT)),
            ]),
            Line::from(vec![
                Span::styled("  Total size:  ", Style::default().fg(SUBTLE)),
                Span::styled(v.size_display(), Style::default().fg(TEXT)),
            ]),
            Line::from(vec![
                Span::styled("  Restorable:  ", Style::default().fg(SUBTLE)),
                Span::styled(
                    if v.is_restorable() { "Yes" } else { "No" },
                    Style::default().fg(if v.is_restorable() { SUCCESS } else { ERROR }),
                ),
            ]),
        ]);
        frame.render_widget(results, chunks[1]);

        // Issues
        if !v.issues.is_empty() {
            let issue_count = v.issues.len();
            let first_issue = &v.issues[0];
            let issue_text = if issue_count > 1 {
                format!("{} (+{} more)", first_issue.message, issue_count - 1)
            } else {
                first_issue.message.clone()
            };
            let issue_color = match first_issue.severity {
                osu_sync_core::backup::IssueSeverity::Info => SUBTLE,
                osu_sync_core::backup::IssueSeverity::Warning => WARNING,
                osu_sync_core::backup::IssueSeverity::Error => ERROR,
            };

            let issues = Paragraph::new(vec![
                Line::from(""),
                Line::from(Span::styled(
                    truncate_path(&issue_text, 45),
                    Style::default().fg(issue_color),
                )),
            ])
            .alignment(Alignment::Center);
            frame.render_widget(issues, chunks[2]);
        }
    }

    // Footer
    let footer = Paragraph::new(Span::styled(
        "Press Esc to close",
        Style::default().fg(SUBTLE),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(footer, chunks[3]);
}

/// Render restore preview
pub fn render_preview(
    frame: &mut Frame,
    area: Rect,
    backup: &BackupInfo,
    preview: &RestorePreview,
    restore_mode: RestoreMode,
    selected_button: usize,
) {
    // Dim the background
    let dim_style = Style::default().bg(Color::Black);
    let dim = Paragraph::new("").style(dim_style);
    frame.render_widget(dim, area);

    // Dialog box
    let dialog_area = centered_rect(60, 18, area);
    let dialog_block = Block::default()
        .title(Span::styled(" Restore Preview ", Style::default().fg(PINK).bold()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PINK))
        .border_type(ratatui::widgets::BorderType::Rounded);

    let dialog_inner = dialog_block.inner(dialog_area);
    frame.render_widget(dialog_block, dialog_area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Backup info
            Constraint::Length(6), // Preview stats
            Constraint::Length(3), // Mode selector
            Constraint::Length(1), // Spacer
            Constraint::Length(2), // Buttons
            Constraint::Length(2), // Help
        ])
        .split(dialog_inner);

    // Backup info
    let info = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("  Backup: ", Style::default().fg(SUBTLE)),
            Span::styled(backup.target.label(), Style::default().fg(TEXT)),
            Span::styled(format!("  ({})", backup.size_display()), Style::default().fg(SUBTLE)),
        ]),
    ]);
    frame.render_widget(info, chunks[0]);

    // Preview stats
    let new_color = SUCCESS;
    let overwrite_color = if preview.overwrites.is_empty() { SUBTLE } else { WARNING };
    let skip_color = if preview.skipped.is_empty() { SUBTLE } else { TEXT };

    let stats = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("  Files to restore: ", Style::default().fg(SUBTLE)),
            Span::styled(format!("{}", preview.files_to_restore), Style::default().fg(TEXT)),
        ]),
        Line::from(vec![
            Span::styled("  Total size:       ", Style::default().fg(SUBTLE)),
            Span::styled(preview.size_display(), Style::default().fg(TEXT)),
        ]),
        Line::from(vec![
            Span::styled("  New files:        ", Style::default().fg(SUBTLE)),
            Span::styled(format!("{}", preview.new_files.len()), Style::default().fg(new_color)),
        ]),
        Line::from(vec![
            Span::styled("  Will overwrite:   ", Style::default().fg(SUBTLE)),
            Span::styled(format!("{}", preview.overwrites.len()), Style::default().fg(overwrite_color)),
        ]),
        Line::from(vec![
            Span::styled("  Will skip:        ", Style::default().fg(SUBTLE)),
            Span::styled(format!("{}", preview.skipped.len()), Style::default().fg(skip_color)),
        ]),
    ]);
    frame.render_widget(stats, chunks[1]);

    // Mode selector
    let mode_text = format!("  Mode: {} (press M to change)", restore_mode);
    let mode = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(mode_text, Style::default().fg(PINK))),
    ]);
    frame.render_widget(mode, chunks[2]);

    // Buttons
    let button_line = Line::from(vec![
        Span::styled(
            if selected_button == 0 { "[ Cancel ]" } else { "  Cancel  " },
            if selected_button == 0 {
                Style::default().fg(PINK).bold()
            } else {
                Style::default().fg(SUBTLE)
            },
        ),
        Span::raw("     "),
        Span::styled(
            if selected_button == 1 { "[ Restore ]" } else { "  Restore  " },
            if selected_button == 1 {
                Style::default().fg(SUCCESS).bold()
            } else {
                Style::default().fg(SUBTLE)
            },
        ),
    ]);

    let buttons = Paragraph::new(button_line).alignment(Alignment::Center);
    frame.render_widget(buttons, chunks[4]);

    // Help
    let help = Paragraph::new(Span::styled(
        "Left/Right to select, Enter to confirm, V to verify",
        Style::default().fg(SUBTLE),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(help, chunks[5]);
}

/// Render confirm restore dialog (enhanced with options)
pub fn render_confirm(
    frame: &mut Frame,
    area: Rect,
    backup: &BackupInfo,
    dest_path: &str,
    selected: usize,
) {
    render_confirm_with_options(frame, area, backup, dest_path, selected, RestoreMode::Overwrite, None)
}

/// Render confirm restore dialog with restore options
pub fn render_confirm_with_options(
    frame: &mut Frame,
    area: Rect,
    backup: &BackupInfo,
    dest_path: &str,
    selected: usize,
    restore_mode: RestoreMode,
    verification: Option<&BackupVerificationResult>,
) {
    // Dim the background
    let dim_style = Style::default().bg(Color::Black);
    let dim = Paragraph::new("").style(dim_style);
    frame.render_widget(dim, area);

    // Dialog box
    let dialog_height = if verification.is_some() { 16 } else { 14 };
    let dialog_area = centered_rect(55, dialog_height, area);
    let dialog_block = Block::default()
        .title(Span::styled(" Confirm Restore ", Style::default().fg(WARNING).bold()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(WARNING))
        .border_type(ratatui::widgets::BorderType::Rounded);

    let dialog_inner = dialog_block.inner(dialog_area);
    frame.render_widget(dialog_block, dialog_area);

    // Layout
    let constraints = if verification.is_some() {
        vec![
            Constraint::Length(2), // Warning
            Constraint::Length(3), // Details
            Constraint::Length(2), // Verification status
            Constraint::Length(2), // Mode
            Constraint::Length(1), // Spacer
            Constraint::Length(2), // Buttons
            Constraint::Length(2), // Help
        ]
    } else {
        vec![
            Constraint::Length(2), // Warning
            Constraint::Length(3), // Details
            Constraint::Length(2), // Mode
            Constraint::Length(1), // Spacer
            Constraint::Length(2), // Buttons
            Constraint::Length(2), // Help
        ]
    };

    let warning_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(dialog_inner);

    let warning = Paragraph::new(vec![
        Line::from(Span::styled(
            "This will restore files to destination!",
            Style::default().fg(WARNING),
        )),
    ])
    .alignment(Alignment::Center);
    frame.render_widget(warning, warning_chunks[0]);

    let dest_display = truncate_path(dest_path, 40);
    let details = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("  Backup: ", Style::default().fg(SUBTLE)),
            Span::styled(backup.target.label(), Style::default().fg(TEXT)),
        ]),
        Line::from(vec![
            Span::styled("  To:     ", Style::default().fg(SUBTLE)),
            Span::styled(dest_display, Style::default().fg(TEXT)),
        ]),
    ]);
    frame.render_widget(details, warning_chunks[1]);

    let mut idx = 2;

    // Verification status if available
    if let Some(v) = verification {
        let status_color = match v.status {
            VerificationStatus::Valid => SUCCESS,
            VerificationStatus::Warning => WARNING,
            VerificationStatus::Invalid => ERROR,
        };
        let verified = Paragraph::new(vec![
            Line::from(vec![
                Span::styled("  Verified: ", Style::default().fg(SUBTLE)),
                Span::styled(format!("{} ({} files)", v.status, v.file_count), Style::default().fg(status_color)),
            ]),
        ]);
        frame.render_widget(verified, warning_chunks[idx]);
        idx += 1;
    }

    // Mode
    let mode_text = format!("  Mode: {} (M to change)", restore_mode);
    let mode = Paragraph::new(vec![
        Line::from(Span::styled(mode_text, Style::default().fg(PINK))),
    ]);
    frame.render_widget(mode, warning_chunks[idx]);
    idx += 2; // Skip spacer

    // Buttons
    let button_line = Line::from(vec![
        Span::styled(
            if selected == 0 { "[ Cancel ]" } else { "  Cancel  " },
            if selected == 0 {
                Style::default().fg(PINK).bold()
            } else {
                Style::default().fg(SUBTLE)
            },
        ),
        Span::raw("   "),
        Span::styled(
            if selected == 1 { "[ Verify ]" } else { "  Verify  " },
            if selected == 1 {
                Style::default().fg(PINK).bold()
            } else {
                Style::default().fg(SUBTLE)
            },
        ),
        Span::raw("   "),
        Span::styled(
            if selected == 2 { "[ Restore ]" } else { "  Restore  " },
            if selected == 2 {
                Style::default().fg(WARNING).bold()
            } else {
                Style::default().fg(SUBTLE)
            },
        ),
    ]);

    let buttons = Paragraph::new(button_line).alignment(Alignment::Center);
    frame.render_widget(buttons, warning_chunks[idx]);
    idx += 1;

    // Help
    let help = Paragraph::new(Span::styled(
        "Left/Right to select, Enter to confirm",
        Style::default().fg(SUBTLE),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(help, warning_chunks[idx]);
}

/// Get icon for backup target
fn get_target_icon(target: &osu_sync_core::backup::BackupTarget) -> &'static str {
    match target {
        osu_sync_core::backup::BackupTarget::StableSongs => "\u{1F3B5}",      // Musical note
        osu_sync_core::backup::BackupTarget::StableCollections => "\u{1F4C1}", // Folder
        osu_sync_core::backup::BackupTarget::StableScores => "\u{1F3C6}",     // Trophy
        osu_sync_core::backup::BackupTarget::LazerData => "\u{2728}",          // Sparkles
        osu_sync_core::backup::BackupTarget::All => "\u{1F4E6}",              // Package
    }
}

/// Create a centered rect of given size within the parent
fn centered_rect(width: u16, height: u16, parent: Rect) -> Rect {
    let x = parent.x + (parent.width.saturating_sub(width)) / 2;
    let y = parent.y + (parent.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(parent.width), height.min(parent.height))
}

/// Truncate a path to fit within a given width
fn truncate_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        path.to_string()
    } else {
        format!("...{}", &path[path.len() - max_len + 3..])
    }
}
