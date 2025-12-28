//! Unified storage configuration screen
//!
//! Allows users to configure unified storage settings including:
//! - Storage mode (Disabled, Stable Master, Lazer Master, True Unified)
//! - Resource types to share (Beatmaps, Skins, Replays, etc.)
//! - Sync triggers (File watcher, On game launch, Manual)

use std::collections::HashSet;

use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, Paragraph};

use crate::app::{PINK, SUBTLE, SUCCESS, TEXT, WARNING};

/// Unified storage mode selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StorageMode {
    #[default]
    Disabled,
    StableMaster,
    LazerMaster,
    TrueUnified,
}

impl StorageMode {
    pub fn all() -> &'static [StorageMode] {
        &[
            Self::Disabled,
            Self::StableMaster,
            Self::LazerMaster,
            Self::TrueUnified,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Disabled => "Disabled (Copy files during sync)",
            Self::StableMaster => "Stable as Master",
            Self::LazerMaster => "Lazer as Master",
            Self::TrueUnified => "True Unified (Shared folder)",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Self::Disabled => "Files are copied between installations",
            Self::StableMaster => "Beatmaps live in stable, lazer links to them",
            Self::LazerMaster => "Beatmaps live in lazer, stable links to them",
            Self::TrueUnified => "Both link to a shared folder location",
        }
    }
}

/// Resource types that can be shared
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceType {
    Beatmaps,
    Skins,
    Replays,
    Screenshots,
    Exports,
    Backgrounds,
}

impl ResourceType {
    pub fn all() -> &'static [ResourceType] {
        &[
            Self::Beatmaps,
            Self::Skins,
            Self::Replays,
            Self::Screenshots,
            Self::Exports,
            Self::Backgrounds,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Beatmaps => "Beatmaps",
            Self::Skins => "Skins",
            Self::Replays => "Replays",
            Self::Screenshots => "Screenshots",
            Self::Exports => "Exports",
            Self::Backgrounds => "Backgrounds",
        }
    }
}

/// Configuration screen state
#[derive(Debug, Clone)]
pub struct UnifiedConfigScreen {
    /// Current storage mode
    pub mode: StorageMode,
    /// Shared folder path (for TrueUnified mode)
    pub shared_path: String,
    /// Resource types to share
    pub shared_resources: HashSet<ResourceType>,
    /// Enable file watcher
    pub file_watcher: bool,
    /// Sync on game launch
    pub on_game_launch: bool,
    /// Currently selected section (0=mode, 1=path, 2=resources, 3=triggers, 4=buttons)
    pub selected_section: usize,
    /// Currently selected item within section
    pub selected_item: usize,
    /// Whether currently editing the path
    pub editing_path: bool,
    /// Status message to display
    pub status_message: Option<String>,
}

impl Default for UnifiedConfigScreen {
    fn default() -> Self {
        let mut resources = HashSet::new();
        resources.insert(ResourceType::Beatmaps);
        resources.insert(ResourceType::Skins);

        Self {
            mode: StorageMode::Disabled,
            shared_path: String::new(),
            shared_resources: resources,
            file_watcher: true,
            on_game_launch: false,
            selected_section: 0,
            selected_item: 0,
            editing_path: false,
            status_message: None,
        }
    }
}

impl UnifiedConfigScreen {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the number of items in the current section
    fn section_item_count(&self) -> usize {
        match self.selected_section {
            0 => StorageMode::all().len(),      // Mode selection
            1 => 1,                              // Path input
            2 => ResourceType::all().len(),     // Resources
            3 => 2,                              // Triggers (file watcher, on launch)
            4 => 2,                              // Buttons (Apply, Cancel)
            _ => 0,
        }
    }

    /// Handle key input
    pub fn handle_key(&mut self, key: KeyCode) -> Option<ConfigAction> {
        if self.editing_path {
            match key {
                KeyCode::Enter | KeyCode::Esc => {
                    self.editing_path = false;
                }
                KeyCode::Char(c) => {
                    self.shared_path.push(c);
                }
                KeyCode::Backspace => {
                    self.shared_path.pop();
                }
                _ => {}
            }
            return None;
        }

        match key {
            KeyCode::Up | KeyCode::Char('k' | 'K') => {
                if self.selected_item > 0 {
                    self.selected_item -= 1;
                } else if self.selected_section > 0 {
                    self.selected_section -= 1;
                    self.selected_item = self.section_item_count().saturating_sub(1);
                }
            }
            KeyCode::Down | KeyCode::Char('j' | 'J') => {
                if self.selected_item + 1 < self.section_item_count() {
                    self.selected_item += 1;
                } else if self.selected_section < 4 {
                    self.selected_section += 1;
                    self.selected_item = 0;
                }
            }
            KeyCode::Tab => {
                if self.selected_section < 4 {
                    self.selected_section += 1;
                    self.selected_item = 0;
                } else {
                    self.selected_section = 0;
                    self.selected_item = 0;
                }
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                return self.handle_select();
            }
            KeyCode::Esc => {
                return Some(ConfigAction::Cancel);
            }
            _ => {}
        }
        None
    }

    fn handle_select(&mut self) -> Option<ConfigAction> {
        match self.selected_section {
            0 => {
                // Mode selection
                if let Some(mode) = StorageMode::all().get(self.selected_item) {
                    self.mode = *mode;
                }
            }
            1 => {
                // Path editing
                self.editing_path = true;
            }
            2 => {
                // Toggle resource
                if let Some(resource) = ResourceType::all().get(self.selected_item) {
                    if self.shared_resources.contains(resource) {
                        self.shared_resources.remove(resource);
                    } else {
                        self.shared_resources.insert(*resource);
                    }
                }
            }
            3 => {
                // Toggle trigger
                match self.selected_item {
                    0 => self.file_watcher = !self.file_watcher,
                    1 => self.on_game_launch = !self.on_game_launch,
                    _ => {}
                }
            }
            4 => {
                // Buttons
                return match self.selected_item {
                    0 => Some(ConfigAction::Apply),
                    1 => Some(ConfigAction::Cancel),
                    _ => None,
                };
            }
            _ => {}
        }
        None
    }
}

/// Actions that can be triggered from the config screen
#[derive(Debug, Clone, Copy)]
pub enum ConfigAction {
    Apply,
    Cancel,
}

/// Render the unified config screen
pub fn render(frame: &mut Frame, area: Rect, screen: &UnifiedConfigScreen) {
    let block = Block::default()
        .title(" Unified Storage Configuration ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(PINK));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Layout sections vertically
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),  // Mode selection
            Constraint::Length(3),  // Path input
            Constraint::Length(9),  // Resources
            Constraint::Length(5),  // Triggers
            Constraint::Length(3),  // Buttons
            Constraint::Min(0),     // Status/remaining
        ])
        .split(inner);

    // Mode selection
    render_mode_section(frame, chunks[0], screen);

    // Path input (only visible for TrueUnified)
    if screen.mode == StorageMode::TrueUnified {
        render_path_section(frame, chunks[1], screen);
    }

    // Resources
    render_resources_section(frame, chunks[2], screen);

    // Triggers
    render_triggers_section(frame, chunks[3], screen);

    // Buttons
    render_buttons(frame, chunks[4], screen);

    // Status message
    if let Some(msg) = &screen.status_message {
        let status = Paragraph::new(msg.as_str())
            .style(Style::default().fg(WARNING));
        frame.render_widget(status, chunks[5]);
    }
}

fn render_mode_section(frame: &mut Frame, area: Rect, screen: &UnifiedConfigScreen) {
    let title = Paragraph::new("Storage Mode:")
        .style(Style::default().fg(PINK).add_modifier(Modifier::BOLD));

    let title_area = Rect { height: 1, ..area };
    frame.render_widget(title, title_area);

    let items: Vec<ListItem> = StorageMode::all()
        .iter()
        .enumerate()
        .map(|(i, mode)| {
            let selected = screen.mode == *mode;
            let focused = screen.selected_section == 0 && screen.selected_item == i;
            let prefix = if selected { "(●)" } else { "( )" };
            let style = if focused {
                Style::default().fg(PINK).add_modifier(Modifier::BOLD)
            } else if selected {
                Style::default().fg(SUCCESS)
            } else {
                Style::default().fg(TEXT)
            };
            ListItem::new(format!("  {} {}", prefix, mode.label())).style(style)
        })
        .collect();

    let list = List::new(items);
    let list_area = Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(1),
        ..area
    };
    frame.render_widget(list, list_area);
}

fn render_path_section(frame: &mut Frame, area: Rect, screen: &UnifiedConfigScreen) {
    let style = if screen.selected_section == 1 {
        Style::default().fg(PINK)
    } else {
        Style::default().fg(SUBTLE)
    };

    let cursor = if screen.editing_path { "│" } else { "" };
    let path_display = if screen.shared_path.is_empty() {
        format!("Shared Path: (press Enter to set){}", cursor)
    } else {
        format!("Shared Path: {}{}", screen.shared_path, cursor)
    };

    let paragraph = Paragraph::new(path_display)
        .style(style)
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(paragraph, area);
}

fn render_resources_section(frame: &mut Frame, area: Rect, screen: &UnifiedConfigScreen) {
    let title = Paragraph::new("Resources to Share:")
        .style(Style::default().fg(PINK).add_modifier(Modifier::BOLD));

    let title_area = Rect { height: 1, ..area };
    frame.render_widget(title, title_area);

    let items: Vec<ListItem> = ResourceType::all()
        .iter()
        .enumerate()
        .map(|(i, resource)| {
            let checked = screen.shared_resources.contains(resource);
            let focused = screen.selected_section == 2 && screen.selected_item == i;
            let prefix = if checked { "[x]" } else { "[ ]" };
            let style = if focused {
                Style::default().fg(PINK).add_modifier(Modifier::BOLD)
            } else if checked {
                Style::default().fg(SUCCESS)
            } else {
                Style::default().fg(TEXT)
            };
            ListItem::new(format!("  {} {}", prefix, resource.label())).style(style)
        })
        .collect();

    let list = List::new(items);
    let list_area = Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(1),
        ..area
    };
    frame.render_widget(list, list_area);
}

fn render_triggers_section(frame: &mut Frame, area: Rect, screen: &UnifiedConfigScreen) {
    let title = Paragraph::new("Sync Triggers:")
        .style(Style::default().fg(PINK).add_modifier(Modifier::BOLD));

    let title_area = Rect { height: 1, ..area };
    frame.render_widget(title, title_area);

    let triggers = [
        ("File Watcher (background monitoring)", screen.file_watcher),
        ("On Game Launch", screen.on_game_launch),
    ];

    let items: Vec<ListItem> = triggers
        .iter()
        .enumerate()
        .map(|(i, (label, checked))| {
            let focused = screen.selected_section == 3 && screen.selected_item == i;
            let prefix = if *checked { "[x]" } else { "[ ]" };
            let style = if focused {
                Style::default().fg(PINK).add_modifier(Modifier::BOLD)
            } else if *checked {
                Style::default().fg(SUCCESS)
            } else {
                Style::default().fg(TEXT)
            };
            ListItem::new(format!("  {} {}", prefix, label)).style(style)
        })
        .collect();

    let list = List::new(items);
    let list_area = Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(1),
        ..area
    };
    frame.render_widget(list, list_area);
}

fn render_buttons(frame: &mut Frame, area: Rect, screen: &UnifiedConfigScreen) {
    let layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let apply_style = if screen.selected_section == 4 && screen.selected_item == 0 {
        Style::default().fg(PINK).add_modifier(Modifier::BOLD | Modifier::REVERSED)
    } else {
        Style::default().fg(SUCCESS)
    };

    let cancel_style = if screen.selected_section == 4 && screen.selected_item == 1 {
        Style::default().fg(PINK).add_modifier(Modifier::BOLD | Modifier::REVERSED)
    } else {
        Style::default().fg(SUBTLE)
    };

    let apply = Paragraph::new(" [Apply Changes] ")
        .style(apply_style)
        .alignment(Alignment::Center);
    let cancel = Paragraph::new(" [Cancel] ")
        .style(cancel_style)
        .alignment(Alignment::Center);

    frame.render_widget(apply, layout[0]);
    frame.render_widget(cancel, layout[1]);
}
