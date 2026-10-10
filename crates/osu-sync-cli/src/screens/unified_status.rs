//! Unified storage status: how much of Songs shares its data with lazer's store.

use crossterm::event::KeyCode;
use osu_sync_core::unified::{LinkedStoreStatus, UnifiedStorageMode};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Paragraph, Row, Table, Wrap};

use crate::app::{ERROR, PINK, SUBTLE, SUCCESS, TEXT, WARNING};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusAction {
    Refresh,
    /// Run the linked-store step now.
    SyncNow,
    Back,
}

#[derive(Debug, Clone, Default)]
pub struct UnifiedStatusScreen {
    /// The saved mode; `None` until the worker reports it with the first count.
    pub mode: Option<UnifiedStorageMode>,
    /// `None` while the count runs.
    pub status: Option<Result<LinkedStoreStatus, String>>,
    pub notes: Vec<String>,
}

impl UnifiedStatusScreen {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handle_key(&mut self, key: KeyCode) -> Option<StatusAction> {
        match key {
            KeyCode::Char('r') => {
                self.status = None;
                Some(StatusAction::Refresh)
            }
            KeyCode::Char('s') => Some(StatusAction::SyncNow),
            KeyCode::Esc | KeyCode::Char('q') => Some(StatusAction::Back),
            _ => None,
        }
    }
}

pub fn render(frame: &mut Frame, area: Rect, screen: &UnifiedStatusScreen) {
    let block = Block::default()
        .title(" Unified Storage Status ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PINK));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(8), Constraint::Min(3)])
        .split(inner);

    let label = |text: &'static str| Cell::from(text).style(Style::default().fg(SUBTLE));
    let rows = match &screen.status {
        None => vec![Row::new(vec![
            label("Status"),
            Cell::from("Counting files in Songs..."),
        ])],
        Some(Err(message)) => vec![Row::new(vec![
            label("Status"),
            Cell::from(message.clone()).style(Style::default().fg(ERROR)),
        ])],
        Some(Ok(status)) => status
            .rows()
            .into_iter()
            .map(|(name, value)| {
                let color = match name {
                    "Linked files" | "Bytes saved" => SUCCESS,
                    "Unreadable files" => WARNING,
                    _ => TEXT,
                };
                Row::new(vec![
                    label(name),
                    Cell::from(value).style(Style::default().fg(color)),
                ])
            })
            .collect(),
    };
    let mut all = vec![Row::new(vec![
        label("Mode"),
        Cell::from(screen.mode.map_or("Reading...", |m| m.label()))
            .style(Style::default().fg(PINK).add_modifier(Modifier::BOLD)),
    ])];
    all.extend(rows);
    frame.render_widget(
        Table::new(all, [Constraint::Length(18), Constraint::Min(10)])
            .block(Block::default().borders(Borders::ALL).title(" Songs "))
            .style(Style::default().fg(TEXT)),
        chunks[0],
    );

    let mut lines = vec![Line::from(Span::styled(
        "Linked files share their data with lazer's files folder. Copied files are .osu and .osb files, stable-only sets and link fallbacks.",
        Style::default().fg(SUBTLE),
    ))];
    for note in &screen.notes {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            note.clone(),
            Style::default().fg(WARNING),
        )));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(Block::default().borders(Borders::ALL).title(" Notes ")),
        chunks[1],
    );
}
