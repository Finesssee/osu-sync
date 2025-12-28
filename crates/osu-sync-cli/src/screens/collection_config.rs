//! Collection configuration screen

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::{PINK, SUBTLE, SUCCESS, TEXT, WARNING};
use crate::widgets::get_spinner_frame;
use osu_sync_core::collection::{Collection, CollectionSyncDirection, CollectionSyncEngine, CollectionSyncStrategy};

pub fn render(
    frame: &mut Frame,
    area: Rect,
    collections: &[Collection],
    selected: usize,
    strategy: CollectionSyncStrategy,
    loading: bool,
    status_message: &str,
) {
    // Calculate preview for duplicate detection
    let preview = CollectionSyncEngine::preview(collections, CollectionSyncDirection::StableToLazer);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Length(2), // Instruction
            Constraint::Min(0),    // Collections list
            Constraint::Length(8), // Enhanced preview panel
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
        // Show collections list with duplicate indicators
        let num_collections = collections.len();
        let items: Vec<ListItem> = preview.collections
            .iter()
            .enumerate()
            .map(|(i, preview_item)| {
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

                // Build spans for the list item
                let mut spans = vec![
                    Span::styled(prefix, style),
                    Span::styled(&preview_item.name, style),
                    Span::styled(format!("  ({} beatmaps)", preview_item.beatmap_count), count_style),
                ];

                // Add duplicate/merge indicators
                if preview_item.is_duplicate {
                    spans.push(Span::styled("  [DUPE]", Style::default().fg(WARNING)));
                } else if preview_item.merge_count > 0 {
                    spans.push(Span::styled(
                        format!("  [+{} merged]", preview_item.merge_count),
                        Style::default().fg(SUCCESS),
                    ));
                }

                ListItem::new(Line::from(spans))
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

        let list_width = 70; // Wider to accommodate duplicate indicators
        let list_height = items.len().min(12) as u16 + 2;
        let list_area = centered_rect(list_width, list_height, chunks[2]);

        // Update title to show unique count vs total if duplicates exist
        let title = if preview.duplicates_merged > 0 {
            format!(" {} Collections ({} unique) ", collections.len(), preview.unique_collections)
        } else {
            format!(" {} Collections ", collections.len())
        };

        let list = List::new(items).block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(SUBTLE)),
        );

        frame.render_widget(list, list_area);
    }

    // Enhanced preview panel
    if !loading && !collections.is_empty() {
        let strategy_desc = match strategy {
            CollectionSyncStrategy::Merge => "Add beatmaps to existing collections",
            CollectionSyncStrategy::Replace => "Replace target collections entirely",
        };

        let mut preview_lines = vec![
            // Direction indicator
            Line::from(vec![
                Span::styled("Direction: ", Style::default().fg(SUBTLE)),
                Span::styled(&preview.source, Style::default().fg(TEXT)),
                Span::styled(" -> ", Style::default().fg(PINK)),
                Span::styled(&preview.target, Style::default().fg(TEXT)),
            ]),
            // Strategy
            Line::from(vec![
                Span::styled("Strategy: ", Style::default().fg(SUBTLE)),
                Span::styled(format!("{}", strategy), Style::default().fg(TEXT)),
                Span::styled(format!(" - {}", strategy_desc), Style::default().fg(SUBTLE)),
            ]),
        ];

        // Show duplicate merge info if applicable
        if preview.duplicates_merged > 0 {
            preview_lines.push(Line::from(vec![
                Span::styled("Duplicates: ", Style::default().fg(SUBTLE)),
                Span::styled(
                    format!("{} collections will be merged", preview.duplicates_merged),
                    Style::default().fg(WARNING),
                ),
            ]));
        }

        // Summary line
        preview_lines.push(Line::from(vec![
            Span::styled("Summary: ", Style::default().fg(SUBTLE)),
            Span::styled(
                format!(
                    "{} unique collections, {} total beatmaps",
                    preview.unique_collections, preview.total_beatmaps
                ),
                Style::default().fg(SUCCESS),
            ),
        ]));

        // Manual steps warning (bidirectional indicator)
        if let Some(ref message) = preview.manual_steps_message {
            preview_lines.push(Line::from(""));
            preview_lines.push(Line::from(vec![
                Span::styled("Note: ", Style::default().fg(WARNING)),
                Span::styled(truncate(message, 60), Style::default().fg(SUBTLE).italic()),
            ]));
        }

        let preview_widget = Paragraph::new(preview_lines)
            .block(
                Block::default()
                    .title(" Sync Preview ")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(SUBTLE)),
            );

        frame.render_widget(preview_widget, chunks[3]);
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

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}
