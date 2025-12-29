//! MCP types for osu-sync - Direct access to osu! data

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ============================================================================
// Config
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
pub struct GetConfigRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetConfigResponse {
    pub success: bool,
    pub stable_path: Option<String>,
    pub lazer_path: Option<String>,
    pub error: Option<String>,
}

// ============================================================================
// Stable Access
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScanStableRequest {
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}
fn default_limit() -> usize { 100 }
impl Default for ScanStableRequest {
    fn default() -> Self { Self { limit: 100, offset: 0 } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BeatmapSetCompact {
    pub id: Option<i32>,
    pub title: String,
    pub artist: String,
    pub creator: String,
    pub difficulty_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResponse {
    pub success: bool,
    pub total_sets: usize,
    pub returned_sets: usize,
    pub beatmap_sets: Vec<BeatmapSetCompact>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
pub struct ListCollectionsRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionInfo {
    pub name: String,
    pub beatmap_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListCollectionsResponse {
    pub success: bool,
    pub collections: Vec<CollectionInfo>,
    pub error: Option<String>,
}

// ============================================================================
// Lazer Access
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScanLazerRequest {
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
}
impl Default for ScanLazerRequest {
    fn default() -> Self { Self { limit: 100, offset: 0 } }
}

// ============================================================================
// Comparison
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
pub struct CompareRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompareResponse {
    pub success: bool,
    pub stable_sets: usize,
    pub lazer_sets: usize,
    pub common: usize,
    pub only_stable: usize,
    pub only_lazer: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FindMissingRequest {
    pub missing_from: String, // "stable" or "lazer"
    #[serde(default = "default_limit")]
    pub limit: usize,
}
impl Default for FindMissingRequest {
    fn default() -> Self { Self { missing_from: "lazer".into(), limit: 100 } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindMissingResponse {
    pub success: bool,
    pub missing_from: String,
    pub total_missing: usize,
    pub beatmap_sets: Vec<BeatmapSetCompact>,
    pub error: Option<String>,
}

// ============================================================================
// Replays
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ListReplaysRequest {
    #[serde(default = "default_limit")]
    pub limit: usize,
}
impl Default for ListReplaysRequest {
    fn default() -> Self { Self { limit: 100 } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayCompact {
    pub beatmap_hash: String,
    pub player: String,
    pub score: u64,
    pub grade: String,
    pub mode: String,
    pub has_data: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListReplaysResponse {
    pub success: bool,
    pub total: usize,
    pub exportable: usize,
    pub replays: Vec<ReplayCompact>,
    pub error: Option<String>,
}

// ============================================================================
// Vision (Windows only)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScreenshotRequest {
    #[serde(default)]
    pub target: String, // "stable", "lazer", or "any"
}
impl Default for ScreenshotRequest {
    fn default() -> Self { Self { target: "any".into() } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScreenshotResponse {
    pub success: bool,
    pub image_base64: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
pub struct ListWindowsRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowInfo {
    pub title: String,
    pub width: u32,
    pub height: u32,
    pub is_lazer: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListWindowsResponse {
    pub success: bool,
    pub windows: Vec<WindowInfo>,
    pub error: Option<String>,
}
