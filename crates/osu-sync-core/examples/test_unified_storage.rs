//! Test unified storage engine with real filesystem operations.
//!
//! Run with: cargo run --package osu-sync-core --example test_unified_storage

use osu_sync_core::unified::{UnifiedStorageConfig, UnifiedStorageEngine};
use std::fs::{self, File};
use std::io::Write;
use tempfile::TempDir;

fn main() {
    println!("=== Unified Storage Engine Test ===\n");

    // Create temporary directories
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let stable_path = temp_dir.path().join("osu-stable");
    let lazer_path = temp_dir.path().join("osu-lazer");
    let shared_path = temp_dir.path().join("shared-storage");

    fs::create_dir_all(&stable_path).expect("Failed to create stable dir");
    fs::create_dir_all(&lazer_path).expect("Failed to create lazer dir");
    fs::create_dir_all(&shared_path).expect("Failed to create shared dir");

    println!("Created test directories:");
    println!("  Stable: {}", stable_path.display());
    println!("  Lazer:  {}", lazer_path.display());
    println!("  Shared: {}", shared_path.display());

    // Create some test beatmaps in stable
    let songs_dir = stable_path.join("Songs");
    fs::create_dir_all(&songs_dir).expect("Failed to create Songs dir");

    for i in 0..5 {
        let beatmap_dir = songs_dir.join(format!("123456 Artist - Song {}", i));
        fs::create_dir_all(&beatmap_dir).expect("Failed to create beatmap dir");
        let osu_file = beatmap_dir.join(format!("song_{}.osu", i));
        let mut file = File::create(&osu_file).expect("Failed to create .osu file");
        writeln!(
            file,
            "osu file format v14\n[General]\nAudioFilename: audio.mp3"
        )
        .unwrap();

        // Create a fake audio file
        let audio_file = beatmap_dir.join("audio.mp3");
        let mut audio = File::create(&audio_file).expect("Failed to create audio");
        writeln!(audio, "fake mp3 data").unwrap();
    }

    // Create some skins
    let skins_dir = stable_path.join("Skins");
    fs::create_dir_all(&skins_dir).expect("Failed to create Skins dir");
    for i in 0..3 {
        let skin_dir = skins_dir.join(format!("CoolSkin{}", i));
        fs::create_dir_all(&skin_dir).expect("Failed to create skin dir");
        let ini_file = skin_dir.join("skin.ini");
        let mut file = File::create(&ini_file).expect("Failed to create skin.ini");
        writeln!(file, "[General]\nName: Cool Skin {}\nAuthor: Test", i).unwrap();
    }

    println!("\nCreated test data:");
    println!("  5 beatmaps in Songs/");
    println!("  3 skins in Skins/");

    // Test 1: StableMaster mode
    println!("\n--- Test 1: StableMaster Mode ---");
    test_stable_master(&stable_path, &lazer_path);

    // Reset lazer directory for next test
    if lazer_path.exists() {
        fs::remove_dir_all(&lazer_path).ok();
    }
    fs::create_dir_all(&lazer_path).expect("Failed to recreate lazer dir");

    // Test 2: LazerMaster mode (create content in lazer first)
    println!("\n--- Test 2: LazerMaster Mode ---");

    // Create beatmaps in lazer
    let lazer_songs = lazer_path.join("Songs");
    fs::create_dir_all(&lazer_songs).expect("Failed to create lazer Songs");
    for i in 0..3 {
        let beatmap_dir = lazer_songs.join(format!("789012 Lazer Song {}", i));
        fs::create_dir_all(&beatmap_dir).expect("Failed to create beatmap dir");
        let osu_file = beatmap_dir.join(format!("lazer_{}.osu", i));
        let mut file = File::create(&osu_file).expect("Failed to create .osu file");
        writeln!(
            file,
            "osu file format v14\n[General]\nAudioFilename: audio.mp3"
        )
        .unwrap();
    }

    // Reset stable songs for lazer master test
    if stable_path.join("Songs").exists() {
        fs::remove_dir_all(stable_path.join("Songs")).ok();
    }
    fs::create_dir_all(stable_path.join("Songs")).expect("Failed to recreate stable Songs");

    test_lazer_master(&stable_path, &lazer_path);

    // Test 3: TrueUnified mode
    println!("\n--- Test 3: TrueUnified Mode ---");

    // Reset both directories
    if lazer_path.exists() {
        fs::remove_dir_all(&lazer_path).ok();
    }
    fs::create_dir_all(&lazer_path).expect("Failed to recreate lazer dir");

    // Recreate stable content
    let songs_dir = stable_path.join("Songs");
    if songs_dir.exists() {
        fs::remove_dir_all(&songs_dir).ok();
    }
    fs::create_dir_all(&songs_dir).expect("Failed to create Songs dir");
    for i in 0..3 {
        let beatmap_dir = songs_dir.join(format!("unified_test_{}", i));
        fs::create_dir_all(&beatmap_dir).expect("Failed to create beatmap dir");
        let osu_file = beatmap_dir.join(format!("map_{}.osu", i));
        let mut file = File::create(&osu_file).expect("Failed to create .osu file");
        writeln!(file, "osu file format v14").unwrap();
    }

    test_true_unified(&stable_path, &lazer_path, &shared_path);

    println!("\n=== All Tests Completed ===");
}

fn test_stable_master(stable_path: &std::path::Path, lazer_path: &std::path::Path) {
    let config = UnifiedStorageConfig::stable_master();

    let mut engine = match UnifiedStorageEngine::new(
        config,
        stable_path.to_path_buf(),
        lazer_path.to_path_buf(),
    ) {
        Ok(e) => e,
        Err(e) => {
            println!("  ERROR: Failed to create engine: {}", e);
            return;
        }
    };

    println!("  Created engine in StableMaster mode");

    // Setup
    match engine.setup() {
        Ok(result) => {
            println!("  Setup complete:");
            println!("    - Links created: {}", result.links_created);
            println!("    - Resources linked: {}", result.resources_linked);
            if !result.warnings.is_empty() {
                println!("    - Warnings: {:?}", result.warnings);
            }
        }
        Err(e) => {
            println!("  ERROR: Setup failed: {}", e);
            return;
        }
    }

    // Verify
    match engine.verify() {
        Ok(result) => {
            println!("  Verification:");
            println!("    - Total links: {}", result.total_links);
            println!("    - Active: {}", result.active);
            println!("    - Broken: {}", result.broken);
            println!("    - Health: {:.1}%", result.health_percentage());
        }
        Err(e) => {
            println!("  ERROR: Verify failed: {}", e);
        }
    }

    // Check if lazer can access stable's content
    let lazer_songs = lazer_path.join("Songs");
    if lazer_songs.exists() {
        let count = fs::read_dir(&lazer_songs).map(|e| e.count()).unwrap_or(0);
        println!("  Lazer Songs accessible: {} beatmaps", count);
    } else {
        println!("  WARNING: Lazer Songs not accessible");
    }

    // Teardown
    match engine.teardown() {
        Ok(()) => println!("  Teardown complete"),
        Err(e) => println!("  ERROR: Teardown failed: {}", e),
    }
}

fn test_lazer_master(stable_path: &std::path::Path, lazer_path: &std::path::Path) {
    let config = UnifiedStorageConfig::lazer_master();

    let mut engine = match UnifiedStorageEngine::new(
        config,
        stable_path.to_path_buf(),
        lazer_path.to_path_buf(),
    ) {
        Ok(e) => e,
        Err(e) => {
            println!("  ERROR: Failed to create engine: {}", e);
            return;
        }
    };

    println!("  Created engine in LazerMaster mode");

    // Setup
    match engine.setup() {
        Ok(result) => {
            println!("  Setup complete:");
            println!("    - Links created: {}", result.links_created);
            println!("    - Resources linked: {}", result.resources_linked);
            if !result.warnings.is_empty() {
                println!("    - Warnings: {:?}", result.warnings);
            }
        }
        Err(e) => {
            println!("  ERROR: Setup failed: {}", e);
            return;
        }
    }

    // Check if stable can access lazer's content
    let stable_songs = stable_path.join("Songs");
    if stable_songs.exists() {
        let count = fs::read_dir(&stable_songs).map(|e| e.count()).unwrap_or(0);
        println!("  Stable Songs accessible: {} beatmaps", count);
    }

    // Teardown
    match engine.teardown() {
        Ok(()) => println!("  Teardown complete"),
        Err(e) => println!("  ERROR: Teardown failed: {}", e),
    }
}

fn test_true_unified(
    stable_path: &std::path::Path,
    lazer_path: &std::path::Path,
    shared_path: &std::path::Path,
) {
    let config = UnifiedStorageConfig::true_unified(shared_path.to_path_buf());

    let mut engine = match UnifiedStorageEngine::new(
        config,
        stable_path.to_path_buf(),
        lazer_path.to_path_buf(),
    ) {
        Ok(e) => e,
        Err(e) => {
            println!("  ERROR: Failed to create engine: {}", e);
            return;
        }
    };

    println!("  Created engine in TrueUnified mode");
    println!("  Shared path: {}", shared_path.display());

    // Setup
    match engine.setup() {
        Ok(result) => {
            println!("  Setup complete:");
            println!("    - Links created: {}", result.links_created);
            println!("    - Resources linked: {}", result.resources_linked);
            if !result.warnings.is_empty() {
                println!("    - Warnings: {:?}", result.warnings);
            }
        }
        Err(e) => {
            println!("  ERROR: Setup failed: {}", e);
            return;
        }
    }

    // Check shared location
    let shared_songs = shared_path.join("Songs");
    if shared_songs.exists() {
        let count = fs::read_dir(&shared_songs).map(|e| e.count()).unwrap_or(0);
        println!("  Shared Songs: {} beatmaps", count);
    }

    // Verify
    match engine.verify() {
        Ok(result) => {
            println!("  Verification: {:.1}% healthy", result.health_percentage());
        }
        Err(e) => {
            println!("  ERROR: Verify failed: {}", e);
        }
    }

    // Teardown
    match engine.teardown() {
        Ok(()) => println!("  Teardown complete"),
        Err(e) => println!("  ERROR: Teardown failed: {}", e),
    }
}
