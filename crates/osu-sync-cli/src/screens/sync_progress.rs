//! Sync progress screen

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph};

use crate::app::{LogEntry, LogLevel, SyncStats, ERROR, PINK, SUBTLE, SUCCESS, TEXT, WARNING};
use crate::widgets::get_spinner_frame;
use osu_sync_core::sync::SyncProgress;

/// Format seconds as "Xm Ys" or "Xh Ym" for display
#[allow(dead_code)]
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

pub fn render(
    frame: &mut Frame,
    area: Rect,
    progress: &Option<SyncProgress>,
    logs: &[LogEntry],
    stats: &SyncStats,
    is_paused: bool,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Length(6), // Progress info (increased for ETA)
            Constraint::Length(3), // Progress bar
            Constraint::Length(3), // Current item
            Constraint::Min(0),    // Log
            Constraint::Length(3), // Stats + hint
        ])
        .split(area);

    // Title - show "PAUSED" if paused
    let title_text = if is_paused {
        "Syncing Beatmaps - PAUSED"
    } else {
        "Syncing Beatmaps"
    };
    let title_color = if is_paused { WARNING } else { PINK };
    let title = Paragraph::new(Span::styled(
        title_text,
        Style::default().fg(title_color).bold(),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // Progress info
    if let Some(prog) = progress {
        let info_lines = vec![
            Line::from(vec![
                Span::styled("Phase: ", Style::default().fg(SUBTLE)),
                Span::styled(format!("{}", prog.phase), Style::default().fg(TEXT)),
            ]),
            Line::from(vec![
                Span::styled("Progress: ", Style::default().fg(SUBTLE)),
                Span::styled(
                    format!("{} / {}", prog.current, prog.total),
                    Style::default().fg(TEXT),
                ),
            ]),
        ];

        let info = Paragraph::new(info_lines).alignment(Alignment::Center);
        frame.render_widget(info, chunks[1]);

        // Progress bar
        let ratio = if prog.total > 0 {
            prog.current as f64 / prog.total as f64
        } else {
            0.0
        };

        let bar_color = if is_paused { WARNING } else { PINK };
        let gauge = Gauge::default()
            .block(Block::default().borders(Borders::NONE))
            .gauge_style(Style::default().fg(bar_color).bg(Color::DarkGray))
            .ratio(ratio)
            .label(format!("{}%", (ratio * 100.0) as u16));

        let gauge_area = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(10),
                Constraint::Percentage(80),
                Constraint::Percentage(10),
            ])
            .split(chunks[2]);

        frame.render_widget(gauge, gauge_area[1]);

        // Current item
        let current = Paragraph::new(Span::styled(
            truncate(&prog.current_name, 60),
            Style::default().fg(SUBTLE),
        ))
        .alignment(Alignment::Center);
        frame.render_widget(current, chunks[3]);
    } else {
        let spinner = get_spinner_frame();
        let waiting = Paragraph::new(Line::from(vec![
            Span::styled(spinner, Style::default().fg(PINK)),
            Span::styled(" Preparing sync operation...", Style::default().fg(SUBTLE)),
        ]))
        .alignment(Alignment::Center);
        frame.render_widget(waiting, chunks[1]);
    }

    // Log
    let log_items: Vec<ListItem> = logs
        .iter()
        .rev()
        .take(10)
        .map(|entry| {
            let style = match entry.level {
                LogLevel::Info => Style::default().fg(TEXT),
                LogLevel::Success => Style::default().fg(SUCCESS),
                LogLevel::Warning => Style::default().fg(WARNING),
                LogLevel::Error => Style::default().fg(ERROR),
            };
            ListItem::new(Span::styled(&entry.message, style))
        })
        .collect();

    let log = List::new(log_items).block(
        Block::default()
            .title(" Log ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(SUBTLE)),
    );

    frame.render_widget(log, chunks[4]);

    // Stats + pause hint
    let stats_line = Line::from(vec![
        Span::styled("Imported: ", Style::default().fg(SUBTLE)),
        Span::styled(format!("{}", stats.imported), Style::default().fg(SUCCESS)),
        Span::styled("   Skipped: ", Style::default().fg(SUBTLE)),
        Span::styled(format!("{}", stats.skipped), Style::default().fg(WARNING)),
        Span::styled("   Failed: ", Style::default().fg(SUBTLE)),
        Span::styled(format!("{}", stats.failed), Style::default().fg(ERROR)),
    ]);

    let hint_text = if is_paused {
        "Press Space to resume | Esc to cancel"
    } else {
        "Press Space to pause | Esc to cancel"
    };
    let hint_line = Line::from(Span::styled(hint_text, Style::default().fg(SUBTLE)));

    let stats_widget = Paragraph::new(vec![stats_line, hint_line]).alignment(Alignment::Center);
    frame.render_widget(stats_widget, chunks[5]);
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}
