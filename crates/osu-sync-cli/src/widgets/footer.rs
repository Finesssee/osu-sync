//! Footer widget with keyboard hints

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{pink, subtle_color, text_color};

/// Render the footer with keyboard hints
pub fn render_footer(frame: &mut Frame, area: Rect, hints: &[(&str, &str)]) {
    let accent = pink();
    let subtle = subtle_color();
    let text = text_color();

    let spans: Vec<Span> = hints
        .iter()
        .enumerate()
        .flat_map(|(i, (key, action))| {
            let mut result = vec![
                Span::styled(format!("[{}]", key), Style::default().fg(accent)),
                Span::styled(format!(" {}", action), Style::default().fg(text)),
            ];
            if i < hints.len() - 1 {
                result.push(Span::styled("   ", Style::default()));
            }
            result
        })
        .collect();

    let footer = Paragraph::new(Line::from(spans))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(subtle)),
        );

    frame.render_widget(footer, area);
}
