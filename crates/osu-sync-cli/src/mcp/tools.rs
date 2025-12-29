//! MCP tools - Direct access to osu!stable and osu!lazer

use rmcp::{model::ServerInfo, tool, ServerHandler};
use std::collections::HashSet;
use super::types::*;
use osu_sync_core::{Config, StableCollectionReader, StableScanner, LazerDatabase};

#[derive(Clone, Default)]
pub struct OsuSyncTools;

impl OsuSyncTools {
    pub fn new() -> Self { Self }
}

fn err(msg: &str) -> String {
    format!(r#"{{"success":false,"error":"{}"}}"#, msg.replace('"', "'"))
}

fn json<T: serde::Serialize>(v: T) -> String {
    serde_json::to_string(&v).unwrap_or_else(|e| err(&e.to_string()))
}

#[tool(tool_box)]
impl OsuSyncTools {
    #[tool(description = "Get osu-sync config with detected paths")]
    async fn get_config(&self, #[tool(aggr)] _req: GetConfigRequest) -> String {
        let c = Config::load();
        json(GetConfigResponse {
            success: true,
            stable_path: c.stable_path.as_ref().map(|p| p.display().to_string()),
            lazer_path: c.lazer_path.as_ref().map(|p| p.display().to_string()),
            error: None,
        })
    }

    #[tool(description = "Scan osu!stable Songs folder for beatmaps")]
    async fn scan_stable(&self, #[tool(aggr)] req: ScanStableRequest) -> String {
        let c = Config::load();
        let path = match c.stable_songs_path() {
            Some(p) => p,
            None => return err("stable not configured"),
        };
        match StableScanner::new(path).skip_hashing().scan_parallel() {
            Ok(sets) => {
                let total = sets.len();
                let items: Vec<_> = sets.into_iter().skip(req.offset).take(req.limit).map(|s| {
                    let m = s.metadata();
                    BeatmapSetCompact {
                        id: s.id,
                        title: m.map(|x| x.title.clone()).unwrap_or_default(),
                        artist: m.map(|x| x.artist.clone()).unwrap_or_default(),
                        creator: m.map(|x| x.creator.clone()).unwrap_or_default(),
                        difficulty_count: s.beatmaps.len(),
                    }
                }).collect();
                json(ScanResponse { success: true, total_sets: total, returned_sets: items.len(), beatmap_sets: items, error: None })
            }
            Err(e) => err(&e.to_string()),
        }
    }

    #[tool(description = "List collections from osu!stable")]
    async fn list_collections(&self, #[tool(aggr)] _req: ListCollectionsRequest) -> String {
        let c = Config::load();
        let path = match &c.stable_path {
            Some(p) => p.join("collection.db"),
            None => return err("stable not configured"),
        };
        match StableCollectionReader::read(&path) {
            Ok(cols) => {
                let items: Vec<_> = cols.into_iter().map(|c| CollectionInfo {
                    name: c.name,
                    beatmap_count: c.beatmap_hashes.len(),
                }).collect();
                json(ListCollectionsResponse { success: true, collections: items, error: None })
            }
            Err(e) => err(&e.to_string()),
        }
    }

    #[tool(description = "Scan osu!lazer Realm database for beatmaps")]
    async fn scan_lazer(&self, #[tool(aggr)] req: ScanLazerRequest) -> String {
        let c = Config::load();
        let path = match &c.lazer_path {
            Some(p) => p,
            None => return err("lazer not configured"),
        };
        match LazerDatabase::open(path) {
            Ok(db) => match db.get_all_beatmap_sets() {
                Ok(sets) => {
                    let total = sets.len();
                    let items: Vec<_> = sets.into_iter().skip(req.offset).take(req.limit).map(|s| {
                        let meta = s.beatmaps.first().map(|b| &b.metadata);
                        BeatmapSetCompact {
                            id: s.online_id,
                            title: meta.map(|m| m.title.clone()).unwrap_or_default(),
                            artist: meta.map(|m| m.artist.clone()).unwrap_or_default(),
                            creator: meta.map(|m| m.creator.clone()).unwrap_or_default(),
                            difficulty_count: s.beatmaps.len(),
                        }
                    }).collect();
                    json(ScanResponse { success: true, total_sets: total, returned_sets: items.len(), beatmap_sets: items, error: None })
                }
                Err(e) => err(&e.to_string()),
            },
            Err(e) => err(&e.to_string()),
        }
    }

    #[tool(description = "Compare stable vs lazer libraries")]
    async fn compare(&self, #[tool(aggr)] _req: CompareRequest) -> String {
        let c = Config::load();
        let stable_ids: HashSet<i32> = c.stable_songs_path()
            .and_then(|p| StableScanner::new(p).skip_hashing().scan_parallel().ok())
            .map(|s| s.into_iter().filter_map(|x| x.id).collect())
            .unwrap_or_default();
        let lazer_ids: HashSet<i32> = c.lazer_path.as_ref()
            .and_then(|p| LazerDatabase::open(p).ok())
            .and_then(|db| db.get_all_beatmap_sets().ok())
            .map(|s| s.into_iter().filter_map(|x| x.online_id).collect())
            .unwrap_or_default();
        json(CompareResponse {
            success: true,
            stable_sets: stable_ids.len(),
            lazer_sets: lazer_ids.len(),
            common: stable_ids.intersection(&lazer_ids).count(),
            only_stable: stable_ids.difference(&lazer_ids).count(),
            only_lazer: lazer_ids.difference(&stable_ids).count(),
            error: None,
        })
    }

    #[tool(description = "Find beatmaps missing from stable or lazer")]
    async fn find_missing(&self, #[tool(aggr)] req: FindMissingRequest) -> String {
        let c = Config::load();
        let stable = c.stable_songs_path()
            .and_then(|p| StableScanner::new(p).skip_hashing().scan_parallel().ok());
        let lazer = c.lazer_path.as_ref()
            .and_then(|p| LazerDatabase::open(p).ok())
            .and_then(|db| db.get_all_beatmap_sets().ok());
        let (stable, lazer) = match (stable, lazer) {
            (Some(s), Some(l)) => (s, l),
            _ => return err("need both stable and lazer"),
        };
        let stable_ids: HashSet<i32> = stable.iter().filter_map(|x| x.id).collect();
        let lazer_ids: HashSet<i32> = lazer.iter().filter_map(|x| x.online_id).collect();
        
        let (missing, items): (Vec<i32>, Vec<BeatmapSetCompact>) = if req.missing_from == "lazer" {
            let ids: Vec<_> = stable_ids.difference(&lazer_ids).copied().collect();
            let items = stable.into_iter()
                .filter(|s| s.id.map(|i| ids.contains(&i)).unwrap_or(false))
                .take(req.limit)
                .map(|s| {
                    let m = s.metadata();
                    BeatmapSetCompact {
                        id: s.id, title: m.map(|x|x.title.clone()).unwrap_or_default(),
                        artist: m.map(|x|x.artist.clone()).unwrap_or_default(),
                        creator: m.map(|x|x.creator.clone()).unwrap_or_default(),
                        difficulty_count: s.beatmaps.len(),
                    }
                }).collect();
            (ids, items)
        } else {
            let ids: Vec<_> = lazer_ids.difference(&stable_ids).copied().collect();
            let items = lazer.into_iter()
                .filter(|s| s.online_id.map(|i| ids.contains(&i)).unwrap_or(false))
                .take(req.limit)
                .map(|s| {
                    let meta = s.beatmaps.first().map(|b| &b.metadata);
                    BeatmapSetCompact {
                        id: s.online_id,
                        title: meta.map(|m| m.title.clone()).unwrap_or_default(),
                        artist: meta.map(|m| m.artist.clone()).unwrap_or_default(),
                        creator: meta.map(|m| m.creator.clone()).unwrap_or_default(),
                        difficulty_count: s.beatmaps.len(),
                    }
                }).collect();
            (ids, items)
        };
        json(FindMissingResponse {
            success: true, missing_from: req.missing_from, total_missing: missing.len(),
            beatmap_sets: items, error: None,
        })
    }

    #[tool(description = "List replay files from osu!stable")]
    async fn list_replays(&self, #[tool(aggr)] req: ListReplaysRequest) -> String {
        use osu_sync_core::replay::StableReplayReader;
        let c = Config::load();
        let path = match &c.stable_path {
            Some(p) => p,
            None => return err("stable not configured"),
        };
        match StableReplayReader::new(path).read_replays() {
            Ok(replays) => {
                let total = replays.len();
                let exportable = replays.iter().filter(|r| r.has_replay_file).count();
                let items: Vec<_> = replays.into_iter().take(req.limit).map(|r| ReplayCompact {
                    beatmap_hash: r.beatmap_hash,
                    player: r.player_name,
                    score: r.score as u64,
                    grade: r.grade.as_str().to_string(),
                    mode: format!("{:?}", r.mode),
                    has_data: r.has_replay_file,
                }).collect();
                json(ListReplaysResponse { success: true, total, exportable, replays: items, error: None })
            }
            Err(e) => err(&e.to_string()),
        }
    }

    #[tool(description = "Capture screenshot of osu! window (Windows only)")]
    async fn screenshot(&self, #[tool(aggr)] req: ScreenshotRequest) -> String {
        #[cfg(windows)]
        {
            use base64::Engine;
            use osu_sync_core::vision::{capture_game_window, CaptureTarget};
            let target = match req.target.as_str() {
                "stable" => CaptureTarget::Stable,
                "lazer" => CaptureTarget::Lazer,
                _ => CaptureTarget::Any,
            };
            match capture_game_window(target) {
                Ok(f) => json(ScreenshotResponse {
                    success: true,
                    image_base64: Some(base64::engine::general_purpose::STANDARD.encode(&f.png_bytes)),
                    width: Some(f.width), height: Some(f.height), error: None,
                }),
                Err(e) => json(ScreenshotResponse { success: false, image_base64: None, width: None, height: None, error: Some(e.to_string()) }),
            }
        }
        #[cfg(not(windows))]
        err("Windows only")
    }

    #[tool(description = "List osu! windows")]
    async fn list_windows(&self, #[tool(aggr)] _req: ListWindowsRequest) -> String {
        #[cfg(windows)]
        {
            use osu_sync_core::vision::list_osu_windows;
            match list_osu_windows() {
                Ok(w) => json(ListWindowsResponse {
                    success: true,
                    windows: w.into_iter().map(|x| WindowInfo { title: x.title, width: x.width, height: x.height, is_lazer: x.is_lazer }).collect(),
                    error: None,
                }),
                Err(e) => err(&e.to_string()),
            }
        }
        #[cfg(not(windows))]
        err("Windows only")
    }
}

#[tool(tool_box)]
impl ServerHandler for OsuSyncTools {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: rmcp::model::Implementation { name: "osu-sync".into(), version: env!("CARGO_PKG_VERSION").into() },
            ..Default::default()
        }
    }
}
