//! Vision module for AI-assisted game interaction.
//!
//! This module provides screen capture capabilities for osu! game windows,
//! enabling AI systems to "see" the game state.
//!
//! ## Features
//!
//! - `CapturedFrame` - Screenshot data from game windows
//! - `CaptureTarget` - Which game variant to capture (stable/lazer/any)
//! - `WindowInfo` - Information about capturable windows
//!
//! ## Platform Support
//!
//! - **Windows**: Full support using Windows Graphics Capture API
//! - **Linux/macOS**: Not supported (stub implementations)
//!
//! ## Usage
//!
//! ```ignore
//! use osu_sync_core::vision::{capture_game_window, CaptureTarget};
//!
//! // Capture any osu! window
//! let frame = capture_game_window(CaptureTarget::Any)?;
//!
//! // Save to file
//! frame.save_to_file(std::path::Path::new("screenshot.png"))?;
//!
//! // Or get as base64 (with base64 feature)
//! #[cfg(feature = "base64")]
//! let b64 = frame.as_base64();
//! ```

mod capture;

pub use capture::{
    capture_game_window, list_osu_windows, CaptureTarget, CapturedFrame, WindowInfo,
};
