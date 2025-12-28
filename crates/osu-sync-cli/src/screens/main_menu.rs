//! Main menu screen

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{pink, selection_bg, subtle_color, text_color, App};
use crate::widgets::render_status_bar;

/// Menu item with icon
struct MenuItem {
    icon: &'static str,
    label: &'static str,
    description: &'static str,
}

const MENU_ITEMS: [MenuItem; 10] = [
    MenuItem {
        icon: "\u{1F50D}",
        label: "Scan Installations",
        description: "Detect osu! installations",
    },
    MenuItem {
        icon: "\u{1F504}",
        label: "Sync Beatmaps",
        description: "Synchronize your beatmaps",
    },
    MenuItem {
        icon: "\u{1F4C1}",
        label: "Collection Sync",
        description: "Sync beatmap collections",
    },
    MenuItem {
        icon: "\u{1F4CA}",
        label: "Statistics",
        description: "View beatmap statistics",
    },
    MenuItem {
        icon: "\u{1F3B5}",
        label: "Extract Media",
        description: "Extract audio/backgrounds",
    },
    MenuItem {
        icon: "\u{1F3AE}",
        label: "Export Replays",
        description: "Export replay files",
    },
    MenuItem {
        icon: "\u{1F4BE}",
        label: "Backup",
        description: "Create backup of osu! data",
    },
    MenuItem {
        icon: "\u{1F4E5}",
        label: "Restore",
        description: "Restore from backup",
    },
    MenuItem {
        icon: "\u{2699}",
        label: "Configuration",
        description: "Configure paths and options",
    },
    MenuItem {
        icon: "\u{1F6AA}",
        label: "Exit",
        description: "Close osu-sync",
    },
];

pub fn render(frame: &mut Frame, area: Rect, selected: usize, app: &App) {
    // Get theme colors
    let accent = pink();
    let subtle = subtle_color();
    let text = text_color();
    let sel_bg = selection_bg();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5), // Title area
            Constraint::Min(0),    // Menu
            Constraint::Length(2), // Status bar
        ])
        .split(area);

    // Title with ASCII art style
    let title = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("\u{266A} ", Style::default().fg(accent)), // Music note
            Span::styled("Welcome to ", Style::default().fg(subtle)),
            Span::styled("osu-sync", Style::default().fg(accent).bold()),
        ]),
        Line::from(Span::styled(
            "Sync beatmaps between osu!stable and osu!lazer",
            Style::default().fg(subtle).italic(),
        )),
    ];
    let title_widget = Paragraph::new(title).alignment(Alignment::Center);
    frame.render_widget(title_widget, chunks[0]);

    // Menu - centered with better styling
    let menu_width = 50;
    let menu_height = (MENU_ITEMS.len() * 2 + 3) as u16;
    let menu_area = centered_rect(menu_width, menu_height, chunks[1]);

    // Render menu box
    let menu_block = Block::default()
        .title(Span::styled(" Menu ", Style::default().fg(accent).bold()))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent))
        .border_type(ratatui::widgets::BorderType::Rounded);

    let inner = menu_block.inner(menu_area);
    frame.render_widget(menu_block, menu_area);

    // Render menu items
    let item_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            MENU_ITEMS
                .iter()
                .map(|_| Constraint::Length(2))
                .collect::<Vec<_>>(),
        )
        .margin(0)
        .split(inner);

    for (i, (item, item_area)) in MENU_ITEMS.iter().zip(item_chunks.iter()).enumerate() {
        let is_selected = i == selected;

        let prefix = if is_selected { "\u{25B6} " } else { "  " }; // Arrow for selected
        let (icon_style, label_style, desc_style, bg) = if is_selected {
            (
                Style::default().fg(accent),
                Style::default().fg(Color::White).bold(),
                Style::default().fg(subtle),
                Some(sel_bg),
            )
        } else {
            (
                Style::default().fg(subtle),
                Style::default().fg(text),
                Style::default().fg(Color::DarkGray),
                None,
            )
        };

        let item_line = Line::from(vec![
            Span::styled(prefix, icon_style),
            Span::styled(format!("{} ", item.icon), icon_style),
            Span::styled(item.label, label_style),
            Span::styled(format!("  {}", item.description), desc_style),
        ]);

        let mut item_widget = Paragraph::new(item_line);
        if let Some(bg_color) = bg {
            item_widget = item_widget.style(Style::default().bg(bg_color));
        }
        frame.render_widget(item_widget, *item_area);
    }

    // Status bar showing detected installations
    render_status_bar(
        frame,
        chunks[2],
        app.cached_stable_scan.as_ref(),
        app.cached_lazer_scan.as_ref(),
    );
}

/// Create a centered rect of given size within the parent
fn centered_rect(width: u16, height: u16, parent: Rect) -> Rect {
    let x = parent.x + (parent.width.saturating_sub(width)) / 2;
    let y = parent.y + (parent.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(parent.width), height.min(parent.height))
}

/// Truncate a path to fit within a given width
#[allow(dead_code)]
fn truncate_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        path.to_string()
    } else {
        format!("...{}", &path[path.len() - max_len + 3..])
    }
}
