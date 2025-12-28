//! CLI/headless mode for scripting and testing
//!
//! Usage:
//!   osu-sync --cli scan                    Scan installations
//!   osu-sync --cli dry-run <direction>     Preview sync
//!   osu-sync --cli sync <direction>        Perform sync
//!
//! Directions: stable-to-lazer, lazer-to-stable, bidirectional
//!
//! Options:
//!   --set-ids <ids>    Comma-separated beatmap set IDs to sync
//!   --json             Output in JSON format

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use osu_sync_core::config::Config;
use osu_sync_core::lazer::LazerDatabase;
use osu_sync_core::stable::StableScanner;
use osu_sync_core::sync::{DryRunResult, SyncDirection, SyncEngineBuilder, SyncProgress, SyncResult};

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
}

/// CLI options
#[derive(Debug, Clone, Default)]
pub struct CliOptions {
    pub json: bool,
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
            "--set-ids" => {
                i += 1;
                if i >= args.len() {
                    return Err("--set-ids requires a value".to_string());
                }
                set_ids = Some(parse_set_ids(&args[i])?);
            }
            "scan" => command = Some(CliCommand::Scan),
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
            _ => {
                if !arg.starts_with('-') && command.is_none() {
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
        None => return Err("No command specified. Use: scan, dry-run <dir>, or sync <dir>".to_string()),
    };

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
    }
}

fn run_scan(options: CliOptions) -> anyhow::Result<()> {
    let config = Config::load();

    let stable_result = if let Some(ref stable_path) = config.stable_path {
        let songs_path = stable_path.join("Songs");
        if songs_path.exists() {
            let scanner = StableScanner::new(songs_path).skip_hashing();
            match scanner.scan() {
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

    let lazer_result = if let Some(ref lazer_path) = config.lazer_path {
        match LazerDatabase::open(lazer_path) {
            Ok(db) => match db.get_all_beatmap_sets() {
                Ok(sets) => Some((lazer_path.clone(), sets.len())),
                Err(e) => {
                    eprintln!("Warning: Failed to read lazer database: {}", e);
                    None
                }
            },
            Err(e) => {
                eprintln!("Warning: Failed to open lazer database: {}", e);
                None
            }
        }
    } else {
        None
    };

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
                "lazer": lazer_result.as_ref().map(|(path, count)| {
                    serde_json::json!({
                        "path": path.to_string_lossy(),
                        "beatmap_sets": count
                    })
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
        if let Some((path, count)) = lazer_result {
            println!("osu!lazer:  {} ({} beatmap sets)", path.display(), count);
        } else {
            println!("osu!lazer:  Not configured or not found");
        }
    }

    Ok(())
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

    let mut builder = SyncEngineBuilder::new()
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

    print_sync_result(&result, options);

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
        let errors: Vec<_> = result
            .errors
            .iter()
            .map(|e| {
                serde_json::json!({
                    "beatmap_set": e.beatmap_set,
                    "message": e.message,
                })
            })
            .collect();

        println!(
            "{}",
            serde_json::json!({
                "imported": result.imported,
                "failed": result.failed,
                "skipped": result.skipped,
                "errors": errors,
            })
        );
    } else {
        println!("Sync Complete:");
        println!("  Imported: {}", result.imported);
        println!("  Failed:   {}", result.failed);
        println!("  Skipped:  {}", result.skipped);

        if !result.errors.is_empty() {
            println!();
            println!("Errors:");
            for error in &result.errors {
                if let Some(ref set) = error.beatmap_set {
                    println!("  - [{}] {}", set, error.message);
                } else {
                    println!("  - {}", error.message);
                }
            }
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
    println!();
    println!("DIRECTIONS:");
    println!("    stable-to-lazer, s2l        Sync from stable to lazer");
    println!("    lazer-to-stable, l2s        Sync from lazer to stable");
    println!("    bidirectional, bi           Sync both directions");
    println!();
    println!("OPTIONS:");
    println!("    --set-ids <ids>             Comma-separated beatmap set IDs");
    println!("    --json                      Output in JSON format");
    println!();
    println!("EXAMPLES:");
    println!("    osu-sync --cli scan");
    println!("    osu-sync --cli dry-run stable-to-lazer");
    println!("    osu-sync --cli sync s2l --set-ids 123,456,789");
    println!("    osu-sync --cli dry-run bi --json");
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(matches!(parse_direction("bi"), Ok(SyncDirection::Bidirectional)));
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
}
