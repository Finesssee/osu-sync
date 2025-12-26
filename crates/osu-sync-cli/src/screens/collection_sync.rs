//! Collection sync progress screen

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph};

use crate::app::{LogEntry, LogLevel, ERROR, PINK, SUBTLE, SUCCESS, TEXT, WARNING};
use crate::widgets::get_spinner_frame;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    progress: f32,
    current_collection: &str,
    logs: &[LogEntry],
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Length(3), // Progress bar
            Constraint::Length(3), // Current collection
            Constraint::Min(0),    // Log
            Constraint::Length(2), // Instructions
        ])
        .split(area);

    // Title
    let spinner = get_spinner_frame();
    let title = Paragraph::new(Line::from(vec![
        Span::styled(spinner, Style::default().fg(PINK)),
        Span::styled(" Syncing Collections", Style::default().fg(PINK).bold()),
    ]))
    .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // Progress bar
    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::NONE))
        .gauge_style(Style::default().fg(PINK).bg(Color::DarkGray))
        .ratio(progress as f64)
        .label(format!("{}%", (progress * 100.0) as u16));

    let gauge_area = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(15),
            Constraint::Percentage(70),
            Constraint::Percentage(15),
        ])
        .split(chunks[1]);

    frame.render_widget(gauge, gauge_area[1]);

    // Current collection
    let current = Paragraph::new(Line::from(vec![
        Span::styled("Current: ", Style::default().fg(SUBTLE)),
        Span::styled(truncate(current_collection, 50), Style::default().fg(TEXT)),
    ]))
    .alignment(Alignment::Center);
    frame.render_widget(current, chunks[2]);

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
            .title(" Progress ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(SUBTLE)),
    );

    frame.render_widget(log, chunks[3]);

    // Instructions
    let instructions = Paragraph::new(Span::styled(
        "Press Esc to cancel",
        Style::default().fg(SUBTLE),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(instructions, chunks[4]);
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}
