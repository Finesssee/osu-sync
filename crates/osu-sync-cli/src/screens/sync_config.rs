//! Sync configuration screen with filter support

use osu_sync_core::beatmap::GameMode;
use osu_sync_core::filter::FilterCriteria;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};

use crate::app::{FilterField, PINK, SUBTLE, TEXT};

pub fn render(
    frame: &mut Frame,
    area: Rect,
    selected: usize,
    stable_count: usize,
    lazer_count: usize,
    filter: &FilterCriteria,
    filter_panel_open: bool,
    filter_field: FilterField,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Title
            Constraint::Length(2), // Instruction
            Constraint::Min(0),    // Options
            Constraint::Length(5), // Preview + filter status
        ])
        .split(area);

    // Title
    let title = Paragraph::new(Span::styled(
        "Sync Configuration",
        Style::default().fg(PINK).bold(),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // Instruction
    let instruction = Paragraph::new(Span::styled(
        "Select sync direction:",
        Style::default().fg(SUBTLE),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(instruction, chunks[1]);

    // Calculate filtered counts based on direction
    // For now, show total counts (actual filtering would need beatmap data)
    let filtered_stable = stable_count;
    let filtered_lazer = lazer_count;

    // Options
    let options = [
        (
            "Stable -> Lazer",
            format!("{} beatmaps to sync", filtered_stable),
        ),
        (
            "Lazer -> Stable",
            format!("{} beatmaps to sync", filtered_lazer),
        ),
        (
            "Bidirectional",
            format!("{} beatmaps to sync", filtered_stable + filtered_lazer),
        ),
    ];

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, (label, count))| {
            let prefix = if i == selected { "> " } else { "  " };
            let style = if i == selected {
                Style::default().fg(PINK).bold()
            } else {
                Style::default().fg(TEXT)
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{}{}", prefix, label), style),
                Span::styled(format!("  ({})", count), Style::default().fg(SUBTLE)),
            ]))
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

    // Preview and filter status
    let preview_text = match selected {
        0 => format!(
            "Will import {} beatmap sets from osu!stable to osu!lazer",
            filtered_stable
        ),
        1 => format!(
            "Will import {} beatmap sets from osu!lazer to osu!stable",
            filtered_lazer
        ),
        2 => format!(
            "Will sync {} beatmap sets in both directions",
            filtered_stable + filtered_lazer
        ),
        _ => String::new(),
    };

    // Build filter status line
    let filter_status = if filter.is_empty() {
        "Filters: None (press 'f' to configure)".to_string()
    } else {
        format!("Filters: {} (press 'f' to edit)", filter.summary())
    };

    let preview_content = vec![
        Line::from(Span::styled(preview_text, Style::default().fg(SUBTLE))),
        Line::from(""),
        Line::from(Span::styled(
            filter_status,
            Style::default().fg(SUBTLE).italic(),
        )),
    ];

    let preview = Paragraph::new(preview_content)
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .title(" Preview ")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(SUBTLE)),
        );

    frame.render_widget(preview, chunks[3]);

    // Render filter panel if open
    if filter_panel_open {
        render_filter_panel(frame, area, filter, filter_field);
    }
}

fn render_filter_panel(
    frame: &mut Frame,
    area: Rect,
    filter: &FilterCriteria,
    filter_field: FilterField,
) {
    use osu_sync_core::stats::RankedStatus;

    // Create a modal dialog
    let panel_width = 60;
    let panel_height = 20;
    let panel_area = centered_rect(panel_width, panel_height, area);

    // Clear the area behind the modal
    frame.render_widget(Clear, panel_area);

    let block = Block::default()
        .title(" Filter Settings ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PINK))
        .style(Style::default().bg(Color::Rgb(40, 40, 60)));

    frame.render_widget(block.clone(), panel_area);

    let inner = block.inner(panel_area);
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Mode title
            Constraint::Length(2), // Mode checkboxes
            Constraint::Length(1), // Star rating title
            Constraint::Length(2), // Star rating inputs
            Constraint::Length(1), // Status title
            Constraint::Length(2), // Status checkboxes row 1
            Constraint::Length(2), // Status checkboxes row 2
            Constraint::Length(1), // Empty separator
            Constraint::Length(2), // Instructions
            Constraint::Min(0),    // Padding
        ])
        .split(inner);

    // Game Modes title
    let title = Paragraph::new(Span::styled(
        "Game Modes:",
        Style::default().fg(TEXT).bold(),
    ));
    frame.render_widget(title, rows[0]);

    // Mode checkboxes
    let mode_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
            Constraint::Ratio(1, 4),
        ])
        .split(rows[1]);

    render_checkbox(
        frame,
        mode_cols[0],
        "osu!",
        filter.is_mode_enabled(GameMode::Osu),
        filter_field == FilterField::ModeOsu,
    );
    render_checkbox(
        frame,
        mode_cols[1],
        "Taiko",
        filter.is_mode_enabled(GameMode::Taiko),
        filter_field == FilterField::ModeTaiko,
    );
    render_checkbox(
        frame,
        mode_cols[2],
        "Catch",
        filter.is_mode_enabled(GameMode::Catch),
        filter_field == FilterField::ModeCatch,
    );
    render_checkbox(
        frame,
        mode_cols[3],
        "Mania",
        filter.is_mode_enabled(GameMode::Mania),
        filter_field == FilterField::ModeMania,
    );

    // Star Rating title
    let star_title = Paragraph::new(Span::styled(
        "Star Rating:",
        Style::default().fg(TEXT).bold(),
    ));
    frame.render_widget(star_title, rows[2]);

    // Star rating inputs
    let star_cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)])
        .split(rows[3]);

    let min_str = filter
        .star_rating_min
        .map(|v| format!("{:.1}", v))
        .unwrap_or_else(|| "—".to_string());
    let max_str = filter
        .star_rating_max
        .map(|v| format!("{:.1}", v))
        .unwrap_or_else(|| "—".to_string());

    render_value_input(
        frame,
        star_cols[0],
        "Min:",
        &min_str,
        filter_field == FilterField::StarMin,
    );
    render_value_input(
        frame,
        star_cols[1],
        "Max:",
        &max_str,
        filter_field == FilterField::StarMax,
    );

    // Ranked Status title
    let status_title = Paragraph::new(Span::styled(
        "Ranked Status:",
        Style::default().fg(TEXT).bold(),
    ));
    frame.render_widget(status_title, rows[4]);

    // Status checkboxes row 1
    let status_cols1 = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
        ])
        .split(rows[5]);

    render_checkbox(
        frame,
        status_cols1[0],
        "Ranked",
        filter.is_status_enabled(RankedStatus::Ranked),
        filter_field == FilterField::StatusRanked,
    );
    render_checkbox(
        frame,
        status_cols1[1],
        "Approved",
        filter.is_status_enabled(RankedStatus::Approved),
        filter_field == FilterField::StatusApproved,
    );
    render_checkbox(
        frame,
        status_cols1[2],
        "Qualified",
        filter.is_status_enabled(RankedStatus::Qualified),
        filter_field == FilterField::StatusQualified,
    );

    // Status checkboxes row 2
    let status_cols2 = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
            Constraint::Ratio(1, 3),
        ])
        .split(rows[6]);

    render_checkbox(
        frame,
        status_cols2[0],
        "Loved",
        filter.is_status_enabled(RankedStatus::Loved),
        filter_field == FilterField::StatusLoved,
    );
    render_checkbox(
        frame,
        status_cols2[1],
        "Pending",
        filter.is_status_enabled(RankedStatus::Pending),
        filter_field == FilterField::StatusPending,
    );

    // Instructions
    let instructions = Paragraph::new(Span::styled(
        "Space: Toggle | +/-: Adjust | Arrows: Navigate | Esc: Close",
        Style::default().fg(SUBTLE),
    ))
    .alignment(Alignment::Center);
    frame.render_widget(instructions, rows[8]);
}

fn render_value_input(frame: &mut Frame, area: Rect, label: &str, value: &str, selected: bool) {
    let style = if selected {
        Style::default().fg(PINK).bold()
    } else {
        Style::default().fg(TEXT)
    };

    let content = format!("{} {}", label, value);
    let widget = Paragraph::new(Span::styled(content, style)).alignment(Alignment::Center);
    frame.render_widget(widget, area);
}

fn render_checkbox(frame: &mut Frame, area: Rect, label: &str, checked: bool, selected: bool) {
    let checkbox = if checked { "[x]" } else { "[ ]" };

    let style = if selected {
        Style::default().fg(PINK).bold()
    } else {
        Style::default().fg(TEXT)
    };

    let content = format!("{} {}", checkbox, label);
    let widget = Paragraph::new(Span::styled(content, style)).alignment(Alignment::Center);

    frame.render_widget(widget, area);
}

fn centered_rect(width: u16, height: u16, parent: Rect) -> Rect {
    let x = parent.x + (parent.width.saturating_sub(width)) / 2;
    let y = parent.y + (parent.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(parent.width), height.min(parent.height))
}
