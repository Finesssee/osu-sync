<p align="center">
  <img src="https://raw.githubusercontent.com/ppy/osu/master/assets/lazer.png" alt="osu!" width="100">
</p>

<h1 align="center">osu-sync</h1>

<p align="center">
  <strong>Sync beatmaps between osu!stable and osu!lazer</strong>
</p>

<p align="center">
  <a href="https://github.com/Finesssee/osu-sync/releases/latest">
    <img src="https://img.shields.io/github/v/release/Finesssee/osu-sync?style=flat-square&color=ff66aa" alt="Release">
  </a>
  <a href="https://github.com/Finesssee/osu-sync/releases">
    <img src="https://img.shields.io/github/downloads/Finesssee/osu-sync/total?style=flat-square&color=ff66aa" alt="Downloads">
  </a>
  <a href="https://github.com/Finesssee/osu-sync/blob/master/LICENSE">
    <img src="https://img.shields.io/github/license/Finesssee/osu-sync?style=flat-square&color=ff66aa" alt="License">
  </a>
  <a href="https://github.com/Finesssee/osu-sync/actions">
    <img src="https://img.shields.io/github/actions/workflow/status/Finesssee/osu-sync/ci.yml?style=flat-square" alt="Build">
  </a>
  <a href="https://www.rust-lang.org/">
    <img src="https://img.shields.io/badge/language-Rust-orange?style=flat-square&logo=rust" alt="Rust">
  </a>
</p>

<p align="center">
  <a href="#features">Features</a> •
  <a href="#installation">Installation</a> •
  <a href="#usage">Usage</a> •
  <a href="#building">Building</a> •
  <a href="#contributing">Contributing</a>
</p>

---

## About

**osu-sync** is a powerful command-line tool for managing your osu! beatmap library across both osu!stable and osu!lazer installations. It features a beautiful terminal UI with Catppuccin theming and supports bidirectional sync, collection management, media extraction, and more.

### Why osu-sync?

- **Bidirectional Sync** - Sync beatmaps in either direction or both at once
- **Smart Deduplication** - Detects duplicates using multiple strategies (hash, metadata, audio fingerprint)
- **Fast Scanning** - Efficient file hashing with caching for 5x faster repeat scans
- **Beautiful TUI** - Clean terminal interface with Catppuccin Mocha theme
- **Single Binary** - Just one executable, no dependencies required

## Features

| Feature | Description |
|---------|-------------|
| **Beatmap Sync** | Bidirectional sync with ETA, pause/resume, and skip list |
| **Collection Sync** | Sync collections with preview and merge duplicate detection |
| **Statistics** | View beatmap stats with HTML export and recommendations |
| **Media Extraction** | Extract audio (with ID3 tags) and backgrounds (with size filtering) |
| **Replay Export** | Export replays with filters, stats, and custom rename patterns |
| **Backup/Restore** | Incremental backups with compression and verification |
| **Theme Support** | Catppuccin Mocha, Dark, and Light themes |

## Installation

### Download Binary (Recommended)

1. Download the latest `osu-sync.exe` from [Releases](https://github.com/Finesssee/osu-sync/releases/latest)
2. Place it anywhere on your system
3. Run it from terminal or double-click

### Using Cargo

```bash
cargo install --git https://github.com/Finesssee/osu-sync
```

### From Source

See [Building](#building) section below.

## Usage

### Quick Start

```bash
# Run the TUI (default)
osu-sync

# Show help
osu-sync --help
```

### Vision Commands (Optional)

```bash
# Capture current TUI state (text or JSON)
osu-sync --tui-snapshot
osu-sync --tui-snapshot --json

# Capture an osu! game window screenshot (Windows + vision feature only)
osu-sync --capture-game
osu-sync --capture-game stable
osu-sync --capture-game lazer
```

Note: `--capture-game` requires Windows and a build with the `vision` feature
enabled.

### TUI Navigation

| Key | Action |
|-----|--------|
| `↑/↓` or `j/k` | Navigate menu |
| `Enter` | Select option |
| `Esc` | Go back |
| `q` | Quit |

### Main Menu Options

1. **Scan Installations** - Detect osu!stable and osu!lazer paths
2. **Sync Beatmaps** - Synchronize beatmaps between installations
3. **Collection Sync** - Sync your beatmap collections
4. **Statistics** - View detailed beatmap statistics
5. **Extract Media** - Extract audio files and backgrounds
6. **Export Replays** - Export replay files with filtering
7. **Backup** - Create backups of your osu! data
8. **Restore** - Restore from a backup
9. **Configuration** - Configure paths and preferences

## Configuration

Configuration is stored in:
- **Windows**: `%APPDATA%\osu-sync\config.json`
- **Linux**: `~/.config/osu-sync/config.json`
- **macOS**: `~/Library/Application Support/osu-sync/config.json`

### Example Config

```json
{
  "stable_path": "C:\\Users\\You\\AppData\\Local\\osu!",
  "lazer_path": "C:\\Users\\You\\AppData\\Roaming\\osu",
  "theme": "Catppuccin",
  "duplicate_strategy": "Hash"
}
```

## Building

### Prerequisites

- [Rust](https://rustup.rs/) 1.75 or later
- Git

### Build Steps

```bash
# Clone the repository
git clone https://github.com/Finesssee/osu-sync.git
cd osu-sync

# Build release version
cargo build --release

# Binary will be at target/release/osu-sync.exe (Windows)
# or target/release/osu-sync (Linux/macOS)
```

### Build with GUI Support (Optional)

```bash
cargo build --release --features gui
```

### Build with Vision Capture (Optional, Windows only)

```bash
cargo build --release --features vision
```

### Run Tests

```bash
cargo test
```

## Project Structure

```
osu-sync/
├── crates/
│   ├── osu-sync-core/     # Core library (sync, parsing, etc.)
│   └── osu-sync-cli/      # TUI application
├── Cargo.toml             # Workspace configuration
└── README.md
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

### Development Setup

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

### Code Style

- Run `cargo fmt` before committing
- Run `cargo clippy` and address warnings
- Add tests for new functionality
- Update documentation as needed

## Roadmap

- [ ] Linux/macOS testing and support
- [ ] GUI mode with Iced framework
- [ ] Beatmap preview player
- [ ] Cloud sync integration
- [ ] Plugin system

## FAQ

<details>
<summary><strong>Where does osu-sync store its data?</strong></summary>

Configuration and cache are stored in your system's standard config directory. On Windows, this is `%APPDATA%\osu-sync`.
</details>

<details>
<summary><strong>Is it safe to use with my beatmaps?</strong></summary>

Yes! osu-sync only reads from source installations and writes to target installations. It never modifies your original files. We recommend creating a backup first anyway.
</details>

<details>
<summary><strong>Why is the first scan slow?</strong></summary>

The first scan computes hashes for all beatmap files to enable deduplication. Subsequent scans use cached hashes and are much faster (~5x).
</details>

<details>
<summary><strong>Does it work with osu!lazer's new storage format?</strong></summary>

Yes! osu-sync reads osu!lazer's Realm database directly and understands its file storage structure.
</details>

## Acknowledgments

- [ppy/osu](https://github.com/ppy/osu) - The osu!lazer project
- [Catppuccin](https://github.com/catppuccin/catppuccin) - Beautiful pastel color scheme
- [ratatui](https://github.com/ratatui-org/ratatui) - Terminal UI framework
- [osu-db](https://github.com/kovaxis/osu-db) - osu!stable database parsing

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---

<p align="center">
  Made with ❤️ for the osu! community
</p>
