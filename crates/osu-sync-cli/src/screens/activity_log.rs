//! Activity log screen showing recent actions

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};

use crate::app::{error_color, pink, subtle_color, success_color, text_color, warning_color};
use osu_sync_core::activity::{ActivityLog, ActivityType};

pub fn render(frame: &mut Frame, area: Rect, log: &ActivityLog, scroll_offset: usize) {
    // Get theme colors
    let accent = pink();
    let subtle = subtle_color();
    let text = text_color();
    let success = success_color();
    let warning = warning_color();
    let error = error_color();

    // Calculate modal size and position (centered)
    let width = 70;
    let height = 24;
    let modal_area = centered_rect(width, height, area);

    // Clear the background
    frame.render_widget(Clear, modal_area);

    // Modal block
    let block = Block::default()
        .title(Span::styled(
            " Activity Log ",
            Style::default().fg(accent).bold(),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent));

    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    // Layout inside modal
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // Log entries
            Constraint::Length(2), // Footer
        ])
        .split(inner);

    // Build log entries
    let entries = log.entries();

    if entries.is_empty() {
        let empty_msg = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "No activity recorded yet.",
                Style::default().fg(subtle),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Actions like scans, syncs, and exports",
                Style::default().fg(subtle),
            )),
            Line::from(Span::styled(
                "will appear here as you use the app.",
                Style::default().fg(subtle),
            )),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(empty_msg, chunks[0]);
    } else {
        let visible_height = chunks[0].height as usize;
        let max_scroll = entries.len().saturating_sub(visible_height);
        let scroll = scroll_offset.min(max_scroll);

        let items: Vec<ListItem> = entries
            .iter()
            .skip(scroll)
            .take(visible_height)
            .map(|entry| {
                let type_color = match entry.activity_type {
                    ActivityType::Error => error,
                    ActivityType::Sync | ActivityType::Scan => success,
                    ActivityType::Backup | ActivityType::Restore => warning,
                    _ => text,
                };

                let icon = entry.activity_type.icon();
                let time = entry.formatted_time();
                let type_name = format!("{:<11}", entry.activity_type.display_name());

                let line = Line::from(vec![
                    Span::styled(format!("{} ", icon), Style::default().fg(type_color)),
                    Span::styled(format!("{} ", time), Style::default().fg(subtle)),
                    Span::styled(type_name, Style::default().fg(type_color)),
                    Span::styled(&entry.description, Style::default().fg(text)),
                ]);

                ListItem::new(line)
            })
            .collect();

        let list = List::new(items);
        frame.render_widget(list, chunks[0]);

        // Scroll indicator if needed
        if entries.len() > visible_height {
            let scroll_text = format!(" {}/{} ", scroll + 1, entries.len());
            let scroll_indicator =
                Paragraph::new(Span::styled(scroll_text, Style::default().fg(subtle)))
                    .alignment(Alignment::Right);
            let indicator_area = Rect::new(chunks[0].right() - 10, chunks[0].y, 10, 1);
            frame.render_widget(scroll_indicator, indicator_area);
        }
    }

    // Footer with count
    let count_text = if entries.is_empty() {
        "No entries".to_string()
    } else {
        format!("{} entries", entries.len())
    };

    let footer = Paragraph::new(Line::from(vec![
        Span::styled(count_text, Style::default().fg(subtle)),
        Span::styled(" | ", Style::default().fg(subtle)),
        Span::styled(
            "Press any key to close",
            Style::default().fg(subtle).italic(),
        ),
    ]))
    .alignment(Alignment::Center);

    // Render separator and footer
    let separator = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(accent));
    frame.render_widget(separator, chunks[1]);

    let footer_inner = Rect::new(chunks[1].x, chunks[1].y + 1, chunks[1].width, 1);
    frame.render_widget(footer, footer_inner);
}

fn centered_rect(width: u16, height: u16, parent: Rect) -> Rect {
    let x = parent.x + (parent.width.saturating_sub(width)) / 2;
    let y = parent.y + (parent.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(parent.width), height.min(parent.height))
}
