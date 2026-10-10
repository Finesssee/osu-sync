//! Unified storage mode screen: the linked store or disabled.

use crossterm::event::KeyCode;
use osu_sync_core::unified::UnifiedStorageMode;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph, Wrap};

use crate::app::{PINK, SUBTLE, SUCCESS, TEXT, WARNING};

/// What the user asked for on the mode screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigAction {
    /// Turn on the linked store: runs setup.
    EnableLinkedStore,
    /// Save the disabled mode.
    Disable,
    ShowStatus,
    Back,
}

#[derive(Debug, Clone, Default)]
pub struct UnifiedConfigScreen {
    /// The saved mode; `None` until the worker reports it.
    pub current: Option<UnifiedStorageMode>,
    /// Index into [`UnifiedStorageMode::ALL`].
    pub selected: usize,
    /// Why the saved mode changed, such as a mode an older version saved.
    pub notice: Option<String>,
    /// The result of the last action on this screen.
    pub message: Option<String>,
}

impl UnifiedConfigScreen {
    /// The screen before the worker reports the saved mode.
    pub fn new() -> Self {
        Self::default()
    }

    /// Shows `mode` as the saved one and selects it.
    pub fn set_saved(&mut self, mode: UnifiedStorageMode, notice: Option<String>) {
        self.current = Some(mode);
        self.selected = UnifiedStorageMode::ALL
            .iter()
            .position(|m| *m == mode)
            .unwrap_or(0);
        self.notice = notice;
    }

    pub fn selected_mode(&self) -> UnifiedStorageMode {
        UnifiedStorageMode::ALL[self.selected]
    }

    /// Moving and Enter wait until the worker reports the saved mode, so a key press
    /// before that cannot start setup from the default selection.
    pub fn handle_key(&mut self, key: KeyCode) -> Option<ConfigAction> {
        let last = UnifiedStorageMode::ALL.len() - 1;
        let loaded = self.current.is_some();
        match key {
            KeyCode::Up | KeyCode::Char('k') if loaded => {
                self.selected = self.selected.saturating_sub(1)
            }
            KeyCode::Down | KeyCode::Char('j') if loaded => {
                self.selected = (self.selected + 1).min(last)
            }
            KeyCode::Enter if loaded => {
                return Some(match self.selected_mode() {
                    UnifiedStorageMode::LinkedStore => ConfigAction::EnableLinkedStore,
                    UnifiedStorageMode::Disabled => ConfigAction::Disable,
                })
            }
            KeyCode::Char('s') => return Some(ConfigAction::ShowStatus),
            KeyCode::Esc | KeyCode::Char('q') => return Some(ConfigAction::Back),
            _ => {}
        }
        None
    }
}

const LOADING: &str = "Loading the saved mode; this waits for any running job to finish.";

pub fn render(frame: &mut Frame, area: Rect, screen: &UnifiedConfigScreen) {
    let block = Block::default()
        .title(" Unified Storage ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PINK));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2 * UnifiedStorageMode::ALL.len() as u16 + 2),
            Constraint::Min(4),
        ])
        .split(inner);

    let items: Vec<ListItem> = UnifiedStorageMode::ALL
        .iter()
        .enumerate()
        .map(|(i, mode)| {
            let marker = if i == screen.selected { "> " } else { "  " };
            let saved = if screen.current == Some(*mode) {
                " (current)"
            } else {
                ""
            };
            let style = if i == screen.selected {
                Style::default().fg(PINK).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(TEXT)
            };
            ListItem::new(vec![
                Line::from(Span::styled(
                    format!("{marker}{}{saved}", mode.label()),
                    style,
                )),
                Line::from(Span::styled(
                    format!("    {}", mode.description()),
                    Style::default().fg(SUBTLE),
                )),
            ])
        })
        .collect();
    frame.render_widget(
        List::new(items).block(Block::default().borders(Borders::ALL).title(" Mode ")),
        chunks[0],
    );

    let mut lines = Vec::new();
    if screen.current.is_none() {
        lines.push(Line::from(Span::styled(
            LOADING,
            Style::default().fg(WARNING),
        )));
        lines.push(Line::from(""));
    }
    lines.extend([
        Line::from(Span::styled(
            "Linked store: Songs gets hard links into lazer's files folder. Both folders must be on one NTFS volume.",
            Style::default().fg(TEXT),
        )),
        Line::from(Span::styled(
            ".osu and .osb files are copies, so stable can edit them. Setup changes nothing in lazer.",
            Style::default().fg(TEXT),
        )),
    ]);
    if let Some(notice) = &screen.notice {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            notice.clone(),
            Style::default().fg(WARNING),
        )));
    }
    if let Some(message) = &screen.message {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            message.clone(),
            Style::default().fg(SUCCESS),
        )));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .block(Block::default().borders(Borders::ALL).title(" About ")),
        chunks[1],
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enter_on_each_mode_asks_for_its_action() {
        let mut screen = UnifiedConfigScreen::new();
        screen.set_saved(UnifiedStorageMode::Disabled, None);
        assert_eq!(screen.selected_mode(), UnifiedStorageMode::Disabled);
        assert_eq!(
            screen.handle_key(KeyCode::Enter),
            Some(ConfigAction::Disable)
        );
        screen.handle_key(KeyCode::Up);
        assert_eq!(
            screen.handle_key(KeyCode::Enter),
            Some(ConfigAction::EnableLinkedStore)
        );
        screen.handle_key(KeyCode::Up);
        assert_eq!(screen.selected_mode(), UnifiedStorageMode::LinkedStore);
    }

    fn rendered(screen: &UnifiedConfigScreen) -> String {
        let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(120, 24)).unwrap();
        terminal
            .draw(|frame| render(frame, frame.area(), screen))
            .unwrap();
        let buffer = terminal.backend().buffer();
        buffer
            .content()
            .chunks(buffer.area.width as usize)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn keys_wait_for_the_saved_mode_and_the_screen_says_so() {
        let mut screen = UnifiedConfigScreen::new();
        assert_eq!(screen.handle_key(KeyCode::Enter), None);
        assert_eq!(screen.handle_key(KeyCode::Down), None);
        assert_eq!(screen.selected, 0);
        assert!(rendered(&screen).contains(LOADING));
        assert_eq!(screen.handle_key(KeyCode::Esc), Some(ConfigAction::Back));

        screen.set_saved(UnifiedStorageMode::LinkedStore, None);
        assert!(!rendered(&screen).contains(LOADING));
        assert_eq!(
            screen.handle_key(KeyCode::Enter),
            Some(ConfigAction::EnableLinkedStore)
        );
    }
}
