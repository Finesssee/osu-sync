//! osu-sync - Beatmap synchronization tool for osu!stable and osu!lazer
//!
//! Usage:
//!   osu-sync              Run TUI mode (default)
//!   osu-sync --gui        Run GUI mode (requires 'gui' feature)
//!   osu-sync --cli <cmd>  Run CLI mode (headless)
//!   osu-sync --test <script>  Run automated TUI test
//!   osu-sync --help       Show help

use std::fs::File;
use std::sync::mpsc;
use std::time::Duration;

use tracing::Level;
use tracing_subscriber::fmt::writer::MakeWriterExt;
use tracing_subscriber::FmtSubscriber;

mod app;
mod cli;
mod event;
mod gui;
mod resolver;
mod screens;
pub mod theme;
mod tui;
mod tui_runner;
pub mod tui_test;
pub mod vision;
mod widgets;
mod worker;

use app::App;
use worker::Worker;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // Check for --help
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return Ok(());
    }

    // Check for --tui-snapshot flag (Vision Phase 1)
    if args.iter().any(|a| a == "--tui-snapshot") {
        let json_output = args.iter().any(|a| a == "--json");
        return run_tui_snapshot(json_output);
    }

    // Check for --capture-game flag (Vision Phase 2)
    if let Some(pos) = args.iter().position(|a| a == "--capture-game") {
        let target = args.get(pos + 1).map(|s| s.as_str()).unwrap_or("any");
        return run_capture_game(target);
    }

    // Check for --cli flag
    if let Some(cli_pos) = args.iter().position(|a| a == "--cli") {
        // Get args after --cli
        let cli_args: Vec<String> = args.iter().skip(cli_pos + 1).cloned().collect();

        if cli_args.is_empty() || cli_args.iter().any(|a| a == "--help" || a == "-h") {
            cli::print_help();
            return Ok(());
        }

        match cli::parse_args(&cli_args) {
            Ok((command, options)) => {
                return cli::run(command, options);
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                eprintln!();
                cli::print_help();
                std::process::exit(1);
            }
        }
    }

    // Check for --test flag
    if let Some(test_pos) = args.iter().position(|a| a == "--test") {
        let script = args.get(test_pos + 1);

        if script.is_none() || script.map(|s| s == "--help" || s == "-h").unwrap_or(false) {
            tui_runner::print_help();
            return Ok(());
        }

        return tui_runner::run_test(script.unwrap());
    }

    // Check for --gui flag
    if args.iter().any(|a| a == "--gui") {
        #[cfg(feature = "gui")]
        {
            gui::run().map_err(|e| anyhow::anyhow!("GUI error: {}", e))?;
            return Ok(());
        }
        #[cfg(not(feature = "gui"))]
        {
            eprintln!("Error: GUI mode requires the 'gui' feature.");
            eprintln!("Rebuild with: cargo build --release --features gui");
            std::process::exit(1);
        }
    }

    // Default: TUI mode
    tui::install_panic_hook();
    init_logging();
    let result = run();
    tui::restore()?;
    result
}

fn print_help() {
    println!("osu-sync v{}", env!("CARGO_PKG_VERSION"));
    println!("Sync beatmaps between osu!stable and osu!lazer");
    println!();
    println!("USAGE:");
    println!("    osu-sync [OPTIONS]");
    println!();
    println!("OPTIONS:");
    println!("    --gui                   Run in GUI mode (requires 'gui' feature)");
    println!("    --cli <cmd>             Run in CLI mode (headless, for scripting)");
    println!("    --test <script>         Run automated TUI test from script");
    println!("    --tui-snapshot [--json] Capture TUI state (for AI vision)");
    println!("    --capture-game [target] Capture osu! game window (Windows only)");
    println!("    --help                  Show this help message");
    println!();
    println!("By default, osu-sync runs in TUI (terminal) mode.");
    println!();
    println!("For CLI mode help: osu-sync --cli --help");
    println!("For test mode help: osu-sync --test --help");
    println!();
    println!("VISION COMMANDS:");
    println!("    --tui-snapshot          Output TUI buffer as text");
    println!("    --tui-snapshot --json   Output TUI state as structured JSON");
    println!("    --capture-game          Capture any osu! window screenshot");
    println!("    --capture-game stable   Capture osu!stable window");
    println!("    --capture-game lazer    Capture osu!lazer window");
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
    // Load config and set theme
    let config = osu_sync_core::config::Config::load();
    theme::set_theme(config.theme);

    // Initialize terminal
    let mut terminal = tui::init()?;

    // Set up worker communication
    let (app_tx, app_rx) = mpsc::channel();
    let worker = Worker::spawn(app_tx);

    // Create app with channels and cancellation flag
    let mut app = App::new().with_channels(worker.sender(), app_rx, worker.cancellation_flag());

    // Auto-scan installations on startup
    app.start_scan();

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

/// Capture TUI snapshot for AI vision (Phase 1)
fn run_tui_snapshot(json_output: bool) -> anyhow::Result<()> {
    use vision::TuiSnapshot;

    let snapshot = TuiSnapshot::capture(120, 30)?;

    if json_output {
        println!("{}", snapshot.as_json()?);
    } else {
        print!("{}", snapshot.as_text());
    }

    Ok(())
}

/// Capture game window screenshot (Phase 2)
fn run_capture_game(target: &str) -> anyhow::Result<()> {
    #[cfg(all(windows, feature = "vision"))]
    {
        use osu_sync_core::vision::{capture_game_window, CaptureTarget};

        let target = match target.to_lowercase().as_str() {
            "stable" => CaptureTarget::Stable,
            "lazer" => CaptureTarget::Lazer,
            _ => CaptureTarget::Any,
        };

        let frame = capture_game_window(target)?;

        // Save to file
        let filename = format!("osu_capture_{}.png", chrono::Utc::now().timestamp());
        std::fs::write(&filename, &frame.png_bytes)?;

        eprintln!(
            "Captured {}x{} from '{}' -> {}",
            frame.width, frame.height, frame.window_title, filename
        );

        Ok(())
    }

    #[cfg(any(not(windows), not(feature = "vision")))]
    {
        let _ = target;
        anyhow::bail!("Game window capture requires Windows and the 'vision' feature")
    }
}
