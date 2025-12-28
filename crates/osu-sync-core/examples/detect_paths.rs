//! Test path detection

use osu_sync_core::config::{detect_lazer_path, detect_stable_path};

fn main() {
    println!("=== osu! Path Detection Test ===\n");

    println!("Detecting osu!stable...");
    match detect_stable_path() {
        Some(path) => println!("  FOUND: {}", path.display()),
        None => println!("  NOT FOUND"),
    }

    println!("\nDetecting osu!lazer...");
    match detect_lazer_path() {
        Some(path) => println!("  FOUND: {}", path.display()),
        None => println!("  NOT FOUND"),
    }

    println!("\n=== Done ===");
}
