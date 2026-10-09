//! Sync completion summary screen

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::{ERROR, PINK, SUBTLE, SUCCESS, TEXT};
use osu_sync_core::sync::{SyncError, SyncResult};

pub fn render(frame: &mut Frame, area: Rect, result: &SyncResult) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),  // Title + status
            Constraint::Length(10), // Results
            Constraint::Min(0),     // Errors, notes, skips
        ])
        .split(area);

    // Title and status
    let status_icon = if result.is_success() { "✓" } else { "!" };
    let status_text = if result.is_success() {
        "Success"
    } else {
        "Completed with errors"
    };
    let status_color = if result.is_success() { SUCCESS } else { ERROR };

    let title = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "Sync Complete",
            Style::default().fg(PINK).bold(),
        )),
        Line::from(Span::styled(
            format!("{} {}", status_icon, status_text),
            Style::default().fg(status_color),
        )),
    ])
    .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // Results panel
    let results_area = centered_rect(45, 8, chunks[1]);
    let results_block = Block::default()
        .title(" Results ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(SUBTLE));

    let results_inner = results_block.inner(results_area);
    frame.render_widget(results_block, results_area);

    let results = Paragraph::new(vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Direction:   ", Style::default().fg(SUBTLE)),
            Span::styled(format!("{}", result.direction), Style::default().fg(TEXT)),
        ]),
        Line::from(vec![
            Span::styled("  Imported:    ", Style::default().fg(SUBTLE)),
            Span::styled(format!("{}", result.imported), Style::default().fg(SUCCESS)),
        ]),
        Line::from(vec![
            Span::styled("  Staged:      ", Style::default().fg(SUBTLE)),
            Span::styled(format!("{}", result.staged), Style::default().fg(TEXT)),
        ]),
        Line::from(vec![
            Span::styled("  Skipped:     ", Style::default().fg(SUBTLE)),
            Span::styled(format!("{}", result.skipped), Style::default().fg(TEXT)),
        ]),
        Line::from(vec![
            Span::styled("  Failed:      ", Style::default().fg(SUBTLE)),
            Span::styled(
                format!("{}", result.failed),
                Style::default().fg(if result.failed > 0 { ERROR } else { TEXT }),
            ),
        ]),
    ]);
    frame.render_widget(results, results_inner);

    // Errors, notes and skip reasons, most urgent first
    let line = |e: &SyncError| match &e.beatmap_set {
        Some(name) => format!("{}: {}", name, e.message),
        None => e.message.clone(),
    };
    let items: Vec<ListItem> = result
        .errors
        .iter()
        .map(|e| (line(e), ERROR))
        .chain(result.notes.iter().map(|n| (n.clone(), TEXT)))
        .chain(result.skips.iter().map(|e| (line(e), SUBTLE)))
        .take(chunks[2].height.saturating_sub(2) as usize)
        .map(|(text, color)| ListItem::new(Span::styled(text, Style::default().fg(color))))
        .collect();

    if !items.is_empty() {
        let border = if result.errors.is_empty() {
            SUBTLE
        } else {
            ERROR
        };
        let details = List::new(items).block(
            Block::default()
                .title(format!(
                    " Errors ({}) / Skipped sets ({}) ",
                    result.errors.len(),
                    result.skips.len()
                ))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border)),
        );

        frame.render_widget(details, chunks[2]);
    }
}

fn centered_rect(width: u16, height: u16, parent: Rect) -> Rect {
    let x = parent.x + (parent.width.saturating_sub(width)) / 2;
    let y = parent.y + (parent.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(parent.width), height.min(parent.height))
}
