//! Scan results screen

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph};

use crate::app::{ScanResult, PINK, SUBTLE, SUCCESS, TEXT};
use crate::widgets::get_spinner_frame;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    in_progress: bool,
    stable_result: &Option<ScanResult>,
    lazer_result: &Option<ScanResult>,
    status_message: &str,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Min(0),    // Results
            Constraint::Length(2), // Status
        ])
        .split(area);

    // Title with spinner when in progress
    let title = if in_progress {
        let spinner = get_spinner_frame();
        Paragraph::new(Line::from(vec![
            Span::styled(spinner, Style::default().fg(PINK)),
            Span::styled(" Scanning installations...", Style::default().fg(PINK)),
        ]))
    } else {
        Paragraph::new(Line::from(vec![
            Span::styled("\u{2714} ", Style::default().fg(SUCCESS)), // Checkmark
            Span::styled("Scan Complete", Style::default().fg(PINK).bold()),
        ]))
    };
    frame.render_widget(title.alignment(Alignment::Center), chunks[0]);

    // Results panels
    let panels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .margin(1)
        .split(chunks[1]);

    // Stable panel
    render_installation_panel(frame, panels[0], "osu!stable", stable_result);

    // Lazer panel
    render_installation_panel(frame, panels[1], "osu!lazer", lazer_result);

    // Status message
    let status = Paragraph::new(Span::styled(status_message, Style::default().fg(SUBTLE)))
        .alignment(Alignment::Center);
    frame.render_widget(status, chunks[2]);
}

fn render_installation_panel(
    frame: &mut Frame,
    area: Rect,
    name: &str,
    result: &Option<ScanResult>,
) {
    let block = Block::default()
        .title(Span::styled(
            format!(" {} ", name),
            Style::default().fg(PINK),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(SUBTLE));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    let content = match result {
        Some(scan) if scan.detected => {
            let mut lines = vec![
                Line::from(vec![
                    Span::styled("Path: ", Style::default().fg(SUBTLE)),
                    Span::styled(
                        scan.path.as_deref().unwrap_or("Unknown"),
                        Style::default().fg(TEXT),
                    ),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Status: ", Style::default().fg(SUBTLE)),
                    Span::styled("Found", Style::default().fg(SUCCESS)),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Beatmap Sets: ", Style::default().fg(SUBTLE)),
                    Span::styled(
                        format!("{}", scan.beatmap_sets),
                        Style::default().fg(TEXT).bold(),
                    ),
                ]),
                Line::from(vec![
                    Span::styled("Total Beatmaps: ", Style::default().fg(SUBTLE)),
                    Span::styled(
                        format!("{}", scan.total_beatmaps),
                        Style::default().fg(TEXT),
                    ),
                ]),
            ];

            // Add timing report if available
            if let Some(ref timing) = scan.timing_report {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "Timing:",
                    Style::default().fg(PINK).bold(),
                )));
                for line in timing.lines() {
                    lines.push(Line::from(Span::styled(
                        line.to_string(),
                        Style::default().fg(SUBTLE),
                    )));
                }
            }

            lines
        }
        Some(_) => {
            vec![Line::from(vec![
                Span::styled("Status: ", Style::default().fg(SUBTLE)),
                Span::styled("Not Found", Style::default().fg(Color::Red)),
            ])]
        }
        None => {
            vec![Line::from(Span::styled(
                "Scanning...",
                Style::default().fg(SUBTLE),
            ))]
        }
    };

    let paragraph = Paragraph::new(content);
    frame.render_widget(paragraph, inner);
}
