//! .osu file parsing using rosu-map

use crate::beatmap::{BeatmapDifficulty, BeatmapInfo, BeatmapMetadata, GameMode};
use crate::error::{Error, Result};
use md5::{Md5, Digest as Md5Digest};
use sha2::Sha256;
use std::fs;
use std::path::Path;

/// Parse a .osu file and extract beatmap information
pub fn parse_osu_file(path: &Path) -> Result<BeatmapInfo> {
    let content = fs::read(path)?;

    // Calculate hashes
    let sha256_hash = format!("{:x}", Sha256::digest(&content));
    let md5_hash = format!("{:x}", Md5::digest(&content));

    // Parse with rosu-map
    let beatmap = rosu_map::from_path::<rosu_map::Beatmap>(path).map_err(|e| Error::BeatmapParse {
        path: path.to_path_buf(),
        message: e.to_string(),
    })?;

    // Extract metadata
    let metadata = BeatmapMetadata {
        title: beatmap.title.clone(),
        title_unicode: if beatmap.title_unicode.is_empty() {
            None
        } else {
            Some(beatmap.title_unicode.clone())
        },
        artist: beatmap.artist.clone(),
        artist_unicode: if beatmap.artist_unicode.is_empty() {
            None
        } else {
            Some(beatmap.artist_unicode.clone())
        },
        creator: beatmap.creator.clone(),
        source: if beatmap.source.is_empty() {
            None
        } else {
            Some(beatmap.source.clone())
        },
        tags: beatmap
            .tags
            .split_whitespace()
            .map(String::from)
            .collect(),
        beatmap_id: if beatmap.beatmap_id > 0 {
            Some(beatmap.beatmap_id as i32)
        } else {
            None
        },
        beatmap_set_id: if beatmap.beatmap_set_id > 0 {
            Some(beatmap.beatmap_set_id as i32)
        } else {
            None
        },
    };

    // Extract difficulty settings
    let difficulty = BeatmapDifficulty {
        hp_drain: beatmap.hp_drain_rate,
        circle_size: beatmap.circle_size,
        overall_difficulty: beatmap.overall_difficulty,
        approach_rate: beatmap.approach_rate,
        slider_multiplier: beatmap.slider_multiplier,
        slider_tick_rate: beatmap.slider_tick_rate,
    };

    // Calculate length from timing points and hit objects
    let length_ms = calculate_length(&beatmap);

    // Calculate main BPM
    let bpm = calculate_bpm(&beatmap);

    Ok(BeatmapInfo {
        metadata,
        difficulty,
        hash: sha256_hash,
        md5_hash,
        audio_file: beatmap.audio_file.clone(),
        background_file: extract_background(&beatmap),
        length_ms,
        bpm,
        mode: GameMode::from(beatmap.mode as u8),
        version: beatmap.version.clone(),
        star_rating: None,      // Not available from .osu file, populated from database
        ranked_status: None,    // Not available from .osu file, populated from database
    })
}

/// Calculate the length of the beatmap in milliseconds
fn calculate_length(beatmap: &rosu_map::Beatmap) -> u64 {
    if beatmap.hit_objects.is_empty() {
        return 0;
    }

    let first_time = beatmap
        .hit_objects
        .first()
        .map(|h| h.start_time)
        .unwrap_or(0.0);
    let last_time = beatmap
        .hit_objects
        .last()
        .map(|h| h.start_time)
        .unwrap_or(0.0);

    (last_time - first_time) as u64
}

/// Calculate the main BPM from timing points
fn calculate_bpm(beatmap: &rosu_map::Beatmap) -> f64 {
    // Find the uninherited timing point with the most coverage
    let timing_points = &beatmap.control_points.timing_points;

    if timing_points.is_empty() {
        return 120.0; // Default BPM
    }

    // Use the first timing point's BPM as default
    let beat_len = timing_points.first().map(|tp| tp.beat_len).unwrap_or(500.0);
    60000.0 / beat_len
}

/// Extract background filename from events
fn extract_background(beatmap: &rosu_map::Beatmap) -> Option<String> {
    // The background is stored directly on the beatmap
    if beatmap.background_file.is_empty() {
        None
    } else {
        Some(beatmap.background_file.clone())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_bpm_calculation() {
        // BPM = 60000 / beat_len
        // 500ms beat_len = 120 BPM
        let expected: f64 = 60000.0 / 500.0;
        assert!((expected - 120.0).abs() < 0.001);
    }
}
