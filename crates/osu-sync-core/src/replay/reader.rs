//! Stable replay reader for reading scores.db and finding .osr files

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::beatmap::GameMode;
use crate::error::{Error, Result};
use crate::lazer::StableDatabase;

use super::model::{Grade, ReplayInfo};

/// Reader for osu!stable replay data
pub struct StableReplayReader {
    /// Path to osu!stable installation
    osu_path: PathBuf,
    /// Cached beatmap metadata for enrichment
    beatmap_metadata: HashMap<String, (String, String)>, // hash -> (title, artist)
}

impl StableReplayReader {
    /// Create a new stable replay reader
    pub fn new(osu_path: impl AsRef<Path>) -> Self {
        Self {
            osu_path: osu_path.as_ref().to_path_buf(),
            beatmap_metadata: HashMap::new(),
        }
    }

    /// Load beatmap metadata from osu!.db for enrichment
    pub fn load_beatmap_metadata(&mut self) -> Result<()> {
        let db_path = self.osu_path.join("osu!.db");
        if !db_path.exists() {
            return Ok(()); // No metadata available, continue without it
        }

        let db = StableDatabase::open(&db_path)?;
        for set in db.get_all_beatmap_sets()? {
            for beatmap in &set.beatmaps {
                // Use md5_hash for matching with scores.db (which uses MD5)
                if !beatmap.md5_hash.is_empty() {
                    self.beatmap_metadata.insert(
                        beatmap.md5_hash.clone(),
                        (
                            beatmap.metadata.title.clone(),
                            beatmap.metadata.artist.clone(),
                        ),
                    );
                }
            }
        }

        Ok(())
    }

    /// Read all replays from scores.db
    pub fn read_replays(&self) -> Result<Vec<ReplayInfo>> {
        let scores_path = self.osu_path.join("scores.db");
        if !scores_path.exists() {
            return Err(Error::OsuNotFound(scores_path));
        }

        let replays_dir = self.osu_path.join("Data").join("r");
        let alt_replays_dir = self.osu_path.join("Replays");

        // Parse scores.db using osu-db crate
        let scores = osu_db::score::ScoreList::from_file(&scores_path)
            .map_err(|e| Error::Other(format!("Failed to parse scores.db: {}", e)))?;

        let mut replays = Vec::new();

        for beatmap_scores in scores.beatmaps {
            let beatmap_hash = match beatmap_scores.hash {
                Some(ref h) => h.clone(),
                None => continue,
            };

            for score in beatmap_scores.scores {
                // Get beatmap metadata if available
                let (beatmap_title, beatmap_artist) = self
                    .beatmap_metadata
                    .get(&beatmap_hash)
                    .cloned()
                    .unwrap_or_default();

                // Check for replay file
                let replay_hash = score.replay_hash.clone();
                let (has_replay_file, replay_path) = if let Some(ref hash) = replay_hash {
                    let replay_filename = format!("{}.osr", hash);

                    // Check both possible replay locations
                    let path1 = replays_dir.join(&replay_filename);
                    let path2 = alt_replays_dir.join(&replay_filename);

                    if path1.exists() {
                        (true, Some(path1.to_string_lossy().to_string()))
                    } else if path2.exists() {
                        (true, Some(path2.to_string_lossy().to_string()))
                    } else {
                        // Also check for score-based filename pattern
                        let score_pattern = format!(
                            "{} - {} ({}).osr",
                            score.player_name.as_deref().unwrap_or("Unknown"),
                            beatmap_title,
                            score.score
                        );
                        let path3 = alt_replays_dir.join(&score_pattern);
                        if path3.exists() {
                            (true, Some(path3.to_string_lossy().to_string()))
                        } else {
                            (false, None)
                        }
                    }
                } else {
                    (false, None)
                };

                // Convert game mode
                let mode = match score.mode {
                    osu_db::Mode::Standard => GameMode::Osu,
                    osu_db::Mode::Taiko => GameMode::Taiko,
                    osu_db::Mode::CatchTheBeat => GameMode::Catch,
                    osu_db::Mode::Mania => GameMode::Mania,
                };

                // Calculate grade from accuracy (Replay struct doesn't have grade field)
                let total_hits = score.count_300 + score.count_100 + score.count_50 + score.count_miss;
                let accuracy = if total_hits > 0 {
                    (score.count_300 as f32 * 300.0 + score.count_100 as f32 * 100.0 + score.count_50 as f32 * 50.0)
                        / (total_hits as f32 * 300.0) * 100.0
                } else {
                    0.0
                };
                let grade = if score.count_miss == 0 && accuracy >= 100.0 {
                    Grade::SS
                } else if accuracy >= 93.0 {
                    Grade::S
                } else if accuracy >= 80.0 {
                    Grade::A
                } else if accuracy >= 70.0 {
                    Grade::B
                } else if accuracy >= 60.0 {
                    Grade::C
                } else {
                    Grade::D
                };

                // Convert timestamp from DateTime<Utc>
                let timestamp = score.timestamp.timestamp();

                let replay_info = ReplayInfo {
                    beatmap_hash: beatmap_hash.clone(),
                    player_name: score.player_name.unwrap_or_else(|| "Unknown".to_string()),
                    replay_hash,
                    score: score.score as u64,
                    max_combo: score.max_combo as u32,
                    count_300: score.count_300 as u32,
                    count_100: score.count_100 as u32,
                    count_50: score.count_50 as u32,
                    count_miss: score.count_miss as u32,
                    timestamp,
                    mode,
                    grade,
                    has_replay_file,
                    replay_path,
                    beatmap_title: if beatmap_title.is_empty() {
                        None
                    } else {
                        Some(beatmap_title)
                    },
                    beatmap_artist: if beatmap_artist.is_empty() {
                        None
                    } else {
                        Some(beatmap_artist)
                    },
                    beatmap_version: None, // Not available from scores.db
                };

                replays.push(replay_info);
            }
        }

        Ok(replays)
    }

    /// Get replays that have .osr files available
    pub fn read_exportable_replays(&self) -> Result<Vec<ReplayInfo>> {
        let replays = self.read_replays()?;
        Ok(replays.into_iter().filter(|r| r.has_replay_file).collect())
    }

    /// Get replay count statistics
    pub fn get_stats(&self) -> Result<ReplayStats> {
        let replays = self.read_replays()?;
        let total = replays.len();
        let with_files = replays.iter().filter(|r| r.has_replay_file).count();

        Ok(ReplayStats {
            total_scores: total,
            with_replay_files: with_files,
            without_replay_files: total - with_files,
        })
    }
}

/// Statistics about available replays
#[derive(Debug, Clone)]
pub struct ReplayStats {
    /// Total number of scores in scores.db
    pub total_scores: usize,
    /// Number of scores with .osr files
    pub with_replay_files: usize,
    /// Number of scores without .osr files
    pub without_replay_files: usize,
}

