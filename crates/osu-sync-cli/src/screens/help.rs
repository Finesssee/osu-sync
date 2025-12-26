//! Help screen showing keyboard shortcuts

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::app::{PINK, SUBTLE, TEXT};

pub fn render(frame: &mut Frame, area: Rect) {
    // Calculate modal size and position (centered)
    let width = 44;
    let height = 20;
    let modal_area = centered_rect(width, height, area);

    // Clear the background
    frame.render_widget(Clear, modal_area);

    // Modal block
    let block = Block::default()
        .title(Span::styled(
            " Keyboard Shortcuts ",
            Style::default().fg(PINK).bold(),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PINK));

    let inner = block.inner(modal_area);
    frame.render_widget(block, modal_area);

    // Build the help content
    let lines = vec![
        // Navigation section
        Line::from(Span::styled("Navigation", Style::default().fg(PINK).bold())),
        shortcut_line("\u{2191}/\u{2193} or j/k", "Move selection"),
        shortcut_line("Enter", "Confirm/Select"),
        shortcut_line("Esc", "Go back"),
        shortcut_line("Tab", "Switch tabs"),
        Line::from(""),
        // Sync Config section
        Line::from(Span::styled(
            "Sync Config",
            Style::default().fg(PINK).bold(),
        )),
        shortcut_line("f", "Toggle filter panel"),
        shortcut_line("d", "Dry run"),
        Line::from(""),
        // General section
        Line::from(Span::styled("General", Style::default().fg(PINK).bold())),
        shortcut_line("?/h", "This help screen"),
        shortcut_line("q", "Quit application"),
    ];

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
        Style::default().fg(SUBTLE).italic(),
    )))
    .alignment(Alignment::Center);

    // Render separator line and footer
    let separator = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(PINK));
    frame.render_widget(separator, chunks[1]);

    let footer_inner = Rect::new(chunks[1].x, chunks[1].y + 1, chunks[1].width, 1);
    frame.render_widget(footer, footer_inner);
}

/// Create a formatted shortcut line with key and description
fn shortcut_line(key: &str, description: &str) -> Line<'static> {
    Line::from(vec![
        Span::raw("  "),
        Span::styled(format!("{:<14}", key), Style::default().fg(TEXT)),
        Span::styled(description.to_string(), Style::default().fg(SUBTLE)),
    ])
}

fn centered_rect(width: u16, height: u16, parent: Rect) -> Rect {
    let x = parent.x + (parent.width.saturating_sub(width)) / 2;
    let y = parent.y + (parent.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(parent.width), height.min(parent.height))
}
