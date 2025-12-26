use osu_sync_core::stable::StableScanner;
use std::path::PathBuf;

fn main() {
    println!("=== osu-sync Scanner Benchmark ===\n");

    let songs_path = PathBuf::from(r"D:\osu!\Songs");

    if !songs_path.exists() {
        println!("Songs folder not found");
        return;
    }

    // Test 1: Fast mode (skip hashing)
    println!(">>> FAST MODE (skip hashing) <<<\n");
    let scanner = StableScanner::new(songs_path.clone()).skip_hashing();

    match scanner.scan_parallel_timed() {
        Ok((sets, timing)) => {
            let total_beatmaps: usize = sets.iter().map(|s| s.beatmaps.len()).sum();
            print_results(&sets, &timing, total_beatmaps);
        }
        Err(e) => println!("Scan failed: {}", e),
    }

    println!("\n{}\n", "=".repeat(50));

    // Test 2: Full mode (with hashing)
    println!(">>> FULL MODE (with hashing) <<<\n");
    let scanner = StableScanner::new(songs_path);

    match scanner.scan_parallel_timed() {
        Ok((sets, timing)) => {
            let total_beatmaps: usize = sets.iter().map(|s| s.beatmaps.len()).sum();
            print_results(&sets, &timing, total_beatmaps);
        }
        Err(e) => println!("Scan failed: {}", e),
    }
}

fn print_results(sets: &[osu_sync_core::beatmap::BeatmapSet], timing: &osu_sync_core::stable::ScanTiming, total_beatmaps: usize) {
    println!("=== Results ===");
    println!("Beatmap sets: {}", sets.len());
    println!("Total beatmaps: {}", total_beatmaps);
    println!();
    println!("=== Timing ===");
    println!("{}", timing.report());

    let secs = timing.total.as_secs_f64();
    if secs > 0.0 {
        println!("\n=== Speed ===");
        println!("Sets/sec: {:.1}", sets.len() as f64 / secs);
        println!("Beatmaps/sec: {:.1}", total_beatmaps as f64 / secs);
    }
}
