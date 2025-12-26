//! osu-sync TUI - Terminal interface for syncing osu! beatmaps between stable and lazer

use std::fs::File;
use std::sync::mpsc;
use std::time::Duration;

use tracing::Level;
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::FmtSubscriber;

mod app;
mod event;
mod resolver;
mod screens;
mod tui;
mod widgets;
mod worker;

use app::App;
use worker::Worker;

fn main() -> anyhow::Result<()> {
    // Install panic hook for terminal restoration
    tui::install_panic_hook();

    // Initialize logging (to file to avoid TUI interference)
    init_logging();

    // Run the application
    let result = run();

    // Restore terminal
    tui::restore()?;

    result
}

fn init_logging() {
    // For TUI apps, log to a file to avoid corrupting the terminal display
    // Try to create a log file, fall back to no logging if it fails
    if let Ok(log_file) = File::create("osu-sync.log") {
        let subscriber = FmtSubscriber::builder()
            .with_max_level(Level::WARN)
            .with_target(false)
            .with_ansi(false)
            .with_writer(log_file.with_max_level(Level::WARN))
            .finish();

        let _ = tracing::subscriber::set_global_default(subscriber);
    }
    // If file creation fails, logging is simply disabled (no subscriber set)
}

fn run() -> anyhow::Result<()> {
    // Initialize terminal
    let mut terminal = tui::init()?;

    // Set up worker communication
    let (app_tx, app_rx) = mpsc::channel();
    let worker = Worker::spawn(app_tx);

    // Create app with channels
    let mut app = App::new().with_channels(worker.sender(), app_rx);

    // Main event loop
    loop {
        // Render
        terminal.draw(|frame| app.render(frame))?;

        // Handle input events
        if let Some(key) = event::poll(Duration::from_millis(50))? {
            app.handle_key(key);
        }

        // Process worker messages
        app.process_worker_messages();

        // Check for quit
        if app.should_quit {
            break;
        }
    }

    // Shutdown worker
    worker.shutdown();

    Ok(())
}
