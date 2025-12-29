# PROJECT KNOWLEDGE BASE

**Generated:** 2025-12-29
**Commit:** 2ee2ac4
**Branch:** master

## OVERVIEW

Rust workspace for syncing beatmaps between osu!stable and osu!lazer. TUI-first with optional Iced GUI. Features bidirectional sync, collection management, media extraction, replay export, and unified storage (symlinks).

## STRUCTURE

```
osu-sync/
├── crates/
│   ├── osu-sync-core/     # Core library: sync, parsing, lazer DB
│   └── osu-sync-cli/      # TUI application (ratatui)
├── docs/                   # Feature documentation
├── Cargo.toml              # Workspace config
└── .github/workflows/      # CI (build, test, clippy, fmt)
```

## WHERE TO LOOK

| Task | Location | Notes |
|------|----------|-------|
| Add sync feature | `core/src/sync/` | SyncEngine orchestrates |
| Add beatmap filter | `core/src/filter/` | FilterCriteria + FilterEngine |
| New TUI screen | `cli/src/screens/` | Add module + AppState variant |
| Lazer DB queries | `core/src/lazer/database.rs` | Uses realm-db-reader |
| Stable DB queries | `core/src/lazer/database.rs` | StableDatabase uses osu-db crate |
| Add widget | `cli/src/widgets/` | Reusable TUI components |
| Unified storage | `core/src/unified/` | Symlink/junction logic |
| Media extraction | `core/src/media/` | Audio + background extraction |
| Replay export | `core/src/replay/` | .osr file handling |
| Backup/restore | `core/src/backup/` | Archive operations |

## CODE MAP

### Core Library Modules

| Module | Purpose |
|--------|---------|
| `beatmap` | BeatmapSet, BeatmapInfo, metadata structs |
| `collection` | Collection sync between stable/lazer |
| `config` | Config loading, path detection |
| `dedup` | Duplicate detection (hash, metadata, audio) |
| `filter` | FilterCriteria for beatmap filtering |
| `lazer` | LazerDatabase, LazerFileStore, StableDatabase |
| `media` | MediaExtractor for audio/backgrounds |
| `parser` | .osu file + .osz archive parsing |
| `replay` | ReplayReader, ReplayExporter |
| `stable` | StableScanner, StableImporter/Exporter |
| `stats` | StatsAnalyzer, comparison, export (JSON/CSV/HTML) |
| `sync` | SyncEngine, DryRun, ConflictResolver |
| `unified` | UnifiedStorageEngine, file watcher, game detection |

### CLI Architecture

```
App (state machine)
├── AppState enum (22 variants)
├── Worker (background thread, mpsc channels)
└── Screens (stateless render functions)
```

## CONVENTIONS

- **State Machine TUI**: `AppState` enum drives all screens
- **Worker Pattern**: Heavy ops via `WorkerMessage` / `AppMessage` channels
- **Catppuccin Theme**: Default theme, configurable in config
- **Re-exports**: `lib.rs` re-exports key types for ergonomic API
- **Error Handling**: thiserror for Error enum, anyhow in CLI

## ANTI-PATTERNS (THIS PROJECT)

- **Direct DB access in CLI**: Always go through core crate
- **Blocking in render**: All I/O via Worker thread
- **Hardcoded paths**: Use `config::detect_*_path()` functions
- **New dependencies**: Check workspace Cargo.toml first

## UNIQUE STYLES

- **Screen Pattern**: Each screen is `render(frame, area, state...)` function
- **Key Handling**: `event::is_*(&key)` helpers for input
- **Progress Callbacks**: Closures passed to engine methods
- **Dry Run**: Preview sync before execution

## COMMANDS

```bash
# Development
cargo build                    # Build all
cargo build -p osu-sync-cli    # Build CLI only
cargo build --features gui     # Build with GUI

# Testing
cargo test                     # All tests
cargo test -p osu-sync-core    # Core tests only
cargo test unified             # Unified storage tests

# Quality
cargo fmt                      # Format code
cargo clippy                   # Lint

# Run
cargo run                      # TUI mode
cargo run -- --help            # Show help
cargo run -- --cli scan        # CLI headless mode
cargo run -- --gui             # GUI mode (requires feature)
```

## DEPENDENCIES (KEY)

| Crate | Purpose |
|-------|---------|
| `ratatui` | TUI framework |
| `crossterm` | Terminal backend |
| `iced` | Optional GUI (feature-gated) |
| `rosu-map` | osu! beatmap parsing |
| `osu-db` | osu!stable database parsing |
| `realm-db-reader` | osu!lazer Realm database |
| `rayon` | Parallel processing |
| `notify` | File watching |
| `sysinfo` | Process detection |
| `blake3` | Fast file hashing (5-10x faster than SHA-256) |
| `bincode` | Binary cache serialization (5-10x faster than JSON) |
| `memmap2` | Memory-mapped file I/O for large files |

## NOTES

- **Windows Focus**: Primary platform, junctions for symlinks
- **Lazer File Store**: Content-addressed by SHA256 hash
- **Stable Songs Format**: `{SetID} Artist - Title/` folders
- **First Scan Slow**: Computes hashes, cached for subsequent runs
- **Game Detection**: Blocks unified storage ops while game running

## PERFORMANCE

- **Blake3 Hashing**: 5-10x faster than SHA-256 for file integrity
- **Bincode Cache**: Binary format, 5-10x faster load than JSON
- **Incremental Hashing**: Skips unchanged files (mtime/size check)
- **Memory-Mapped I/O**: Uses memmap2 for files >1MB
- **Parallel File Collection**: Rayon for concurrent file reads
- **Time-Based Progress**: Reports every 50ms instead of N items
