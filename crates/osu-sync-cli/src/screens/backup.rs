//! Backup screen for creating osu! data backups

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Gauge};

use crate::app::{PINK, SUBTLE, SUCCESS, TEXT};
use osu_sync_core::backup::{BackupProgress, BackupTarget};

/// Menu items for backup selection
const BACKUP_TARGETS: [BackupTarget; 5] = [
    BackupTarget::StableSongs,
    BackupTarget::StableCollections,
    BackupTarget::StableScores,
    BackupTarget::LazerData,
    BackupTarget::All,
];

/// Render the backup screen (target selection)
pub fn render(
    frame: &mut Frame,
    area: Rect,
    selected: usize,
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
        "Create Backup",
        Style::default().fg(PINK).bold(),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // Backup options - centered
    let menu_width = 45;
    let menu_height = (BACKUP_TARGETS.len() * 2 + 3) as u16;
    let menu_area = centered_rect(menu_width, menu_height, chunks[1]);

    let menu_block = Block::default()
        .title(Span::styled(" Select what to backup ", Style::default().fg(PINK).bold()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PINK))
        .border_type(ratatui::widgets::BorderType::Rounded);

    let inner = menu_block.inner(menu_area);
    frame.render_widget(menu_block, menu_area);

    // Render menu items
    let item_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            BACKUP_TARGETS
                .iter()
                .map(|_| Constraint::Length(2))
                .collect::<Vec<_>>(),
        )
        .margin(0)
        .split(inner);

    for (i, (target, item_area)) in BACKUP_TARGETS.iter().zip(item_chunks.iter()).enumerate() {
        let is_selected = i == selected;

        let prefix = if is_selected { "> " } else { "  " };
        let icon = get_target_icon(target);

        let (prefix_style, label_style) = if is_selected {
            (
                Style::default().fg(PINK),
                Style::default().fg(Color::White).bold(),
            )
        } else {
            (
                Style::default().fg(SUBTLE),
                Style::default().fg(TEXT),
            )
        };

        let item_line = Line::from(vec![
            Span::styled(prefix, prefix_style),
            Span::styled(format!("{} ", icon), prefix_style),
            Span::styled(target.label(), label_style),
        ]);

        let item_widget = Paragraph::new(item_line);
        frame.render_widget(item_widget, *item_area);
    }

    // Status message
    let status = Paragraph::new(Span::styled(
        status_message,
        Style::default().fg(SUBTLE),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(status, chunks[2]);
}

/// Render the backup progress screen
pub fn render_progress(
    frame: &mut Frame,
    area: Rect,
    progress: &BackupProgress,
    target: BackupTarget,
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
            format!("Backing up: {}", target.label()),
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

/// Render the backup complete screen
pub fn render_complete(
    frame: &mut Frame,
    area: Rect,
    backup_path: &str,
    size_bytes: u64,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),  // Title + status
            Constraint::Length(10), // Results
            Constraint::Min(0),     // Spacer
        ])
        .split(area);

    // Title and status
    let title = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "Backup Complete",
            Style::default().fg(PINK).bold(),
        )),
        Line::from(Span::styled(
            "Backup created successfully",
            Style::default().fg(SUCCESS),
        )),
    ])
    .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // Results panel
    let results_area = centered_rect(55, 6, chunks[1]);
    let results_block = Block::default()
        .title(" Backup Details ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(SUBTLE));

    let results_inner = results_block.inner(results_area);
    frame.render_widget(results_block, results_area);

    let size_display = format_size(size_bytes);
    let path_display = truncate_path(backup_path, 45);

    let results = Paragraph::new(vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Location: ", Style::default().fg(SUBTLE)),
            Span::styled(path_display, Style::default().fg(TEXT)),
        ]),
        Line::from(vec![
            Span::styled("  Size:     ", Style::default().fg(SUBTLE)),
            Span::styled(size_display, Style::default().fg(SUCCESS)),
        ]),
    ]);
    frame.render_widget(results, results_inner);
}

/// Get icon for backup target
fn get_target_icon(target: &BackupTarget) -> &'static str {
    match target {
        BackupTarget::StableSongs => "\u{1F3B5}",      // Musical note
        BackupTarget::StableCollections => "\u{1F4C1}", // Folder
        BackupTarget::StableScores => "\u{1F3C6}",     // Trophy
        BackupTarget::LazerData => "\u{2728}",          // Sparkles
        BackupTarget::All => "\u{1F4E6}",              // Package
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

/// Format bytes to human-readable size
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
