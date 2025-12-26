//! Sync progress screen

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph};

use crate::app::{LogEntry, LogLevel, SyncStats, ERROR, PINK, SUBTLE, SUCCESS, TEXT, WARNING};
use crate::widgets::get_spinner_frame;
use osu_sync_core::sync::SyncProgress;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    progress: &Option<SyncProgress>,
    logs: &[LogEntry],
    stats: &SyncStats,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Length(5), // Progress info
            Constraint::Length(3), // Progress bar
            Constraint::Length(3), // Current item
            Constraint::Min(0),    // Log
            Constraint::Length(2), // Stats
        ])
        .split(area);

    // Title
    let title = Paragraph::new(Span::styled(
        "Syncing Beatmaps",
        Style::default().fg(PINK).bold(),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // Progress info
    if let Some(prog) = progress {
        let info = Paragraph::new(vec![
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
        ])
        .alignment(Alignment::Center);
        frame.render_widget(info, chunks[1]);

        // Progress bar
        let ratio = if prog.total > 0 {
            prog.current as f64 / prog.total as f64
        } else {
            0.0
        };

        let gauge = Gauge::default()
            .block(Block::default().borders(Borders::NONE))
            .gauge_style(Style::default().fg(PINK).bg(Color::DarkGray))
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

    // Stats
    let stats_line = Line::from(vec![
        Span::styled("Imported: ", Style::default().fg(SUBTLE)),
        Span::styled(format!("{}", stats.imported), Style::default().fg(SUCCESS)),
        Span::styled("   Skipped: ", Style::default().fg(SUBTLE)),
        Span::styled(format!("{}", stats.skipped), Style::default().fg(WARNING)),
        Span::styled("   Failed: ", Style::default().fg(SUBTLE)),
        Span::styled(format!("{}", stats.failed), Style::default().fg(ERROR)),
    ]);

    let stats_widget = Paragraph::new(stats_line).alignment(Alignment::Center);
    frame.render_widget(stats_widget, chunks[5]);
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}
