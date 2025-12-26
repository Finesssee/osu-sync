//! Replay export screens

use osu_sync_core::replay::{ExportOrganization, ReplayExportResult, ReplayProgress};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph};

use crate::app::{PINK, SUBTLE, TEXT};

/// Render replay export configuration screen
pub fn render_config(
    frame: &mut Frame,
    area: Rect,
    selected: usize,
    organization: ExportOrganization,
    output_path: &str,
    replay_count: usize,
    status_message: &Option<String>,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Length(2), // Info
            Constraint::Min(0),    // Options
            Constraint::Length(3), // Status
        ])
        .split(area);

    // Title
    let title = Paragraph::new(Span::styled(
        "Export Replays",
        Style::default().fg(PINK).bold(),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // Info
    let info_text = format!("{} replays with .osr files available", replay_count);
    let info = Paragraph::new(Span::styled(info_text, Style::default().fg(SUBTLE)))
        .alignment(Alignment::Center);
    frame.render_widget(info, chunks[1]);

    // Options
    let org_str = match organization {
        ExportOrganization::Flat => "Flat (all in one folder)",
        ExportOrganization::ByBeatmap => "By Beatmap",
        ExportOrganization::ByDate => "By Date",
        ExportOrganization::ByPlayer => "By Player",
        ExportOrganization::ByGrade => "By Grade",
    };

    let options = [
        format!("Organization: {}", org_str),
        format!("Output: {}", output_path),
        "Start Export".to_string(),
    ];

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let prefix = if i == selected { "> " } else { "  " };
            let style = if i == selected {
                Style::default().fg(PINK).bold()
            } else {
                Style::default().fg(TEXT)
            };
            ListItem::new(Span::styled(format!("{}{}", prefix, label), style))
        })
        .collect();

    let menu_width = 50;
    let menu_height = options.len() as u16 + 2;
    let menu_area = centered_rect(menu_width, menu_height, chunks[2]);

    let menu = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(SUBTLE)),
    );

    frame.render_widget(menu, menu_area);

    // Status message
    if let Some(ref msg) = status_message {
        let status = Paragraph::new(Span::styled(msg.as_str(), Style::default().fg(SUBTLE)))
            .alignment(Alignment::Center);
        frame.render_widget(status, chunks[3]);
    }
}

/// Render replay export progress screen
pub fn render_progress(
    frame: &mut Frame,
    area: Rect,
    progress: &Option<ReplayProgress>,
    current_replay: &str,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Length(3), // Progress bar
            Constraint::Length(2), // Current file
            Constraint::Min(0),    // Stats
            Constraint::Length(2), // Hint
        ])
        .split(area);

    // Title
    let title = Paragraph::new(Span::styled(
        "Exporting Replays...",
        Style::default().fg(PINK).bold(),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // Progress bar
    let percentage = progress.as_ref().map(|p| p.percentage()).unwrap_or(0.0);

    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL))
        .gauge_style(Style::default().fg(PINK))
        .percent(percentage as u16)
        .label(format!("{:.1}%", percentage));

    let gauge_area = centered_rect(60, 3, chunks[1]);
    frame.render_widget(gauge, gauge_area);

    // Current file
    let current = Paragraph::new(Span::styled(current_replay, Style::default().fg(TEXT)))
        .alignment(Alignment::Center);
    frame.render_widget(current, chunks[2]);

    // Stats
    if let Some(ref p) = progress {
        let stats_text = format!(
            "Replays: {}/{} | Written: {:.1} MB",
            p.replays_processed,
            p.total_replays,
            p.bytes_written as f64 / 1_048_576.0
        );
        let stats = Paragraph::new(Span::styled(stats_text, Style::default().fg(SUBTLE)))
            .alignment(Alignment::Center);
        frame.render_widget(stats, chunks[3]);
    }

    // Hint
    let hint = Paragraph::new(Span::styled(
        "Press Esc to cancel",
        Style::default().fg(SUBTLE).italic(),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(hint, chunks[4]);
}

/// Render replay export complete screen
pub fn render_complete(frame: &mut Frame, area: Rect, result: &ReplayExportResult) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Min(0),    // Results
            Constraint::Length(2), // Hint
        ])
        .split(area);

    // Title
    let title = Paragraph::new(Span::styled(
        "Export Complete!",
        Style::default().fg(PINK).bold(),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // Results
    let results_text = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("Replays exported: {}", result.replays_exported),
            Style::default().fg(TEXT),
        )),
        Line::from(Span::styled(
            format!("Replays skipped: {}", result.replays_skipped),
            Style::default().fg(SUBTLE),
        )),
        Line::from(Span::styled(
            format!(
                "Total data written: {:.1} MB",
                result.bytes_written as f64 / 1_048_576.0
            ),
            Style::default().fg(TEXT),
        )),
    ];

    let mut results_text = results_text;
    if !result.errors.is_empty() {
        results_text.push(Line::from(""));
        results_text.push(Line::from(Span::styled(
            format!("Errors: {}", result.errors.len()),
            Style::default().fg(Color::Red),
        )));
    }

    let results = Paragraph::new(results_text)
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .title(" Summary ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(SUBTLE)),
        );

    let results_area = centered_rect(50, 10, chunks[1]);
    frame.render_widget(results, results_area);

    // Hint
    let hint = Paragraph::new(Span::styled(
        "Press Enter to return to menu",
        Style::default().fg(SUBTLE).italic(),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(hint, chunks[2]);
}

fn centered_rect(width: u16, height: u16, parent: Rect) -> Rect {
    let x = parent.x + (parent.width.saturating_sub(width)) / 2;
    let y = parent.y + (parent.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(parent.width), height.min(parent.height))
}
