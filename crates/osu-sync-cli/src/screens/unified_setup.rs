//! Unified storage setup: progress of one linked-store step, then its report.

use crossterm::event::KeyCode;
use osu_sync_core::unified::{StepPhase, StepReport};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Gauge, Paragraph, Row, Table, Wrap};

use crate::app::{ERROR, PINK, SUBTLE, SUCCESS, TEXT, WARNING};

#[derive(Debug, Clone, Default)]
pub struct UnifiedSetupScreen {
    /// The part of the step that runs now; `None` before the first report.
    pub phase: Option<StepPhase>,
    pub current: usize,
    pub total: usize,
    /// `None` while the step runs; on success, the report and what happened to the
    /// saved mode.
    pub result: Option<Result<(StepReport, String), String>>,
}

impl UnifiedSetupScreen {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_finished(&self) -> bool {
        self.result.is_some()
    }

    /// True when the user leaves the screen. The step cannot be stopped midway, so
    /// keys do nothing until it ends.
    pub fn handle_key(&self, key: KeyCode) -> bool {
        self.is_finished() && matches!(key, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q'))
    }
}

pub fn render(frame: &mut Frame, area: Rect, screen: &UnifiedSetupScreen) {
    let block = Block::default()
        .title(" Unified Storage Setup ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PINK));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(11),
            Constraint::Min(3),
        ])
        .split(inner);

    let ratio = if screen.total > 0 {
        (screen.current as f64 / screen.total as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let phase = screen.phase.map_or("Starting", |p| p.label());
    let (gauge_label, ratio) = match &screen.result {
        None if screen.total > 0 => (
            format!("{phase}: {}/{}", screen.current, screen.total),
            ratio,
        ),
        None => (phase.to_string(), 0.0),
        Some(Ok(_)) => ("Done".to_string(), 1.0),
        Some(Err(_)) => ("Stopped".to_string(), ratio),
    };
    frame.render_widget(
        Gauge::default()
            .block(Block::default().borders(Borders::ALL))
            .gauge_style(Style::default().fg(PINK))
            .label(gauge_label)
            .ratio(ratio),
        chunks[0],
    );

    let notes_area = if matches!(screen.result, Some(Ok(_))) {
        chunks[2]
    } else {
        chunks[1].union(chunks[2])
    };
    let mut notes: Vec<Line> = Vec::new();
    match &screen.result {
        None => notes.push(Line::from(Span::styled(
            "Linking lazer's beatmaps into Songs. Keep osu!stable closed until this ends; the step cannot be stopped midway.",
            Style::default().fg(SUBTLE),
        ))),
        Some(Ok((report, saved))) => {
            let rows = report.rows().into_iter().map(|(name, value)| {
                Row::new(vec![
                    Cell::from(name).style(Style::default().fg(SUBTLE)),
                    Cell::from(value),
                ])
            });
            frame.render_widget(
                Table::new(rows, [Constraint::Length(18), Constraint::Min(10)])
                    .block(Block::default().borders(Borders::ALL).title(" Report "))
                    .style(Style::default().fg(TEXT)),
                chunks[1],
            );
            let color = if report.errors.is_empty() {
                SUCCESS
            } else {
                WARNING
            };
            notes.push(Line::styled(saved.as_str(), Style::default().fg(color)));
            let errors = report
                .errors
                .iter()
                .map(|e| Line::styled(format!("Error: {e}"), Style::default().fg(ERROR)));
            let warnings = report
                .notes
                .iter()
                .map(|n| Line::styled(n.as_str(), Style::default().fg(WARNING)));
            let all: Vec<Line> = errors.chain(warnings).collect();
            // Each line takes at least one row inside the border; the rest are counted.
            let room = usize::from(notes_area.height.saturating_sub(3));
            if all.len() > room {
                let shown = room.saturating_sub(1);
                let more = all.len() - shown;
                notes.extend(all.into_iter().take(shown));
                notes.push(Line::styled(
                    format!("...and {more} more; the CLI's unified setup prints them all."),
                    Style::default().fg(SUBTLE),
                ));
            } else {
                notes.extend(all);
            }
        }
        Some(Err(message)) => {
            notes.push(Line::from(Span::styled(
                format!("Setup failed: {message}"),
                Style::default().fg(ERROR),
            )));
            notes.push(Line::from(Span::styled(
                "The saved mode was not changed.",
                Style::default().fg(SUBTLE),
            )));
        }
    }
    frame.render_widget(
        Paragraph::new(notes)
            .wrap(Wrap { trim: true })
            .block(Block::default().borders(Borders::ALL).title(" Notes ")),
        notes_area,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_leave_only_after_the_step_ends() {
        let mut screen = UnifiedSetupScreen::new();
        assert!(!screen.handle_key(KeyCode::Esc));
        screen.result = Some(Err("osu!stable is running".to_string()));
        assert!(screen.handle_key(KeyCode::Esc));
        assert!(!screen.handle_key(KeyCode::Char('x')));
    }
}
