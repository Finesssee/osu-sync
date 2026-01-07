//! TUI Test Runner - Automated testing of the actual TUI
//!
//! Usage:
//!   osu-sync --test <script.txt>   Run test script
//!   osu-sync --test -              Read commands from stdin
//!
//! Script format (one command per line):
//!   key <keyname>        Send a key (enter, esc, up, down, left, right, space, tab, etc.)
//!   char <c>             Send a character
//!   ctrl+<key>           Send Ctrl+key (e.g., ctrl+a, ctrl+c)
//!   wait <ms>            Wait for milliseconds
//!   wait_for <text>      Wait until screen contains text (timeout 5s)
//!   assert_screen <text> Assert screen contains text
//!   assert_state <name>  Assert current state name
//!   screenshot [file]    Save screenshot to file (default: screenshot.txt)
//!   print                Print current screen to stdout
//!   # comment            Comments start with #

use std::fs;
use std::io::{self, BufRead, Write};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::{CrosstermBackend, TestBackend};
use ratatui::Terminal;

use crate::app::{App, AppState};
use crate::worker::Worker;
use crate::{theme, tui};

/// Test context with both real terminal and capture backend
struct TestContext {
    terminal: Terminal<CrosstermBackend<io::Stdout>>,
    capture: Terminal<TestBackend>,
}

/// Run automated TUI test from script file or stdin
pub fn run_test(script_path: &str) -> anyhow::Result<()> {
    // Load config and set theme
    let config = osu_sync_core::config::Config::load();
    theme::set_theme(config.theme);

    // Read script
    let commands = if script_path == "-" {
        read_commands_from_stdin()?
    } else {
        read_commands_from_file(script_path)?
    };

    println!("Running TUI test with {} commands...", commands.len());

    // Initialize terminal
    tui::install_panic_hook();
    let terminal = tui::init()?;
    let size = terminal.size()?;

    // Create capture backend with same size
    let capture_backend = TestBackend::new(size.width, size.height);
    let capture = Terminal::new(capture_backend)?;

    let mut ctx = TestContext { terminal, capture };

    // Set up worker communication
    let (app_tx, app_rx) = mpsc::channel();
    let worker = Worker::spawn(app_tx);

    // Create app
    let mut app = App::new().with_channels(worker.sender(), app_rx, worker.cancellation_flag());

    // Auto-scan on startup
    app.start_scan();

    // Run test commands
    let result = run_commands(&mut ctx, &mut app, &commands);

    // Cleanup
    tui::restore()?;

    match result {
        Ok(()) => {
            println!("\n=== TEST PASSED ===");
            Ok(())
        }
        Err(e) => {
            eprintln!("\n=== TEST FAILED ===");
            eprintln!("Error: {}", e);
            Err(e)
        }
    }
}

fn read_commands_from_stdin() -> anyhow::Result<Vec<TestCommand>> {
    let stdin = io::stdin();
    let mut commands = Vec::new();

    for line in stdin.lock().lines() {
        let line = line?;
        if let Some(cmd) = parse_command(&line)? {
            commands.push(cmd);
        }
    }

    Ok(commands)
}

fn read_commands_from_file(path: &str) -> anyhow::Result<Vec<TestCommand>> {
    let content = fs::read_to_string(path)?;
    let mut commands = Vec::new();

    for line in content.lines() {
        if let Some(cmd) = parse_command(line)? {
            commands.push(cmd);
        }
    }

    Ok(commands)
}

#[derive(Debug, Clone)]
enum TestCommand {
    Key(KeyCode),
    Char(char),
    CtrlKey(char),
    Wait(u64),
    WaitFor(String),
    AssertScreen(String),
    AssertNotScreen(String),
    AssertState(String),
    Screenshot(Option<String>),
    Print,
}

fn parse_command(line: &str) -> anyhow::Result<Option<TestCommand>> {
    let line = line.trim();

    // Skip empty lines and comments
    if line.is_empty() || line.starts_with('#') {
        return Ok(None);
    }

    let parts: Vec<&str> = line.splitn(2, ' ').collect();
    let cmd = parts[0].to_lowercase();
    let arg = parts.get(1).map(|s| s.trim());

    let command = match cmd.as_str() {
        "key" => {
            let key = arg.ok_or_else(|| anyhow::anyhow!("key requires an argument"))?;
            TestCommand::Key(parse_key(key)?)
        }
        "char" => {
            let c = arg.ok_or_else(|| anyhow::anyhow!("char requires an argument"))?;
            let c = c
                .chars()
                .next()
                .ok_or_else(|| anyhow::anyhow!("invalid char"))?;
            TestCommand::Char(c)
        }
        "ctrl+a" | "ctrl+b" | "ctrl+c" | "ctrl+d" | "ctrl+e" | "ctrl+f" | "ctrl+g" | "ctrl+h"
        | "ctrl+i" | "ctrl+j" | "ctrl+k" | "ctrl+l" | "ctrl+m" | "ctrl+n" | "ctrl+o" | "ctrl+p"
        | "ctrl+q" | "ctrl+r" | "ctrl+s" | "ctrl+t" | "ctrl+u" | "ctrl+v" | "ctrl+w" | "ctrl+x"
        | "ctrl+y" | "ctrl+z" => {
            let c = cmd.chars().last().unwrap();
            TestCommand::CtrlKey(c)
        }
        "wait" => {
            let ms = arg
                .ok_or_else(|| anyhow::anyhow!("wait requires milliseconds"))?
                .parse()?;
            TestCommand::Wait(ms)
        }
        "wait_for" => {
            let text = arg.ok_or_else(|| anyhow::anyhow!("wait_for requires text"))?;
            TestCommand::WaitFor(text.to_string())
        }
        "assert_screen" | "assert" => {
            let text = arg.ok_or_else(|| anyhow::anyhow!("assert_screen requires text"))?;
            TestCommand::AssertScreen(text.to_string())
        }
        "assert_not_screen" | "assert_not" => {
            let text = arg.ok_or_else(|| anyhow::anyhow!("assert_not_screen requires text"))?;
            TestCommand::AssertNotScreen(text.to_string())
        }
        "assert_state" => {
            let state = arg.ok_or_else(|| anyhow::anyhow!("assert_state requires state name"))?;
            TestCommand::AssertState(state.to_string())
        }
        "screenshot" => TestCommand::Screenshot(arg.map(|s| s.to_string())),
        "print" => TestCommand::Print,
        "enter" => TestCommand::Key(KeyCode::Enter),
        "esc" | "escape" => TestCommand::Key(KeyCode::Esc),
        "up" => TestCommand::Key(KeyCode::Up),
        "down" => TestCommand::Key(KeyCode::Down),
        "left" => TestCommand::Key(KeyCode::Left),
        "right" => TestCommand::Key(KeyCode::Right),
        "space" => TestCommand::Char(' '),
        "tab" => TestCommand::Key(KeyCode::Tab),
        "backspace" => TestCommand::Key(KeyCode::Backspace),
        "pageup" => TestCommand::Key(KeyCode::PageUp),
        "pagedown" => TestCommand::Key(KeyCode::PageDown),
        "home" => TestCommand::Key(KeyCode::Home),
        "end" => TestCommand::Key(KeyCode::End),
        _ => return Err(anyhow::anyhow!("Unknown command: {}", cmd)),
    };

    Ok(Some(command))
}

fn parse_key(key: &str) -> anyhow::Result<KeyCode> {
    let key_lower = key.to_lowercase();
    match key_lower.as_str() {
        "enter" | "return" => Ok(KeyCode::Enter),
        "esc" | "escape" => Ok(KeyCode::Esc),
        "up" => Ok(KeyCode::Up),
        "down" => Ok(KeyCode::Down),
        "left" => Ok(KeyCode::Left),
        "right" => Ok(KeyCode::Right),
        "tab" => Ok(KeyCode::Tab),
        "backspace" | "bs" => Ok(KeyCode::Backspace),
        "delete" | "del" => Ok(KeyCode::Delete),
        "insert" | "ins" => Ok(KeyCode::Insert),
        "home" => Ok(KeyCode::Home),
        "end" => Ok(KeyCode::End),
        "pageup" | "pgup" => Ok(KeyCode::PageUp),
        "pagedown" | "pgdn" => Ok(KeyCode::PageDown),
        "f1" => Ok(KeyCode::F(1)),
        "f2" => Ok(KeyCode::F(2)),
        "f3" => Ok(KeyCode::F(3)),
        "f4" => Ok(KeyCode::F(4)),
        "f5" => Ok(KeyCode::F(5)),
        "f6" => Ok(KeyCode::F(6)),
        "f7" => Ok(KeyCode::F(7)),
        "f8" => Ok(KeyCode::F(8)),
        "f9" => Ok(KeyCode::F(9)),
        "f10" => Ok(KeyCode::F(10)),
        "f11" => Ok(KeyCode::F(11)),
        "f12" => Ok(KeyCode::F(12)),
        "space" => Ok(KeyCode::Char(' ')),
        _ if key.len() == 1 => Ok(KeyCode::Char(key.chars().next().unwrap())),
        _ => Err(anyhow::anyhow!("Unknown key: {}", key)),
    }
}

fn run_commands(
    ctx: &mut TestContext,
    app: &mut App,
    commands: &[TestCommand],
) -> anyhow::Result<()> {
    for (i, cmd) in commands.iter().enumerate() {
        // Render to both terminals
        render_both(ctx, app)?;

        // Process any pending messages
        app.process_worker_messages();

        // Execute command
        print!("[{}/{}] {:?} ... ", i + 1, commands.len(), cmd);
        io::stdout().flush()?;

        match cmd {
            TestCommand::Key(code) => {
                let event = KeyEvent::new(*code, KeyModifiers::NONE);
                app.handle_key(event);
                println!("OK");
            }
            TestCommand::Char(c) => {
                let event = KeyEvent::new(KeyCode::Char(*c), KeyModifiers::NONE);
                app.handle_key(event);
                println!("OK");
            }
            TestCommand::CtrlKey(c) => {
                let event = KeyEvent::new(KeyCode::Char(*c), KeyModifiers::CONTROL);
                app.handle_key(event);
                println!("OK");
            }
            TestCommand::Wait(ms) => {
                std::thread::sleep(Duration::from_millis(*ms));
                // Process messages that may have arrived
                app.process_worker_messages();
                println!("OK (waited {}ms)", ms);
            }
            TestCommand::WaitFor(text) => {
                let start = Instant::now();
                let timeout = Duration::from_secs(10);
                let mut found = false;

                while start.elapsed() < timeout {
                    render_both(ctx, app)?;
                    app.process_worker_messages();

                    let screen = capture_screen(&ctx.capture)?;
                    if screen.contains(text) {
                        found = true;
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }

                if found {
                    println!("OK (found after {:?})", start.elapsed());
                } else {
                    return Err(anyhow::anyhow!(
                        "Timeout waiting for '{}' (waited {:?})",
                        text,
                        timeout
                    ));
                }
            }
            TestCommand::AssertScreen(text) => {
                render_both(ctx, app)?;
                let screen = capture_screen(&ctx.capture)?;
                if screen.contains(text) {
                    println!("OK");
                } else {
                    println!("FAILED");
                    println!("\n--- Screen content ---");
                    println!("{}", screen);
                    println!("--- End screen ---\n");
                    return Err(anyhow::anyhow!("Screen does not contain: '{}'", text));
                }
            }
            TestCommand::AssertNotScreen(text) => {
                render_both(ctx, app)?;
                let screen = capture_screen(&ctx.capture)?;
                if !screen.contains(text) {
                    println!("OK");
                } else {
                    println!("FAILED");
                    return Err(anyhow::anyhow!("Screen should not contain: '{}'", text));
                }
            }
            TestCommand::AssertState(expected) => {
                let actual = get_state_name(&app.state);
                if actual == expected {
                    println!("OK (state: {})", actual);
                } else {
                    println!("FAILED");
                    return Err(anyhow::anyhow!(
                        "Expected state '{}' but got '{}'",
                        expected,
                        actual
                    ));
                }
            }
            TestCommand::Screenshot(path) => {
                render_both(ctx, app)?;
                let screen = capture_screen(&ctx.capture)?;
                let path = path.as_deref().unwrap_or("screenshot.txt");
                fs::write(path, &screen)?;
                println!("OK (saved to {})", path);
            }
            TestCommand::Print => {
                render_both(ctx, app)?;
                let screen = capture_screen(&ctx.capture)?;
                println!("\n--- Screen ---");
                println!("{}", screen);
                println!("--- End ---");
            }
        }

        // Check if app wants to quit
        if app.should_quit {
            println!("App requested quit");
            break;
        }
    }

    Ok(())
}

/// Render to both real terminal and capture backend
fn render_both(ctx: &mut TestContext, app: &mut App) -> anyhow::Result<()> {
    ctx.terminal.draw(|frame| app.render(frame))?;
    ctx.capture.draw(|frame| app.render(frame))?;
    Ok(())
}

fn capture_screen(terminal: &Terminal<TestBackend>) -> anyhow::Result<String> {
    let buffer = terminal.backend().buffer();
    let mut output = String::new();

    for y in 0..buffer.area.height {
        for x in 0..buffer.area.width {
            let cell = &buffer[(x, y)];
            output.push_str(cell.symbol());
        }
        output.push('\n');
    }

    Ok(output)
}

fn get_state_name(state: &AppState) -> &'static str {
    match state {
        AppState::MainMenu { .. } => "MainMenu",
        AppState::Scanning { .. } => "Scanning",
        AppState::SyncConfig { .. } => "SyncConfig",
        AppState::Syncing { .. } => "Syncing",
        AppState::DuplicateDialog { .. } => "DuplicateDialog",
        AppState::SyncComplete { .. } => "SyncComplete",
        AppState::Config { .. } => "Config",
        AppState::Statistics { .. } => "Statistics",
        AppState::CollectionConfig { .. } => "CollectionConfig",
        AppState::CollectionSync { .. } => "CollectionSync",
        AppState::CollectionSummary { .. } => "CollectionSummary",
        AppState::DryRunPreview { .. } => "DryRunPreview",
        AppState::BackupConfig { .. } => "BackupConfig",
        AppState::BackupProgress { .. } => "BackupProgress",
        AppState::BackupComplete { .. } => "BackupComplete",
        AppState::RestoreConfig { .. } => "RestoreConfig",
        AppState::RestoreConfirm { .. } => "RestoreConfirm",
        AppState::RestoreProgress { .. } => "RestoreProgress",
        AppState::RestoreComplete { .. } => "RestoreComplete",
        AppState::MediaConfig { .. } => "MediaConfig",
        AppState::MediaProgress { .. } => "MediaProgress",
        AppState::MediaComplete { .. } => "MediaComplete",
        AppState::ReplayConfig { .. } => "ReplayConfig",
        AppState::ReplayProgress { .. } => "ReplayProgress",
        AppState::ReplayComplete { .. } => "ReplayComplete",
        AppState::Help { .. } => "Help",
        AppState::UnifiedConfig { .. } => "UnifiedConfig",
        AppState::UnifiedSetup { .. } => "UnifiedSetup",
        AppState::UnifiedStatus { .. } => "UnifiedStatus",
        AppState::Exiting => "Exiting",
    }
}

/// Print help for test mode
pub fn print_help() {
    println!("TUI Test Runner");
    println!();
    println!("USAGE:");
    println!("    osu-sync --test <script.txt>   Run test from script file");
    println!("    osu-sync --test -              Read commands from stdin");
    println!();
    println!("SCRIPT COMMANDS:");
    println!("    key <name>           Send key (enter, esc, up, down, left, right, tab, etc.)");
    println!("    char <c>             Send character");
    println!("    ctrl+<key>           Send Ctrl+key (e.g., ctrl+a, ctrl+c)");
    println!("    enter                Shorthand for key enter");
    println!("    esc                  Shorthand for key esc");
    println!("    up/down/left/right   Shorthand for arrow keys");
    println!("    space                Shorthand for space character");
    println!("    pageup/pagedown      Page navigation");
    println!();
    println!("    wait <ms>            Wait for milliseconds");
    println!("    wait_for <text>      Wait until screen contains text (timeout 10s)");
    println!();
    println!("    assert <text>        Assert screen contains text");
    println!("    assert_not <text>    Assert screen does NOT contain text");
    println!("    assert_state <name>  Assert current state (MainMenu, DryRunPreview, etc.)");
    println!();
    println!("    screenshot [file]    Save screen to file (default: screenshot.txt)");
    println!("    print                Print current screen to stdout");
    println!();
    println!("    # comment            Lines starting with # are comments");
    println!();
    println!("EXAMPLE SCRIPT:");
    println!("    # Wait for scan to complete");
    println!("    wait_for Scan Complete");
    println!("    enter");
    println!();
    println!("    # Navigate to Sync Beatmaps");
    println!("    assert_state MainMenu");
    println!("    down");
    println!("    enter");
    println!();
    println!("    # Check we're in SyncConfig");
    println!("    assert_state SyncConfig");
}
