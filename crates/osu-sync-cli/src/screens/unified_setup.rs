//! Unified storage setup progress screen
//!
//! Displays progress for unified storage migration operations including:
//! - Creating junctions/symlinks
//! - Copying beatmaps
//! - Verifying file integrity

use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph};

use crate::app::{ERROR, PINK, SUBTLE, SUCCESS, TEXT, WARNING};
use crate::widgets::get_spinner_frame;

/// Progress information for migration operations
#[derive(Debug, Clone, Default)]
pub struct MigrationProgress {
    /// Current phase of the migration
    pub phase: MigrationPhase,
    /// Number of items processed in current phase
    pub current: usize,
    /// Total number of items in current phase
    pub total: usize,
    /// Name of the current item being processed
    pub current_item: String,
    /// Bytes processed (for copy operations)
    pub bytes_processed: u64,
    /// Total bytes to process (for copy operations)
    pub bytes_total: u64,
}

impl MigrationProgress {
    /// Calculate the progress ratio (0.0 to 1.0)
    pub fn ratio(&self) -> f64 {
        if self.total > 0 {
            self.current as f64 / self.total as f64
        } else {
            0.0
        }
    }

    /// Calculate the percentage (0 to 100)
    pub fn percentage(&self) -> u16 {
        (self.ratio() * 100.0) as u16
    }
}

/// Phases of the unified storage migration
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MigrationPhase {
    #[default]
    Preparing,
    CreatingJunctions,
    CopyingBeatmaps,
    VerifyingFiles,
    CleaningUp,
    Complete,
}

impl std::fmt::Display for MigrationPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MigrationPhase::Preparing => write!(f, "Preparing..."),
            MigrationPhase::CreatingJunctions => write!(f, "Creating junctions..."),
            MigrationPhase::CopyingBeatmaps => write!(f, "Copying beatmaps..."),
            MigrationPhase::VerifyingFiles => write!(f, "Verifying files..."),
            MigrationPhase::CleaningUp => write!(f, "Cleaning up..."),
            MigrationPhase::Complete => write!(f, "Complete"),
        }
    }
}

/// Log level for entries
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Success,
    Warning,
    Error,
}

/// A single log entry with timestamp and level
#[derive(Debug, Clone)]
pub struct LogEntry {
    /// Timestamp string (e.g., "00:01:23")
    pub timestamp: String,
    /// Log message
    pub message: String,
    /// Severity level
    pub level: LogLevel,
}

impl LogEntry {
    /// Create a new log entry with the current timestamp
    pub fn new(message: impl Into<String>, level: LogLevel, elapsed_secs: u64) -> Self {
        let hours = elapsed_secs / 3600;
        let mins = (elapsed_secs % 3600) / 60;
        let secs = elapsed_secs % 60;
        let timestamp = format!("{:02}:{:02}:{:02}", hours, mins, secs);

        Self {
            timestamp,
            message: message.into(),
            level,
        }
    }
}

/// Actions that can be triggered from the unified setup screen
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenAction {
    /// Cancel the current operation
    Cancel,
    /// Go back to the previous screen
    Back,
    /// No action
    None,
}

/// State for the unified storage setup progress screen
#[derive(Debug, Clone)]
pub struct UnifiedSetupScreen {
    /// Current progress (None if not started)
    pub progress: Option<MigrationProgress>,
    /// Current operation description
    pub current_operation: String,
    /// Log of completed steps
    pub logs: Vec<LogEntry>,
    /// Whether the operation is complete
    pub is_complete: bool,
    /// Error message if the operation failed
    pub error: Option<String>,
    /// Whether the operation can be cancelled
    pub can_cancel: bool,
    /// Elapsed seconds since start (for cloning purposes)
    pub elapsed_seconds: u64,
}

impl Default for UnifiedSetupScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl UnifiedSetupScreen {
    /// Create a new unified setup screen
    pub fn new() -> Self {
        Self {
            progress: None,
            current_operation: String::from("Initializing..."),
            logs: Vec::new(),
            is_complete: false,
            error: None,
            can_cancel: true,
            elapsed_seconds: 0,
        }
    }

    /// Update the elapsed time (call this periodically)
    pub fn tick(&mut self, elapsed_secs: u64) {
        self.elapsed_seconds = elapsed_secs;
    }

    /// Update the current progress
    pub fn update_progress(&mut self, progress: MigrationProgress) {
        self.current_operation = format!("{}", progress.phase);
        self.progress = Some(progress);
    }

    /// Add a log entry
    pub fn add_log(&mut self, message: &str, level: LogLevel) {
        self.logs
            .push(LogEntry::new(message, level, self.elapsed_seconds));
    }

    /// Mark the operation as complete
    pub fn set_complete(&mut self, success: bool, message: Option<String>) {
        self.is_complete = true;
        self.can_cancel = false;

        if success {
            if let Some(ref msg) = message {
                self.add_log(msg, LogLevel::Success);
            } else {
                self.add_log("Unified storage setup complete!", LogLevel::Success);
            }
            self.current_operation = String::from("Setup Complete");
        } else {
            self.error = message.clone();
            if let Some(ref msg) = message {
                self.add_log(msg, LogLevel::Error);
            } else {
                self.add_log("Setup failed", LogLevel::Error);
            }
            self.current_operation = String::from("Setup Failed");
        }
    }

    /// Get elapsed time in seconds
    fn elapsed_secs(&self) -> u64 {
        self.elapsed_seconds
    }

    /// Estimate remaining time based on progress
    fn estimate_remaining(&self) -> Option<u64> {
        let progress = self.progress.as_ref()?;
        if progress.current == 0 {
            return None;
        }

        let elapsed = self.elapsed_secs();
        let ratio = progress.ratio();
        if ratio <= 0.0 || ratio >= 1.0 {
            return None;
        }

        let total_estimated = elapsed as f64 / ratio;
        let remaining = total_estimated - elapsed as f64;
        Some(remaining.max(0.0) as u64)
    }

    /// Handle key input
    pub fn handle_key(&mut self, key: KeyCode) -> Option<ScreenAction> {
        match key {
            KeyCode::Esc => {
                if self.can_cancel && !self.is_complete {
                    Some(ScreenAction::Cancel)
                } else if self.is_complete {
                    Some(ScreenAction::Back)
                } else {
                    None
                }
            }
            KeyCode::Enter if self.is_complete => Some(ScreenAction::Back),
            _ => Some(ScreenAction::None),
        }
    }

    /// Render the screen
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Title
                Constraint::Length(5), // Operation + timing info
                Constraint::Length(3), // Progress bar
                Constraint::Length(3), // Current item
                Constraint::Min(0),    // Log
                Constraint::Length(3), // Status + hint
            ])
            .split(area);

        // Title
        self.render_title(frame, chunks[0]);

        // Operation info with timing
        self.render_operation_info(frame, chunks[1]);

        // Progress bar
        self.render_progress_bar(frame, chunks[2]);

        // Current item
        self.render_current_item(frame, chunks[3]);

        // Log
        self.render_log(frame, chunks[4]);

        // Status and hints
        self.render_status(frame, chunks[5]);
    }

    fn render_title(&self, frame: &mut Frame, area: Rect) {
        let title_text = if self.is_complete {
            if self.error.is_some() {
                "Unified Storage Setup - Failed"
            } else {
                "Unified Storage Setup - Complete"
            }
        } else {
            "Unified Storage Setup"
        };

        let title_color = if self.is_complete {
            if self.error.is_some() {
                ERROR
            } else {
                SUCCESS
            }
        } else {
            PINK
        };

        let title = Paragraph::new(Span::styled(
            title_text,
            Style::default().fg(title_color).bold(),
        ))
        .alignment(Alignment::Center);
        frame.render_widget(title, area);
    }

    fn render_operation_info(&self, frame: &mut Frame, area: Rect) {
        if let Some(ref progress) = self.progress {
            let elapsed = self.elapsed_secs();
            let eta_text = self
                .estimate_remaining()
                .map(format_duration)
                .unwrap_or_else(|| String::from("--:--"));

            let info_lines = vec![
                Line::from(vec![
                    Span::styled("Operation: ", Style::default().fg(SUBTLE)),
                    Span::styled(&self.current_operation, Style::default().fg(TEXT)),
                ]),
                Line::from(vec![
                    Span::styled("Progress: ", Style::default().fg(SUBTLE)),
                    Span::styled(
                        format!("{} / {} items", progress.current, progress.total),
                        Style::default().fg(TEXT),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Elapsed: ", Style::default().fg(SUBTLE)),
                    Span::styled(format_duration(elapsed), Style::default().fg(TEXT)),
                    Span::styled("   ETA: ", Style::default().fg(SUBTLE)),
                    Span::styled(eta_text, Style::default().fg(TEXT)),
                ]),
            ];

            let info = Paragraph::new(info_lines).alignment(Alignment::Center);
            frame.render_widget(info, area);
        } else {
            // Show spinner while preparing
            let spinner = get_spinner_frame();
            let waiting = Paragraph::new(Line::from(vec![
                Span::styled(spinner, Style::default().fg(PINK)),
                Span::styled(
                    format!(" {}", self.current_operation),
                    Style::default().fg(SUBTLE),
                ),
            ]))
            .alignment(Alignment::Center);
            frame.render_widget(waiting, area);
        }
    }

    fn render_progress_bar(&self, frame: &mut Frame, area: Rect) {
        let (ratio, label) = if let Some(ref progress) = self.progress {
            let r = progress.ratio();
            let pct = progress.percentage();
            (r, format!("{}%", pct))
        } else {
            (0.0, String::from("0%"))
        };

        let bar_color = if self.is_complete {
            if self.error.is_some() {
                ERROR
            } else {
                SUCCESS
            }
        } else {
            PINK
        };

        let gauge = Gauge::default()
            .block(Block::default().borders(Borders::NONE))
            .gauge_style(Style::default().fg(bar_color).bg(Color::DarkGray))
            .ratio(ratio)
            .label(label);

        // Center the progress bar horizontally
        let gauge_area = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(10),
                Constraint::Percentage(80),
                Constraint::Percentage(10),
            ])
            .split(area);

        frame.render_widget(gauge, gauge_area[1]);
    }

    fn render_current_item(&self, frame: &mut Frame, area: Rect) {
        let item_text = if let Some(ref progress) = self.progress {
            if progress.current_item.is_empty() {
                String::new()
            } else {
                truncate(&progress.current_item, 60)
            }
        } else {
            String::new()
        };

        let current = Paragraph::new(Span::styled(item_text, Style::default().fg(SUBTLE)))
            .alignment(Alignment::Center);
        frame.render_widget(current, area);
    }

    fn render_log(&self, frame: &mut Frame, area: Rect) {
        let log_items: Vec<ListItem> = self
            .logs
            .iter()
            .rev()
            .take(15)
            .map(|entry| {
                let level_style = match entry.level {
                    LogLevel::Info => Style::default().fg(TEXT),
                    LogLevel::Success => Style::default().fg(SUCCESS),
                    LogLevel::Warning => Style::default().fg(WARNING),
                    LogLevel::Error => Style::default().fg(ERROR),
                };

                let line = Line::from(vec![
                    Span::styled(
                        format!("[{}] ", entry.timestamp),
                        Style::default().fg(SUBTLE),
                    ),
                    Span::styled(&entry.message, level_style),
                ]);

                ListItem::new(line)
            })
            .collect();

        let log = List::new(log_items).block(
            Block::default()
                .title(" Log ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(SUBTLE)),
        );

        frame.render_widget(log, area);
    }

    fn render_status(&self, frame: &mut Frame, area: Rect) {
        let status_line = if let Some(ref error) = self.error {
            Line::from(vec![
                Span::styled("Error: ", Style::default().fg(ERROR)),
                Span::styled(truncate(error, 50), Style::default().fg(ERROR)),
            ])
        } else if self.is_complete {
            Line::from(Span::styled(
                "Setup completed successfully!",
                Style::default().fg(SUCCESS),
            ))
        } else if let Some(ref progress) = self.progress {
            // Show bytes progress if available
            if progress.bytes_total > 0 {
                let bytes_pct = if progress.bytes_total > 0 {
                    (progress.bytes_processed as f64 / progress.bytes_total as f64 * 100.0) as u32
                } else {
                    0
                };
                Line::from(vec![
                    Span::styled("Data: ", Style::default().fg(SUBTLE)),
                    Span::styled(
                        format!(
                            "{} / {} ({}%)",
                            format_bytes(progress.bytes_processed),
                            format_bytes(progress.bytes_total),
                            bytes_pct
                        ),
                        Style::default().fg(TEXT),
                    ),
                ])
            } else {
                Line::from(Span::styled("", Style::default().fg(SUBTLE)))
            }
        } else {
            Line::from(Span::styled("", Style::default().fg(SUBTLE)))
        };

        let hint_text = if self.is_complete {
            "Press Enter or Esc to continue"
        } else if self.can_cancel {
            "Press Esc to cancel"
        } else {
            "Please wait..."
        };

        let hint_line = Line::from(Span::styled(hint_text, Style::default().fg(SUBTLE)));

        let status_widget =
            Paragraph::new(vec![status_line, hint_line]).alignment(Alignment::Center);
        frame.render_widget(status_widget, area);
    }
}

/// Format seconds as "Xm Ys" or "Xh Ym" for display
fn format_duration(seconds: u64) -> String {
    if seconds < 60 {
        format!("{}s", seconds)
    } else if seconds < 3600 {
        let mins = seconds / 60;
        let secs = seconds % 60;
        format!("{}m {}s", mins, secs)
    } else {
        let hours = seconds / 3600;
        let mins = (seconds % 3600) / 60;
        format!("{}h {}m", hours, mins)
    }
}

/// Truncate a string to fit within a given width
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

/// Format bytes to human-readable size
fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
