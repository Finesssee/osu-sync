//! Reusable TUI widgets

mod footer;
mod header;
mod spinner;
mod status_bar;
mod tabs;

pub use footer::render_footer;
pub use header::render_header;
pub use spinner::get_spinner_frame;
pub use status_bar::render_status_bar;
pub use tabs::render_tabs;
