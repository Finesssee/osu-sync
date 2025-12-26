//! osu-sync GUI application
//!
//! A graphical user interface for osu-sync built with iced.

mod components;
mod views;

use iced::widget::{center, text};
use iced::{Element, Task};

/// Main entry point for the GUI application.
fn main() -> iced::Result {
    iced::application("osu-sync", OsuSyncApp::update, OsuSyncApp::view).run()
}

/// Main application state.
#[derive(Debug, Default)]
struct OsuSyncApp {
    // Placeholder for future state
}

/// Application messages.
#[derive(Debug, Clone)]
enum Message {
    /// Placeholder message variant
    #[allow(dead_code)]
    Placeholder,
}

impl OsuSyncApp {
    /// Update the application state based on messages.
    fn update(&mut self, _message: Message) -> Task<Message> {
        // Placeholder - no-op for now
        Task::none()
    }

    /// Render the application view.
    fn view(&self) -> Element<'_, Message> {
        center(text("osu-sync GUI - Coming Soon").size(24)).into()
    }
}
