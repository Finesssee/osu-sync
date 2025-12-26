//! Duplicate resolution modal dialog

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};

use crate::app::{PINK, SUBTLE, TEXT};
use osu_sync_core::dedup::DuplicateInfo;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    info: &DuplicateInfo,
    selected: usize,
    apply_to_all: bool,
) {
    // Calculate modal size and position
    let width = 50;
    let height = 18;
    let modal_area = centered_rect(width, height, area);

    // Clear the background
    frame.render_widget(Clear, modal_area);

    // Modal block
    let block = Block::default()
        .title(Span::styled(
            " Duplicate Detected ",
            Style::default().fg(PINK).bold(),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PINK));

    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    // Layout inside modal
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // Source info
            Constraint::Length(3), // Match info
            Constraint::Length(1), // Separator
            Constraint::Length(5), // Actions
            Constraint::Length(2), // Apply to all checkbox
        ])
        .split(inner);

    // Source info
    let source_info = Paragraph::new(vec![
        Line::from(vec![
            Span::styled("Source: ", Style::default().fg(SUBTLE)),
            Span::styled(
                format!("{} - {}", info.source.artist, info.source.title),
                Style::default().fg(TEXT),
            ),
        ]),
        Line::from(vec![
            Span::styled("Creator: ", Style::default().fg(SUBTLE)),
            Span::styled(&info.source.creator, Style::default().fg(TEXT)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Existing: ", Style::default().fg(SUBTLE)),
            Span::styled(
                format!("{} - {}", info.existing.artist, info.existing.title),
                Style::default().fg(TEXT),
            ),
        ]),
    ]);
    frame.render_widget(source_info, chunks[0]);

    // Match info
    let confidence_pct = (info.confidence * 100.0) as u8;
    let match_info = Paragraph::new(Line::from(vec![
        Span::styled("Match: ", Style::default().fg(SUBTLE)),
        Span::styled(format!("{:?}", info.match_type), Style::default().fg(PINK)),
        Span::styled(
            format!(" ({}% confidence)", confidence_pct),
            Style::default().fg(SUBTLE),
        ),
    ]));
    frame.render_widget(match_info, chunks[1]);

    // Actions
    let actions = [
        "Skip this beatmap",
        "Replace existing",
        "Keep both versions",
    ];

    let items: Vec<ListItem> = actions
        .iter()
        .enumerate()
        .map(|(i, action)| {
            let prefix = if i == selected { "> " } else { "  " };
            let style = if i == selected {
                Style::default().fg(PINK).bold()
            } else {
                Style::default().fg(TEXT)
            };
            ListItem::new(format!("{}{}", prefix, action)).style(style)
        })
        .collect();

    let menu = List::new(items);
    frame.render_widget(menu, chunks[3]);

    // Apply to all checkbox
    let checkbox = if apply_to_all { "[x]" } else { "[ ]" };
    let checkbox_line = Paragraph::new(Line::from(vec![
        Span::styled(checkbox, Style::default().fg(PINK)),
        Span::styled(
            " Apply to all similar duplicates",
            Style::default().fg(TEXT),
        ),
    ]));
    frame.render_widget(checkbox_line, chunks[4]);
}

fn centered_rect(width: u16, height: u16, parent: Rect) -> Rect {
    let x = parent.x + (parent.width.saturating_sub(width)) / 2;
    let y = parent.y + (parent.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(parent.width), height.min(parent.height))
}
