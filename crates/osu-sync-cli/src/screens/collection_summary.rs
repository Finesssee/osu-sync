//! Collection sync summary screen

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::{ERROR, PINK, SUBTLE, SUCCESS, TEXT, WARNING};
use osu_sync_core::collection::CollectionSyncResult;

pub fn render(frame: &mut Frame, area: Rect, result: &CollectionSyncResult) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),  // Title + status
            Constraint::Length(10), // Results
            Constraint::Min(0),     // Missing beatmaps (if any)
            Constraint::Length(2),  // Instructions
        ])
        .split(area);

    // Title and status
    let (status_icon, status_text, status_color) = if result.success {
        ("OK", "Sync Complete", SUCCESS)
    } else {
        ("!", "Sync Failed", ERROR)
    };

    let title = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(
            "Collection Sync",
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
    let results_area = centered_rect(50, 8, chunks[1]);
    let results_block = Block::default()
        .title(" Results ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(SUBTLE));

    let results_inner = results_block.inner(results_area);
    frame.render_widget(results_block, results_area);

    let mut result_lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  Collections synced:  ", Style::default().fg(SUBTLE)),
            Span::styled(
                format!("{}", result.collections_synced),
                Style::default().fg(SUCCESS),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Beatmaps added:      ", Style::default().fg(SUBTLE)),
            Span::styled(
                format!("{}", result.beatmaps_added),
                Style::default().fg(SUCCESS),
            ),
        ]),
        Line::from(vec![
            Span::styled("  Beatmaps skipped:    ", Style::default().fg(SUBTLE)),
            Span::styled(
                format!("{}", result.beatmaps_skipped),
                Style::default().fg(TEXT),
            ),
        ]),
    ];

    if !result.missing_beatmaps.is_empty() {
        result_lines.push(Line::from(vec![
            Span::styled("  Missing beatmaps:    ", Style::default().fg(SUBTLE)),
            Span::styled(
                format!("{}", result.missing_beatmaps.len()),
                Style::default().fg(WARNING),
            ),
        ]));
    }

    if let Some(error) = &result.error_message {
        result_lines.push(Line::from(""));
        result_lines.push(Line::from(Span::styled(
            truncate(error, 45),
            Style::default().fg(ERROR).italic(),
        )));
    }

    let results = Paragraph::new(result_lines);
    frame.render_widget(results, results_inner);

    // Missing beatmaps (if any and space available)
    if !result.missing_beatmaps.is_empty() && !result.success {
        let missing_items: Vec<ListItem> = result
            .missing_beatmaps
            .iter()
            .take(10)
            .map(|hash| {
                ListItem::new(Span::styled(
                    format!("  {} ", truncate(hash, 35)),
                    Style::default().fg(WARNING),
                ))
            })
            .collect();

        let remaining = result.missing_beatmaps.len().saturating_sub(10);
        let title = if remaining > 0 {
            format!(" Missing Beatmaps (+{} more) ", remaining)
        } else {
            " Missing Beatmaps ".to_string()
        };

        let missing_list = List::new(missing_items).block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(WARNING)),
        );

        frame.render_widget(missing_list, chunks[2]);
    }

    // Instructions
    let instructions = Paragraph::new(Span::styled(
        "Press Enter or Esc to return to menu",
        Style::default().fg(SUBTLE),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(instructions, chunks[3]);
}

fn centered_rect(width: u16, height: u16, parent: Rect) -> Rect {
    let x = parent.x + (parent.width.saturating_sub(width)) / 2;
    let y = parent.y + (parent.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(parent.width), height.min(parent.height))
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}
