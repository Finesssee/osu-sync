//! Help screen showing keyboard shortcuts

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::{pink, subtle_color, text_color};

/// Shortcut category for organization
struct ShortcutCategory {
    name: &'static str,
    shortcuts: &'static [(&'static str, &'static str)],
}

/// All global shortcuts
const GLOBAL_SHORTCUTS: ShortcutCategory = ShortcutCategory {
    name: "Global",
    shortcuts: &[
        ("?/h", "Open help"),
        ("q", "Quit (from menu)"),
        ("Ctrl+C", "Force quit"),
    ],
};

/// Navigation shortcuts
const NAVIGATION_SHORTCUTS: ShortcutCategory = ShortcutCategory {
    name: "Navigation",
    shortcuts: &[
        ("j/Down", "Move down"),
        ("k/Up", "Move up"),
        ("h/Left", "Move left / Previous"),
        ("l/Right", "Move right / Next"),
        ("Enter", "Confirm / Select"),
        ("Esc", "Go back / Cancel"),
        ("Tab", "Switch tabs"),
        ("PgUp/PgDn", "Page scroll"),
    ],
};

/// Main menu shortcuts
const MENU_SHORTCUTS: ShortcutCategory = ShortcutCategory {
    name: "Main Menu",
    shortcuts: &[
        ("1-9", "Quick select option"),
        ("Enter", "Open selected item"),
    ],
};

/// Sync config shortcuts
const SYNC_SHORTCUTS: ShortcutCategory = ShortcutCategory {
    name: "Sync Config",
    shortcuts: &[
        ("f", "Open filter panel"),
        ("d", "Preview (dry run)"),
        ("Enter", "Start sync"),
        ("Space", "Toggle filter option"),
    ],
};

/// Statistics shortcuts
const STATS_SHORTCUTS: ShortcutCategory = ShortcutCategory {
    name: "Statistics",
    shortcuts: &[
        ("Tab/h/l", "Switch tabs"),
        ("e", "Export statistics"),
    ],
};

/// Config screen shortcuts
const CONFIG_SHORTCUTS: ShortcutCategory = ShortcutCategory {
    name: "Configuration",
    shortcuts: &[
        ("Enter", "Edit path / Cycle theme"),
        ("d", "Auto-detect paths"),
        ("Left/Right", "Cycle theme"),
    ],
};

/// Backup/Restore shortcuts
const BACKUP_SHORTCUTS: ShortcutCategory = ShortcutCategory {
    name: "Backup/Restore",
    shortcuts: &[
        ("Enter", "Start operation"),
        ("Esc", "Cancel operation"),
    ],
};

pub fn render(frame: &mut Frame, area: Rect) {
    // Get theme colors
    let accent = pink();
    let subtle = subtle_color();
    let text = text_color();

    // Calculate modal size and position (centered)
    let width = 52;
    let height = 28;
    let modal_area = centered_rect(width, height, area);

    // Clear the background
    frame.render_widget(Clear, modal_area);

    // Modal block
    let block = Block::default()
        .title(Span::styled(
            " Keyboard Shortcuts ",
            Style::default().fg(accent).bold(),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(accent));

    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    // Build the help content
    let mut lines: Vec<Line> = Vec::new();

    // Add all categories
    let categories = [
        &GLOBAL_SHORTCUTS,
        &NAVIGATION_SHORTCUTS,
        &MENU_SHORTCUTS,
        &SYNC_SHORTCUTS,
        &STATS_SHORTCUTS,
        &CONFIG_SHORTCUTS,
        &BACKUP_SHORTCUTS,
    ];

    for (idx, category) in categories.iter().enumerate() {
        // Section header
        lines.push(Line::from(Span::styled(
            category.name,
            Style::default().fg(accent).bold(),
        )));

        // Shortcuts
        for (key, desc) in category.shortcuts.iter() {
            lines.push(shortcut_line(key, desc, text, subtle));
        }

        // Add spacing between categories (except last)
        if idx < categories.len() - 1 {
            lines.push(Line::from(""));
        }
    }

    let help_text = Paragraph::new(lines);

    // Layout inside modal: help content + footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // Help content
            Constraint::Length(2), // Footer
        ])
        .split(inner);

    frame.render_widget(help_text, chunks[0]);

    // Footer
    let footer = Paragraph::new(Line::from(Span::styled(
        "Press any key to close",
        Style::default().fg(subtle).italic(),
    )))
    .alignment(Alignment::Center);

    // Render separator line and footer
    let separator = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(accent));
    frame.render_widget(separator, chunks[1]);

    let footer_inner = Rect::new(chunks[1].x, chunks[1].y + 1, chunks[1].width, 1);
    frame.render_widget(footer, footer_inner);
}

/// Create a formatted shortcut line with key and description
fn shortcut_line<'a>(key: &str, description: &str, text: Color, subtle: Color) -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(format!("{:<14}", key), Style::default().fg(text)),
        Span::styled(description.to_string(), Style::default().fg(subtle)),
    ])
}

fn centered_rect(width: u16, height: u16, parent: Rect) -> Rect {
    let x = parent.x + (parent.width.saturating_sub(width)) / 2;
    let y = parent.y + (parent.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(parent.width), height.min(parent.height))
}
