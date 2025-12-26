//! Animated spinner widget for loading states

use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::PINK;

/// Spinner animation frames
const SPINNER_FRAMES: &[&str] = &[
    "\u{280B}", // Braille dots for smooth animation
    "\u{2819}",
    "\u{2839}",
    "\u{2838}",
    "\u{283C}",
    "\u{2834}",
    "\u{2826}",
    "\u{2827}",
    "\u{2807}",
    "\u{280F}",
];

/// Alternative spinner with circles
#[allow(dead_code)]
const CIRCLE_SPINNER: &[&str] = &[
    "\u{25DC}", // Quarter circles
    "\u{25DD}",
    "\u{25DE}",
    "\u{25DF}",
];

/// Get the current spinner frame based on time
pub fn get_spinner_frame() -> &'static str {
    let frame_idx = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() / 80) as usize % SPINNER_FRAMES.len();
    SPINNER_FRAMES[frame_idx]
}

/// Get a circle spinner frame
#[allow(dead_code)]
pub fn get_circle_spinner() -> &'static str {
    let frame_idx = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() / 150) as usize % CIRCLE_SPINNER.len();
    CIRCLE_SPINNER[frame_idx]
}

/// Render a spinner with optional label
#[allow(dead_code)]
pub fn render_spinner(frame: &mut Frame, area: Rect, label: &str) {
    let spinner_char = get_spinner_frame();

    let content = Line::from(vec![
        Span::styled(spinner_char, Style::default().fg(PINK)),
        Span::styled(format!(" {}", label), Style::default().fg(Color::White)),
    ]);

    let spinner = Paragraph::new(content)
        .alignment(Alignment::Center);

    frame.render_widget(spinner, area);
}

/// Render a simple loading dots animation
#[allow(dead_code)]
pub fn render_loading_dots(frame: &mut Frame, area: Rect, label: &str) {
    let dot_count = (std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() / 400) as usize % 4;

    let dots = ".".repeat(dot_count);
    let padding = " ".repeat(3 - dot_count);

    let content = format!("{}{}{}", label, dots, padding);
    let loading = Paragraph::new(Span::styled(content, Style::default().fg(PINK)))
        .alignment(Alignment::Center);

    frame.render_widget(loading, area);
}
