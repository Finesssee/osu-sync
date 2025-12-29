//! Vision module for AI-assisted TUI interaction.
//!
//! This module provides capabilities for AI systems to "see" the TUI state:
//! - `TuiSnapshot` - Capture TUI buffer and state as text/JSON
//! - `StateData` - Structured representation of each screen's state
//!
//! ## Usage
//!
//! ```ignore
//! use osu_sync_cli::vision::TuiSnapshot;
//!
//! // Capture current state
//! let snapshot = TuiSnapshot::capture(120, 30)?;
//! println!("{}", snapshot.as_json()?);
//! ```

mod snapshot;

pub use snapshot::{StateData, TuiSnapshot};
