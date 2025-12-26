//! Reusable TUI widgets

mod header;
mod footer;
mod status_bar;
mod spinner;
mod tabs;

pub use header::render_header;
pub use footer::render_footer;
pub use status_bar::render_status_bar;
pub use spinner::get_spinner_frame;
pub use tabs::render_tabs;
