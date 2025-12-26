//! Tab bar widget for navigation

use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::{PINK, SUBTLE};

/// Render a tab bar with the given labels and selected index
pub fn render_tabs(frame: &mut Frame, area: Rect, labels: &[&str], selected: usize) {
    let spans: Vec<Span> = labels
        .iter()
        .enumerate()
        .flat_map(|(i, label)| {
            let style = if i == selected {
                Style::default().fg(PINK).bold()
            } else {
                Style::default().fg(SUBTLE)
            };
            let bracket_style = if i == selected {
                Style::default().fg(PINK)
            } else {
                Style::default().fg(SUBTLE)
            };
            vec![
                Span::styled("[", bracket_style),
                Span::styled(*label, style),
                Span::styled("]", bracket_style),
                Span::styled("  ", Style::default()),
            ]
        })
        .collect();

    let tabs = Paragraph::new(Line::from(spans))
        .alignment(Alignment::Center);
    frame.render_widget(tabs, area);
}
