# osu-sync-core

Core library for beatmap synchronization. All I/O and business logic lives here.

## STRUCTURE

```
src/
├── beatmap/        # BeatmapSet, BeatmapInfo, GameMode
├── collection/     # Collection model + sync engine
├── config/         # Config struct, path detection
├── dedup/          # DuplicateDetector, strategies
├── filter/         # FilterCriteria, FilterEngine
├── lazer/          # LazerDatabase, StableDatabase, FileStore
├── media/          # MediaExtractor (audio, backgrounds)
├── parser/         # .osu file parser, .osz handler
├── replay/         # ReplayReader, ReplayExporter
├── stable/         # StableScanner, Importer/Exporter
├── stats/          # StatsAnalyzer, export formats
├── sync/           # SyncEngine, DryRun, conflicts
├── unified/        # Unified storage (symlinks/junctions)
├── activity.rs     # ActivityLog for recent operations
├── error.rs        # Error enum with thiserror
└── lib.rs          # Public API re-exports
```

## WHERE TO LOOK

| Task | Location |
|------|----------|
| Parse beatmap file | `parser/osu_file.rs` |
| Create/extract .osz | `parser/osz.rs` |
| Read lazer Realm DB | `lazer/database.rs` → LazerDatabase |
| Read stable osu!.db | `lazer/database.rs` → StableDatabase |
| Lazer file store access | `lazer/file_store.rs` |
| Sync orchestration | `sync/engine.rs` → SyncEngine |
| Dry run preview | `sync/dry_run.rs` |
| Conflict resolution | `sync/conflict.rs` |
| Duplicate detection | `dedup/detector.rs` |
| Beatmap filtering | `filter/engine.rs` |
| Statistics analysis | `stats/analyzer.rs` |
| Export stats | `stats/export.rs` |
| Create symlinks | `unified/link.rs` |
| File watching | `unified/watcher.rs` |
| Game detection | `unified/game_detect.rs` |
| Migration | `unified/migration.rs` |

## CONVENTIONS

- **Builder Pattern**: `SyncEngineBuilder` for complex construction
- **Progress Callbacks**: `Box<dyn Fn(Progress) + Send>` for updates
- **Result<T>**: Custom `Result<T, Error>` alias in `error.rs`
- **Rayon**: Parallel iterators for file operations
- **Chrono**: DateTime handling with serde support

## KEY TYPES

```rust
// Beatmaps
BeatmapSet { id, beatmaps: Vec<BeatmapInfo>, ... }
BeatmapInfo { hash, metadata, difficulty, ... }
GameMode { Osu, Taiko, Catch, Mania }

// Sync
SyncDirection { StableToLazer, LazerToStable, Bidirectional }
SyncProgress { phase, current, total, message }
DryRunResult { items: Vec<DryRunItem>, ... }

// Duplicates
DuplicateInfo { source, existing, match_type }
DuplicateStrategy { Skip, Overwrite, Ask }

// Unified
UnifiedStorageMode { Disabled, StableMaster, LazerMaster, TrueUnified }
LinkType { Symlink, Junction, HardLink }
```

## ANTI-PATTERNS

- **Panic in library**: Return `Error`, never panic
- **Hardcoded Windows paths**: Use `directories` crate
- **Sync file ops in callbacks**: Callbacks should be fast

## NOTES

- **LazerDatabase** uses realm-db-reader (read-only)
- **StableDatabase** uses osu-db crate (read-only)
- **LazerFileStore**: Files stored as `{hash[0..2]}/{hash}` in files directory
- **Junctions on Windows**: Preferred over symlinks (no admin)
- **SHA256 hashing**: Cached for performance
