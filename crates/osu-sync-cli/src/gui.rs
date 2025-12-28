//! GUI mode for osu-sync (requires `gui` feature)

#[cfg(feature = "gui")]
use iced::widget::{center, text};
#[cfg(feature = "gui")]
use iced::{Element, Task};

/// Run the GUI application
#[cfg(feature = "gui")]
pub fn run() -> iced::Result {
    iced::application("osu-sync", OsuSyncApp::update, OsuSyncApp::view).run()
}

/// Main application state
#[cfg(feature = "gui")]
#[derive(Debug, Default)]
struct OsuSyncApp {}

/// Application messages
#[cfg(feature = "gui")]
#[derive(Debug, Clone)]
enum Message {
    #[allow(dead_code)]
    Placeholder,
}

#[cfg(feature = "gui")]
impl OsuSyncApp {
    fn update(&mut self, _message: Message) -> Task<Message> {
        Task::none()
    }

    fn view(&self) -> Element<'_, Message> {
        center(text("osu-sync GUI - Coming Soon").size(24)).into()
    }
}

/// Stub when GUI feature is not enabled
#[cfg(not(feature = "gui"))]
pub fn run() -> anyhow::Result<()> {
    eprintln!("GUI mode requires the 'gui' feature. Build with: cargo build --features gui");
    eprintln!("Or use the TUI mode (default): osu-sync");
    std::process::exit(1);
}
