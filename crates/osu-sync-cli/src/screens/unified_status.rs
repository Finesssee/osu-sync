//! Unified storage status dashboard
//!
//! Shows current unified storage status including:
//! - Current mode and configuration
//! - Link health (active, broken, stale)
//! - Storage statistics
//! - Quick actions (Verify, Repair, Sync, Disable)

use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Cell, Gauge, List, ListItem, Paragraph, Row, Table};

use crate::app::{ERROR, PINK, SUBTLE, SUCCESS, TEXT, WARNING};

/// Link health statistics
#[derive(Debug, Clone, Default)]
pub struct LinkHealth {
    pub total: usize,
    pub active: usize,
    pub broken: usize,
    pub stale: usize,
}

impl LinkHealth {
    pub fn health_percentage(&self) -> u16 {
        if self.total > 0 {
            ((self.active as f64 / self.total as f64) * 100.0) as u16
        } else {
            100
        }
    }
}

/// Storage statistics
#[derive(Debug, Clone, Default)]
pub struct StorageStats {
    pub total_links: usize,
    pub space_used: u64,
    pub space_saved: u64,
}

impl StorageStats {
    pub fn format_size(bytes: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = KB * 1024;
        const GB: u64 = MB * 1024;

        if bytes >= GB {
            format!("{:.2} GB", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            format!("{:.2} MB", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            format!("{:.2} KB", bytes as f64 / KB as f64)
        } else {
            format!("{} B", bytes)
        }
    }
}

/// Recent sync event
#[derive(Debug, Clone)]
pub struct SyncEvent {
    pub timestamp: String,
    pub event_type: EventType,
    pub description: String,
}

#[derive(Debug, Clone, Copy)]
pub enum EventType {
    Sync,
    LinkCreated,
    LinkRemoved,
    Error,
    Warning,
}

impl EventType {
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Sync => "↻",
            Self::LinkCreated => "+",
            Self::LinkRemoved => "-",
            Self::Error => "✗",
            Self::Warning => "!",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            Self::Sync => PINK,
            Self::LinkCreated => SUCCESS,
            Self::LinkRemoved => WARNING,
            Self::Error => ERROR,
            Self::Warning => WARNING,
        }
    }
}

/// Status screen state
#[derive(Debug, Clone)]
pub struct UnifiedStatusScreen {
    pub mode: String,
    pub health: LinkHealth,
    pub stats: StorageStats,
    pub events: Vec<SyncEvent>,
    pub selected_action: usize,
    pub loading: bool,
    pub status_message: Option<String>,
}

impl Default for UnifiedStatusScreen {
    fn default() -> Self {
        Self {
            mode: "Disabled".to_string(),
            health: LinkHealth::default(),
            stats: StorageStats::default(),
            events: Vec::new(),
            selected_action: 0,
            loading: false,
            status_message: None,
        }
    }
}

impl UnifiedStatusScreen {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handle_key(&mut self, key: KeyCode) -> Option<StatusAction> {
        if self.loading {
            return None;
        }

        match key {
            KeyCode::Left | KeyCode::Char('h' | 'H') => {
                if self.selected_action > 0 {
                    self.selected_action -= 1;
                }
            }
            KeyCode::Right | KeyCode::Char('l' | 'L') => {
                if self.selected_action < 3 {
                    self.selected_action += 1;
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                return match self.selected_action {
                    0 => Some(StatusAction::Verify),
                    1 => Some(StatusAction::Repair),
                    2 => Some(StatusAction::SyncNow),
                    3 => Some(StatusAction::Configure),
                    _ => None,
                };
            }
            KeyCode::Esc => {
                return Some(StatusAction::Back);
            }
            _ => {}
        }
        None
    }

    pub fn add_event(&mut self, event: SyncEvent) {
        self.events.insert(0, event);
        // Keep only last 20 events
        if self.events.len() > 20 {
            self.events.truncate(20);
        }
    }
}

/// Actions from the status screen
#[derive(Debug, Clone, Copy)]
pub enum StatusAction {
    Verify,
    Repair,
    SyncNow,
    Configure,
    Back,
}

/// Render the status screen
pub fn render(frame: &mut Frame, area: Rect, screen: &UnifiedStatusScreen) {
    let block = Block::default()
        .title(" Unified Storage Status ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PINK));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4), // Mode and health
            Constraint::Length(6), // Statistics
            Constraint::Min(8),    // Events
            Constraint::Length(3), // Actions
        ])
        .split(inner);

    render_header(frame, chunks[0], screen);
    render_statistics(frame, chunks[1], screen);
    render_events(frame, chunks[2], screen);
    render_actions(frame, chunks[3], screen);
}

fn render_header(frame: &mut Frame, area: Rect, screen: &UnifiedStatusScreen) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Mode display
    let mode_text = format!("Mode: {}", screen.mode);
    let mode = Paragraph::new(mode_text)
        .style(Style::default().fg(PINK).add_modifier(Modifier::BOLD))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Configuration "),
        );
    frame.render_widget(mode, layout[0]);

    // Health gauge
    let health = &screen.health;
    let health_pct = health.health_percentage();
    let health_color = if health_pct >= 90 {
        SUCCESS
    } else if health_pct >= 70 {
        WARNING
    } else {
        ERROR
    };

    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Link Health "),
        )
        .gauge_style(Style::default().fg(health_color))
        .ratio(health_pct as f64 / 100.0)
        .label(format!(
            "{}/{} active ({}%)",
            health.active, health.total, health_pct
        ));
    frame.render_widget(gauge, layout[1]);
}

fn render_statistics(frame: &mut Frame, area: Rect, screen: &UnifiedStatusScreen) {
    let health = &screen.health;
    let stats = &screen.stats;

    let rows = vec![
        Row::new(vec![
            Cell::from("Total Links:"),
            Cell::from(stats.total_links.to_string()),
            Cell::from("Active:"),
            Cell::from(health.active.to_string()).style(Style::default().fg(SUCCESS)),
        ]),
        Row::new(vec![
            Cell::from("Space Used:"),
            Cell::from(StorageStats::format_size(stats.space_used)),
            Cell::from("Broken:"),
            Cell::from(health.broken.to_string()).style(Style::default().fg(ERROR)),
        ]),
        Row::new(vec![
            Cell::from("Space Saved:"),
            Cell::from(StorageStats::format_size(stats.space_saved))
                .style(Style::default().fg(SUCCESS)),
            Cell::from("Stale:"),
            Cell::from(health.stale.to_string()).style(Style::default().fg(WARNING)),
        ]),
    ];

    let widths = [
        Constraint::Length(15),
        Constraint::Length(15),
        Constraint::Length(10),
        Constraint::Length(10),
    ];

    let table = Table::new(rows, widths)
        .block(Block::default().borders(Borders::ALL).title(" Statistics "))
        .style(Style::default().fg(TEXT));

    frame.render_widget(table, area);
}

fn render_events(frame: &mut Frame, area: Rect, screen: &UnifiedStatusScreen) {
    let items: Vec<ListItem> = screen
        .events
        .iter()
        .map(|event| {
            let icon = event.event_type.icon();
            let color = event.event_type.color();
            let text = format!(
                "{} [{}] {} {}",
                icon, event.timestamp, icon, event.description
            );
            ListItem::new(text).style(Style::default().fg(color))
        })
        .collect();

    let list = if items.is_empty() {
        List::new(vec![
            ListItem::new("No recent events").style(Style::default().fg(SUBTLE))
        ])
    } else {
        List::new(items)
    };

    let list = list.block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Recent Events "),
    );

    frame.render_widget(list, area);
}

fn render_actions(frame: &mut Frame, area: Rect, screen: &UnifiedStatusScreen) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
            Constraint::Percentage(25),
        ])
        .split(area);

    let actions = [
        ("Verify", 0),
        ("Repair", 1),
        ("Sync Now", 2),
        ("Configure", 3),
    ];

    for (i, (label, idx)) in actions.iter().enumerate() {
        let style = if screen.selected_action == *idx {
            Style::default()
                .fg(PINK)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED)
        } else {
            Style::default().fg(TEXT)
        };

        let button = Paragraph::new(format!(" [{}] ", label))
            .style(style)
            .alignment(Alignment::Center);
        frame.render_widget(button, layout[i]);
    }
}
