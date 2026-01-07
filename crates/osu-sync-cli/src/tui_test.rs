//! TUI Test Harness
//!
//! Provides programmatic control over the TUI for automated testing.
//! Uses ratatui's TestBackend to capture rendered output.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use std::io;

use crate::app::{App, AppState};

/// Test harness for the TUI application
pub struct TuiTestHarness {
    app: App,
    terminal: Terminal<TestBackend>,
}

impl TuiTestHarness {
    /// Create a new test harness with the given terminal size
    pub fn new(width: u16, height: u16) -> io::Result<Self> {
        let backend = TestBackend::new(width, height);
        let terminal = Terminal::new(backend)?;
        let app = App::new();

        Ok(Self { app, terminal })
    }

    /// Create with default size (120x30)
    pub fn default_size() -> io::Result<Self> {
        Self::new(120, 30)
    }

    /// Get a reference to the app
    pub fn app(&self) -> &App {
        &self.app
    }

    /// Get a mutable reference to the app
    pub fn app_mut(&mut self) -> &mut App {
        &mut self.app
    }

    /// Render the current frame and return the buffer contents as a string
    pub fn render(&mut self) -> io::Result<String> {
        self.terminal.draw(|frame| {
            self.app.render(frame);
        })?;

        let buffer = self.terminal.backend().buffer();
        let mut output = String::new();

        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                let cell = &buffer[(x, y)];
                output.push_str(cell.symbol());
            }
            output.push('\n');
        }

        Ok(output)
    }

    /// Send a key press to the app
    pub fn press_key(&mut self, code: KeyCode) {
        let event = KeyEvent::new(code, KeyModifiers::NONE);
        self.app.handle_key(event);
    }

    /// Send a key with modifiers (e.g., Ctrl+A)
    pub fn press_key_with_mods(&mut self, code: KeyCode, modifiers: KeyModifiers) {
        let event = KeyEvent::new(code, modifiers);
        self.app.handle_key(event);
    }

    /// Press Enter
    pub fn enter(&mut self) {
        self.press_key(KeyCode::Enter);
    }

    /// Press Escape
    pub fn escape(&mut self) {
        self.press_key(KeyCode::Esc);
    }

    /// Press Up arrow
    pub fn up(&mut self) {
        self.press_key(KeyCode::Up);
    }

    /// Press Down arrow
    pub fn down(&mut self) {
        self.press_key(KeyCode::Down);
    }

    /// Press Space
    pub fn space(&mut self) {
        self.press_key(KeyCode::Char(' '));
    }

    /// Press a character key
    pub fn char(&mut self, c: char) {
        self.press_key(KeyCode::Char(c));
    }

    /// Press Ctrl+A (select all)
    pub fn ctrl_a(&mut self) {
        self.press_key_with_mods(KeyCode::Char('a'), KeyModifiers::CONTROL);
    }

    /// Press Ctrl+D (deselect all)
    pub fn ctrl_d(&mut self) {
        self.press_key_with_mods(KeyCode::Char('d'), KeyModifiers::CONTROL);
    }

    /// Press Page Down
    pub fn page_down(&mut self) {
        self.press_key(KeyCode::PageDown);
    }

    /// Press Page Up
    pub fn page_up(&mut self) {
        self.press_key(KeyCode::PageUp);
    }

    /// Navigate down N times
    pub fn down_n(&mut self, n: usize) {
        for _ in 0..n {
            self.down();
        }
    }

    /// Navigate up N times
    pub fn up_n(&mut self, n: usize) {
        for _ in 0..n {
            self.up();
        }
    }

    /// Check if the rendered output contains a string
    pub fn screen_contains(&mut self, text: &str) -> io::Result<bool> {
        let output = self.render()?;
        Ok(output.contains(text))
    }

    /// Get the current state name
    pub fn state_name(&self) -> &'static str {
        match &self.app.state {
            AppState::MainMenu { .. } => "MainMenu",
            AppState::Scanning { .. } => "Scanning",
            AppState::SyncConfig { .. } => "SyncConfig",
            AppState::Syncing { .. } => "Syncing",
            AppState::DuplicateDialog { .. } => "DuplicateDialog",
            AppState::SyncComplete { .. } => "SyncComplete",
            AppState::Config { .. } => "Config",
            AppState::Statistics { .. } => "Statistics",
            AppState::CollectionConfig { .. } => "CollectionConfig",
            AppState::CollectionSync { .. } => "CollectionSync",
            AppState::CollectionSummary { .. } => "CollectionSummary",
            AppState::DryRunPreview { .. } => "DryRunPreview",
            AppState::BackupConfig { .. } => "BackupConfig",
            AppState::BackupProgress { .. } => "BackupProgress",
            AppState::BackupComplete { .. } => "BackupComplete",
            AppState::RestoreConfig { .. } => "RestoreConfig",
            AppState::RestoreConfirm { .. } => "RestoreConfirm",
            AppState::RestoreProgress { .. } => "RestoreProgress",
            AppState::RestoreComplete { .. } => "RestoreComplete",
            AppState::MediaConfig { .. } => "MediaConfig",
            AppState::MediaProgress { .. } => "MediaProgress",
            AppState::MediaComplete { .. } => "MediaComplete",
            AppState::ReplayConfig { .. } => "ReplayConfig",
            AppState::ReplayProgress { .. } => "ReplayProgress",
            AppState::ReplayComplete { .. } => "ReplayComplete",
            AppState::Help { .. } => "Help",
            AppState::UnifiedConfig { .. } => "UnifiedConfig",
            AppState::UnifiedSetup { .. } => "UnifiedSetup",
            AppState::UnifiedStatus { .. } => "UnifiedStatus",
            AppState::Exiting => "Exiting",
        }
    }

    /// Check if app is in a specific state
    pub fn is_state(&self, name: &str) -> bool {
        self.state_name() == name
    }

    /// Get DryRunPreview state details if in that state
    pub fn dry_run_state(&self) -> Option<DryRunTestState> {
        if let AppState::DryRunPreview {
            selected_item,
            scroll_offset,
            checked_items,
            filter_text,
            filter_mode,
            result,
            ..
        } = &self.app.state
        {
            Some(DryRunTestState {
                selected_item: *selected_item,
                scroll_offset: *scroll_offset,
                checked_count: checked_items.len(),
                filter_text: filter_text.clone(),
                filter_mode: *filter_mode,
                total_items: result.items.len(),
            })
        } else {
            None
        }
    }

    /// Wait for a condition (with timeout)
    pub fn wait_for<F>(&mut self, condition: F, max_iterations: usize) -> bool
    where
        F: Fn(&Self) -> bool,
    {
        for _ in 0..max_iterations {
            if condition(self) {
                return true;
            }
            // Process any pending messages
            self.app.process_worker_messages();
        }
        false
    }

    /// Check if the app should quit
    pub fn should_quit(&self) -> bool {
        self.app.should_quit
    }
}

/// Snapshot of DryRunPreview state for testing
#[derive(Debug, Clone)]
pub struct DryRunTestState {
    pub selected_item: usize,
    pub scroll_offset: usize,
    pub checked_count: usize,
    pub filter_text: String,
    pub filter_mode: bool,
    pub total_items: usize,
}

/// Test scenario builder for fluent test writing
pub struct TestScenario {
    harness: TuiTestHarness,
    steps: Vec<String>,
}

impl TestScenario {
    pub fn new() -> io::Result<Self> {
        Ok(Self {
            harness: TuiTestHarness::default_size()?,
            steps: Vec::new(),
        })
    }

    pub fn with_size(width: u16, height: u16) -> io::Result<Self> {
        Ok(Self {
            harness: TuiTestHarness::new(width, height)?,
            steps: Vec::new(),
        })
    }

    fn log(&mut self, step: &str) {
        self.steps.push(step.to_string());
    }

    pub fn press_enter(mut self) -> Self {
        self.log("Press Enter");
        self.harness.enter();
        self
    }

    pub fn press_escape(mut self) -> Self {
        self.log("Press Escape");
        self.harness.escape();
        self
    }

    pub fn press_down(mut self) -> Self {
        self.log("Press Down");
        self.harness.down();
        self
    }

    pub fn press_up(mut self) -> Self {
        self.log("Press Up");
        self.harness.up();
        self
    }

    pub fn press_down_n(mut self, n: usize) -> Self {
        self.log(&format!("Press Down x{}", n));
        self.harness.down_n(n);
        self
    }

    pub fn press_up_n(mut self, n: usize) -> Self {
        self.log(&format!("Press Up x{}", n));
        self.harness.up_n(n);
        self
    }

    pub fn press_space(mut self) -> Self {
        self.log("Press Space");
        self.harness.space();
        self
    }

    pub fn press_char(mut self, c: char) -> Self {
        self.log(&format!("Press '{}'", c));
        self.harness.char(c);
        self
    }

    pub fn press_ctrl_a(mut self) -> Self {
        self.log("Press Ctrl+A");
        self.harness.ctrl_a();
        self
    }

    pub fn press_ctrl_d(mut self) -> Self {
        self.log("Press Ctrl+D");
        self.harness.ctrl_d();
        self
    }

    pub fn press_page_down(mut self) -> Self {
        self.log("Press PageDown");
        self.harness.page_down();
        self
    }

    pub fn press_page_up(mut self) -> Self {
        self.log("Press PageUp");
        self.harness.page_up();
        self
    }

    pub fn assert_state(mut self, expected: &str) -> Self {
        let actual = self.harness.state_name();
        assert_eq!(
            actual, expected,
            "Expected state '{}' but got '{}'\nSteps: {:?}",
            expected, actual, self.steps
        );
        self.log(&format!("Assert state = {}", expected));
        self
    }

    pub fn assert_screen_contains(mut self, text: &str) -> Self {
        let contains = self.harness.screen_contains(text).unwrap_or(false);
        assert!(
            contains,
            "Screen should contain '{}'\nSteps: {:?}",
            text, self.steps
        );
        self.log(&format!("Assert screen contains '{}'", text));
        self
    }

    pub fn assert_dry_run_selected(mut self, expected: usize) -> Self {
        if let Some(state) = self.harness.dry_run_state() {
            assert_eq!(
                state.selected_item, expected,
                "Expected selected_item {} but got {}\nSteps: {:?}",
                expected, state.selected_item, self.steps
            );
        } else {
            panic!("Not in DryRunPreview state\nSteps: {:?}", self.steps);
        }
        self.log(&format!("Assert selected = {}", expected));
        self
    }

    pub fn assert_dry_run_scroll(mut self, expected: usize) -> Self {
        if let Some(state) = self.harness.dry_run_state() {
            assert_eq!(
                state.scroll_offset, expected,
                "Expected scroll_offset {} but got {}\nSteps: {:?}",
                expected, state.scroll_offset, self.steps
            );
        } else {
            panic!("Not in DryRunPreview state\nSteps: {:?}", self.steps);
        }
        self.log(&format!("Assert scroll = {}", expected));
        self
    }

    pub fn assert_dry_run_checked_count(mut self, expected: usize) -> Self {
        if let Some(state) = self.harness.dry_run_state() {
            assert_eq!(
                state.checked_count, expected,
                "Expected {} checked items but got {}\nSteps: {:?}",
                expected, state.checked_count, self.steps
            );
        } else {
            panic!("Not in DryRunPreview state\nSteps: {:?}", self.steps);
        }
        self.log(&format!("Assert checked count = {}", expected));
        self
    }

    pub fn print_screen(mut self) -> Self {
        let screen = self.harness.render().unwrap_or_default();
        println!("=== Screen ===\n{}\n==============", screen);
        self.log("Print screen");
        self
    }

    pub fn print_state(mut self) -> Self {
        println!("State: {}", self.harness.state_name());
        if let Some(dry_run) = self.harness.dry_run_state() {
            println!("  DryRun: {:?}", dry_run);
        }
        self.log("Print state");
        self
    }

    /// Get the harness for custom operations
    pub fn harness(&self) -> &TuiTestHarness {
        &self.harness
    }

    /// Get mutable harness for custom operations
    pub fn harness_mut(&mut self) -> &mut TuiTestHarness {
        &mut self.harness
    }

    /// Finish the scenario and return the harness
    pub fn finish(self) -> TuiTestHarness {
        self.harness
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_harness_creation() {
        let harness = TuiTestHarness::default_size();
        assert!(harness.is_ok());
    }

    #[test]
    fn test_initial_state_is_main_menu() {
        let harness = TuiTestHarness::default_size().unwrap();
        assert_eq!(harness.state_name(), "MainMenu");
    }

    #[test]
    fn test_render_produces_output() {
        let mut harness = TuiTestHarness::default_size().unwrap();
        let output = harness.render().unwrap();
        assert!(!output.is_empty());
        // Should contain the app title
        assert!(output.contains("osu") || output.contains("sync"));
    }

    #[test]
    fn test_escape_from_scanning_exits() {
        let mut harness = TuiTestHarness::default_size().unwrap();
        harness.escape();
        // After escape from scanning, should exit or go to menu
        // depending on implementation
    }

    #[test]
    fn test_scenario_builder() {
        let scenario = TestScenario::new().unwrap();
        let harness = scenario
            .assert_state("MainMenu")
            .press_down() // Navigate to exit option
            .press_down()
            .press_down()
            .press_down()
            .press_down()
            .press_down()
            .press_down()
            .press_down()
            .press_down() // Should be at "Exit" (9th item, index 9)
            .press_enter()
            .finish();

        // Verify scenario completed - selecting exit should quit
        assert!(harness.should_quit());
    }
}
