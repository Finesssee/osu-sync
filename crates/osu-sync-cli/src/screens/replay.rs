//! Replay export screens

use osu_sync_core::beatmap::GameMode;
use osu_sync_core::replay::{
    ExportOrganization, ReplayExportResult, ReplayExportStats, ReplayFilter, ReplayProgress,
};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph};

use crate::app::{PINK, SUBTLE, TEXT};

/// Render replay export configuration screen
pub fn render_config(
    frame: &mut Frame,
    area: Rect,
    selected: usize,
    organization: ExportOrganization,
    output_path: &str,
    replay_count: usize,
    status_message: &Option<String>,
    filter: &ReplayFilter,
    rename_pattern: &str,
    filter_panel_open: bool,
    filter_field: usize,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Length(2), // Info
            Constraint::Min(0),    // Options
            Constraint::Length(3), // Status
        ])
        .split(area);

    // Title
    let title = Paragraph::new(Span::styled(
        "Export Replays",
        Style::default().fg(PINK).bold(),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // Info with filter summary
    let filter_info = if filter.is_empty() {
        format!("{} replays with .osr files available", replay_count)
    } else {
        format!(
            "{} replays available | Filter: {}",
            replay_count,
            filter.describe()
        )
    };
    let info = Paragraph::new(Span::styled(filter_info, Style::default().fg(SUBTLE)))
        .alignment(Alignment::Center);
    frame.render_widget(info, chunks[1]);

    // Options
    let org_str = match organization {
        ExportOrganization::Flat => "Flat (all in one folder)",
        ExportOrganization::ByBeatmap => "By Beatmap",
        ExportOrganization::ByDate => "By Date",
        ExportOrganization::ByPlayer => "By Player",
        ExportOrganization::ByGrade => "By Grade",
    };

    let filter_str = if filter.is_empty() {
        "No filters (export all)".to_string()
    } else {
        filter.describe()
    };

    let rename_str = if rename_pattern.is_empty() {
        "Default naming".to_string()
    } else {
        rename_pattern.to_string()
    };

    let options = [
        format!("Organization: {}", org_str),
        format!("Output: {}", output_path),
        format!("Filters: {}", filter_str),
        format!("Rename: {}", rename_str),
        "Start Export".to_string(),
    ];

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, label)| {
            let prefix = if i == selected { "> " } else { "  " };
            let style = if i == selected {
                Style::default().fg(PINK).bold()
            } else {
                Style::default().fg(TEXT)
            };
            ListItem::new(Span::styled(format!("{}{}", prefix, label), style))
        })
        .collect();

    let menu_width = 60;
    let menu_height = options.len() as u16 + 2;
    let menu_area = centered_rect(menu_width, menu_height, chunks[2]);

    let menu = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(SUBTLE)),
    );

    frame.render_widget(menu, menu_area);

    // Render filter panel if open
    if filter_panel_open {
        render_filter_panel(frame, area, filter, filter_field);
    }

    // Status message
    if let Some(ref msg) = status_message {
        let status = Paragraph::new(Span::styled(msg.as_str(), Style::default().fg(SUBTLE)))
            .alignment(Alignment::Center);
        frame.render_widget(status, chunks[3]);
    }
}

/// Render the filter panel overlay
fn render_filter_panel(frame: &mut Frame, area: Rect, filter: &ReplayFilter, selected_field: usize) {
    let panel_width = 40;
    let panel_height = 10;
    let panel_area = centered_rect(panel_width, panel_height, area);

    // Clear background
    frame.render_widget(
        Block::default().style(Style::default().bg(Color::Black)),
        panel_area,
    );

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(1), // Title
            Constraint::Length(1), // Grade
            Constraint::Length(1), // Modes header
            Constraint::Length(1), // Mode checkboxes
            Constraint::Length(1), // Hint
        ])
        .split(panel_area);

    // Panel title
    let title = Paragraph::new(Span::styled(
        "Filter Settings",
        Style::default().fg(PINK).bold(),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(title, inner[0]);

    // Grade filter
    let grade_str = match &filter.min_grade {
        None => "Any".to_string(),
        Some(g) => format!(">= {}", g),
    };
    let grade_style = if selected_field == 0 {
        Style::default().fg(PINK).bold()
    } else {
        Style::default().fg(TEXT)
    };
    let grade_line = Paragraph::new(Span::styled(
        format!("Min Grade: {}", grade_str),
        grade_style,
    ));
    frame.render_widget(grade_line, inner[1]);

    // Modes header
    let modes_title = Paragraph::new(Span::styled("Game Modes:", Style::default().fg(SUBTLE)));
    frame.render_widget(modes_title, inner[2]);

    // Mode checkboxes
    let mode_items: Vec<Span> = [
        (GameMode::Osu, "osu!", 1),
        (GameMode::Taiko, "taiko", 2),
        (GameMode::Catch, "catch", 3),
        (GameMode::Mania, "mania", 4),
    ]
    .iter()
    .map(|(mode, name, field_idx)| {
        let checked = filter.modes.is_empty() || filter.modes.contains(mode);
        let checkbox = if checked { "[x]" } else { "[ ]" };
        let style = if selected_field == *field_idx {
            Style::default().fg(PINK).bold()
        } else if checked {
            Style::default().fg(TEXT)
        } else {
            Style::default().fg(SUBTLE)
        };
        Span::styled(format!(" {} {} ", checkbox, name), style)
    })
    .collect();

    let modes_line = Paragraph::new(Line::from(mode_items));
    frame.render_widget(modes_line, inner[3]);

    // Hint
    let hint = Paragraph::new(Span::styled(
        "Space: toggle, Esc: close",
        Style::default().fg(SUBTLE).italic(),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(hint, inner[4]);

    // Border
    let border = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PINK))
        .title(" Filters ");
    frame.render_widget(border, panel_area);
}

/// Render replay export progress screen
pub fn render_progress(
    frame: &mut Frame,
    area: Rect,
    progress: &Option<ReplayProgress>,
    current_replay: &str,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Length(3), // Progress bar
            Constraint::Length(2), // Current file
            Constraint::Min(0),    // Stats
            Constraint::Length(2), // Hint
        ])
        .split(area);

    // Title
    let title = Paragraph::new(Span::styled(
        "Exporting Replays...",
        Style::default().fg(PINK).bold(),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // Progress bar
    let percentage = progress.as_ref().map(|p| p.percentage()).unwrap_or(0.0);

    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL))
        .gauge_style(Style::default().fg(PINK))
        .percent(percentage as u16)
        .label(format!("{:.1}%", percentage));

    let gauge_area = centered_rect(60, 3, chunks[1]);
    frame.render_widget(gauge, gauge_area);

    // Current file
    let current = Paragraph::new(Span::styled(current_replay, Style::default().fg(TEXT)))
        .alignment(Alignment::Center);
    frame.render_widget(current, chunks[2]);

    // Stats
    if let Some(ref p) = progress {
        let stats_text = format!(
            "Replays: {}/{} | Written: {:.1} MB",
            p.replays_processed,
            p.total_replays,
            p.bytes_written as f64 / 1_048_576.0
        );
        let stats = Paragraph::new(Span::styled(stats_text, Style::default().fg(SUBTLE)))
            .alignment(Alignment::Center);
        frame.render_widget(stats, chunks[3]);
    }

    // Hint
    let hint = Paragraph::new(Span::styled(
        "Press Esc to cancel",
        Style::default().fg(SUBTLE).italic(),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(hint, chunks[4]);
}

/// Render replay export complete screen
pub fn render_complete(
    frame: &mut Frame,
    area: Rect,
    result: &ReplayExportResult,
    stats: &Option<ReplayExportStats>,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Min(0),    // Results
            Constraint::Length(2), // Hint
        ])
        .split(area);

    // Title
    let title = Paragraph::new(Span::styled(
        "Export Complete!",
        Style::default().fg(PINK).bold(),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // Build results text
    let mut results_text = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("Replays exported: {}", result.replays_exported),
            Style::default().fg(TEXT),
        )),
        Line::from(Span::styled(
            format!("Replays skipped: {}", result.replays_skipped),
            Style::default().fg(SUBTLE),
        )),
    ];

    // Add filtered count if any
    if result.replays_filtered > 0 {
        results_text.push(Line::from(Span::styled(
            format!("Replays filtered: {}", result.replays_filtered),
            Style::default().fg(SUBTLE),
        )));
    }

    results_text.push(Line::from(Span::styled(
        format!(
            "Total data written: {:.1} MB",
            result.bytes_written as f64 / 1_048_576.0
        ),
        Style::default().fg(TEXT),
    )));

    // Add statistics breakdown
    if let Some(ref s) = stats {
        results_text.push(Line::from(""));
        results_text.push(Line::from(Span::styled(
            "Grade Breakdown:",
            Style::default().fg(PINK),
        )));
        for (grade, count) in s.grade_breakdown() {
            results_text.push(Line::from(Span::styled(
                format!("  {}: {}", grade, count),
                Style::default().fg(TEXT),
            )));
        }

        if !s.by_mode.is_empty() {
            results_text.push(Line::from(""));
            results_text.push(Line::from(Span::styled(
                "Mode Breakdown:",
                Style::default().fg(PINK),
            )));
            for (mode, count) in s.mode_breakdown() {
                results_text.push(Line::from(Span::styled(
                    format!("  {}: {}", mode, count),
                    Style::default().fg(TEXT),
                )));
            }
        }

        results_text.push(Line::from(""));
        results_text.push(Line::from(Span::styled(
            format!("Date range: {}", s.date_range_str()),
            Style::default().fg(SUBTLE),
        )));
    }

    if !result.errors.is_empty() {
        results_text.push(Line::from(""));
        results_text.push(Line::from(Span::styled(
            format!("Errors: {}", result.errors.len()),
            Style::default().fg(Color::Red),
        )));
    }

    let results = Paragraph::new(results_text)
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .title(" Summary ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(SUBTLE)),
        );

    let results_height = if stats.is_some() { 20 } else { 10 };
    let results_area = centered_rect(50, results_height, chunks[1]);
    frame.render_widget(results, results_area);

    // Hint
    let hint = Paragraph::new(Span::styled(
        "Press Enter to return to menu",
        Style::default().fg(SUBTLE).italic(),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(hint, chunks[2]);
}

fn centered_rect(width: u16, height: u16, parent: Rect) -> Rect {
    let x = parent.x + (parent.width.saturating_sub(width)) / 2;
    let y = parent.y + (parent.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(parent.width), height.min(parent.height))
}
