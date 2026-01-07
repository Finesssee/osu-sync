//! Game window capture using Windows Graphics Capture API.
//!
//! This module provides functionality to capture screenshots of osu! game windows
//! on Windows. Uses the `windows-capture` crate for high-performance capture.
//!
//! # Platform Support
//!
//! - **Windows**: Full support using Windows Graphics Capture API
//! - **Linux/macOS**: Not supported (returns error)

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};

/// Captured frame data from a game window
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedFrame {
    /// PNG image bytes
    #[serde(skip)]
    pub png_bytes: Vec<u8>,
    /// Frame width in pixels
    pub width: u32,
    /// Frame height in pixels
    pub height: u32,
    /// Window title captured from
    pub window_title: String,
    /// Timestamp of capture (ISO 8601)
    pub timestamp: String,
}

impl CapturedFrame {
    /// Save the captured frame to a file
    pub fn save_to_file(&self, path: &std::path::Path) -> Result<()> {
        std::fs::write(path, &self.png_bytes)
            .map_err(|e| Error::Other(format!("Failed to save capture: {}", e)))
    }
}

/// Game variant to capture
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaptureTarget {
    /// osu!stable
    Stable,
    /// osu!lazer
    Lazer,
    /// Any osu! window found first
    Any,
}

impl CaptureTarget {
    /// Get display name
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Stable => "osu!stable",
            Self::Lazer => "osu!lazer",
            Self::Any => "osu!",
        }
    }
}

impl std::fmt::Display for CaptureTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Information about a capturable window
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    /// Window title
    pub title: String,
    /// Window width
    pub width: u32,
    /// Window height
    pub height: u32,
    /// Process name (if available)
    pub process_name: Option<String>,
    /// Whether this appears to be osu!lazer
    pub is_lazer: bool,
}

// ============================================================================
// Windows Implementation
// ============================================================================

#[cfg(windows)]
mod windows_impl {
    use super::*;
    use std::sync::{Arc, Mutex};

    use windows_capture::{
        capture::{Context, GraphicsCaptureApiHandler},
        frame::{Frame, ImageFormat},
        graphics_capture_api::InternalCaptureControl,
        settings::{
            ColorFormat, CursorCaptureSettings, DirtyRegionSettings, DrawBorderSettings,
            MinimumUpdateIntervalSettings, SecondaryWindowSettings, Settings,
        },
        window::Window,
    };

    /// Shared state for capturing
    struct CaptureState {
        result: Option<CapturedFrame>,
        window_title: String,
        temp_path: std::path::PathBuf,
    }

    /// Handler for single-frame capture
    struct SingleFrameCapture {
        state: Arc<Mutex<CaptureState>>,
    }

    impl GraphicsCaptureApiHandler for SingleFrameCapture {
        type Flags = Arc<Mutex<CaptureState>>;
        type Error = Box<dyn std::error::Error + Send + Sync>;

        fn new(ctx: Context<Self::Flags>) -> std::result::Result<Self, Self::Error> {
            Ok(Self { state: ctx.flags })
        }

        fn on_frame_arrived(
            &mut self,
            frame: &mut Frame,
            capture_control: InternalCaptureControl,
        ) -> std::result::Result<(), Self::Error> {
            // Get frame dimensions
            let width = frame.width();
            let height = frame.height();

            // Lock state to get temp path
            let state = self
                .state
                .lock()
                .map_err(|e| format!("Lock error: {}", e))?;
            let temp_path = state.temp_path.clone();
            let window_title = state.window_title.clone();
            drop(state);

            // Save to temp file
            let mut buffer = frame.buffer()?;
            buffer.save_as_image(&temp_path, ImageFormat::Png)?;

            // Read back the file
            let png_bytes = std::fs::read(&temp_path)?;

            // Clean up temp file
            let _ = std::fs::remove_file(&temp_path);

            // Store result
            if let Ok(mut state) = self.state.lock() {
                state.result = Some(CapturedFrame {
                    png_bytes,
                    width,
                    height,
                    window_title,
                    timestamp: chrono::Utc::now().to_rfc3339(),
                });
            }

            // Stop after first frame
            capture_control.stop();
            Ok(())
        }

        fn on_closed(&mut self) -> std::result::Result<(), Self::Error> {
            Ok(())
        }
    }

    /// Capture a screenshot of an osu! game window
    pub fn capture_game_window(target: CaptureTarget) -> Result<CapturedFrame> {
        // Find the window
        let window = find_osu_window(target)?;
        let window_title = window
            .title()
            .map_err(|e| Error::Other(format!("Failed to get window title: {}", e)))?;

        // Create temp file path
        let temp_path = std::env::temp_dir().join(format!(
            "osu_capture_{}.png",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));

        // Set up capture state
        let state = Arc::new(Mutex::new(CaptureState {
            result: None,
            window_title,
            temp_path,
        }));

        // Configure capture settings with all required parameters
        let settings = Settings::new(
            window,
            CursorCaptureSettings::WithoutCursor,
            DrawBorderSettings::WithoutBorder,
            SecondaryWindowSettings::Default,
            MinimumUpdateIntervalSettings::Default,
            DirtyRegionSettings::Default,
            ColorFormat::Rgba8,
            Arc::clone(&state),
        );

        // Start capture (blocks until frame captured or error)
        SingleFrameCapture::start(settings)
            .map_err(|e| Error::Other(format!("Capture failed: {}", e)))?;

        // Extract result
        let captured = state
            .lock()
            .map_err(|_| Error::Other("Failed to lock result".into()))?
            .result
            .take()
            .ok_or_else(|| Error::Other("No frame captured".into()))?;

        Ok(captured)
    }

    /// Find an osu! window by target type
    fn find_osu_window(target: CaptureTarget) -> Result<Window> {
        // Enumerate all windows
        let windows = Window::enumerate()
            .map_err(|e| Error::Other(format!("Failed to enumerate windows: {}", e)))?;

        for window in windows {
            if !window.is_valid() {
                continue;
            }

            let title = match window.title() {
                Ok(t) => t,
                Err(_) => continue,
            };

            let title_lower = title.to_lowercase();

            // Check if this is an osu! window
            if !title_lower.contains("osu") {
                continue;
            }

            // Determine if this is lazer
            let is_lazer = is_lazer_window(&title);

            // Match based on target
            match target {
                CaptureTarget::Stable if !is_lazer => return Ok(window),
                CaptureTarget::Lazer if is_lazer => return Ok(window),
                CaptureTarget::Any => return Ok(window),
                _ => continue,
            }
        }

        Err(Error::Other(format!(
            "No {} window found. Make sure the game is running.",
            target
        )))
    }

    /// Check if a window title indicates osu!lazer
    fn is_lazer_window(title: &str) -> bool {
        let title_lower = title.to_lowercase();
        title_lower.contains("lazer") || title_lower.contains("osu! -") // lazer format
    }

    /// List all capturable osu! windows
    pub fn list_osu_windows() -> Result<Vec<WindowInfo>> {
        let windows = Window::enumerate()
            .map_err(|e| Error::Other(format!("Failed to enumerate windows: {}", e)))?;

        let mut result = Vec::new();

        for window in windows {
            if !window.is_valid() {
                continue;
            }

            let title = match window.title() {
                Ok(t) => t,
                Err(_) => continue,
            };

            if !title.to_lowercase().contains("osu") {
                continue;
            }

            // Get window dimensions from rect (RECT has left, top, right, bottom)
            let (width, height) = match window.rect() {
                Ok(rect) => (
                    (rect.right - rect.left) as u32,
                    (rect.bottom - rect.top) as u32,
                ),
                Err(_) => (0, 0),
            };
            let process_name = window.process_name().ok();
            let is_lazer = is_lazer_window(&title);

            result.push(WindowInfo {
                title,
                width,
                height,
                process_name,
                is_lazer,
            });
        }

        Ok(result)
    }
}

// ============================================================================
// Non-Windows Stubs
// ============================================================================

#[cfg(not(windows))]
mod stub_impl {
    use super::*;

    /// Capture is not supported on non-Windows platforms
    pub fn capture_game_window(_target: CaptureTarget) -> Result<CapturedFrame> {
        Err(Error::Other(
            "Game window capture is only supported on Windows".into(),
        ))
    }

    /// List is not supported on non-Windows platforms
    pub fn list_osu_windows() -> Result<Vec<WindowInfo>> {
        Ok(Vec::new())
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Capture a screenshot of an osu! game window.
///
/// # Platform Support
///
/// This function only works on Windows. On other platforms, it returns an error.
///
/// # Example
///
/// ```ignore
/// use osu_sync_core::vision::{capture_game_window, CaptureTarget};
///
/// let frame = capture_game_window(CaptureTarget::Any)?;
/// frame.save_to_file(std::path::Path::new("screenshot.png"))?;
/// ```
#[cfg(windows)]
pub use windows_impl::capture_game_window;

#[cfg(not(windows))]
pub use stub_impl::capture_game_window;

/// List all capturable osu! windows.
#[cfg(windows)]
pub use windows_impl::list_osu_windows;

#[cfg(not(windows))]
pub use stub_impl::list_osu_windows;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capture_target_display() {
        assert_eq!(CaptureTarget::Stable.display_name(), "osu!stable");
        assert_eq!(CaptureTarget::Lazer.display_name(), "osu!lazer");
        assert_eq!(CaptureTarget::Any.display_name(), "osu!");
    }

    #[test]
    fn test_list_windows_doesnt_crash() {
        // Should not panic even if no windows found
        let _ = list_osu_windows();
    }

    #[test]
    #[ignore] // Requires osu! to be running
    fn test_capture_any_window() {
        let result = capture_game_window(CaptureTarget::Any);
        if let Ok(frame) = result {
            assert!(frame.width > 0);
            assert!(frame.height > 0);
            assert!(!frame.png_bytes.is_empty());
        }
    }
}
