# Contributing to osu-sync

First off, thank you for considering contributing to osu-sync! It's people like you that make osu-sync such a great tool.

## Code of Conduct

This project and everyone participating in it is governed by our commitment to providing a welcoming and inclusive environment. Please be respectful and constructive in all interactions.

## How Can I Contribute?

### Reporting Bugs

Before creating bug reports, please check the existing issues to avoid duplicates. When you create a bug report, include as many details as possible:

- **Use a clear and descriptive title**
- **Describe the exact steps to reproduce the problem**
- **Describe the behavior you observed and what you expected**
- **Include your OS version and osu-sync version**
- **Include any error messages or logs** (`osu-sync.log`)

### Suggesting Features

Feature suggestions are welcome! Please:

- **Use a clear and descriptive title**
- **Provide a detailed description of the proposed feature**
- **Explain why this feature would be useful**
- **Consider how it fits with existing functionality**

### Pull Requests

1. **Fork the repo** and create your branch from `master`
2. **Make your changes** following the code style guidelines
3. **Add tests** if you've added code that should be tested
4. **Ensure tests pass** with `cargo test`
5. **Run formatting** with `cargo fmt`
6. **Run linting** with `cargo clippy`
7. **Write a clear commit message** following conventional commits
8. **Open a Pull Request** with a clear description

## Development Setup

```bash
# Clone your fork
git clone https://github.com/YOUR_USERNAME/osu-sync.git
cd osu-sync

# Build
cargo build

# Run tests
cargo test

# Run with logging
RUST_LOG=debug cargo run
```

## Code Style

### Rust Guidelines

- Follow the [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `cargo fmt` for formatting
- Address all `cargo clippy` warnings
- Write documentation for public APIs
- Prefer explicit error handling over `.unwrap()`

### Commit Messages

We follow [Conventional Commits](https://www.conventionalcommits.org/):

```
feat(core): add beatmap filtering by star rating
fix(cli): correct progress bar calculation
docs: update installation instructions
refactor(sync): simplify duplicate detection logic
test(backup): add compression level tests
chore: update dependencies
```

### Branch Naming

- `feature/description` - New features
- `fix/description` - Bug fixes
- `docs/description` - Documentation changes
- `refactor/description` - Code refactoring

## Project Structure

```
crates/
├── osu-sync-core/          # Core library
│   ├── src/
│   │   ├── beatmap/        # Beatmap parsing and models
│   │   ├── sync/           # Sync engine and strategies
│   │   ├── backup/         # Backup/restore functionality
│   │   ├── collection/     # Collection management
│   │   ├── media/          # Media extraction
│   │   ├── replay/         # Replay export
│   │   ├── stats/          # Statistics and analysis
│   │   └── config/         # Configuration management
│   └── Cargo.toml
│
└── osu-sync-cli/           # TUI application
    ├── src/
    │   ├── screens/        # TUI screens
    │   ├── widgets/        # Reusable UI components
    │   ├── app.rs          # Main application state
    │   └── theme.rs        # Color themes
    └── Cargo.toml
```

## Testing

### Running Tests

```bash
# All tests
cargo test

# Specific crate
cargo test -p osu-sync-core

# With output
cargo test -- --nocapture
```

### Writing Tests

- Place unit tests in the same file with `#[cfg(test)]`
- Use descriptive test names: `test_filter_by_star_rating_excludes_below_minimum`
- Test edge cases and error conditions
- Use `tempfile` for tests that need file system access

## Questions?

Feel free to open an issue with the "question" label if you have any questions about contributing.

Thank you for contributing! 🎮
