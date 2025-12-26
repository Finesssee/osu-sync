//! Collection configuration screen

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::{PINK, SUBTLE, SUCCESS, TEXT};
use crate::widgets::get_spinner_frame;
use osu_sync_core::collection::{Collection, CollectionSyncStrategy};

pub fn render(
    frame: &mut Frame,
    area: Rect,
    collections: &[Collection],
    selected: usize,
    strategy: CollectionSyncStrategy,
    loading: bool,
    status_message: &str,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Length(2), // Instruction
            Constraint::Min(0),    // Collections list
            Constraint::Length(5), // Options and preview
            Constraint::Length(2), // Status
        ])
        .split(area);

    // Title
    let title = Paragraph::new(Span::styled(
        "Collection Sync",
        Style::default().fg(PINK).bold(),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // Instruction
    let instruction = if loading {
        let spinner = get_spinner_frame();
        Paragraph::new(Line::from(vec![
            Span::styled(spinner, Style::default().fg(PINK)),
            Span::styled(
                " Loading collections from osu!stable...",
                Style::default().fg(SUBTLE),
            ),
        ]))
    } else {
        Paragraph::new(Span::styled(
            "Detected collections (Stable -> Lazer)",
            Style::default().fg(SUBTLE),
        ))
    };
    frame.render_widget(instruction.alignment(Alignment::Center), chunks[1]);

    // Collections list
    if loading {
        // Show loading indicator
        let loading_msg = Paragraph::new(Span::styled(
            "Scanning collection.db...",
            Style::default().fg(SUBTLE),
        ))
        .alignment(Alignment::Center);
        frame.render_widget(loading_msg, chunks[2]);
    } else if collections.is_empty() {
        // No collections found
        let no_collections = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "No collections found in osu!stable",
                Style::default().fg(SUBTLE),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Make sure you have collection.db in your osu! folder",
                Style::default().fg(SUBTLE),
            )),
        ])
        .alignment(Alignment::Center);
        frame.render_widget(no_collections, chunks[2]);
    } else {
        // Show collections list
        let num_collections = collections.len();
        let items: Vec<ListItem> = collections
            .iter()
            .enumerate()
            .map(|(i, collection)| {
                let is_selected = i == selected;
                let prefix = if is_selected { "> " } else { "  " };
                let style = if is_selected {
                    Style::default().fg(PINK).bold()
                } else {
                    Style::default().fg(TEXT)
                };
                let count_style = if is_selected {
                    Style::default().fg(SUCCESS)
                } else {
                    Style::default().fg(SUBTLE)
                };

                ListItem::new(Line::from(vec![
                    Span::styled(prefix, style),
                    Span::styled(&collection.name, style),
                    Span::styled(format!("  ({} beatmaps)", collection.len()), count_style),
                ]))
            })
            .chain(std::iter::once({
                // Strategy option as last item
                let is_selected = selected == num_collections;
                let prefix = if is_selected { "> " } else { "  " };
                let style = if is_selected {
                    Style::default().fg(PINK).bold()
                } else {
                    Style::default().fg(TEXT)
                };

                ListItem::new(Line::from(vec![
                    Span::styled(prefix, style),
                    Span::styled("Strategy: ", style),
                    Span::styled(
                        format!("{}", strategy),
                        if is_selected {
                            Style::default().fg(SUCCESS)
                        } else {
                            Style::default().fg(SUBTLE)
                        },
                    ),
                    Span::styled("  (Enter to toggle)", Style::default().fg(SUBTLE).italic()),
                ]))
            }))
            .collect();

        let list_width = 60;
        let list_height = items.len().min(12) as u16 + 2;
        let list_area = centered_rect(list_width, list_height, chunks[2]);

        let list = List::new(items).block(
            Block::default()
                .title(format!(" {} Collections ", collections.len()))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(SUBTLE)),
        );

        frame.render_widget(list, list_area);
    }

    // Options and preview
    if !loading && !collections.is_empty() {
        let total_beatmaps: usize = collections.iter().map(|c| c.len()).sum();
        let strategy_desc = match strategy {
            CollectionSyncStrategy::Merge => "Add beatmaps to existing collections",
            CollectionSyncStrategy::Replace => "Replace target collections entirely",
        };

        let preview = Paragraph::new(vec![
            Line::from(vec![
                Span::styled("Strategy: ", Style::default().fg(SUBTLE)),
                Span::styled(format!("{}", strategy), Style::default().fg(TEXT)),
                Span::styled(format!(" - {}", strategy_desc), Style::default().fg(SUBTLE)),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("Press ", Style::default().fg(SUBTLE)),
                Span::styled("Enter", Style::default().fg(PINK)),
                Span::styled(
                    format!(
                        " to sync {} collections ({} beatmaps)",
                        collections.len(),
                        total_beatmaps
                    ),
                    Style::default().fg(SUBTLE),
                ),
            ]),
        ])
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .title(" Preview ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(SUBTLE)),
        );

        frame.render_widget(preview, chunks[3]);
    }

    // Status
    let status = Paragraph::new(Span::styled(status_message, Style::default().fg(SUBTLE)))
        .alignment(Alignment::Center);
    frame.render_widget(status, chunks[4]);
}

fn centered_rect(width: u16, height: u16, parent: Rect) -> Rect {
    let x = parent.x + (parent.width.saturating_sub(width)) / 2;
    let y = parent.y + (parent.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(parent.width), height.min(parent.height))
}
