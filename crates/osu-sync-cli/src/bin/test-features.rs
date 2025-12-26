use osu_sync_core::lazer::StableDatabase;
use osu_sync_core::replay::StableReplayReader;
use osu_sync_core::stable::StableScanner;
use osu_sync_core::filter::{FilterCriteria, FilterEngine};
use osu_sync_core::beatmap::GameMode;
use osu_sync_core::media::{MediaExtractor, MediaType, OutputOrganization};
use std::path::PathBuf;

fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║           osu-sync Feature Test Suite                        ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    test_stable_database();
    test_stable_scanner();
    test_replay_reader();
    test_filter_engine();
    test_media_extractor();

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                    All Tests Complete                         ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
}

fn test_stable_database() {
    println!("━━━ TEST 1: Stable Database (osu!.db) ━━━");
    let osu_path = PathBuf::from(r"D:\osu!");
    let db_path = osu_path.join("osu!.db");

    if !db_path.exists() {
        println!("  ○ osu!.db not found at expected path");
        println!();
        return;
    }

    match StableDatabase::open(&osu_path) {
        Ok(db) => {
            println!("  ✓ Opened osu!.db successfully");
            match db.get_all_beatmap_sets() {
                Ok(sets) => {
                    println!("  ✓ Loaded {} beatmap sets", sets.len());

                    // Count by mode
                    let mut osu = 0;
                    let mut taiko = 0;
                    let mut catch = 0;
                    let mut mania = 0;

                    for set in &sets {
                        for bm in &set.beatmaps {
                            match bm.mode {
                                GameMode::Osu => osu += 1,
                                GameMode::Taiko => taiko += 1,
                                GameMode::Catch => catch += 1,
                                GameMode::Mania => mania += 1,
                            }
                        }
                    }
                    println!("    - osu!: {} maps", osu);
                    println!("    - Taiko: {} maps", taiko);
                    println!("    - Catch: {} maps", catch);
                    println!("    - Mania: {} maps", mania);

                    // Sample with star rating
                    if let Some(set) = sets.iter().find(|s| s.beatmaps.iter().any(|b| b.star_rating.is_some())) {
                        if let Some(bm) = set.beatmaps.iter().find(|b| b.star_rating.is_some()) {
                            println!("  ✓ Star rating data available: {:.2}★", bm.star_rating.unwrap());
                        }
                    }
                }
                Err(e) => println!("  ✗ Failed to read: {}", e),
            }
        }
        Err(e) => println!("  ✗ Failed to open: {}", e),
    }
    println!();
}

fn test_stable_scanner() {
    println!("━━━ TEST 2: Stable Scanner (Songs folder) ━━━");
    let songs_path = PathBuf::from(r"D:\osu!\Songs");

    if !songs_path.exists() {
        println!("  ○ Songs folder not found");
        println!();
        return;
    }

    let scanner = StableScanner::new(songs_path);
    match scanner.scan() {
        Ok(sets) => {
            println!("  ✓ Scanned {} beatmap sets from filesystem", sets.len());

            // Show sample
            if let Some(set) = sets.first() {
                if let Some(meta) = set.metadata() {
                    println!("  ✓ Sample: {} - {}", meta.artist, meta.title);
                    println!("    Creator: {}", meta.creator);
                }
                println!("    Difficulties: {}", set.beatmaps.len());
            }

            // Test filter on scanned sets
            let osu_only = FilterCriteria::new().with_mode(GameMode::Osu);
            let filtered = FilterEngine::filter_stable(&sets, &osu_only);
            println!("  ✓ Filter test: {} osu! mode sets", filtered.len());
        }
        Err(e) => println!("  ✗ Scan failed: {}", e),
    }
    println!();
}

fn test_replay_reader() {
    println!("━━━ TEST 3: Replay Reader (scores.db) ━━━");
    let osu_path = PathBuf::from(r"D:\osu!");

    let scores_path = osu_path.join("scores.db");
    if !scores_path.exists() {
        println!("  ○ scores.db not found");
        println!();
        return;
    }

    let mut reader = StableReplayReader::new(&osu_path);

    // Load beatmap metadata for enrichment
    let _ = reader.load_beatmap_metadata();

    match reader.read_replays() {
        Ok(replays) => {
            let with_files = replays.iter().filter(|r| r.has_replay_file).count();
            println!("  ✓ Found {} scores", replays.len());
            println!("  ✓ {} have replay files (.osr)", with_files);

            // Show grade distribution
            let mut grades = std::collections::HashMap::new();
            for r in &replays {
                *grades.entry(r.grade.as_str()).or_insert(0) += 1;
            }
            print!("    Grades: ");
            for (grade, count) in &grades {
                print!("{}={} ", grade, count);
            }
            println!();

            // Show sample replay
            if let Some(r) = replays.iter().find(|r| r.has_replay_file && r.beatmap_title.is_some()) {
                println!("  ✓ Sample: {} - {} ({} pts)",
                    r.player_name,
                    r.beatmap_title.as_ref().unwrap(),
                    r.score
                );
            }
        }
        Err(e) => println!("  ✗ Failed: {}", e),
    }
    println!();
}

fn test_filter_engine() {
    println!("━━━ TEST 4: Filter Engine ━━━");
    let songs_path = PathBuf::from(r"D:\osu!\Songs");

    if !songs_path.exists() {
        println!("  ○ Cannot test without Songs folder");
        println!();
        return;
    }

    let scanner = StableScanner::new(songs_path);
    match scanner.scan() {
        Ok(sets) => {
            let total = sets.len();

            // Test mode filter
            let osu_only = FilterCriteria::new().with_mode(GameMode::Osu);
            let osu_count = FilterEngine::count_stable(&sets, &osu_only);
            println!("  ✓ osu! mode filter: {} of {} sets", osu_count, total);

            // Test mania filter
            let mania = FilterCriteria::new().with_mode(GameMode::Mania);
            let mania_count = FilterEngine::count_stable(&sets, &mania);
            println!("  ✓ Mania mode filter: {} sets", mania_count);

            // Test search
            let search = FilterCriteria::new().with_search("freedom");
            let search_count = FilterEngine::count_stable(&sets, &search);
            println!("  ✓ Search 'freedom': {} sets", search_count);

            // Test artist filter
            let artist = FilterCriteria::new().with_artist("camellia");
            let artist_count = FilterEngine::count_stable(&sets, &artist);
            println!("  ✓ Artist 'camellia': {} sets", artist_count);
        }
        Err(e) => println!("  ✗ Scan failed: {}", e),
    }
    println!();
}

fn test_media_extractor() {
    println!("━━━ TEST 5: Media Extractor ━━━");

    // Just test that it initializes correctly, don't actually extract
    let temp_dir = std::env::temp_dir().join("osu-sync-test");

    let _extractor = MediaExtractor::new(&temp_dir)
        .with_media_type(MediaType::Audio)
        .with_organization(OutputOrganization::ByArtist);

    println!("  ✓ MediaExtractor initialized");
    println!("    Output: {}", temp_dir.display());
    println!("    Type: Audio only");
    println!("    Organization: By Artist");

    // Clean up
    let _ = std::fs::remove_dir_all(&temp_dir);
    println!();
}
