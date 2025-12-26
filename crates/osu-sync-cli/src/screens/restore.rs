//! Restore screen for restoring osu! data backups

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Gauge, List, ListItem};

use crate::app::{PINK, SUBTLE, SUCCESS, WARNING, TEXT};
use osu_sync_core::backup::{BackupInfo, BackupProgress};

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

/// Render confirm restore dialog
pub fn render_confirm(
    frame: &mut Frame,
    area: Rect,
    backup: &BackupInfo,
    dest_path: &str,
    selected: usize,
) {
    // Dim the background
    let dim_style = Style::default().bg(Color::Black);
    let dim = Paragraph::new("").style(dim_style);
    frame.render_widget(dim, area);

    // Dialog box
    let dialog_area = centered_rect(50, 12, area);
    let dialog_block = Block::default()
        .title(Span::styled(" Confirm Restore ", Style::default().fg(WARNING).bold()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(WARNING))
        .border_type(ratatui::widgets::BorderType::Rounded);

    let dialog_inner = dialog_block.inner(dialog_area);
    frame.render_widget(dialog_block, dialog_area);

    // Warning message
    let warning_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Warning
            Constraint::Length(3), // Details
            Constraint::Length(1), // Spacer
            Constraint::Length(2), // Buttons
        ])
        .split(dialog_inner);

    let warning = Paragraph::new(vec![
        Line::from(Span::styled(
            "This will overwrite existing files!",
            Style::default().fg(WARNING),
        )),
    ])
    .alignment(Alignment::Center);
    frame.render_widget(warning, warning_chunks[0]);

    let dest_display = truncate_path(dest_path, 35);
    let details = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Backup: ", Style::default().fg(SUBTLE)),
            Span::styled(backup.target.label(), Style::default().fg(TEXT)),
        ]),
        Line::from(vec![
            Span::styled("To:     ", Style::default().fg(SUBTLE)),
            Span::styled(dest_display, Style::default().fg(TEXT)),
        ]),
    ])
    .alignment(Alignment::Center);
    frame.render_widget(details, warning_chunks[1]);

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
        Span::raw("     "),
        Span::styled(
            if selected == 1 { "[ Restore ]" } else { "  Restore  " },
            if selected == 1 {
                Style::default().fg(WARNING).bold()
            } else {
                Style::default().fg(SUBTLE)
            },
        ),
    ]);

    let buttons = Paragraph::new(button_line).alignment(Alignment::Center);
    frame.render_widget(buttons, warning_chunks[3]);
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
