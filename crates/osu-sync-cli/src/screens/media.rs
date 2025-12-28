//! Media extraction screens

use osu_sync_core::media::{ExtractionProgress, ExtractionResult, MediaType, OutputOrganization};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph};

use crate::app::{PINK, SUBTLE, TEXT};

/// Render media extraction configuration screen
#[allow(clippy::too_many_arguments)]
pub fn render_config(
    frame: &mut Frame,
    area: Rect,
    selected: usize,
    media_type: MediaType,
    organization: OutputOrganization,
    output_path: &str,
    skip_duplicates: bool,
    include_metadata: bool,
    status_message: &Option<String>,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Length(2), // Instruction
            Constraint::Min(0),    // Options
            Constraint::Length(3), // Status
        ])
        .split(area);

    // Title
    let title = Paragraph::new(Span::styled(
        "Extract Media",
        Style::default().fg(PINK).bold(),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // Instruction
    let instruction = Paragraph::new(Span::styled(
        "Configure media extraction settings:",
        Style::default().fg(SUBTLE),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(instruction, chunks[1]);

    // Options
    let media_type_str = match media_type {
        MediaType::Audio => "Audio only",
        MediaType::Backgrounds => "Backgrounds only",
        MediaType::Both => "Audio + Backgrounds",
    };

    let org_str = match organization {
        OutputOrganization::Flat => "Flat (all in one folder)",
        OutputOrganization::ByArtist => "By Artist",
        OutputOrganization::ByBeatmap => "By Beatmap",
    };

    let skip_dup_str = if skip_duplicates { "[x] Yes" } else { "[ ] No" };
    let metadata_str = if include_metadata {
        "[x] Yes"
    } else {
        "[ ] No"
    };

    let options = [
        format!("Media Type: {}", media_type_str),
        format!("Organization: {}", org_str),
        format!("Skip Duplicates: {}", skip_dup_str),
        format!("Include Metadata: {}", metadata_str),
        format!("Output: {}", output_path),
        "Start Extraction".to_string(),
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

    let menu_width = 50;
    let menu_height = options.len() as u16 + 2;
    let menu_area = centered_rect(menu_width, menu_height, chunks[2]);

    let menu = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(SUBTLE)),
    );

    frame.render_widget(menu, menu_area);

    // Status message
    if let Some(ref msg) = status_message {
        let status = Paragraph::new(Span::styled(msg.as_str(), Style::default().fg(SUBTLE)))
            .alignment(Alignment::Center);
        frame.render_widget(status, chunks[3]);
    }
}

/// Render media extraction progress screen
pub fn render_progress(
    frame: &mut Frame,
    area: Rect,
    progress: &Option<ExtractionProgress>,
    current_set: &str,
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
        "Extracting Media...",
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
    let current = Paragraph::new(Span::styled(current_set, Style::default().fg(TEXT)))
        .alignment(Alignment::Center);
    frame.render_widget(current, chunks[2]);

    // Stats
    if let Some(ref p) = progress {
        let stats_text = format!(
            "Sets: {}/{} | Files: {} | Written: {:.1} MB",
            p.sets_processed,
            p.total_sets,
            p.files_extracted,
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

/// Render media extraction complete screen
pub fn render_complete(frame: &mut Frame, area: Rect, result: &ExtractionResult) {
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
        "Extraction Complete!",
        Style::default().fg(PINK).bold(),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // Results
    let mut results_text = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("Audio files extracted: {}", result.audio_extracted),
            Style::default().fg(TEXT),
        )),
        Line::from(Span::styled(
            format!(
                "Background images extracted: {}",
                result.backgrounds_extracted
            ),
            Style::default().fg(TEXT),
        )),
        Line::from(Span::styled(
            format!("Duplicates skipped: {}", result.duplicates_skipped),
            Style::default().fg(SUBTLE),
        )),
        Line::from(Span::styled(
            format!(
                "Total data written: {:.1} MB",
                result.bytes_written as f64 / 1_048_576.0
            ),
            Style::default().fg(TEXT),
        )),
    ];

    // Show metadata files created if any
    if result.metadata_files_created > 0 {
        results_text.push(Line::from(Span::styled(
            format!("Metadata files created: {}", result.metadata_files_created),
            Style::default().fg(TEXT),
        )));
    }

    // Show audio format breakdown if available
    if !result.audio_by_format.is_empty() {
        let format_info: Vec<String> = result
            .audio_by_format
            .iter()
            .map(|(k, v)| format!("{}: {}", k, v))
            .collect();
        results_text.push(Line::from(Span::styled(
            format!("Audio formats: {}", format_info.join(", ")),
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

    let results_area = centered_rect(50, 12, chunks[1]);
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
