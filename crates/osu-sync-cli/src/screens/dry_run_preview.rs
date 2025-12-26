//! Dry run preview screen

use ratatui::prelude::*;
use ratatui::widgets::{
    Block, Borders, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};

use crate::app::{PINK, SUBTLE, SUCCESS, TEXT, WARNING};
use osu_sync_core::sync::{DryRunAction, DryRunResult, SyncDirection};

pub fn render(
    frame: &mut Frame,
    area: Rect,
    result: &DryRunResult,
    direction: SyncDirection,
    selected_item: usize,
    scroll_offset: usize,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Length(4), // Summary stats
            Constraint::Length(3), // Size and time info
            Constraint::Min(0),    // Item list
        ])
        .split(area);

    // Title
    let direction_text = match direction {
        SyncDirection::StableToLazer => "Stable -> Lazer",
        SyncDirection::LazerToStable => "Lazer -> Stable",
        SyncDirection::Bidirectional => "Bidirectional",
    };
    let title = Paragraph::new(vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("Dry Run Preview ", Style::default().fg(PINK).bold()),
            Span::styled(format!("({})", direction_text), Style::default().fg(SUBTLE)),
        ]),
    ])
    .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // Summary stats
    let summary_area = centered_rect(60, 4, chunks[1]);
    let summary_block = Block::default()
        .title(" Summary ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(SUBTLE));

    let summary_inner = summary_block.inner(summary_area);
    frame.render_widget(summary_block, summary_area);

    let summary = Paragraph::new(vec![Line::from(vec![
        Span::styled("  Will Import: ", Style::default().fg(SUBTLE)),
        Span::styled(
            format!("{}", result.total_import),
            Style::default().fg(SUCCESS).bold(),
        ),
        Span::styled("    Skip: ", Style::default().fg(SUBTLE)),
        Span::styled(format!("{}", result.total_skip), Style::default().fg(TEXT)),
        Span::styled("    Duplicates: ", Style::default().fg(SUBTLE)),
        Span::styled(
            format!("{}", result.total_duplicate),
            Style::default().fg(WARNING),
        ),
    ])])
    .alignment(Alignment::Center);
    frame.render_widget(summary, summary_inner);

    // Size and time info
    let info_area = centered_rect(50, 3, chunks[2]);
    let info_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(SUBTLE));

    let info_inner = info_block.inner(info_area);
    frame.render_widget(info_block, info_area);

    let info = Paragraph::new(Line::from(vec![
        Span::styled("Size: ", Style::default().fg(SUBTLE)),
        Span::styled(result.size_display(), Style::default().fg(TEXT)),
        Span::styled("    Est. Time: ", Style::default().fg(SUBTLE)),
        Span::styled(result.estimated_time_display(), Style::default().fg(TEXT)),
    ]))
    .alignment(Alignment::Center);
    frame.render_widget(info, info_inner);

    // Item list
    let list_block = Block::default()
        .title(format!(" Beatmap Sets ({}) ", result.items.len()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(SUBTLE));

    let list_inner = list_block.inner(chunks[3]);
    frame.render_widget(list_block, chunks[3]);

    // Calculate visible items
    let visible_height = list_inner.height as usize;
    let total_items = result.items.len();

    // Create list items
    let items: Vec<ListItem> = result
        .items
        .iter()
        .enumerate()
        .skip(scroll_offset)
        .take(visible_height)
        .map(|(idx, item)| {
            let is_selected = idx == selected_item;

            // Action icon and color
            let (icon, action_color) = match item.action {
                DryRunAction::Import => ("+", SUCCESS),
                DryRunAction::Skip => ("-", SUBTLE),
                DryRunAction::Duplicate => ("!", WARNING),
            };

            // Format the display
            let prefix = if is_selected { "> " } else { "  " };
            let set_id_str = item
                .set_id
                .map(|id| format!("{}", id))
                .unwrap_or_else(|| "?".to_string());

            let style = if is_selected {
                Style::default().fg(PINK).bold()
            } else {
                Style::default().fg(TEXT)
            };

            ListItem::new(Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(format!("[{}] ", icon), Style::default().fg(action_color)),
                Span::styled(format!("{} ", set_id_str), Style::default().fg(SUBTLE)),
                Span::styled(format!("{} - {}", item.artist, item.title), style),
            ]))
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, list_inner);

    // Scrollbar if needed
    if total_items > visible_height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None);

        let mut scrollbar_state = ScrollbarState::new(total_items).position(scroll_offset);

        // Render scrollbar next to the list
        let scrollbar_area = Rect {
            x: chunks[3].x + chunks[3].width - 1,
            y: chunks[3].y + 1,
            width: 1,
            height: chunks[3].height.saturating_sub(2),
        };

        frame.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
    }

    // Hint at the bottom if no imports
    if !result.has_imports() {
        let no_imports = Paragraph::new(Span::styled(
            "No beatmaps to import - all are already synced or duplicates",
            Style::default().fg(WARNING),
        ))
        .alignment(Alignment::Center);

        let hint_area = Rect {
            x: list_inner.x,
            y: list_inner.y + list_inner.height.saturating_sub(1),
            width: list_inner.width,
            height: 1,
        };
        frame.render_widget(no_imports, hint_area);
    }
}

fn centered_rect(width: u16, height: u16, parent: Rect) -> Rect {
    let x = parent.x + (parent.width.saturating_sub(width)) / 2;
    let y = parent.y + (parent.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(parent.width), height.min(parent.height))
}
