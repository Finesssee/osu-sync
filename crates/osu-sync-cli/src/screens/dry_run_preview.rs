//! Dry run preview screen

use std::collections::HashSet;

use ratatui::prelude::*;
use ratatui::widgets::{
    Block, Borders, List, ListItem, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};

use crate::app::{PINK, SUBTLE, SUCCESS, TEXT, WARNING};
use osu_sync_core::sync::{DryRunAction, DryRunItem, DryRunResult, SyncDirection};

/// Filter dry run items by search text, returns indices of matching items
pub fn filter_items(items: &[DryRunItem], filter_text: &str) -> Vec<usize> {
    if filter_text.is_empty() {
        return (0..items.len()).collect();
    }

    let filter_lower = filter_text.to_lowercase();
    items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            item.title.to_lowercase().contains(&filter_lower)
                || item.artist.to_lowercase().contains(&filter_lower)
                || item.set_id.map(|id| id.to_string().contains(&filter_lower)).unwrap_or(false)
        })
        .map(|(idx, _)| idx)
        .collect()
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    result: &DryRunResult,
    direction: SyncDirection,
    selected_item: usize,
    scroll_offset: usize,
    checked_items: &HashSet<usize>,
    filter_text: &str,
    filter_mode: bool,
) {
    // Determine if we need filter bar
    let show_filter = filter_mode || !filter_text.is_empty();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(if show_filter {
            vec![
                Constraint::Length(3), // Title
                Constraint::Length(4), // Summary stats
                Constraint::Length(3), // Size and time info
                Constraint::Length(3), // Filter input
                Constraint::Min(0),    // Item list
            ]
        } else {
            vec![
                Constraint::Length(3), // Title
                Constraint::Length(4), // Summary stats
                Constraint::Length(3), // Size and time info
                Constraint::Min(0),    // Item list
            ]
        })
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

    // Summary stats - show selection count
    let summary_area = centered_rect(70, 4, chunks[1]);
    let summary_block = Block::default()
        .title(" Summary ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(SUBTLE));

    let summary_inner = summary_block.inner(summary_area);
    frame.render_widget(summary_block, summary_area);

    // Calculate selection stats
    let total_importable = result.items.iter()
        .filter(|i| i.action == DryRunAction::Import)
        .count();
    let selected_count = checked_items.len();

    let summary = Paragraph::new(vec![Line::from(vec![
        Span::styled("  Selected: ", Style::default().fg(SUBTLE)),
        Span::styled(
            format!("{}/{}", selected_count, total_importable),
            Style::default().fg(if selected_count > 0 { SUCCESS } else { WARNING }).bold(),
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

    // Filter input bar (if visible)
    let list_chunk_idx = if show_filter {
        // Render filter bar
        let filter_area = centered_rect(50, 3, chunks[3]);
        let filter_block = Block::default()
            .title(" Filter (Esc to clear) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(if filter_mode { PINK } else { SUBTLE }));

        let filter_inner = filter_block.inner(filter_area);
        frame.render_widget(filter_block, filter_area);

        let filter_display = if filter_mode {
            format!("{}_", filter_text) // Show cursor
        } else {
            filter_text.to_string()
        };

        let filter_widget = Paragraph::new(filter_display)
            .style(Style::default().fg(TEXT))
            .alignment(Alignment::Left);
        frame.render_widget(filter_widget, filter_inner);

        4 // List is at index 4 when filter is shown
    } else {
        3 // List is at index 3 when no filter
    };

    // Get filtered items
    let visible_indices = filter_items(&result.items, filter_text);
    let total_visible = visible_indices.len();

    // Item list
    let list_block = Block::default()
        .title(format!(
            " Beatmap Sets ({}{}) ",
            total_visible,
            if !filter_text.is_empty() {
                format!(" of {}", result.items.len())
            } else {
                String::new()
            }
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(SUBTLE));

    let list_inner = list_block.inner(chunks[list_chunk_idx]);
    frame.render_widget(list_block, chunks[list_chunk_idx]);

    // Calculate visible items
    let visible_height = list_inner.height as usize;

    // Create list items from filtered indices
    // Note: selected_item is the index in visible_indices (0 to total_visible-1)
    // We render items from scroll_offset to scroll_offset + visible_height
    let items: Vec<ListItem> = visible_indices
        .iter()
        .skip(scroll_offset)
        .take(visible_height)
        .enumerate()
        .map(|(screen_row, &actual_idx)| {
            let item = &result.items[actual_idx];
            // The item at screen_row corresponds to visible_indices[scroll_offset + screen_row]
            // So it should be highlighted when selected_item == scroll_offset + screen_row
            let is_cursor = selected_item == scroll_offset + screen_row;
            let is_checked = checked_items.contains(&actual_idx);
            let is_selectable = item.action == DryRunAction::Import;

            // Checkbox display
            let checkbox = if !is_selectable {
                "   " // Not selectable (Skip/Duplicate)
            } else if is_checked {
                "[x]"
            } else {
                "[ ]"
            };

            // Action icon and color
            let (icon, action_color) = match item.action {
                DryRunAction::Import => ("+", SUCCESS),
                DryRunAction::Skip => ("-", SUBTLE),
                DryRunAction::Duplicate => ("!", WARNING),
            };

            // Format the display
            let prefix = if is_cursor { "> " } else { "  " };
            let set_id_str = item
                .set_id
                .map(|id| format!("{}", id))
                .unwrap_or_else(|| "?".to_string());

            let style = if is_cursor {
                Style::default().fg(PINK).bold()
            } else {
                Style::default().fg(TEXT)
            };

            let checkbox_color = if is_checked { SUCCESS } else { SUBTLE };

            ListItem::new(Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(format!("{} ", checkbox), Style::default().fg(checkbox_color)),
                Span::styled(format!("[{}] ", icon), Style::default().fg(action_color)),
                Span::styled(format!("{} ", set_id_str), Style::default().fg(SUBTLE)),
                Span::styled(format!("{} - {}", item.artist, item.title), style),
            ]))
        })
        .collect();

    let list = List::new(items);
    frame.render_widget(list, list_inner);

    // Scrollbar if needed
    if total_visible > visible_height {
        let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None);

        let mut scrollbar_state = ScrollbarState::new(total_visible).position(scroll_offset);

        // Render scrollbar next to the list
        let scrollbar_area = Rect {
            x: chunks[list_chunk_idx].x + chunks[list_chunk_idx].width - 1,
            y: chunks[list_chunk_idx].y + 1,
            width: 1,
            height: chunks[list_chunk_idx].height.saturating_sub(2),
        };

        frame.render_stateful_widget(scrollbar, scrollbar_area, &mut scrollbar_state);
    }

    // Hint at the bottom if no items selected
    if checked_items.is_empty() && result.has_imports() {
        let no_selection = Paragraph::new(Span::styled(
            "No beatmaps selected - use Space to select, Ctrl+A to select all",
            Style::default().fg(WARNING),
        ))
        .alignment(Alignment::Center);

        let hint_area = Rect {
            x: list_inner.x,
            y: list_inner.y + list_inner.height.saturating_sub(1),
            width: list_inner.width,
            height: 1,
        };
        frame.render_widget(no_selection, hint_area);
    } else if !result.has_imports() {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_item(set_id: Option<i32>, title: &str, artist: &str) -> DryRunItem {
        let folder_name = set_id.map(|id| format!("{} {} - {}", id, artist, title));
        DryRunItem {
            set_id,
            folder_name,
            title: title.to_string(),
            artist: artist.to_string(),
            action: DryRunAction::Import,
            size_bytes: 1000,
            difficulty_count: 1,
        }
    }

    fn make_test_items() -> Vec<DryRunItem> {
        vec![
            make_test_item(Some(123), "UNION!!", "765 MILLION ALLSTARS"),
            make_test_item(Some(456), "Harumachi Clover", "Hanatan"),
            make_test_item(Some(789), "UNION!! Remix", "Some Artist"),
            make_test_item(None, "No ID Song", "Unknown"),
            make_test_item(Some(111), "Test Song", "UNION Band"),
        ]
    }

    #[test]
    fn test_filter_items_empty_filter_returns_all() {
        let items = make_test_items();
        let result = filter_items(&items, "");
        assert_eq!(result, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn test_filter_items_by_title() {
        let items = make_test_items();
        let result = filter_items(&items, "UNION");
        // Should match items 0, 2 (title contains UNION)
        assert_eq!(result, vec![0, 2, 4]); // Item 4 has artist "UNION Band"
    }

    #[test]
    fn test_filter_items_by_artist() {
        let items = make_test_items();
        let result = filter_items(&items, "Hanatan");
        assert_eq!(result, vec![1]);
    }

    #[test]
    fn test_filter_items_by_set_id() {
        let items = make_test_items();
        let result = filter_items(&items, "456");
        assert_eq!(result, vec![1]);
    }

    #[test]
    fn test_filter_items_case_insensitive() {
        let items = make_test_items();
        let result_lower = filter_items(&items, "union");
        let result_upper = filter_items(&items, "UNION");
        assert_eq!(result_lower, result_upper);
    }

    #[test]
    fn test_filter_items_no_matches() {
        let items = make_test_items();
        let result = filter_items(&items, "nonexistent");
        assert!(result.is_empty());
    }

    #[test]
    fn test_filter_items_partial_match() {
        let items = make_test_items();
        let result = filter_items(&items, "Clover");
        assert_eq!(result, vec![1]); // Harumachi Clover
    }

    #[test]
    fn test_extract_set_ids_from_checked_items() {
        let items = make_test_items();
        let checked_items: HashSet<usize> = [0, 2].into_iter().collect();

        let set_ids: HashSet<i32> = checked_items
            .iter()
            .filter_map(|&idx| items.get(idx).and_then(|item| item.set_id))
            .collect();

        assert_eq!(set_ids.len(), 2);
        assert!(set_ids.contains(&123));
        assert!(set_ids.contains(&789));
    }

    #[test]
    fn test_extract_set_ids_skips_items_without_id() {
        let items = make_test_items();
        let checked_items: HashSet<usize> = [3].into_iter().collect(); // Item 3 has no ID

        let set_ids: HashSet<i32> = checked_items
            .iter()
            .filter_map(|&idx| items.get(idx).and_then(|item| item.set_id))
            .collect();

        assert!(set_ids.is_empty());
    }

    #[test]
    fn test_display_index_to_actual_index_conversion() {
        let items = make_test_items();
        let visible_indices = filter_items(&items, "UNION");

        // visible_indices = [0, 2, 4] (items containing UNION)
        // display_index 0 -> actual_index 0
        // display_index 1 -> actual_index 2
        // display_index 2 -> actual_index 4

        assert_eq!(visible_indices.get(0), Some(&0));
        assert_eq!(visible_indices.get(1), Some(&2));
        assert_eq!(visible_indices.get(2), Some(&4));
    }

    #[test]
    fn test_checked_items_with_filter() {
        let items = make_test_items();
        let visible_indices = filter_items(&items, "UNION");
        let mut checked_items: HashSet<usize> = HashSet::new();

        // Simulate user selecting display_index 1 (which is actual_index 2)
        let display_idx = 1;
        if let Some(&actual_idx) = visible_indices.get(display_idx) {
            checked_items.insert(actual_idx);
        }

        // The checked_items should contain the actual index, not display index
        assert!(checked_items.contains(&2));
        assert!(!checked_items.contains(&1));

        // Verify we can get the set_id
        let set_id = items.get(2).and_then(|i| i.set_id);
        assert_eq!(set_id, Some(789)); // UNION!! Remix
    }
}
