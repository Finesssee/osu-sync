//! CLI/headless mode for scripting and testing
//!
//! Usage:
//!   osu-sync --cli scan                    Scan installations
//!   osu-sync --cli dry-run <direction>     Preview sync
//!   osu-sync --cli sync <direction>        Perform sync
//!   osu-sync --cli relink                  Relink stable copies onto lazer's files
//!
//! Directions: stable-to-lazer, lazer-to-stable, bidirectional
//!
//! Options:
//!   --set-ids <ids>    Comma-separated beatmap set IDs to sync
//!   --json             Output in JSON format
//!   --relink           After sync s2l or bi, relink stable copies onto lazer's files

use std::collections::HashSet;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use osu_sync_core::config::{
    live_guard, validate_lazer_path, validate_stable_path, Config, PathOverrides,
};
use osu_sync_core::lazer::LazerDatabase;
use osu_sync_core::linkstore::{ensure_stable_closed, RelinkReport, Relinker};
use osu_sync_core::stable::StableScanner;
use osu_sync_core::sync::{
    DryRunResult, SyncDirection, SyncEngineBuilder, SyncError, SyncProgress, SyncResult,
};

/// CLI command to execute
#[derive(Debug, Clone)]
pub enum CliCommand {
    Scan,
    DryRun {
        direction: SyncDirection,
        set_ids: Option<HashSet<i32>>,
    },
    Sync {
        direction: SyncDirection,
        set_ids: Option<HashSet<i32>>,
    },
    Relink,
}

/// CLI options
#[derive(Debug, Clone, Default)]
pub struct CliOptions {
    pub json: bool,
    /// Relink stable copies onto lazer's files after a stable-to-lazer sync.
    pub relink: bool,
    /// Threads for a relink; `None` uses the relink default.
    pub threads: Option<NonZeroUsize>,
}

/// Flags accepted in every mode, before or after `--cli`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GlobalFlags {
    pub overrides: PathOverrides,
    pub allow_live: bool,
}

impl GlobalFlags {
    /// Removes the global flags from `args` and returns them with the remaining args.
    pub fn take(args: Vec<String>) -> Result<(Self, Vec<String>), String> {
        let mut flags = Self::default();
        let mut rest = Vec::with_capacity(args.len());
        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            let (name, inline) = match arg.split_once('=') {
                Some((name, value)) => (name, Some(value.to_string())),
                None => (arg.as_str(), None),
            };
            let slot = match name {
                "--stable-path" => &mut flags.overrides.stable,
                "--lazer-path" => &mut flags.overrides.lazer,
                "--allow-live" if inline.is_none() => {
                    flags.allow_live = true;
                    continue;
                }
                _ => {
                    rest.push(arg);
                    continue;
                }
            };
            let value = match inline {
                Some(value) => Some(value),
                None => args.next().filter(|v| !v.starts_with("--")),
            }
            .filter(|v| !v.is_empty())
            .ok_or_else(|| format!("{} requires a folder", name))?;
            *slot = Some(PathBuf::from(value));
        }
        Ok((flags, rest))
    }

    pub fn validate(&self) -> Result<(), String> {
        if let Some(stable) = &self.overrides.stable {
            if !validate_stable_path(stable) {
                return Err(format!(
                    "--stable-path {} is not an osu!stable folder (no Songs folder)",
                    stable.display()
                ));
            }
        }
        if let Some(lazer) = &self.overrides.lazer {
            if !validate_lazer_path(lazer) {
                return Err(format!(
                    "--lazer-path {} is not an osu!lazer data folder (needs client.realm and files)",
                    lazer.display()
                ));
            }
        }
        Ok(())
    }
}

/// Parse CLI arguments and return command + options
pub fn parse_args(args: &[String]) -> Result<(CliCommand, CliOptions), String> {
    let mut options = CliOptions::default();
    let mut command: Option<CliCommand> = None;
    let mut set_ids: Option<HashSet<i32>> = None;

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        match arg.as_str() {
            "--json" => options.json = true,
            "--relink" => options.relink = true,
            "--threads" => {
                i += 1;
                let value = args.get(i).ok_or("--threads requires a value")?;
                options.threads = Some(value.parse().map_err(|_| {
                    format!("--threads needs a whole number of 1 or more, not '{value}'")
                })?);
            }
            "--set-ids" => {
                i += 1;
                if i >= args.len() {
                    return Err("--set-ids requires a value".to_string());
                }
                set_ids = Some(parse_set_ids(&args[i])?);
            }
            "scan" => command = Some(CliCommand::Scan),
            "relink" => command = Some(CliCommand::Relink),
            "dry-run" => {
                i += 1;
                if i >= args.len() {
                    return Err("dry-run requires a direction".to_string());
                }
                let direction = parse_direction(&args[i])?;
                command = Some(CliCommand::DryRun {
                    direction,
                    set_ids: None,
                });
            }
            "sync" => {
                i += 1;
                if i >= args.len() {
                    return Err("sync requires a direction".to_string());
                }
                let direction = parse_direction(&args[i])?;
                command = Some(CliCommand::Sync {
                    direction,
                    set_ids: None,
                });
            }
            // Flags main reads itself; the path flags are normally taken out already.
            "--cli" | "--gui" | "--allow-live" | "--help" | "-h" => {}
            "--stable-path" | "--lazer-path" => i += 1,
            _ if arg.starts_with("--stable-path=") || arg.starts_with("--lazer-path=") => {}
            "--dry-run" => return Err(
                "Unknown flag: --dry-run. For a dry run use the subcommand: dry-run <direction>"
                    .to_string(),
            ),
            _ if arg.starts_with('-') => return Err(format!("Unknown flag: {}", arg)),
            _ => {
                if command.is_none() {
                    return Err(format!("Unknown command: {}", arg));
                }
            }
        }
        i += 1;
    }

    // Apply set_ids to command if present
    let command = match command {
        Some(CliCommand::DryRun { direction, .. }) => CliCommand::DryRun { direction, set_ids },
        Some(CliCommand::Sync { direction, .. }) => CliCommand::Sync { direction, set_ids },
        Some(cmd) => cmd,
        None => {
            return Err(
                "No command specified. Use: scan, dry-run <dir>, sync <dir>, or relink".to_string(),
            )
        }
    };
    let relinks = matches!(
        command,
        CliCommand::Sync {
            direction: SyncDirection::StableToLazer | SyncDirection::Bidirectional,
            ..
        }
    );
    if options.relink && !relinks {
        return Err("--relink works only with sync s2l or sync bi".to_string());
    }
    if options.threads.is_some() && !options.relink && !matches!(command, CliCommand::Relink) {
        return Err("--threads works only with relink or sync s2l/bi --relink".to_string());
    }

    Ok((command, options))
}

fn parse_direction(s: &str) -> Result<SyncDirection, String> {
    match s.to_lowercase().as_str() {
        "stable-to-lazer" | "s2l" | "stl" => Ok(SyncDirection::StableToLazer),
        "lazer-to-stable" | "l2s" | "lts" => Ok(SyncDirection::LazerToStable),
        "bidirectional" | "bi" | "both" => Ok(SyncDirection::Bidirectional),
        _ => Err(format!(
            "Invalid direction '{}'. Use: stable-to-lazer, lazer-to-stable, or bidirectional",
            s
        )),
    }
}

fn parse_set_ids(s: &str) -> Result<HashSet<i32>, String> {
    s.split(',')
        .map(|id| {
            id.trim()
                .parse::<i32>()
                .map_err(|_| format!("Invalid set ID: {}", id))
        })
        .collect()
}

/// Run CLI command
pub fn run(command: CliCommand, options: CliOptions) -> anyhow::Result<()> {
    match command {
        CliCommand::Scan => run_scan(options),
        CliCommand::DryRun { direction, set_ids } => run_dry_run(direction, set_ids, options),
        CliCommand::Sync { direction, set_ids } => run_sync(direction, set_ids, options),
        CliCommand::Relink => run_relink(options),
    }
}

/// Replaces the stable files that are plain copies of lazer files with hard links to
/// them. Both folders pass the live guard and stable must be closed.
fn run_relink(options: CliOptions) -> anyhow::Result<()> {
    let config = Config::load();
    let songs = config
        .stable_songs_path()
        .ok_or_else(|| anyhow::anyhow!("osu!stable path not configured"))?;
    let files = config
        .lazer_files_path()
        .ok_or_else(|| anyhow::anyhow!("osu!lazer path not configured"))?;
    live_guard::check_write(&songs)?;
    live_guard::check_write(&files)?;
    ensure_stable_closed()?;

    let relinker = relinker(&songs, &files, &options);
    let show_progress = !options.json;
    let report = relinker.run(&mut |done, total| {
        if show_progress && (done % 1000 == 0 || done == total) {
            eprint!("\rRelinking: {done}/{total}");
        }
    })?;
    if show_progress {
        eprintln!();
    }
    print_relink_report(&report, options);
    relink_failures(&report)
}

/// The relinker `relink` runs, from `songs` onto `files` with the cache for `songs`.
fn relinker(songs: &Path, files: &Path, options: &CliOptions) -> Relinker {
    Relinker::new(songs, files, Relinker::default_cache(songs)).threads(options.threads)
}

/// A relink with any file that failed exits nonzero, after its report is printed.
fn relink_failures(report: &RelinkReport) -> anyhow::Result<()> {
    if !report.errors.is_empty() {
        anyhow::bail!("{} files failed to relink", report.errors.len());
    }
    Ok(())
}

fn print_relink_report(report: &RelinkReport, options: CliOptions) {
    if options.json {
        println!("{}", serde_json::json!(report));
        return;
    }
    println!("Relink Complete:");
    println!("  Relinked:        {}", report.relinked);
    println!("  Bytes reclaimed: {}", report.bytes_reclaimed);
    println!(
        "  Hashed:          {} files, {} bytes",
        report.hashed_files, report.hashed_bytes
    );
    for (reason, count) in &report.skipped {
        println!("  Skipped ({reason:?}): {count}");
    }
    for error in &report.errors {
        println!("  Error: {error}");
    }
    for note in &report.notes {
        println!();
        println!("Note: {note}");
    }
}

fn run_scan(options: CliOptions) -> anyhow::Result<()> {
    let config = Config::load();

    let stable_result = if let Some(ref stable_path) = config.stable_path {
        let songs_path = stable_path.join("Songs");
        if songs_path.exists() {
            let scanner = StableScanner::new(songs_path).skip_hashing();
            match scanner.scan_parallel() {
                Ok(sets) => Some((stable_path.clone(), sets.len())),
                Err(e) => {
                    eprintln!("Warning: Failed to scan stable: {}", e);
                    None
                }
            }
        } else {
            None
        }
    } else {
        None
    };

    let lazer_result = config.lazer_path.as_ref().map(|lazer_path| {
        let counts = LazerDatabase::open(lazer_path)
            .and_then(|db| {
                let sets = db.get_all_beatmap_sets()?;
                Ok((
                    sets.len(),
                    sets.iter().map(|s| s.files.len()).sum::<usize>(),
                    db.skipped().map(ToString::to_string),
                ))
            })
            .map_err(|e| format!("Failed to open lazer database: {}", e));
        (lazer_path.clone(), counts)
    });

    if options.json {
        println!(
            "{}",
            serde_json::json!({
                "stable": stable_result.as_ref().map(|(path, count)| {
                    serde_json::json!({
                        "path": path.to_string_lossy(),
                        "beatmap_sets": count
                    })
                }),
                "lazer": lazer_result.as_ref().map(|(path, counts)| match counts {
                    Ok((count, named_files, warning)) => {
                        let mut lazer = serde_json::json!({
                            "path": path.to_string_lossy(),
                            "beatmap_sets": count,
                            "named_files": named_files
                        });
                        if let Some(warning) = warning {
                            lazer["warning"] = warning.as_str().into();
                        }
                        lazer
                    }
                    Err(error) => serde_json::json!({
                        "path": path.to_string_lossy(),
                        "error": error
                    }),
                })
            })
        );
    } else {
        println!("osu-sync scan results:");
        println!();
        if let Some((path, count)) = stable_result {
            println!("osu!stable: {} ({} beatmap sets)", path.display(), count);
        } else {
            println!("osu!stable: Not configured or not found");
        }
        match &lazer_result {
            Some((path, Ok((count, _, warning)))) => {
                println!("osu!lazer:  {} ({} beatmap sets)", path.display(), count);
                if let Some(warning) = warning {
                    println!("Warning: {}", warning);
                }
            }
            Some((path, Err(error))) => println!("osu!lazer:  {} ({})", path.display(), error),
            None => println!("osu!lazer:  Not configured or not found"),
        }
    }

    match lazer_result {
        Some((_, Err(error))) => Err(anyhow::anyhow!(error)),
        _ => Ok(()),
    }
}

fn run_dry_run(
    direction: SyncDirection,
    set_ids: Option<HashSet<i32>>,
    options: CliOptions,
) -> anyhow::Result<()> {
    let config = Config::load();

    let stable_path = config
        .stable_path
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("osu!stable path not configured"))?;
    let lazer_path = config
        .lazer_path
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("osu!lazer path not configured"))?;

    let songs_path = stable_path.join("Songs");
    let scanner = StableScanner::new(songs_path).skip_hashing();
    let database = LazerDatabase::open(lazer_path)?;

    let cancelled = Arc::new(AtomicBool::new(false));

    let mut builder = SyncEngineBuilder::new()
        .config(config)
        .stable_scanner(scanner)
        .lazer_database(database)
        .cancellation(Arc::clone(&cancelled));

    if let Some(ids) = set_ids {
        builder = builder.selected_set_ids(ids);
    }

    let engine = builder.build()?;
    let result = engine.dry_run(direction)?;

    print_dry_run_result(&result, options);

    Ok(())
}

/// The sync engine builder with the relink settings from `options`.
fn sync_builder(options: &CliOptions) -> SyncEngineBuilder {
    SyncEngineBuilder::new()
        .relink(options.relink)
        .relink_threads(options.threads)
}

fn run_sync(
    direction: SyncDirection,
    set_ids: Option<HashSet<i32>>,
    options: CliOptions,
) -> anyhow::Result<()> {
    let config = Config::load();

    let stable_path = config
        .stable_path
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("osu!stable path not configured"))?;
    let lazer_path = config
        .lazer_path
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("osu!lazer path not configured"))?;
    live_guard::check_sync(direction, &config)?;
    let import_dir = config.lazer_import_path().unwrap_or_default();

    let songs_path = stable_path.join("Songs");
    let scanner = StableScanner::new(songs_path).skip_hashing();
    let database = LazerDatabase::open(lazer_path)?;

    let cancelled = Arc::new(AtomicBool::new(false));

    // Progress callback for non-JSON mode
    let show_progress = !options.json;
    let progress_callback: Box<dyn Fn(SyncProgress) + Send + Sync> = if show_progress {
        Box::new(|progress: SyncProgress| {
            eprint!(
                "\rSyncing: {}/{} - {}",
                progress.current, progress.total, progress.current_name
            );
        })
    } else {
        Box::new(|_| {})
    };

    let mut builder = sync_builder(&options)
        .config(config)
        .stable_scanner(scanner)
        .lazer_database(database)
        .progress_callback(progress_callback)
        .cancellation(Arc::clone(&cancelled));

    if let Some(ids) = set_ids {
        builder = builder.selected_set_ids(ids);
    }

    let engine = builder.build()?;
    let resolver = osu_sync_core::sync::AutoResolver::skip_all();
    let result = engine.sync(direction, &resolver)?;

    if show_progress {
        eprintln!(); // New line after progress
    }

    let json = options.json;
    print_sync_result(&result, options);
    if result.staged > 0 && !json {
        println!();
        println!(
            "osu!lazer was not started. The staged .osz files are in {}",
            import_dir.display()
        );
    }

    failures(&result)
}

/// A sync with failed sets exits nonzero, after its result is printed.
fn failures(result: &SyncResult) -> anyhow::Result<()> {
    if result.failed > 0 {
        anyhow::bail!("{} beatmap sets failed", result.failed);
    }
    Ok(())
}

fn print_dry_run_result(result: &DryRunResult, options: CliOptions) {
    use osu_sync_core::sync::DryRunAction;

    if options.json {
        let items: Vec<_> = result
            .items
            .iter()
            .map(|item| {
                serde_json::json!({
                    "set_id": item.set_id,
                    "title": item.title,
                    "artist": item.artist,
                    "action": format!("{:?}", item.action),
                    "size_bytes": item.size_bytes,
                    "difficulty_count": item.difficulty_count,
                })
            })
            .collect();

        let import_count = result
            .items
            .iter()
            .filter(|i| matches!(i.action, DryRunAction::Import))
            .count();
        let skip_count = result
            .items
            .iter()
            .filter(|i| matches!(i.action, DryRunAction::Skip))
            .count();
        let duplicate_count = result
            .items
            .iter()
            .filter(|i| matches!(i.action, DryRunAction::Duplicate))
            .count();

        println!(
            "{}",
            serde_json::json!({
                "summary": {
                    "total": result.items.len(),
                    "import": import_count,
                    "skip": skip_count,
                    "duplicate": duplicate_count,
                },
                "items": items
            })
        );
    } else {
        let import_count = result
            .items
            .iter()
            .filter(|i| matches!(i.action, DryRunAction::Import))
            .count();
        let skip_count = result
            .items
            .iter()
            .filter(|i| matches!(i.action, DryRunAction::Skip))
            .count();
        let duplicate_count = result
            .items
            .iter()
            .filter(|i| matches!(i.action, DryRunAction::Duplicate))
            .count();

        println!("Dry Run Results:");
        println!("  Total:      {}", result.items.len());
        println!("  To Import:  {}", import_count);
        println!("  Skip:       {}", skip_count);
        println!("  Duplicates: {}", duplicate_count);
        println!();

        // Show first 20 items to import
        let imports: Vec<_> = result
            .items
            .iter()
            .filter(|i| matches!(i.action, DryRunAction::Import))
            .take(20)
            .collect();

        if !imports.is_empty() {
            println!("Items to import (first 20):");
            for item in imports {
                println!(
                    "  [{}] {} - {}",
                    item.set_id.map(|id| id.to_string()).unwrap_or_default(),
                    item.artist,
                    item.title
                );
            }
            if import_count > 20 {
                println!("  ... and {} more", import_count - 20);
            }
        }
    }
}

fn print_sync_result(result: &SyncResult, options: CliOptions) {
    if options.json {
        let entries = |list: &[SyncError]| -> Vec<serde_json::Value> {
            list.iter()
                .map(|e| {
                    serde_json::json!({
                        "beatmap_set": e.beatmap_set,
                        "message": e.message,
                    })
                })
                .collect()
        };

        println!(
            "{}",
            serde_json::json!({
                "imported": result.imported,
                "staged": result.staged,
                "failed": result.failed,
                "skipped": result.skipped,
                "errors": entries(&result.errors),
                "skips": entries(&result.skips),
                "notes": result.notes,
            })
        );
    } else {
        println!("Sync Complete:");
        println!("  Imported: {}", result.imported);
        println!("  Staged:   {}", result.staged);
        println!("  Failed:   {}", result.failed);
        println!("  Skipped:  {}", result.skipped);

        for (title, list) in [
            ("Errors:", &result.errors),
            ("Skipped sets:", &result.skips),
        ] {
            if list.is_empty() {
                continue;
            }
            println!();
            println!("{title}");
            for entry in list {
                if let Some(ref set) = entry.beatmap_set {
                    println!("  - [{}] {}", set, entry.message);
                } else {
                    println!("  - {}", entry.message);
                }
            }
        }
        for note in &result.notes {
            println!();
            println!("Note: {note}");
        }
    }
}

/// Print CLI help
pub fn print_help() {
    println!("osu-sync CLI Mode");
    println!();
    println!("USAGE:");
    println!("    osu-sync --cli <command> [options]");
    println!();
    println!("COMMANDS:");
    println!("    scan                        Scan and show installations");
    println!("    dry-run <direction>         Preview what would be synced");
    println!("    sync <direction>            Perform sync");
    println!("    relink                      Hard-link stable copies of lazer files to them");
    println!();
    println!("DIRECTIONS:");
    println!("    stable-to-lazer, s2l        Sync from stable to lazer");
    println!("    lazer-to-stable, l2s        Sync from lazer to stable");
    println!("    bidirectional, bi           Sync both directions");
    println!();
    println!("OPTIONS:");
    println!("    --set-ids <ids>             Comma-separated beatmap set IDs");
    println!("    --json                      Output in JSON format");
    println!(
        "    --relink                    After sync s2l or bi, relink stable copies to lazer's files"
    );
    println!(
        "    --threads <n>               Threads for relink (default: a quarter of the CPUs, 1 to 4)"
    );
    println!("    --stable-path <dir>         Use this osu!stable folder");
    println!("    --lazer-path <dir>          Use this osu!lazer data folder");
    println!("    --allow-live                Allow writes into the detected live installs");
    println!();
    println!("EXAMPLES:");
    println!("    osu-sync --cli scan");
    println!("    osu-sync --cli dry-run stable-to-lazer");
    println!("    osu-sync --cli sync s2l --set-ids 123,456,789");
    println!("    osu-sync --cli dry-run bi --json");
    println!("    osu-sync --cli relink --json");
}

#[cfg(test)]
mod tests {
    use super::*;
    use osu_sync_core::config::SaveOutcome;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn failed_sets_exit_nonzero() {
        let failed = SyncResult {
            failed: 500,
            ..Default::default()
        };
        assert_eq!(
            failures(&failed).unwrap_err().to_string(),
            "500 beatmap sets failed"
        );
        let clean = SyncResult {
            imported: 3,
            skipped: 2,
            ..Default::default()
        };
        assert!(failures(&clean).is_ok());
    }

    #[test]
    fn unknown_flag_is_rejected() {
        assert_eq!(
            parse_args(&strings(&["sync", "l2s", "--dry-run"])).unwrap_err(),
            "Unknown flag: --dry-run. For a dry run use the subcommand: dry-run <direction>"
        );
        assert_eq!(
            parse_args(&strings(&["sync", "l2s", "--jsno"])).unwrap_err(),
            "Unknown flag: --jsno"
        );
        assert_eq!(
            parse_args(&strings(&["-x", "scan"])).unwrap_err(),
            "Unknown flag: -x"
        );
    }

    #[test]
    fn known_flags_are_accepted_after_global_flags_are_taken() {
        let (_, rest) = GlobalFlags::take(strings(&[
            "osu-sync",
            "--stable-path",
            "D:/stable",
            "--allow-live",
            "--cli",
            "--lazer-path=D:/lazer",
            "sync",
            "l2s",
            "--json",
            "--set-ids",
            "1,2",
            "--gui",
            "--allow-live",
            "--stable-path=D:/other",
        ]))
        .unwrap();
        let cli = rest.iter().position(|a| a == "--cli").unwrap();
        let (command, options) = parse_args(&rest[cli + 1..]).unwrap();
        assert!(options.json);
        assert!(matches!(
            command,
            CliCommand::Sync {
                direction: SyncDirection::LazerToStable,
                set_ids: Some(ref ids),
            } if ids.len() == 2
        ));
        assert!(parse_args(&strings(&["--cli", "--stable-path", "D:/s", "scan"])).is_ok());
    }

    #[test]
    fn parses_path_overrides() {
        let (flags, rest) = GlobalFlags::take(strings(&[
            "osu-sync",
            "--stable-path",
            "D:/osu-sync-sandbox/a/stable",
            "--cli",
            "sync",
            "l2s",
            "--lazer-path",
            "D:/osu-sync-sandbox/a/lazer",
            "--json",
        ]))
        .unwrap();

        assert_eq!(
            flags,
            GlobalFlags {
                overrides: PathOverrides {
                    stable: Some(PathBuf::from("D:/osu-sync-sandbox/a/stable")),
                    lazer: Some(PathBuf::from("D:/osu-sync-sandbox/a/lazer")),
                },
                allow_live: false,
            }
        );
        assert_eq!(
            rest,
            strings(&["osu-sync", "--cli", "sync", "l2s", "--json"])
        );

        let (flags, rest) = GlobalFlags::take(strings(&["osu-sync", "--allow-live"])).unwrap();
        assert!(flags.allow_live);
        assert!(flags.overrides.is_empty());
        assert_eq!(rest, strings(&["osu-sync"]));

        assert_eq!(
            GlobalFlags::take(strings(&["osu-sync", "--lazer-path", "--cli"])).unwrap_err(),
            "--lazer-path requires a folder"
        );
    }

    #[test]
    fn parses_path_overrides_with_equals() {
        let (flags, rest) = GlobalFlags::take(strings(&[
            "osu-sync",
            "--stable-path=D:/osu-sync-sandbox/a/stable",
            "--cli",
            "scan",
            "--lazer-path=D:/osu-sync-sandbox/a b/lazer",
        ]))
        .unwrap();

        assert_eq!(
            flags.overrides,
            PathOverrides {
                stable: Some(PathBuf::from("D:/osu-sync-sandbox/a/stable")),
                lazer: Some(PathBuf::from("D:/osu-sync-sandbox/a b/lazer")),
            }
        );
        assert_eq!(rest, strings(&["osu-sync", "--cli", "scan"]));

        assert_eq!(
            GlobalFlags::take(strings(&["osu-sync", "--lazer-path="])).unwrap_err(),
            "--lazer-path requires a folder"
        );

        let (flags, _) =
            GlobalFlags::take(strings(&["osu-sync", "--stable-path=D:/missing-folder"])).unwrap();
        assert_eq!(
            flags.validate().unwrap_err(),
            "--stable-path D:/missing-folder is not an osu!stable folder (no Songs folder)"
        );
    }

    #[test]
    fn override_skips_config_save() {
        let (flags, _) =
            GlobalFlags::take(strings(&["osu-sync", "--lazer-path", "D:/sandbox/lazer"])).unwrap();
        assert!(!flags.overrides.is_empty());

        let outcome = Config::default().save_with(Some(&flags.overrides)).unwrap();
        assert_eq!(outcome, SaveOutcome::SkippedForPathOverrides);
    }

    #[test]
    fn test_parse_direction() {
        assert!(matches!(
            parse_direction("stable-to-lazer"),
            Ok(SyncDirection::StableToLazer)
        ));
        assert!(matches!(
            parse_direction("s2l"),
            Ok(SyncDirection::StableToLazer)
        ));
        assert!(matches!(
            parse_direction("lazer-to-stable"),
            Ok(SyncDirection::LazerToStable)
        ));
        assert!(matches!(
            parse_direction("l2s"),
            Ok(SyncDirection::LazerToStable)
        ));
        assert!(matches!(
            parse_direction("bidirectional"),
            Ok(SyncDirection::Bidirectional)
        ));
        assert!(matches!(
            parse_direction("bi"),
            Ok(SyncDirection::Bidirectional)
        ));
        assert!(parse_direction("invalid").is_err());
    }

    #[test]
    fn test_parse_set_ids() {
        let ids = parse_set_ids("123,456,789").unwrap();
        assert_eq!(ids.len(), 3);
        assert!(ids.contains(&123));
        assert!(ids.contains(&456));
        assert!(ids.contains(&789));

        let ids = parse_set_ids("123").unwrap();
        assert_eq!(ids.len(), 1);
        assert!(ids.contains(&123));

        assert!(parse_set_ids("abc").is_err());
    }

    #[test]
    fn test_parse_args_scan() {
        let args = vec!["scan".to_string()];
        let (cmd, _) = parse_args(&args).unwrap();
        assert!(matches!(cmd, CliCommand::Scan));
    }

    #[test]
    fn test_parse_args_dry_run() {
        let args = vec!["dry-run".to_string(), "stable-to-lazer".to_string()];
        let (cmd, _) = parse_args(&args).unwrap();
        assert!(matches!(
            cmd,
            CliCommand::DryRun {
                direction: SyncDirection::StableToLazer,
                ..
            }
        ));
    }

    #[test]
    fn test_parse_args_sync_with_set_ids() {
        let args = vec![
            "sync".to_string(),
            "s2l".to_string(),
            "--set-ids".to_string(),
            "123,456".to_string(),
        ];
        let (cmd, _) = parse_args(&args).unwrap();
        match cmd {
            CliCommand::Sync { direction, set_ids } => {
                assert!(matches!(direction, SyncDirection::StableToLazer));
                let ids = set_ids.unwrap();
                assert!(ids.contains(&123));
                assert!(ids.contains(&456));
            }
            _ => panic!("Expected Sync command"),
        }
    }

    #[test]
    fn test_parse_args_json_option() {
        let args = vec!["scan".to_string(), "--json".to_string()];
        let (_, options) = parse_args(&args).unwrap();
        assert!(options.json);
    }

    #[test]
    fn parses_relink() {
        let (cmd, options) = parse_args(&strings(&["relink", "--json"])).unwrap();
        assert!(matches!(cmd, CliCommand::Relink));
        assert!(options.json);
        assert!(!options.relink);

        let (_, options) = parse_args(&strings(&["sync", "s2l", "--relink"])).unwrap();
        assert!(options.relink);
        assert_eq!(
            parse_args(&strings(&["relink", "--relnik"])).unwrap_err(),
            "Unknown flag: --relnik"
        );
    }

    #[test]
    fn relink_flag_is_rejected_outside_s2l_and_bi_sync() {
        for args in [
            &["sync", "l2s", "--relink"][..],
            &["--relink", "dry-run", "s2l"],
            &["dry-run", "bi", "--relink"],
            &["scan", "--relink"],
            &["relink", "--relink"],
        ] {
            assert_eq!(
                parse_args(&strings(args)).unwrap_err(),
                "--relink works only with sync s2l or sync bi",
                "{args:?}"
            );
        }
        for args in [
            &["sync", "s2l", "--relink"][..],
            &["--relink", "sync", "stable-to-lazer"],
            &["sync", "bi", "--relink", "--json"],
        ] {
            let (_, options) = parse_args(&strings(args)).unwrap();
            assert!(options.relink, "{args:?}");
        }
    }

    #[test]
    fn threads_flag_sets_the_relink_thread_count() {
        let (_, options) = parse_args(&strings(&["relink", "--threads", "2"])).unwrap();
        assert_eq!(options.threads, NonZeroUsize::new(2));
        let (_, options) =
            parse_args(&strings(&["sync", "s2l", "--relink", "--threads", "32"])).unwrap();
        assert_eq!(options.threads, NonZeroUsize::new(32));
        let (_, options) = parse_args(&strings(&["relink"])).unwrap();
        assert_eq!(options.threads, None);
    }

    #[test]
    fn threads_flag_reaches_the_relinker_and_the_sync_engine() {
        // Building a relinker touches no file, so the folders need not exist.
        let dir = Path::new("not-there");
        for (threads, expected) in [("7", 7), ("32", 32), ("1", 1)] {
            let (_, options) = parse_args(&strings(&["relink", "--threads", threads])).unwrap();
            let r = relinker(&dir.join("Songs"), &dir.join("files"), &options);
            assert_eq!(r.thread_count(), expected);

            let (_, options) =
                parse_args(&strings(&["sync", "s2l", "--relink", "--threads", threads])).unwrap();
            assert_eq!(
                sync_builder(&options).requested_relink_threads(),
                NonZeroUsize::new(expected)
            );
        }
    }

    #[test]
    fn threads_flag_rejects_zero_words_and_commands_that_do_not_relink() {
        for (args, error) in [
            (
                &["relink", "--threads", "0"][..],
                "--threads needs a whole number of 1 or more, not '0'",
            ),
            (
                &["relink", "--threads", "four"],
                "--threads needs a whole number of 1 or more, not 'four'",
            ),
            (
                &["relink", "--threads", "-1"],
                "--threads needs a whole number of 1 or more, not '-1'",
            ),
            (&["relink", "--threads"], "--threads requires a value"),
            (
                &["sync", "s2l", "--threads", "2"],
                "--threads works only with relink or sync s2l/bi --relink",
            ),
            (
                &["scan", "--threads", "2"],
                "--threads works only with relink or sync s2l/bi --relink",
            ),
            (
                &["dry-run", "s2l", "--threads", "2"],
                "--threads works only with relink or sync s2l/bi --relink",
            ),
        ] {
            assert_eq!(parse_args(&strings(args)).unwrap_err(), error, "{args:?}");
        }
    }

    #[test]
    fn relink_with_failed_files_exits_nonzero() {
        let failed = RelinkReport {
            relinked: 3,
            errors: vec!["a: denied".to_string(), "b: denied".to_string()],
            ..Default::default()
        };
        assert_eq!(
            relink_failures(&failed).unwrap_err().to_string(),
            "2 files failed to relink"
        );
        let locked_only = RelinkReport {
            relinked: 3,
            notes: vec!["left a as it is".to_string()],
            ..Default::default()
        };
        assert!(relink_failures(&locked_only).is_ok());
    }
}
