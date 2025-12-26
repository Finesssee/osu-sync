//! Statistics dashboard screen

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Row, Table};

use crate::app::{ExportState, StatisticsTab, PINK, SUBTLE, SUCCESS, TEXT, WARNING};
use crate::widgets::{get_spinner_frame, render_tabs};
use osu_sync_core::stats::ComparisonStats;
use osu_sync_core::ExportFormat;

pub fn render(
    frame: &mut Frame,
    area: Rect,
    stats: &Option<ComparisonStats>,
    loading: bool,
    tab: StatisticsTab,
    status_message: &str,
    export_state: &ExportState,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title + tabs
            Constraint::Min(0),    // Content
            Constraint::Length(2), // Status
        ])
        .split(area);

    // Tab bar
    let tab_labels = ["Overview", "Stable", "Lazer", "Duplicates"];
    let selected_idx = match tab {
        StatisticsTab::Overview => 0,
        StatisticsTab::Stable => 1,
        StatisticsTab::Lazer => 2,
        StatisticsTab::Duplicates => 3,
    };
    render_tabs(frame, chunks[0], &tab_labels, selected_idx);

    // Content
    if loading {
        let spinner = get_spinner_frame();
        let loading_msg = Paragraph::new(Line::from(vec![
            Span::styled(spinner, Style::default().fg(PINK)),
            Span::styled(" Calculating statistics...", Style::default().fg(SUBTLE)),
        ]))
        .alignment(Alignment::Center);
        frame.render_widget(loading_msg, chunks[1]);
    } else if let Some(stats) = stats {
        // Check if export dialog is open
        if export_state.dialog_open {
            render_export_dialog(frame, chunks[1], export_state);
        } else {
            match tab {
                StatisticsTab::Overview => render_overview(frame, chunks[1], stats),
                StatisticsTab::Stable => {
                    render_installation(frame, chunks[1], "osu!stable", &stats.stable)
                }
                StatisticsTab::Lazer => {
                    render_installation(frame, chunks[1], "osu!lazer", &stats.lazer)
                }
                StatisticsTab::Duplicates => render_duplicates(frame, chunks[1], stats),
            }
        }
    } else {
        let no_data = Paragraph::new("No statistics available. Run a scan first.")
            .style(Style::default().fg(SUBTLE))
            .alignment(Alignment::Center);
        frame.render_widget(no_data, chunks[1]);
    }

    // Status - show export hint when stats are available
    let status_text = if stats.is_some() && !export_state.dialog_open {
        format!("{} | Press 'e' to export", status_message)
    } else {
        status_message.to_string()
    };
    let status = Paragraph::new(Span::styled(status_text, Style::default().fg(SUBTLE)))
        .alignment(Alignment::Center);
    frame.render_widget(status, chunks[2]);
}

fn render_overview(frame: &mut Frame, area: Rect, stats: &ComparisonStats) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .margin(1)
        .split(area);

    // Left: Comparison table
    let rows = vec![
        Row::new(vec!["", "Stable", "Lazer"]).style(Style::default().fg(PINK).bold()),
        Row::new(vec![
            "Beatmap Sets".to_string(),
            stats.stable.total_beatmap_sets.to_string(),
            stats.lazer.total_beatmap_sets.to_string(),
        ]),
        Row::new(vec![
            "Total Beatmaps".to_string(),
            stats.stable.total_beatmaps.to_string(),
            stats.lazer.total_beatmaps.to_string(),
        ]),
        Row::new(vec![
            "Storage".to_string(),
            stats.stable.storage_display(),
            stats.lazer.storage_display(),
        ]),
    ];

    let table = Table::new(
        rows,
        [
            Constraint::Length(15),
            Constraint::Length(12),
            Constraint::Length(12),
        ],
    )
    .block(
        Block::default()
            .title(" Comparison ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(SUBTLE)),
    );
    frame.render_widget(table, chunks[0]);

    // Right: Summary
    let summary = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("Common: ", Style::default().fg(SUBTLE)),
            Span::styled(
                stats.common_beatmaps.to_string(),
                Style::default().fg(SUCCESS),
            ),
            Span::styled(" beatmap sets", Style::default().fg(SUBTLE)),
        ]),
        Line::from(vec![
            Span::styled("Unique to Stable: ", Style::default().fg(SUBTLE)),
            Span::styled(
                stats.unique_to_stable.to_string(),
                Style::default().fg(TEXT),
            ),
        ]),
        Line::from(vec![
            Span::styled("Unique to Lazer: ", Style::default().fg(SUBTLE)),
            Span::styled(stats.unique_to_lazer.to_string(), Style::default().fg(TEXT)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Duplicates: ", Style::default().fg(SUBTLE)),
            Span::styled(
                stats.duplicates.count.to_string(),
                Style::default().fg(WARNING),
            ),
            Span::styled(
                format!(" ({})", stats.duplicates.wasted_display()),
                Style::default().fg(WARNING),
            ),
        ]),
    ];

    let summary_widget = Paragraph::new(summary).block(
        Block::default()
            .title(" Summary ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(SUBTLE)),
    );
    frame.render_widget(summary_widget, chunks[1]);
}

fn render_installation(
    frame: &mut Frame,
    area: Rect,
    name: &str,
    stats: &osu_sync_core::stats::InstallationStats,
) {
    let content = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("Total Sets: ", Style::default().fg(SUBTLE)),
            Span::styled(
                stats.total_beatmap_sets.to_string(),
                Style::default().fg(TEXT).bold(),
            ),
        ]),
        Line::from(vec![
            Span::styled("Total Beatmaps: ", Style::default().fg(SUBTLE)),
            Span::styled(stats.total_beatmaps.to_string(), Style::default().fg(TEXT)),
        ]),
        Line::from(vec![
            Span::styled("Storage Used: ", Style::default().fg(SUBTLE)),
            Span::styled(stats.storage_display(), Style::default().fg(TEXT)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("Avg Star Rating: ", Style::default().fg(SUBTLE)),
            Span::styled(
                format!("{:.2}*", stats.average_star_rating),
                Style::default().fg(PINK),
            ),
        ]),
    ];

    let widget = Paragraph::new(content)
        .block(
            Block::default()
                .title(format!(" {} ", name))
                .borders(Borders::ALL)
                .border_style(Style::default().fg(PINK)),
        )
        .alignment(Alignment::Center);
    frame.render_widget(widget, area);
}

fn render_duplicates(frame: &mut Frame, area: Rect, stats: &ComparisonStats) {
    let content = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("Duplicate Sets: ", Style::default().fg(SUBTLE)),
            Span::styled(
                stats.duplicates.count.to_string(),
                Style::default().fg(WARNING).bold(),
            ),
        ]),
        Line::from(vec![
            Span::styled("Wasted Space: ", Style::default().fg(SUBTLE)),
            Span::styled(
                stats.duplicates.wasted_display(),
                Style::default().fg(WARNING),
            ),
        ]),
        Line::from(""),
        Line::from(Span::styled("Match Types:", Style::default().fg(SUBTLE))),
    ];

    let mut lines = content;
    for (match_type, count) in &stats.duplicates.by_match_type {
        lines.push(Line::from(vec![
            Span::styled(format!("  {}: ", match_type), Style::default().fg(SUBTLE)),
            Span::styled(count.to_string(), Style::default().fg(TEXT)),
        ]));
    }

    let widget = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Duplicates ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(WARNING)),
        )
        .alignment(Alignment::Center);
    frame.render_widget(widget, area);
}

fn render_export_dialog(frame: &mut Frame, area: Rect, export_state: &ExportState) {
    // Center the dialog
    let dialog_width = 50u16;
    let dialog_height = 12u16;
    let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect::new(
        x,
        y,
        dialog_width.min(area.width),
        dialog_height.min(area.height),
    );

    // Build dialog content
    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Export Statistics",
            Style::default().fg(PINK).bold(),
        )),
        Line::from(""),
        Line::from(Span::styled("Select format:", Style::default().fg(SUBTLE))),
        Line::from(""),
    ];

    // Format options
    let formats = [ExportFormat::Json, ExportFormat::Csv];
    for (i, format) in formats.iter().enumerate() {
        let is_selected = i == export_state.selected_format;
        let prefix = if is_selected { "> " } else { "  " };
        let style = if is_selected {
            Style::default().fg(PINK).bold()
        } else {
            Style::default().fg(TEXT)
        };
        lines.push(Line::from(Span::styled(
            format!("{}{}", prefix, format),
            style,
        )));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Press Enter to export, Esc to cancel",
        Style::default().fg(SUBTLE),
    )));

    // Show result message if present
    if let Some(ref message) = export_state.result_message {
        lines.push(Line::from(""));
        let style = if export_state.export_success {
            Style::default().fg(SUCCESS)
        } else {
            Style::default().fg(WARNING)
        };
        lines.push(Line::from(Span::styled(message.clone(), style)));
    }

    let dialog = Paragraph::new(lines)
        .block(
            Block::default()
                .title(" Export ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(PINK)),
        )
        .alignment(Alignment::Center);

    // Clear the background
    frame.render_widget(ratatui::widgets::Clear, dialog_area);
    frame.render_widget(dialog, dialog_area);
}
