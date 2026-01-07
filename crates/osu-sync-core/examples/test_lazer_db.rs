//! Test reading osu!lazer Realm database

use osu_sync_core::lazer::LazerDatabase;
use std::path::Path;

fn main() {
    // Initialize tracing for debug output
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let lazer_path = Path::new("D:\\osu!lazer");

    println!("Testing Realm database reading from: {:?}", lazer_path);
    println!("---");

    match LazerDatabase::open(lazer_path) {
        Ok(db) => {
            println!("Successfully opened LazerDatabase");
            println!("Realm available: {}", db.is_realm_available());
            println!();

            match db.get_all_beatmap_sets() {
                Ok(sets) => {
                    println!("Found {} beatmap sets!", sets.len());
                    println!();

                    // Show first 10 beatmap sets
                    for (i, set) in sets.iter().take(10).enumerate() {
                        let first_beatmap = set.beatmaps.first();
                        let title = first_beatmap
                            .map(|b| b.metadata.title.as_str())
                            .unwrap_or("Unknown");
                        let artist = first_beatmap
                            .map(|b| b.metadata.artist.as_str())
                            .unwrap_or("Unknown");

                        println!(
                            "{}. {} - {} (ID: {:?}, {} difficulties, {} files)",
                            i + 1,
                            artist,
                            title,
                            set.online_id,
                            set.beatmaps.len(),
                            set.files.len()
                        );
                    }

                    if sets.len() > 10 {
                        println!("... and {} more", sets.len() - 10);
                    }

                    // Count total beatmaps
                    let total_beatmaps: usize = sets.iter().map(|s| s.beatmaps.len()).sum();
                    println!();
                    println!(
                        "Total: {} beatmap sets, {} individual beatmaps",
                        sets.len(),
                        total_beatmaps
                    );
                }
                Err(e) => {
                    println!("Error getting beatmap sets: {}", e);
                }
            }
        }
        Err(e) => {
            println!("Error opening LazerDatabase: {}", e);
        }
    }
}
