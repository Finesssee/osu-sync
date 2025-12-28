//! Test unified storage module components

use osu_sync_core::config::{detect_lazer_path, detect_stable_path};
use osu_sync_core::unified::{
    find_running_processes, is_process_running, GameLaunchDetector,
    UnifiedStorageConfig, UnifiedStorageMode,
};
use std::path::Path;

fn main() {
    // Initialize tracing for debug output
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("=== Unified Storage Module Test ===\n");

    // Test 1: Path Detection
    println!("--- 1. Path Detection ---");
    let stable_path = detect_stable_path();
    let lazer_path = detect_lazer_path();

    println!("osu!stable: {:?}", stable_path);
    println!("osu!lazer: {:?}", lazer_path);

    // Test 2: Process Detection
    println!("\n--- 2. Process Detection ---");

    // List all osu processes
    println!("All osu! processes:");
    let osu_processes = find_running_processes("osu");
    if osu_processes.is_empty() {
        println!("  No osu! processes found");
    } else {
        for proc in &osu_processes {
            println!("  - PID: {}, Name: {}", proc.pid, proc.name);
        }
    }

    // Test 3: Unified Storage Config
    println!("\n--- 3. Unified Storage Config ---");

    // Test disabled config
    let disabled_config = UnifiedStorageConfig::disabled();
    println!("Disabled config:");
    println!("  Mode: {:?}", disabled_config.mode);
    println!("  Enabled: {}", disabled_config.mode.is_enabled());

    // Test stable master config
    let stable_master_config = UnifiedStorageConfig::stable_master();
    println!("\nStable Master config:");
    println!("  Mode: {:?}", stable_master_config.mode);
    println!("  Enabled: {}", stable_master_config.mode.is_enabled());
    println!("  Shared resources:");
    for resource in &stable_master_config.shared_resources {
        println!("    - {:?}", resource);
    }

    // Test 4: Game Launch Detector
    println!("\n--- 4. Game Launch Detector ---");
    let detector = GameLaunchDetector::new();

    // Check current running games
    println!("Currently running osu! games:");
    let stable_running = detector.is_stable_running();
    let lazer_running = detector.is_lazer_running();

    if !stable_running && !lazer_running {
        println!("  None running");
    } else {
        if stable_running {
            println!("  - osu! stable is running");
        }
        if lazer_running {
            println!("  - osu! lazer is running");
        }
    }

    // Test 5: Storage Mode Descriptions
    println!("\n--- 5. Storage Modes Available ---");
    let modes = [
        UnifiedStorageMode::StableMaster,
        UnifiedStorageMode::LazerMaster,
        UnifiedStorageMode::TrueUnified,
    ];

    for mode in modes {
        let desc = match mode {
            UnifiedStorageMode::Disabled => {
                "Unified storage is disabled"
            }
            UnifiedStorageMode::StableMaster => {
                "osu!stable is the master, lazer links to stable's Songs folder"
            }
            UnifiedStorageMode::LazerMaster => {
                "osu!lazer is the master, stable links to lazer's file store"
            }
            UnifiedStorageMode::TrueUnified => {
                "Both clients link to a shared unified storage location"
            }
        };
        println!("  {:?}: {}", mode, desc);
    }

    // Test 6: Beatmap Count Comparison
    println!("\n--- 6. Beatmap Detection ---");

    // osu!stable beatmaps
    if let Some(stable) = &stable_path {
        let songs_path = stable.join("Songs");
        if songs_path.exists() {
            match std::fs::read_dir(&songs_path) {
                Ok(entries) => {
                    let count = entries
                        .filter_map(|e| e.ok())
                        .filter(|e| e.path().is_dir())
                        .count();
                    println!("osu!stable Songs folder: {} beatmap folders", count);
                }
                Err(e) => println!("Error reading stable Songs: {}", e),
            }
        } else {
            println!("osu!stable Songs folder not found");
        }
    }

    // osu!lazer beatmaps (using the database)
    // Try D:\osu!lazer first (known location from previous tests)
    let lazer_db_path = Path::new("D:\\osu!lazer");
    if lazer_db_path.exists() {
        match osu_sync_core::lazer::LazerDatabase::open(lazer_db_path) {
            Ok(db) => {
                match db.get_all_beatmap_sets() {
                    Ok(sets) => {
                        let total_beatmaps: usize = sets.iter().map(|s| s.beatmaps.len()).sum();
                        println!("osu!lazer (D:\\osu!lazer): {} beatmap sets, {} beatmaps", sets.len(), total_beatmaps);
                    }
                    Err(e) => println!("Error reading lazer beatmaps: {}", e),
                }
            }
            Err(e) => println!("Error opening lazer database: {}", e),
        }
    }

    println!("\n=== All Tests Complete ===");
}
