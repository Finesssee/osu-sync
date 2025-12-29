# osu-sync-cli

TUI application for osu-sync. Uses ratatui + crossterm.

## STRUCTURE

```
src/
├── screens/           # 22 screen render modules
├── widgets/           # Reusable UI components
├── app.rs             # App struct, AppState, key handling
├── cli.rs             # CLI headless mode
├── event.rs           # Input event helpers
├── gui.rs             # Iced GUI (optional)
├── main.rs            # Entry point, mode selection
├── resolver.rs        # Duplicate resolution UI
├── theme.rs           # Catppuccin + theme system
├── tui.rs             # Terminal setup/restore
├── tui_runner.rs      # Automated test runner
├── tui_test.rs        # Test utilities
└── worker.rs          # Background worker thread
```

## WHERE TO LOOK

| Task | Location |
|------|----------|
| Add new screen | `screens/` + add AppState variant in `app.rs` |
| Handle key input | `app.rs` → `handle_*_key()` methods |
| Add widget | `widgets/` + use in screen |
| Background operation | `worker.rs` → add WorkerMessage variant |
| Theme colors | `theme.rs` |
| CLI command | `cli.rs` |

## SCREEN PATTERN

Each screen is a stateless render function:

```rust
// screens/my_screen.rs
pub fn render(
    frame: &mut Frame,
    area: Rect,
    // ... state fields passed in
) {
    // Use widgets to render
}
```

Routing in `screens/mod.rs`:
```rust
match &app.state {
    AppState::MyScreen { ... } => my_screen::render(frame, area, ...),
    ...
}
```

## APP STATE MACHINE

```rust
pub enum AppState {
    MainMenu { selected: usize },
    Scanning { in_progress, stable_result, lazer_result, ... },
    SyncConfig { selected, filter, ... },
    Syncing { progress, logs, stats, is_paused },
    DuplicateDialog { info, selected, apply_to_all },
    SyncComplete { result },
    // ... 16 more variants
}
```

## WORKER COMMUNICATION

```
App ──WorkerMessage──> Worker (background thread)
     <──AppMessage───
```

- `WorkerMessage`: Commands (StartScan, StartSync, etc.)
- `AppMessage`: Results (ScanComplete, SyncProgress, etc.)
- Cancellation via `Arc<AtomicBool>`

## KEY HANDLING

Use `event::is_*()` helpers:
```rust
if event::is_down(&key) { ... }
if event::is_enter(&key) { ... }
if event::is_key(&key, 'f') { ... }  // specific char
```

## WIDGETS

| Widget | Purpose |
|--------|---------|
| `header` | App title bar |
| `footer` | Keyboard hints |
| `spinner` | Loading indicator |
| `status_bar` | Status messages |
| `tabs` | Tab navigation |

## CONVENTIONS

- **No state in screens**: All state in `AppState`
- **Render pure**: Screens only read, never mutate
- **Key handling in app.rs**: Central input routing
- **Worker for I/O**: Never block UI thread

## ANTI-PATTERNS

- **State in screen modules**: Keep in AppState
- **Direct core calls in render**: Use Worker
- **Complex key handling in screens**: Delegate to app.rs

## NOTES

- **10 main menu items**: Sync, Collections, Stats, Media, Replay, Backup, Restore, Config, Unified, Exit
- **Help overlay**: Press `?` or `h` on most screens
- **Vim-style navigation**: j/k for up/down
- **Filter panel**: 'f' toggles in sync config
- **Dry run**: 'd' in sync config for preview
