//! Theme system for UI customization
//!
//! Provides different color schemes for the TUI interface.

use ratatui::prelude::Color;

// Re-export ThemeName from core
pub use osu_sync_core::config::ThemeName;

/// Theme color palette
#[derive(Debug, Clone)]
pub struct Theme {
    /// Primary accent color (used for headers, selection, highlights)
    pub accent: Color,
    /// Secondary accent color (used for active elements)
    pub accent_secondary: Color,
    /// Background color for highlighted items
    pub highlight_bg: Color,
    /// Main text color
    pub text: Color,
    /// Subtle/dimmed text color
    pub subtle: Color,
    /// Success indicator color
    pub success: Color,
    /// Warning indicator color
    pub warning: Color,
    /// Error indicator color
    pub error: Color,
    /// Border color
    pub border: Color,
    /// Background color for selection highlights
    pub selection_bg: Color,
}

impl Theme {
    /// Create the default osu! pink theme
    pub fn default_theme() -> Self {
        Self {
            accent: Color::Rgb(255, 102, 170),           // osu! pink
            accent_secondary: Color::Rgb(255, 153, 200), // lighter pink
            highlight_bg: Color::Rgb(45, 45, 60),        // dark purple-ish
            text: Color::Rgb(205, 214, 244),             // light text
            subtle: Color::Rgb(147, 153, 178),           // dimmed text
            success: Color::Green,
            warning: Color::Yellow,
            error: Color::Red,
            border: Color::Rgb(147, 153, 178),    // same as subtle
            selection_bg: Color::Rgb(45, 45, 60), // same as highlight
        }
    }

    /// Create the ocean blue theme
    pub fn ocean_theme() -> Self {
        Self {
            accent: Color::Rgb(100, 180, 255),           // ocean blue
            accent_secondary: Color::Rgb(150, 200, 255), // lighter blue
            highlight_bg: Color::Rgb(30, 50, 70),        // dark blue-ish
            text: Color::Rgb(200, 220, 240),             // light blue-white
            subtle: Color::Rgb(120, 150, 180),           // dimmed blue-gray
            success: Color::Rgb(100, 220, 150),          // teal green
            warning: Color::Rgb(255, 200, 100),          // warm yellow
            error: Color::Rgb(255, 100, 100),            // coral red
            border: Color::Rgb(80, 120, 160),            // mid blue
            selection_bg: Color::Rgb(40, 60, 90),        // selection blue
        }
    }

    /// Create the monochrome theme
    pub fn monochrome_theme() -> Self {
        Self {
            accent: Color::White,
            accent_secondary: Color::Rgb(200, 200, 200), // light gray
            highlight_bg: Color::Rgb(50, 50, 50),        // dark gray
            text: Color::Rgb(220, 220, 220),             // bright gray
            subtle: Color::Rgb(128, 128, 128),           // mid gray
            success: Color::Rgb(180, 220, 180),          // light green-gray
            warning: Color::Rgb(220, 200, 140),          // light yellow-gray
            error: Color::Rgb(220, 140, 140),            // light red-gray
            border: Color::Rgb(100, 100, 100),           // border gray
            selection_bg: Color::Rgb(60, 60, 60),        // selection gray
        }
    }

    /// Get theme by name
    pub fn from_name(name: ThemeName) -> Self {
        match name {
            ThemeName::Default => Self::default_theme(),
            ThemeName::Ocean => Self::ocean_theme(),
            ThemeName::Monochrome => Self::monochrome_theme(),
        }
    }
}

impl Default for Theme {
    fn default() -> Self {
        Self::default_theme()
    }
}

/// Global theme instance for easy access
/// This uses thread-local storage for safety
use std::cell::RefCell;

thread_local! {
    static CURRENT_THEME: RefCell<Theme> = RefCell::new(Theme::default());
    static CURRENT_THEME_NAME: RefCell<ThemeName> = RefCell::new(ThemeName::Default);
}

/// Set the current global theme
pub fn set_theme(name: ThemeName) {
    CURRENT_THEME.with(|t| {
        *t.borrow_mut() = Theme::from_name(name);
    });
    CURRENT_THEME_NAME.with(|n| {
        *n.borrow_mut() = name;
    });
}

/// Get the current theme name
pub fn current_theme_name() -> ThemeName {
    CURRENT_THEME_NAME.with(|n| *n.borrow())
}

/// Get the current accent color
pub fn accent() -> Color {
    CURRENT_THEME.with(|t| t.borrow().accent)
}

/// Get the current secondary accent color
pub fn accent_secondary() -> Color {
    CURRENT_THEME.with(|t| t.borrow().accent_secondary)
}

/// Get the current highlight background color
pub fn highlight_bg() -> Color {
    CURRENT_THEME.with(|t| t.borrow().highlight_bg)
}

/// Get the current text color
pub fn text() -> Color {
    CURRENT_THEME.with(|t| t.borrow().text)
}

/// Get the current subtle text color
pub fn subtle() -> Color {
    CURRENT_THEME.with(|t| t.borrow().subtle)
}

/// Get the current success color
pub fn success() -> Color {
    CURRENT_THEME.with(|t| t.borrow().success)
}

/// Get the current warning color
pub fn warning() -> Color {
    CURRENT_THEME.with(|t| t.borrow().warning)
}

/// Get the current error color
pub fn error() -> Color {
    CURRENT_THEME.with(|t| t.borrow().error)
}

/// Get the current border color
pub fn border() -> Color {
    CURRENT_THEME.with(|t| t.borrow().border)
}

/// Get the current selection background color
pub fn selection_bg() -> Color {
    CURRENT_THEME.with(|t| t.borrow().selection_bg)
}
