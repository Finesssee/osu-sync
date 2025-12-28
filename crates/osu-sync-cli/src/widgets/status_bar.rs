//! Status bar widget showing installation info

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{ScanResult, PINK, SUBTLE, SUCCESS, WARNING};

/// Render a status bar showing installation detection status
pub fn render_status_bar(
    frame: &mut Frame,
    area: Rect,
    stable_scan: Option<&ScanResult>,
    lazer_scan: Option<&ScanResult>,
) {
    let (stable_icon, stable_text, stable_color) = match stable_scan {
        Some(scan) if scan.detected => {
            let count = format!("{} sets", scan.beatmap_sets);
            ("\u{2714}", count, SUCCESS) // Checkmark
        }
        Some(_) => ("\u{2718}", "Not found".to_string(), WARNING), // X mark
        None => ("\u{2022}", "Not scanned".to_string(), SUBTLE),   // Bullet
    };

    let (lazer_icon, lazer_text, lazer_color) = match lazer_scan {
        Some(scan) if scan.detected => {
            let count = format!("{} sets", scan.beatmap_sets);
            ("\u{2714}", count, SUCCESS)
        }
        Some(_) => ("\u{2718}", "Not found".to_string(), WARNING),
        None => ("\u{2022}", "Not scanned".to_string(), SUBTLE),
    };

    let status_line = Line::from(vec![
        Span::styled(" \u{25CF} ", Style::default().fg(PINK)), // Pink circle
        Span::styled("Stable: ", Style::default().fg(SUBTLE)),
        Span::styled(stable_icon, Style::default().fg(stable_color)),
        Span::styled(
            format!(" {} ", stable_text),
            Style::default().fg(stable_color),
        ),
        Span::styled("\u{2502} ", Style::default().fg(SUBTLE)), // Separator
        Span::styled("Lazer: ", Style::default().fg(SUBTLE)),
        Span::styled(lazer_icon, Style::default().fg(lazer_color)),
        Span::styled(
            format!(" {} ", lazer_text),
            Style::default().fg(lazer_color),
        ),
    ]);

    let status = Paragraph::new(status_line)
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::TOP)
                .border_style(Style::default().fg(SUBTLE)),
        );

    frame.render_widget(status, area);
}
