//! Backup screen for creating osu! data backups

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};

use crate::app::{PINK, SUBTLE, SUCCESS, TEXT};
use osu_sync_core::backup::{BackupMode, BackupProgress, BackupTarget, CompressionLevel};

/// Menu items for backup selection
const BACKUP_TARGETS: [BackupTarget; 5] = [
    BackupTarget::StableSongs,
    BackupTarget::StableCollections,
    BackupTarget::StableScores,
    BackupTarget::LazerData,
    BackupTarget::All,
];

/// Backup options state for UI
#[derive(Debug, Clone)]
pub struct BackupUIState {
    /// Selected backup target index
    pub selected_target: usize,
    /// Current compression level
    pub compression: CompressionLevel,
    /// Current backup mode
    pub mode: BackupMode,
    /// Which option is focused (0 = target, 1 = compression, 2 = mode)
    pub focused_option: usize,
}

impl Default for BackupUIState {
    fn default() -> Self {
        Self {
            selected_target: 0,
            compression: CompressionLevel::Normal,
            mode: BackupMode::Full,
            focused_option: 0,
        }
    }
}

impl BackupUIState {
    /// Move focus to next option
    pub fn focus_next(&mut self) {
        self.focused_option = (self.focused_option + 1) % 3;
    }

    /// Move focus to previous option
    pub fn focus_prev(&mut self) {
        self.focused_option = (self.focused_option + 2) % 3;
    }

    /// Handle up arrow
    pub fn handle_up(&mut self) {
        match self.focused_option {
            0 => {
                if self.selected_target > 0 {
                    self.selected_target -= 1;
                }
            }
            _ => self.focus_prev(),
        }
    }

    /// Handle down arrow
    pub fn handle_down(&mut self) {
        match self.focused_option {
            0 => {
                if self.selected_target < BACKUP_TARGETS.len() - 1 {
                    self.selected_target += 1;
                }
            }
            _ => self.focus_next(),
        }
    }

    /// Handle left/right arrow to cycle options
    pub fn cycle_option(&mut self) {
        match self.focused_option {
            1 => self.compression = self.compression.next(),
            2 => self.mode = self.mode.toggle(),
            _ => {}
        }
    }

    /// Get the selected target
    pub fn selected_target(&self) -> BackupTarget {
        BACKUP_TARGETS[self.selected_target]
    }
}

/// Render the backup screen (target selection) - legacy version
pub fn render(
    frame: &mut Frame,
    area: Rect,
    selected: usize,
    status_message: &str,
) {
    let state = BackupUIState {
        selected_target: selected,
        ..Default::default()
    };
    render_with_state(frame, area, &state, status_message);
}

/// Render the backup screen with full state
pub fn render_with_state(
    frame: &mut Frame,
    area: Rect,
    state: &BackupUIState,
    status_message: &str,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Title
            Constraint::Min(0),     // Content (targets + options)
            Constraint::Length(2),  // Status
        ])
        .split(area);

    // Title
    let title = Paragraph::new(Span::styled(
        "Create Backup",
        Style::default().fg(PINK).bold(),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // Content area - split into targets and options
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(55), // Targets
            Constraint::Percentage(45), // Options
        ])
        .split(chunks[1]);

    // Backup targets - left side
    let targets_height = (BACKUP_TARGETS.len() * 2 + 2) as u16;
    let targets_area = centered_rect(42, targets_height.min(content_chunks[0].height), content_chunks[0]);

    let targets_border_style = if state.focused_option == 0 {
        Style::default().fg(PINK)
    } else {
        Style::default().fg(SUBTLE)
    };

    let targets_block = Block::default()
        .title(Span::styled(" Select Target ", targets_border_style.bold()))
        .borders(Borders::ALL)
        .border_style(targets_border_style)
        .border_type(ratatui::widgets::BorderType::Rounded);

    let targets_inner = targets_block.inner(targets_area);
    frame.render_widget(targets_block, targets_area);

    // Render target menu items
    let item_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            BACKUP_TARGETS
                .iter()
                .map(|_| Constraint::Length(2))
                .collect::<Vec<_>>(),
        )
        .margin(0)
        .split(targets_inner);

    for (i, (target, item_area)) in BACKUP_TARGETS.iter().zip(item_chunks.iter()).enumerate() {
        let is_selected = i == state.selected_target;

        let prefix = if is_selected { "> " } else { "  " };
        let icon = get_target_icon(target);

        let (prefix_style, label_style) = if is_selected && state.focused_option == 0 {
            (
                Style::default().fg(PINK),
                Style::default().fg(Color::White).bold(),
            )
        } else if is_selected {
            (
                Style::default().fg(SUBTLE),
                Style::default().fg(TEXT).bold(),
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

    // Options panel - right side
    let options_area = centered_rect(35, 8, content_chunks[1]);

    let options_block = Block::default()
        .title(Span::styled(" Options ", Style::default().fg(SUBTLE).bold()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(SUBTLE))
        .border_type(ratatui::widgets::BorderType::Rounded);

    let options_inner = options_block.inner(options_area);
    frame.render_widget(options_block, options_area);

    // Render options
    let option_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Compression
            Constraint::Length(1), // Spacer
            Constraint::Length(2), // Mode
        ])
        .split(options_inner);

    // Compression level option
    let compression_focused = state.focused_option == 1;
    let compression_style = if compression_focused {
        Style::default().fg(PINK).bold()
    } else {
        Style::default().fg(TEXT)
    };
    let compression_prefix = if compression_focused { "> " } else { "  " };

    let compression_line = Line::from(vec![
        Span::styled(compression_prefix, compression_style),
        Span::styled("Compression: ", Style::default().fg(SUBTLE)),
        Span::styled(
            format!("[{}]", state.compression.short_label()),
            compression_style,
        ),
    ]);
    frame.render_widget(Paragraph::new(compression_line), option_chunks[0]);

    // Mode option
    let mode_focused = state.focused_option == 2;
    let mode_style = if mode_focused {
        Style::default().fg(PINK).bold()
    } else {
        Style::default().fg(TEXT)
    };
    let mode_prefix = if mode_focused { "> " } else { "  " };

    let mode_line = Line::from(vec![
        Span::styled(mode_prefix, mode_style),
        Span::styled("Mode: ", Style::default().fg(SUBTLE)),
        Span::styled(
            format!("[{}]", state.mode.short_label()),
            mode_style,
        ),
    ]);
    frame.render_widget(Paragraph::new(mode_line), option_chunks[2]);

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
    render_complete_with_type(frame, area, backup_path, size_bytes, false);
}

/// Render the backup complete screen with backup type info
pub fn render_complete_with_type(
    frame: &mut Frame,
    area: Rect,
    backup_path: &str,
    size_bytes: u64,
    is_incremental: bool,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),  // Title + status
            Constraint::Length(12), // Results
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
    let results_area = centered_rect(55, 8, chunks[1]);
    let results_block = Block::default()
        .title(" Backup Details ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(SUBTLE));

    let results_inner = results_block.inner(results_area);
    frame.render_widget(results_block, results_area);

    let size_display = format_size(size_bytes);
    let path_display = truncate_path(backup_path, 45);
    let type_display = if is_incremental { "Incremental" } else { "Full" };

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
        Line::from(vec![
            Span::styled("  Type:     ", Style::default().fg(SUBTLE)),
            Span::styled(type_display, Style::default().fg(TEXT)),
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
