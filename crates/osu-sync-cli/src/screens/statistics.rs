//! Statistics dashboard screen

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Paragraph, Row, Table};

use crate::app::{ExportState, StatisticsTab, PINK, SUBTLE, SUCCESS, TEXT, WARNING};
use crate::widgets::{get_spinner_frame, render_tabs};
use osu_sync_core::stats::{ComparisonStats, ModeCount, Recommendations};
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
    let tab_labels = [
        "Overview",
        "Stable",
        "Lazer",
        "Duplicates",
        "Recommendations",
    ];
    let selected_idx = match tab {
        StatisticsTab::Overview => 0,
        StatisticsTab::Stable => 1,
        StatisticsTab::Lazer => 2,
        StatisticsTab::Duplicates => 3,
        StatisticsTab::Recommendations => 4,
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
                StatisticsTab::Recommendations => {
                    render_recommendations(frame, chunks[1], &stats.recommendations)
                }
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
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .margin(1)
        .split(area);

    let top_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(main_chunks[0]);

    // Top Left: Comparison table
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
    frame.render_widget(table, top_chunks[0]);

    // Top Right: Summary
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
    frame.render_widget(summary_widget, top_chunks[1]);

    // Bottom: Mode Breakdown
    render_mode_breakdown(
        frame,
        main_chunks[1],
        &stats.mode_breakdown.stable_counts,
        &stats.mode_breakdown.lazer_counts,
    );
}

fn render_mode_breakdown(
    frame: &mut Frame,
    area: Rect,
    stable_counts: &ModeCount,
    lazer_counts: &ModeCount,
) {
    let stable_total = stable_counts.total();
    let lazer_total = lazer_counts.total();

    let rows = vec![
        Row::new(vec!["Mode", "Stable", "%", "Lazer", "%"]).style(Style::default().fg(PINK).bold()),
        Row::new(vec![
            "osu!".to_string(),
            stable_counts.osu.to_string(),
            format_percent(stable_counts.osu, stable_total),
            lazer_counts.osu.to_string(),
            format_percent(lazer_counts.osu, lazer_total),
        ]),
        Row::new(vec![
            "Taiko".to_string(),
            stable_counts.taiko.to_string(),
            format_percent(stable_counts.taiko, stable_total),
            lazer_counts.taiko.to_string(),
            format_percent(lazer_counts.taiko, lazer_total),
        ]),
        Row::new(vec![
            "Catch".to_string(),
            stable_counts.catch.to_string(),
            format_percent(stable_counts.catch, stable_total),
            lazer_counts.catch.to_string(),
            format_percent(lazer_counts.catch, lazer_total),
        ]),
        Row::new(vec![
            "Mania".to_string(),
            stable_counts.mania.to_string(),
            format_percent(stable_counts.mania, stable_total),
            lazer_counts.mania.to_string(),
            format_percent(lazer_counts.mania, lazer_total),
        ]),
    ];

    let table = Table::new(
        rows,
        [
            Constraint::Length(8),
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Length(10),
            Constraint::Length(8),
        ],
    )
    .block(
        Block::default()
            .title(" Mode Breakdown ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(SUBTLE)),
    );
    frame.render_widget(table, area);
}

fn format_percent(count: usize, total: usize) -> String {
    if total == 0 {
        "0%".to_string()
    } else {
        format!("{:.1}%", (count as f32 / total as f32) * 100.0)
    }
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
    let dialog_height = 14u16; // Increased to accommodate third option
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

    // Format options - now includes HTML
    let formats = [ExportFormat::Json, ExportFormat::Csv, ExportFormat::Html];
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

fn render_recommendations(frame: &mut Frame, area: Rect, recommendations: &Recommendations) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // Sync summary
            Constraint::Min(0),    // Top star maps and popular artists
        ])
        .margin(1)
        .split(area);

    // Sync summary
    let summary = vec![Line::from(vec![
        Span::styled("Sync Candidates: ", Style::default().fg(SUBTLE)),
        Span::styled(
            format!("{} stable -> lazer", recommendations.stable_to_lazer_count),
            Style::default().fg(PINK),
        ),
        Span::styled(" | ", Style::default().fg(SUBTLE)),
        Span::styled(
            format!("{} lazer -> stable", recommendations.lazer_to_stable_count),
            Style::default().fg(TEXT),
        ),
    ])];

    let summary_widget = Paragraph::new(summary).block(
        Block::default()
            .title(" Sync Overview ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(PINK)),
    );
    frame.render_widget(summary_widget, chunks[0]);

    // Bottom section split into two columns
    let bottom_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    // Left: Top star rating maps
    render_top_star_maps(frame, bottom_chunks[0], recommendations);

    // Right: Popular unsynced artists
    render_popular_artists(frame, bottom_chunks[1], recommendations);
}

fn render_top_star_maps(frame: &mut Frame, area: Rect, recommendations: &Recommendations) {
    let mut lines = vec![
        Line::from(Span::styled(
            "Highest Star (Stable only):",
            Style::default().fg(PINK).bold(),
        )),
        Line::from(""),
    ];

    if recommendations.top_star_stable.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No unique stable maps found",
            Style::default().fg(SUBTLE),
        )));
    } else {
        for rec in recommendations.top_star_stable.iter().take(5) {
            let star_str = rec
                .star_rating
                .map(|s| format!("{:.2}*", s))
                .unwrap_or_else(|| "?*".to_string());
            lines.push(Line::from(vec![
                Span::styled(format!("  {} ", star_str), Style::default().fg(WARNING)),
                Span::styled(truncate_str(&rec.artist, 12), Style::default().fg(SUBTLE)),
                Span::styled(" - ", Style::default().fg(SUBTLE)),
                Span::styled(truncate_str(&rec.title, 15), Style::default().fg(TEXT)),
            ]));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Highest Star (Lazer only):",
        Style::default().fg(PINK).bold(),
    )));
    lines.push(Line::from(""));

    if recommendations.top_star_lazer.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No unique lazer maps found",
            Style::default().fg(SUBTLE),
        )));
    } else {
        for rec in recommendations.top_star_lazer.iter().take(5) {
            let star_str = rec
                .star_rating
                .map(|s| format!("{:.2}*", s))
                .unwrap_or_else(|| "?*".to_string());
            lines.push(Line::from(vec![
                Span::styled(format!("  {} ", star_str), Style::default().fg(WARNING)),
                Span::styled(truncate_str(&rec.artist, 12), Style::default().fg(SUBTLE)),
                Span::styled(" - ", Style::default().fg(SUBTLE)),
                Span::styled(truncate_str(&rec.title, 15), Style::default().fg(TEXT)),
            ]));
        }
    }

    let widget = Paragraph::new(lines).block(
        Block::default()
            .title(" Top Star Maps ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(SUBTLE)),
    );
    frame.render_widget(widget, area);
}

fn render_popular_artists(frame: &mut Frame, area: Rect, recommendations: &Recommendations) {
    let mut lines = vec![
        Line::from(Span::styled(
            "Top Unsynced Artists:",
            Style::default().fg(PINK).bold(),
        )),
        Line::from(""),
    ];

    if recommendations.unsynced_artist_counts.is_empty() {
        lines.push(Line::from(Span::styled(
            "  All artists synced!",
            Style::default().fg(SUCCESS),
        )));
    } else {
        for (artist, count) in recommendations.unsynced_artist_counts.iter().take(8) {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {:>4} maps: ", count),
                    Style::default().fg(SUBTLE),
                ),
                Span::styled(truncate_str(artist, 20), Style::default().fg(TEXT)),
            ]));
        }
    }

    let widget = Paragraph::new(lines).block(
        Block::default()
            .title(" Popular Artists Not Synced ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(SUBTLE)),
    );
    frame.render_widget(widget, area);
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}
