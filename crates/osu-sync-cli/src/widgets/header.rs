//! Header widget

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{pink, subtle_color};

/// Render the application header
pub fn render_header(frame: &mut Frame, area: Rect) {
    let accent = pink();
    let subtle = subtle_color();

    let title = Paragraph::new(Line::from(vec![
        Span::styled(" ", Style::default()),
        Span::styled("\u{25CF}", Style::default().fg(accent)), // Accent circle
        Span::styled(" osu", Style::default().fg(Color::White).bold()),
        Span::styled("-sync ", Style::default().fg(accent).bold()),
        Span::styled("v0.1.0 ", Style::default().fg(subtle)),
        Span::styled("\u{2502} ", Style::default().fg(subtle)), // Separator
        Span::styled(
            "Beatmap Synchronization Tool",
            Style::default().fg(subtle).italic(),
        ),
    ]))
    .alignment(Alignment::Center)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(accent))
            .border_type(ratatui::widgets::BorderType::Rounded),
    );

    frame.render_widget(title, area);
}
