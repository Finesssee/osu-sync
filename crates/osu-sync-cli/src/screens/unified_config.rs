//! Unified storage configuration screen
//!
//! Allows users to configure unified storage settings including:
//! - Storage mode (Disabled, Stable Master, Lazer Master, True Unified)
//! - Resource types to share (Beatmaps, Skins, Replays, etc.)
//! - Sync triggers (File watcher, On game launch, Manual)

use std::collections::HashSet;

use crossterm::event::KeyCode;
use ratatui::prelude::*;
use ratatui::style::Modifier;
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
    /// Show confirmation dialog
    pub show_confirm: bool,
    /// Selected confirm button (0=Cancel, 1=Confirm)
    pub confirm_selected: usize,
    /// Games currently running (detected before confirm)
    pub games_running: Vec<String>,
    /// Estimated changes from dry run
    pub dry_run_info: Option<DryRunInfo>,
}

/// Information from dry run preview
#[derive(Debug, Clone, Default)]
pub struct DryRunInfo {
    /// Number of files that will be moved
    pub files_to_move: usize,
    /// Number of links that will be created
    pub links_to_create: usize,
    /// Total size of files affected (bytes)
    pub total_size: u64,
    /// Warnings about the operation
    pub warnings: Vec<String>,
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
            show_confirm: false,
            confirm_selected: 0,
            games_running: Vec::new(),
            dry_run_info: None,
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
        // Handle confirmation dialog
        if self.show_confirm {
            return self.handle_confirm_key(key);
        }

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
                    0 => Some(ConfigAction::RequestConfirm), // Show confirmation first
                    1 => Some(ConfigAction::Cancel),
                    _ => None,
                };
            }
            _ => {}
        }
        None
    }

    /// Handle key input in confirmation dialog
    fn handle_confirm_key(&mut self, key: KeyCode) -> Option<ConfigAction> {
        match key {
            KeyCode::Left | KeyCode::Right | KeyCode::Tab => {
                self.confirm_selected = if self.confirm_selected == 0 { 1 } else { 0 };
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.show_confirm = false;
                if self.confirm_selected == 1 {
                    return Some(ConfigAction::ConfirmApply);
                } else {
                    return Some(ConfigAction::CancelConfirm);
                }
            }
            KeyCode::Esc | KeyCode::Char('n' | 'N') => {
                self.show_confirm = false;
                return Some(ConfigAction::CancelConfirm);
            }
            KeyCode::Char('y' | 'Y') => {
                self.show_confirm = false;
                return Some(ConfigAction::ConfirmApply);
            }
            _ => {}
        }
        None
    }

    /// Show the confirmation dialog with game detection info
    pub fn show_confirmation(&mut self, games_running: Vec<String>, dry_run_info: DryRunInfo) {
        self.games_running = games_running;
        self.dry_run_info = Some(dry_run_info);
        self.show_confirm = true;
        self.confirm_selected = 0; // Default to Cancel for safety
    }
}

/// Actions that can be triggered from the config screen
#[derive(Debug, Clone, Copy)]
pub enum ConfigAction {
    /// Show confirmation dialog before applying
    RequestConfirm,
    /// User confirmed - proceed with migration
    ConfirmApply,
    /// User cancelled confirmation
    CancelConfirm,
    /// Cancel and go back
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

    // Confirmation dialog overlay
    if screen.show_confirm {
        render_confirm_dialog(frame, area, screen);
    }
}

/// Render the confirmation dialog as an overlay
fn render_confirm_dialog(frame: &mut Frame, area: Rect, screen: &UnifiedConfigScreen) {
    use ratatui::widgets::Clear;

    // Calculate dialog size - make it large enough for warnings
    let dialog_width = 60.min(area.width.saturating_sub(4));
    let dialog_height = 20.min(area.height.saturating_sub(4));
    let dialog_x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let dialog_y = area.y + (area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect::new(dialog_x, dialog_y, dialog_width, dialog_height);

    // Clear the area behind dialog
    frame.render_widget(Clear, dialog_area);

    // Dialog border
    let block = Block::default()
        .title(" ⚠ Confirm Unified Storage Setup ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(WARNING));
    let inner = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    // Build content
    let mut lines: Vec<Line> = Vec::new();

    // Warning header
    lines.push(Line::from(Span::styled(
        "This operation will modify your file system!",
        Style::default().fg(WARNING).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));

    // Games running warning
    if !screen.games_running.is_empty() {
        lines.push(Line::from(Span::styled(
            "⛔ WARNING: Games are currently running!",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )));
        for game in &screen.games_running {
            lines.push(Line::from(Span::styled(
                format!("   • {}", game),
                Style::default().fg(Color::Red),
            )));
        }
        lines.push(Line::from(Span::styled(
            "   Close all osu! instances before proceeding!",
            Style::default().fg(Color::Red),
        )));
        lines.push(Line::from(""));
    }

    // Dry run info
    if let Some(info) = &screen.dry_run_info {
        lines.push(Line::from(Span::styled(
            "Changes to be made:",
            Style::default().fg(TEXT).add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(format!(
            "   • Files to move: {}",
            info.files_to_move
        )));
        lines.push(Line::from(format!(
            "   • Links to create: {}",
            info.links_to_create
        )));
        lines.push(Line::from(format!(
            "   • Total size: {:.2} GB",
            info.total_size as f64 / 1_073_741_824.0
        )));

        // Warnings from dry run
        for warning in &info.warnings {
            lines.push(Line::from(Span::styled(
                format!("   ⚠ {}", warning),
                Style::default().fg(WARNING),
            )));
        }
        lines.push(Line::from(""));
    }

    // Backup reminder
    lines.push(Line::from(Span::styled(
        "📁 BACKUP REMINDER:",
        Style::default().fg(PINK).add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from("   Make sure you have backed up your data!"));
    lines.push(Line::from(""));

    // Buttons
    let cancel_style = if screen.confirm_selected == 0 {
        Style::default().fg(Color::Black).bg(WARNING)
    } else {
        Style::default().fg(TEXT)
    };
    let confirm_style = if screen.confirm_selected == 1 {
        Style::default().fg(Color::Black).bg(SUCCESS)
    } else {
        Style::default().fg(TEXT)
    };

    lines.push(Line::from(vec![
        Span::raw("          "),
        Span::styled(" [N] Cancel ", cancel_style),
        Span::raw("    "),
        Span::styled(" [Y] Confirm ", confirm_style),
    ]));

    let content = Paragraph::new(lines);
    frame.render_widget(content, inner);
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

#[cfg(test)]
mod tests {
    use super::*;

    // ============================================================================
    // StorageMode Tests
    // ============================================================================

    #[test]
    fn test_storage_mode_all_returns_all_variants() {
        let modes = StorageMode::all();
        assert_eq!(modes.len(), 4);
        assert!(modes.contains(&StorageMode::Disabled));
        assert!(modes.contains(&StorageMode::StableMaster));
        assert!(modes.contains(&StorageMode::LazerMaster));
        assert!(modes.contains(&StorageMode::TrueUnified));
    }

    #[test]
    fn test_storage_mode_labels() {
        assert!(StorageMode::Disabled.label().contains("Disabled"));
        assert!(StorageMode::StableMaster.label().contains("Stable"));
        assert!(StorageMode::LazerMaster.label().contains("Lazer"));
        assert!(StorageMode::TrueUnified.label().contains("Unified"));
    }

    #[test]
    fn test_storage_mode_default() {
        assert_eq!(StorageMode::default(), StorageMode::Disabled);
    }

    // ============================================================================
    // ResourceType Tests
    // ============================================================================

    #[test]
    fn test_resource_type_all_returns_all_variants() {
        let resources = ResourceType::all();
        assert_eq!(resources.len(), 6);
        assert!(resources.contains(&ResourceType::Beatmaps));
        assert!(resources.contains(&ResourceType::Skins));
        assert!(resources.contains(&ResourceType::Replays));
        assert!(resources.contains(&ResourceType::Screenshots));
        assert!(resources.contains(&ResourceType::Exports));
        assert!(resources.contains(&ResourceType::Backgrounds));
    }

    #[test]
    fn test_resource_type_labels() {
        assert_eq!(ResourceType::Beatmaps.label(), "Beatmaps");
        assert_eq!(ResourceType::Skins.label(), "Skins");
        assert_eq!(ResourceType::Replays.label(), "Replays");
    }

    // ============================================================================
    // UnifiedConfigScreen Tests
    // ============================================================================

    #[test]
    fn test_screen_default_values() {
        let screen = UnifiedConfigScreen::new();
        assert_eq!(screen.mode, StorageMode::Disabled);
        assert!(screen.shared_path.is_empty());
        assert!(screen.shared_resources.contains(&ResourceType::Beatmaps));
        assert!(screen.shared_resources.contains(&ResourceType::Skins));
        assert!(screen.file_watcher);
        assert!(!screen.on_game_launch);
        assert_eq!(screen.selected_section, 0);
        assert_eq!(screen.selected_item, 0);
        assert!(!screen.editing_path);
        assert!(!screen.show_confirm);
    }

    #[test]
    fn test_navigation_down() {
        let mut screen = UnifiedConfigScreen::new();

        // Navigate down within section
        screen.handle_key(KeyCode::Down);
        assert_eq!(screen.selected_item, 1);

        // Continue down
        screen.handle_key(KeyCode::Down);
        assert_eq!(screen.selected_item, 2);
        screen.handle_key(KeyCode::Down);
        assert_eq!(screen.selected_item, 3);

        // At end of mode section (4 items), should move to next section
        screen.handle_key(KeyCode::Down);
        assert_eq!(screen.selected_section, 1);
        assert_eq!(screen.selected_item, 0);
    }

    #[test]
    fn test_navigation_up() {
        let mut screen = UnifiedConfigScreen::new();

        // Start at section 1, item 0
        screen.selected_section = 1;
        screen.selected_item = 0;

        // Navigate up should go to previous section
        screen.handle_key(KeyCode::Up);
        assert_eq!(screen.selected_section, 0);
        // Should be at last item of previous section
        assert_eq!(screen.selected_item, StorageMode::all().len() - 1);
    }

    #[test]
    fn test_tab_navigation() {
        let mut screen = UnifiedConfigScreen::new();

        // Tab moves to next section
        screen.handle_key(KeyCode::Tab);
        assert_eq!(screen.selected_section, 1);
        assert_eq!(screen.selected_item, 0);

        // Tab through all sections
        screen.handle_key(KeyCode::Tab);
        assert_eq!(screen.selected_section, 2);
        screen.handle_key(KeyCode::Tab);
        assert_eq!(screen.selected_section, 3);
        screen.handle_key(KeyCode::Tab);
        assert_eq!(screen.selected_section, 4);

        // Tab from last section wraps to first
        screen.handle_key(KeyCode::Tab);
        assert_eq!(screen.selected_section, 0);
    }

    #[test]
    fn test_mode_selection() {
        let mut screen = UnifiedConfigScreen::new();
        assert_eq!(screen.mode, StorageMode::Disabled);

        // Select second mode (StableMaster)
        screen.handle_key(KeyCode::Down);
        screen.handle_key(KeyCode::Enter);
        assert_eq!(screen.mode, StorageMode::StableMaster);

        // Select TrueUnified
        screen.handle_key(KeyCode::Down);
        screen.handle_key(KeyCode::Down);
        screen.handle_key(KeyCode::Enter);
        assert_eq!(screen.mode, StorageMode::TrueUnified);
    }

    #[test]
    fn test_resource_toggle() {
        let mut screen = UnifiedConfigScreen::new();

        // Navigate to resources section
        screen.selected_section = 2;
        screen.selected_item = 0; // Beatmaps

        // Beatmaps should be selected by default
        assert!(screen.shared_resources.contains(&ResourceType::Beatmaps));

        // Toggle off
        screen.handle_key(KeyCode::Enter);
        assert!(!screen.shared_resources.contains(&ResourceType::Beatmaps));

        // Toggle back on
        screen.handle_key(KeyCode::Enter);
        assert!(screen.shared_resources.contains(&ResourceType::Beatmaps));
    }

    #[test]
    fn test_trigger_toggle() {
        let mut screen = UnifiedConfigScreen::new();

        // Navigate to triggers section
        screen.selected_section = 3;
        screen.selected_item = 0; // File watcher

        // File watcher is on by default
        assert!(screen.file_watcher);

        // Toggle off
        screen.handle_key(KeyCode::Enter);
        assert!(!screen.file_watcher);

        // Toggle on game launch
        screen.selected_item = 1;
        assert!(!screen.on_game_launch);
        screen.handle_key(KeyCode::Enter);
        assert!(screen.on_game_launch);
    }

    #[test]
    fn test_cancel_action() {
        let mut screen = UnifiedConfigScreen::new();

        // Escape should return Cancel action
        let action = screen.handle_key(KeyCode::Esc);
        assert!(matches!(action, Some(ConfigAction::Cancel)));
    }

    #[test]
    fn test_apply_button_requests_confirm() {
        let mut screen = UnifiedConfigScreen::new();

        // Navigate to Apply button
        screen.selected_section = 4;
        screen.selected_item = 0;

        // Enter on Apply should request confirmation
        let action = screen.handle_key(KeyCode::Enter);
        assert!(matches!(action, Some(ConfigAction::RequestConfirm)));
    }

    #[test]
    fn test_cancel_button() {
        let mut screen = UnifiedConfigScreen::new();

        // Navigate to Cancel button
        screen.selected_section = 4;
        screen.selected_item = 1;

        // Enter on Cancel should return Cancel action
        let action = screen.handle_key(KeyCode::Enter);
        assert!(matches!(action, Some(ConfigAction::Cancel)));
    }

    // ============================================================================
    // Path Editing Tests
    // ============================================================================

    #[test]
    fn test_path_editing() {
        let mut screen = UnifiedConfigScreen::new();

        // Navigate to path section
        screen.selected_section = 1;

        // Enter editing mode
        screen.handle_key(KeyCode::Enter);
        assert!(screen.editing_path);

        // Type characters
        screen.handle_key(KeyCode::Char('C'));
        screen.handle_key(KeyCode::Char(':'));
        screen.handle_key(KeyCode::Char('\\'));
        assert_eq!(screen.shared_path, "C:\\");

        // Backspace
        screen.handle_key(KeyCode::Backspace);
        assert_eq!(screen.shared_path, "C:");

        // Exit editing with Enter
        screen.handle_key(KeyCode::Enter);
        assert!(!screen.editing_path);
    }

    #[test]
    fn test_path_editing_escape() {
        let mut screen = UnifiedConfigScreen::new();
        screen.selected_section = 1;

        screen.handle_key(KeyCode::Enter);
        assert!(screen.editing_path);

        screen.handle_key(KeyCode::Char('t'));
        screen.handle_key(KeyCode::Char('e'));
        screen.handle_key(KeyCode::Char('s'));
        screen.handle_key(KeyCode::Char('t'));

        // Escape also exits editing mode (path is kept)
        screen.handle_key(KeyCode::Esc);
        assert!(!screen.editing_path);
        assert_eq!(screen.shared_path, "test");
    }

    // ============================================================================
    // Confirmation Dialog Tests
    // ============================================================================

    #[test]
    fn test_show_confirmation() {
        let mut screen = UnifiedConfigScreen::new();

        let games = vec!["osu!stable (osu!.exe)".to_string()];
        let dry_run = DryRunInfo {
            files_to_move: 100,
            links_to_create: 100,
            total_size: 5 * 1024 * 1024 * 1024, // 5GB
            warnings: vec!["Test warning".to_string()],
        };

        screen.show_confirmation(games.clone(), dry_run.clone());

        assert!(screen.show_confirm);
        assert_eq!(screen.games_running, games);
        assert!(screen.dry_run_info.is_some());
        let info = screen.dry_run_info.as_ref().unwrap();
        assert_eq!(info.files_to_move, 100);
        assert_eq!(info.links_to_create, 100);
        assert_eq!(screen.confirm_selected, 0); // Cancel is default
    }

    #[test]
    fn test_confirm_dialog_cancel_with_n() {
        let mut screen = UnifiedConfigScreen::new();
        screen.show_confirm = true;

        let action = screen.handle_key(KeyCode::Char('n'));
        assert!(!screen.show_confirm);
        assert!(matches!(action, Some(ConfigAction::CancelConfirm)));
    }

    #[test]
    fn test_confirm_dialog_confirm_with_y() {
        let mut screen = UnifiedConfigScreen::new();
        screen.show_confirm = true;

        let action = screen.handle_key(KeyCode::Char('y'));
        assert!(!screen.show_confirm);
        assert!(matches!(action, Some(ConfigAction::ConfirmApply)));
    }

    #[test]
    fn test_confirm_dialog_navigation() {
        let mut screen = UnifiedConfigScreen::new();
        screen.show_confirm = true;
        screen.confirm_selected = 0;

        // Navigate right
        screen.handle_key(KeyCode::Right);
        assert_eq!(screen.confirm_selected, 1);

        // Navigate left
        screen.handle_key(KeyCode::Left);
        assert_eq!(screen.confirm_selected, 0);

        // Tab toggles
        screen.handle_key(KeyCode::Tab);
        assert_eq!(screen.confirm_selected, 1);
    }

    #[test]
    fn test_confirm_dialog_enter_cancel() {
        let mut screen = UnifiedConfigScreen::new();
        screen.show_confirm = true;
        screen.confirm_selected = 0; // Cancel selected

        let action = screen.handle_key(KeyCode::Enter);
        assert!(!screen.show_confirm);
        assert!(matches!(action, Some(ConfigAction::CancelConfirm)));
    }

    #[test]
    fn test_confirm_dialog_enter_confirm() {
        let mut screen = UnifiedConfigScreen::new();
        screen.show_confirm = true;
        screen.confirm_selected = 1; // Confirm selected

        let action = screen.handle_key(KeyCode::Enter);
        assert!(!screen.show_confirm);
        assert!(matches!(action, Some(ConfigAction::ConfirmApply)));
    }

    #[test]
    fn test_confirm_dialog_escape() {
        let mut screen = UnifiedConfigScreen::new();
        screen.show_confirm = true;
        screen.confirm_selected = 1; // Even with confirm selected

        let action = screen.handle_key(KeyCode::Esc);
        assert!(!screen.show_confirm);
        assert!(matches!(action, Some(ConfigAction::CancelConfirm)));
    }

    // ============================================================================
    // DryRunInfo Tests
    // ============================================================================

    #[test]
    fn test_dry_run_info_default() {
        let info = DryRunInfo::default();
        assert_eq!(info.files_to_move, 0);
        assert_eq!(info.links_to_create, 0);
        assert_eq!(info.total_size, 0);
        assert!(info.warnings.is_empty());
    }

    #[test]
    fn test_dry_run_info_with_values() {
        let info = DryRunInfo {
            files_to_move: 500,
            links_to_create: 500,
            total_size: 10 * 1024 * 1024 * 1024, // 10GB
            warnings: vec!["Warning 1".into(), "Warning 2".into()],
        };

        assert_eq!(info.files_to_move, 500);
        assert_eq!(info.warnings.len(), 2);
    }

    // ============================================================================
    // Vim-style Navigation Tests
    // ============================================================================

    #[test]
    fn test_vim_navigation() {
        let mut screen = UnifiedConfigScreen::new();

        // j = down
        screen.handle_key(KeyCode::Char('j'));
        assert_eq!(screen.selected_item, 1);

        // k = up
        screen.handle_key(KeyCode::Char('k'));
        assert_eq!(screen.selected_item, 0);

        // J and K (uppercase) should also work
        screen.handle_key(KeyCode::Char('J'));
        assert_eq!(screen.selected_item, 1);

        screen.handle_key(KeyCode::Char('K'));
        assert_eq!(screen.selected_item, 0);
    }

    #[test]
    fn test_space_selects() {
        let mut screen = UnifiedConfigScreen::new();

        // Space should work like Enter for selection
        screen.selected_section = 0;
        screen.selected_item = 1; // StableMaster

        screen.handle_key(KeyCode::Char(' '));
        assert_eq!(screen.mode, StorageMode::StableMaster);
    }
}
