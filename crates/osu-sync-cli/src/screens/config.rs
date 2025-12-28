//! Configuration screen

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{pink, subtle_color, success_color, text_color};
use crate::theme::current_theme_name;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    selected: usize,
    stable_path: &Option<String>,
    lazer_path: &Option<String>,
    status_message: &str,
    editing: Option<&str>,
) {
    // Get theme colors
    let accent = pink();
    let subtle = subtle_color();
    let text = text_color();
    let success = success_color();
    let current_theme = current_theme_name();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Length(8), // Paths section
            Constraint::Length(10), // Settings section (increased for rescan)
            Constraint::Length(3), // Status message
            Constraint::Min(0),    // Spacer
            Constraint::Length(4), // About
        ])
        .split(area);

    // Title
    let title = Paragraph::new(Span::styled(
        "Configuration",
        Style::default().fg(accent).bold(),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // Paths section
    let paths_block = Block::default()
        .title(" Installation Paths ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(subtle));

    let paths_inner = paths_block.inner(chunks[1]);
    frame.render_widget(paths_block, chunks[1]);

    let stable_display = stable_path.as_deref().unwrap_or("Not detected");
    let lazer_display = lazer_path.as_deref().unwrap_or("Not detected");

    // Check if we're editing
    let is_editing_stable = editing.is_some() && selected == 0;
    let is_editing_lazer = editing.is_some() && selected == 1;

    let stable_content = if is_editing_stable {
        format!("{}|", editing.unwrap_or(""))
    } else {
        truncate_path(stable_display, 45)
    };

    let lazer_content = if is_editing_lazer {
        format!("{}|", editing.unwrap_or(""))
    } else {
        truncate_path(lazer_display, 45)
    };

    let paths = Paragraph::new(vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                if selected == 0 { "> " } else { "  " },
                Style::default().fg(if selected == 0 { accent } else { text }),
            ),
            Span::styled("osu!stable: ", Style::default().fg(subtle)),
            Span::styled(
                stable_content,
                Style::default().fg(if is_editing_stable {
                    Color::White
                } else if selected == 0 {
                    accent
                } else {
                    text
                }),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                if selected == 1 { "> " } else { "  " },
                Style::default().fg(if selected == 1 { accent } else { text }),
            ),
            Span::styled("osu!lazer:  ", Style::default().fg(subtle)),
            Span::styled(
                lazer_content,
                Style::default().fg(if is_editing_lazer {
                    Color::White
                } else if selected == 1 {
                    accent
                } else {
                    text
                }),
            ),
        ]),
    ]);
    frame.render_widget(paths, paths_inner);

    // Settings section
    let settings_block = Block::default()
        .title(" Settings ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(subtle));

    let settings_inner = settings_block.inner(chunks[2]);
    frame.render_widget(settings_block, chunks[2]);

    // Theme selection indicator
    let theme_indicator = format!("< {} >", current_theme.display_name());

    let settings = Paragraph::new(vec![
        Line::from(""),
        Line::from(vec![
            Span::styled(
                if selected == 2 { "> " } else { "  " },
                Style::default().fg(if selected == 2 { accent } else { text }),
            ),
            Span::styled("Theme:              ", Style::default().fg(subtle)),
            Span::styled(
                theme_indicator,
                Style::default()
                    .fg(if selected == 2 { accent } else { text })
                    .bold(),
            ),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                if selected == 3 { "> " } else { "  " },
                Style::default().fg(if selected == 3 { accent } else { text }),
            ),
            Span::styled("\u{1F50D} ", Style::default().fg(if selected == 3 { accent } else { subtle })),
            Span::styled(
                "Rescan Installations",
                Style::default()
                    .fg(if selected == 3 { accent } else { text })
                    .bold(),
            ),
            Span::styled("  Re-detect osu! paths", Style::default().fg(Color::DarkGray)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("  Duplicate Strategy: ", Style::default().fg(subtle)),
            Span::styled("Ask", Style::default().fg(text)),
        ]),
    ]);
    frame.render_widget(settings, settings_inner);

    // Status message
    let status_color = if status_message.contains("detected!")
        || status_message.contains("saved")
        || status_message.contains("applied")
    {
        success
    } else if status_message.contains("not found") || status_message.contains("No installations") {
        accent
    } else {
        subtle
    };
    let status = Paragraph::new(Span::styled(
        status_message,
        Style::default().fg(status_color),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(status, chunks[3]);

    // About
    let about_block = Block::default()
        .title(" About ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(subtle));

    let about_inner = about_block.inner(chunks[5]);
    frame.render_widget(about_block, chunks[5]);

    let about = Paragraph::new(vec![Line::from(vec![
        Span::styled("  Version: ", Style::default().fg(subtle)),
        Span::styled("0.1.0", Style::default().fg(text)),
    ])]);
    frame.render_widget(about, about_inner);
}

fn truncate_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        path.to_string()
    } else {
        format!("...{}", &path[path.len() - max_len + 3..])
    }
}
