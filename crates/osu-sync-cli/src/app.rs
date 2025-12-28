//! Application state and logic

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

use crossterm::event::{KeyCode, KeyEvent};
use std::path::PathBuf;

use osu_sync_core::backup::{
    BackupInfo, BackupMode, BackupProgress, BackupTarget, CompressionLevel,
};
use osu_sync_core::beatmap::GameMode;
use osu_sync_core::collection::{Collection, CollectionSyncResult, CollectionSyncStrategy};
use osu_sync_core::dedup::DuplicateInfo;
use osu_sync_core::filter::FilterCriteria;
use osu_sync_core::media::{ExtractionProgress, ExtractionResult, MediaType, OutputOrganization};
use osu_sync_core::replay::{
    ExportOrganization, Grade, ReplayExportResult, ReplayExportStats, ReplayFilter, ReplayInfo,
    ReplayProgress,
};
use osu_sync_core::stats::ComparisonStats;
use osu_sync_core::sync::{DryRunResult, SyncDirection, SyncProgress, SyncResult};
use ratatui::prelude::*;

use crate::event;
use crate::screens;
use crate::theme;

/// osu! pink color
pub const PINK: Color = Color::Rgb(255, 102, 170);
/// Dark background
#[allow(dead_code)]
pub const DARK_BG: Color = Color::Rgb(30, 30, 46);
/// Light text
pub const TEXT: Color = Color::Rgb(205, 214, 244);
/// Subtle/dimmed text
pub const SUBTLE: Color = Color::Rgb(147, 153, 178);
/// Success color
pub const SUCCESS: Color = Color::Green;
/// Warning color
pub const WARNING: Color = Color::Yellow;
/// Error color
pub const ERROR: Color = Color::Red;
/// Selection background color
pub const SELECTION_BG: Color = Color::Rgb(69, 71, 90);

// Helper functions for color access

/// Get the pink accent color
pub fn pink() -> Color {
    PINK
}
/// Get the text color
pub fn text_color() -> Color {
    TEXT
}
/// Get the subtle/dimmed text color
pub fn subtle_color() -> Color {
    SUBTLE
}
/// Get the success color
pub fn success_color() -> Color {
    SUCCESS
}
/// Get the warning color
#[allow(dead_code)]
pub fn warning_color() -> Color {
    WARNING
}
/// Get the error color
#[allow(dead_code)]
pub fn error_color() -> Color {
    ERROR
}
/// Get the selection background color
pub fn selection_bg() -> Color {
    SELECTION_BG
}

/// Log entry for sync operations
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub message: String,
    pub level: LogLevel,
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum LogLevel {
    Info,
    Success,
    Warning,
    Error,
}

/// Tab selection for statistics screen
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StatisticsTab {
    #[default]
    Overview,
    Stable,
    Lazer,
    Duplicates,
    Recommendations,
}

/// State for export dialog
#[derive(Debug, Clone, Default)]
pub struct ExportState {
    /// Whether the export dialog is open
    pub dialog_open: bool,
    /// Currently selected format index (0 = JSON, 1 = CSV)
    pub selected_format: usize,
    /// Result message after export attempt
    pub result_message: Option<String>,
    /// Whether the last export was successful
    pub export_success: bool,
}

/// Field being edited in filter panel
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FilterField {
    #[default]
    ModeOsu,
    ModeTaiko,
    ModeCatch,
    ModeMania,
    StarMin,
    StarMax,
    StatusRanked,
    StatusApproved,
    StatusQualified,
    StatusLoved,
    StatusPending,
    Artist,
    Mapper,
    Search,
}

/// Scan results for an installation
#[derive(Debug, Clone, Default)]
pub struct ScanResult {
    pub path: Option<String>,
    pub detected: bool,
    pub beatmap_sets: usize,
    pub total_beatmaps: usize,
    /// Timing report (if available)
    pub timing_report: Option<String>,
}

/// Messages from the background worker to the UI
#[derive(Debug)]
#[allow(dead_code)]
pub enum AppMessage {
    ScanProgress {
        stable: bool,
        message: String,
    },
    ScanComplete {
        stable: Option<ScanResult>,
        lazer: Option<ScanResult>,
    },
    SyncProgress(SyncProgress),
    DuplicateFound(DuplicateInfo),
    SyncComplete(SyncResult),
    SyncCancelled,
    StatsProgress(String),
    StatsComplete(ComparisonStats),
    CollectionsLoaded(Vec<Collection>),
    CollectionSyncProgress {
        collection: String,
        progress: f32,
    },
    CollectionSyncComplete(CollectionSyncResult),
    DryRunComplete {
        result: DryRunResult,
        direction: SyncDirection,
    },
    BackupProgress(BackupProgress),
    BackupComplete {
        path: PathBuf,
        size_bytes: u64,
        is_incremental: bool,
    },
    BackupsLoaded(Vec<BackupInfo>),
    RestoreProgress(BackupProgress),
    RestoreComplete {
        dest_path: PathBuf,
        files_restored: usize,
    },
    // Media extraction
    MediaProgress(ExtractionProgress),
    MediaComplete(ExtractionResult),
    // Replay export
    ReplaysLoaded {
        replays: Vec<ReplayInfo>,
        exportable_count: usize,
    },
    ReplayProgress(ReplayProgress),
    ReplayComplete(ReplayExportResult),
    // Unified storage
    UnifiedStorageProgress {
        phase: String,
        current: usize,
        total: usize,
        message: String,
    },
    UnifiedStorageComplete {
        success: bool,
        message: String,
        links_created: usize,
        space_saved: u64,
    },
    UnifiedStorageStatus {
        mode: String,
        active_links: usize,
        broken_links: usize,
        space_saved: u64,
    },
    UnifiedStorageVerifyComplete {
        healthy: usize,
        broken: usize,
        repaired: usize,
    },
    Error(String),
}

/// Messages from the UI to the background worker
#[derive(Debug)]
pub enum WorkerMessage {
    StartScan {
        stable: bool,
        lazer: bool,
    },
    StartSync {
        direction: SyncDirection,
        selected_set_ids: Option<HashSet<i32>>,
    },
    StartDryRun {
        direction: SyncDirection,
    },
    CalculateStats,
    ResolveDuplicate(osu_sync_core::dedup::DuplicateResolution),
    LoadCollections,
    SyncCollections {
        strategy: CollectionSyncStrategy,
    },
    CreateBackup {
        target: BackupTarget,
        compression: CompressionLevel,
        mode: BackupMode,
    },
    LoadBackups,
    RestoreBackup {
        backup_path: PathBuf,
    },
    // Media extraction
    StartMediaExtraction {
        media_type: MediaType,
        organization: OutputOrganization,
        output_path: PathBuf,
        skip_duplicates: bool,
        include_metadata: bool,
    },
    // Replay export
    LoadReplays,
    StartReplayExport {
        organization: ExportOrganization,
        output_path: PathBuf,
        filter: ReplayFilter,
        rename_pattern: Option<String>,
    },
    // Unified storage
    StartUnifiedSetup {
        mode: UnifiedStorageMode,
        shared_path: Option<PathBuf>,
        resources: Vec<SharedResourceType>,
    },
    GetUnifiedStatus,
    VerifyUnifiedLinks,
    RepairUnifiedLinks,
    DisableUnifiedStorage,
    Cancel,
    Shutdown,
}

/// Re-export unified storage types for worker messages
pub use osu_sync_core::unified::{SharedResourceType, UnifiedStorageMode};

/// Application state enum
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum AppState {
    MainMenu {
        selected: usize,
    },
    Scanning {
        in_progress: bool,
        stable_result: Option<ScanResult>,
        lazer_result: Option<ScanResult>,
        status_message: String,
    },
    SyncConfig {
        selected: usize,
        stable_count: usize,
        lazer_count: usize,
        filter: FilterCriteria,
        filter_panel_open: bool,
        filter_field: FilterField,
    },
    Syncing {
        progress: Option<SyncProgress>,
        logs: Vec<LogEntry>,
        stats: SyncStats,
        is_paused: bool,
    },
    DuplicateDialog {
        info: DuplicateInfo,
        selected: usize,
        apply_to_all: bool,
    },
    SyncComplete {
        result: SyncResult,
    },
    Config {
        selected: usize,
        stable_path: Option<String>,
        lazer_path: Option<String>,
        status_message: String,
        /// Whether we're editing a path (and the current input buffer)
        editing: Option<String>,
    },
    Statistics {
        stats: Option<ComparisonStats>,
        loading: bool,
        tab: StatisticsTab,
        status_message: String,
        export_state: ExportState,
    },
    CollectionConfig {
        collections: Vec<Collection>,
        selected: usize,
        strategy: CollectionSyncStrategy,
        loading: bool,
        status_message: String,
    },
    CollectionSync {
        progress: f32,
        current_collection: String,
        logs: Vec<LogEntry>,
    },
    CollectionSummary {
        result: CollectionSyncResult,
    },
    DryRunPreview {
        result: DryRunResult,
        direction: SyncDirection,
        selected_item: usize,
        scroll_offset: usize,
        checked_items: HashSet<usize>,
        filter_text: String,
        filter_mode: bool,
    },
    BackupConfig {
        selected: usize,
        status_message: String,
    },
    BackupProgress {
        target: BackupTarget,
        progress: BackupProgress,
    },
    BackupComplete {
        backup_path: PathBuf,
        size_bytes: u64,
        is_incremental: bool,
    },
    RestoreConfig {
        backups: Vec<BackupInfo>,
        selected: usize,
        loading: bool,
        status_message: String,
    },
    RestoreConfirm {
        backup: BackupInfo,
        dest_path: PathBuf,
        selected: usize,
    },
    RestoreProgress {
        backup_name: String,
        progress: BackupProgress,
    },
    RestoreComplete {
        dest_path: PathBuf,
        files_restored: usize,
    },
    // Media extraction states
    MediaConfig {
        selected: usize,
        media_type: MediaType,
        organization: OutputOrganization,
        output_path: String,
        skip_duplicates: bool,
        include_metadata: bool,
        status_message: Option<String>,
    },
    MediaProgress {
        progress: Option<ExtractionProgress>,
        current_set: String,
    },
    MediaComplete {
        result: ExtractionResult,
    },
    // Replay export states
    ReplayConfig {
        selected: usize,
        organization: ExportOrganization,
        output_path: String,
        replays: Vec<ReplayInfo>,
        loading: bool,
        status_message: Option<String>,
        /// Filter settings
        filter: ReplayFilter,
        /// Custom rename pattern (empty = default)
        rename_pattern: String,
        /// Whether filter panel is open
        filter_panel_open: bool,
        /// Which filter field is selected (0-4: grade, osu, taiko, catch, mania)
        filter_field: usize,
    },
    ReplayProgress {
        progress: Option<ReplayProgress>,
        current_replay: String,
    },
    ReplayComplete {
        result: ReplayExportResult,
        stats: Option<ReplayExportStats>,
    },
    Help {
        /// The state to return to when help is closed
        previous_state: Box<AppState>,
    },
    // Unified storage states
    UnifiedConfig {
        screen: crate::screens::unified_config::UnifiedConfigScreen,
    },
    UnifiedSetup {
        screen: crate::screens::unified_setup::UnifiedSetupScreen,
    },
    UnifiedStatus {
        screen: crate::screens::unified_status::UnifiedStatusScreen,
    },
    Exiting,
}

/// Statistics during sync
#[derive(Debug, Clone, Default)]
pub struct SyncStats {
    pub imported: usize,
    pub skipped: usize,
    pub failed: usize,
}

impl Default for AppState {
    fn default() -> Self {
        Self::MainMenu { selected: 0 }
    }
}

/// Main application struct
pub struct App {
    pub state: AppState,
    pub should_quit: bool,
    pub last_error: Option<String>,

    // Cached scan results
    pub cached_stable_scan: Option<ScanResult>,
    pub cached_lazer_scan: Option<ScanResult>,

    // Cached statistics
    pub cached_stats: Option<ComparisonStats>,

    // Worker communication
    pub worker_tx: Sender<WorkerMessage>,
    pub worker_rx: Receiver<AppMessage>,

    // Cancellation flag shared with worker
    pub cancellation_flag: Arc<AtomicBool>,
}

impl App {
    /// Create a new application instance
    pub fn new() -> Self {
        let (worker_tx, _worker_rx) = mpsc::channel::<WorkerMessage>();
        let (_app_tx, worker_rx) = mpsc::channel::<AppMessage>();

        Self {
            state: AppState::default(),
            should_quit: false,
            last_error: None,
            cached_stable_scan: None,
            cached_lazer_scan: None,
            cached_stats: None,
            worker_tx,
            worker_rx,
            cancellation_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Set up worker communication channels
    pub fn with_channels(
        mut self,
        worker_tx: Sender<WorkerMessage>,
        worker_rx: Receiver<AppMessage>,
        cancellation_flag: Arc<AtomicBool>,
    ) -> Self {
        self.worker_tx = worker_tx;
        self.worker_rx = worker_rx;
        self.cancellation_flag = cancellation_flag;
        self
    }

    /// Request cancellation of current operation
    fn request_cancel(&self) {
        self.cancellation_flag.store(true, Ordering::SeqCst);
    }

    /// Reset cancellation flag (called before starting new operations)
    fn reset_cancel(&self) {
        self.cancellation_flag.store(false, Ordering::SeqCst);
    }

    /// Handle a keyboard event
    pub fn handle_key(&mut self, key: KeyEvent) {
        // Global quit handling
        if event::is_quit(&key) && matches!(self.state, AppState::MainMenu { .. }) {
            self.should_quit = true;
            return;
        }

        // Handle help screen - any key closes it
        if let AppState::Help { previous_state } = &self.state {
            self.state = *previous_state.clone();
            return;
        }

        // Global help key handling (? or h) - available from most screens
        if event::is_help(&key) && self.can_show_help() {
            self.show_help();
            return;
        }

        // Copy values needed from state first to avoid borrow conflicts
        let state_info = match &self.state {
            AppState::MainMenu { selected } => Some(("menu", *selected, false)),
            AppState::Scanning { .. } => Some(("scanning", 0, false)),
            AppState::SyncConfig { selected, .. } => Some(("sync_config", *selected, false)),
            AppState::Syncing { .. } => Some(("syncing", 0, false)),
            AppState::DuplicateDialog {
                selected,
                apply_to_all,
                ..
            } => Some(("duplicate", *selected, *apply_to_all)),
            AppState::SyncComplete { .. } => Some(("complete", 0, false)),
            AppState::Config { selected, .. } => Some(("config", *selected, false)),
            AppState::Statistics { tab, .. } => Some(("statistics", *tab as usize, false)),
            AppState::CollectionConfig { selected, .. } => {
                Some(("collection_config", *selected, false))
            }
            AppState::CollectionSync { .. } => Some(("collection_sync", 0, false)),
            AppState::CollectionSummary { .. } => Some(("collection_summary", 0, false)),
            AppState::DryRunPreview { selected_item, .. } => {
                Some(("dry_run_preview", *selected_item, false))
            }
            AppState::BackupConfig { selected, .. } => Some(("backup_config", *selected, false)),
            AppState::BackupProgress { .. } => Some(("backup_progress", 0, false)),
            AppState::BackupComplete { .. } => Some(("backup_complete", 0, false)),
            AppState::RestoreConfig { selected, .. } => Some(("restore_config", *selected, false)),
            AppState::RestoreConfirm { selected, .. } => {
                Some(("restore_confirm", *selected, false))
            }
            AppState::RestoreProgress { .. } => Some(("restore_progress", 0, false)),
            AppState::RestoreComplete { .. } => Some(("restore_complete", 0, false)),
            AppState::MediaConfig { selected, .. } => Some(("media_config", *selected, false)),
            AppState::MediaProgress { .. } => Some(("media_progress", 0, false)),
            AppState::MediaComplete { .. } => Some(("media_complete", 0, false)),
            AppState::ReplayConfig { selected, .. } => Some(("replay_config", *selected, false)),
            AppState::ReplayProgress { .. } => Some(("replay_progress", 0, false)),
            AppState::ReplayComplete { .. } => Some(("replay_complete", 0, false)),
            AppState::UnifiedConfig { .. } => Some(("unified_config", 0, false)),
            AppState::UnifiedSetup { .. } => Some(("unified_setup", 0, false)),
            AppState::UnifiedStatus { .. } => Some(("unified_status", 0, false)),
            AppState::Help { .. } => None, // Already handled above
            AppState::Exiting => None,
        };

        // Delegate to current screen handler
        if let Some((screen, selected, apply_to_all)) = state_info {
            match screen {
                "menu" => self.handle_main_menu_key(key, selected),
                "scanning" => self.handle_scanning_key(key),
                "sync_config" => self.handle_sync_config_key(key, selected),
                "syncing" => self.handle_syncing_key(key),
                "duplicate" => self.handle_duplicate_dialog_key(key, selected, apply_to_all),
                "complete" => self.handle_sync_complete_key(key),
                "config" => self.handle_config_key(key, selected),
                "statistics" => self.handle_statistics_key(key),
                "collection_config" => self.handle_collection_config_key(key, selected),
                "collection_sync" => self.handle_collection_sync_key(key),
                "collection_summary" => self.handle_collection_summary_key(key),
                "dry_run_preview" => self.handle_dry_run_preview_key(key),
                "backup_config" => self.handle_backup_config_key(key, selected),
                "backup_progress" => self.handle_backup_progress_key(key),
                "backup_complete" => self.handle_backup_complete_key(key),
                "restore_config" => self.handle_restore_config_key(key, selected),
                "restore_confirm" => self.handle_restore_confirm_key(key, selected),
                "restore_progress" => self.handle_restore_progress_key(key),
                "restore_complete" => self.handle_restore_complete_key(key),
                "media_config" => self.handle_media_config_key(key, selected),
                "media_progress" => self.handle_media_progress_key(key),
                "media_complete" => self.handle_media_complete_key(key),
                "replay_config" => self.handle_replay_config_key(key, selected),
                "replay_progress" => self.handle_replay_progress_key(key),
                "replay_complete" => self.handle_replay_complete_key(key),
                "unified_config" => self.handle_unified_config_key(key),
                "unified_setup" => self.handle_unified_setup_key(key),
                "unified_status" => self.handle_unified_status_key(key),
                _ => {}
            }
        }
    }

    fn handle_main_menu_key(&mut self, key: KeyEvent, selected: usize) {
        const MENU_ITEMS: usize = 10;

        if event::is_down(&key) {
            self.state = AppState::MainMenu {
                selected: (selected + 1) % MENU_ITEMS,
            };
        } else if event::is_up(&key) {
            self.state = AppState::MainMenu {
                selected: selected.checked_sub(1).unwrap_or(MENU_ITEMS - 1),
            };
        } else if event::is_enter(&key) {
            match selected {
                0 => self.go_to_sync_config(),
                1 => self.go_to_collection_config(),
                2 => self.go_to_statistics(),
                3 => self.go_to_media_config(),
                4 => self.go_to_replay_config(),
                5 => self.go_to_backup_config(),
                6 => self.go_to_restore_config(),
                7 => self.go_to_config(),
                8 => self.go_to_unified_config(),
                9 => self.should_quit = true,
                _ => {}
            }
        }
    }

    fn handle_scanning_key(&mut self, key: KeyEvent) {
        if event::is_escape(&key) {
            // Cancel and return to menu
            self.request_cancel();
            self.state = AppState::MainMenu { selected: 0 };
        }
    }

    fn handle_sync_config_key(&mut self, key: KeyEvent, selected: usize) {
        const OPTIONS: usize = 3;

        // Extract filter state to avoid borrow issues
        let (filter_panel_open, filter, filter_field, stable_count, lazer_count) =
            if let AppState::SyncConfig {
                filter_panel_open,
                filter,
                filter_field,
                stable_count,
                lazer_count,
                ..
            } = &self.state
            {
                (
                    *filter_panel_open,
                    filter.clone(),
                    *filter_field,
                    *stable_count,
                    *lazer_count,
                )
            } else {
                return;
            };

        if event::is_escape(&key) {
            if filter_panel_open {
                // Close filter panel
                self.state = AppState::SyncConfig {
                    selected,
                    stable_count,
                    lazer_count,
                    filter,
                    filter_panel_open: false,
                    filter_field,
                };
            } else {
                self.state = AppState::MainMenu { selected: 0 };
            }
        } else if event::is_key(&key, 'f') && !filter_panel_open {
            // Toggle filter panel open
            self.state = AppState::SyncConfig {
                selected,
                stable_count,
                lazer_count,
                filter,
                filter_panel_open: true,
                filter_field: FilterField::ModeOsu,
            };
        } else if filter_panel_open {
            // Handle filter panel navigation
            self.handle_filter_panel_key(
                key,
                selected,
                stable_count,
                lazer_count,
                filter,
                filter_field,
            );
        } else if event::is_down(&key) {
            self.state = AppState::SyncConfig {
                selected: (selected + 1) % OPTIONS,
                stable_count,
                lazer_count,
                filter,
                filter_panel_open: false,
                filter_field,
            };
        } else if event::is_up(&key) {
            self.state = AppState::SyncConfig {
                selected: selected.checked_sub(1).unwrap_or(OPTIONS - 1),
                stable_count,
                lazer_count,
                filter,
                filter_panel_open: false,
                filter_field,
            };
        } else if event::is_enter(&key) {
            let direction = match selected {
                0 => SyncDirection::StableToLazer,
                1 => SyncDirection::LazerToStable,
                2 => SyncDirection::Bidirectional,
                _ => return,
            };
            self.start_sync(direction, None); // Sync all (no selection)
        } else if event::is_key(&key, 'd') {
            // Start dry run
            let direction = match selected {
                0 => SyncDirection::StableToLazer,
                1 => SyncDirection::LazerToStable,
                2 => SyncDirection::Bidirectional,
                _ => return,
            };
            self.start_dry_run(direction);
        }
    }

    fn handle_filter_panel_key(
        &mut self,
        key: KeyEvent,
        selected: usize,
        stable_count: usize,
        lazer_count: usize,
        mut filter: FilterCriteria,
        filter_field: FilterField,
    ) {
        use osu_sync_core::stats::RankedStatus;

        // All fields in order for navigation
        const ALL_FIELDS: [FilterField; 14] = [
            FilterField::ModeOsu,
            FilterField::ModeTaiko,
            FilterField::ModeCatch,
            FilterField::ModeMania,
            FilterField::StarMin,
            FilterField::StarMax,
            FilterField::StatusRanked,
            FilterField::StatusApproved,
            FilterField::StatusQualified,
            FilterField::StatusLoved,
            FilterField::StatusPending,
            FilterField::Artist,
            FilterField::Mapper,
            FilterField::Search,
        ];

        let current_idx = ALL_FIELDS
            .iter()
            .position(|&f| f == filter_field)
            .unwrap_or(0);

        if event::is_down(&key) || event::is_right(&key) {
            // Navigate to next filter field
            let next_idx = (current_idx + 1) % ALL_FIELDS.len();
            self.state = AppState::SyncConfig {
                selected,
                stable_count,
                lazer_count,
                filter,
                filter_panel_open: true,
                filter_field: ALL_FIELDS[next_idx],
            };
        } else if event::is_up(&key) || event::is_left(&key) {
            // Navigate to previous filter field
            let prev_idx = if current_idx == 0 {
                ALL_FIELDS.len() - 1
            } else {
                current_idx - 1
            };
            self.state = AppState::SyncConfig {
                selected,
                stable_count,
                lazer_count,
                filter,
                filter_panel_open: true,
                filter_field: ALL_FIELDS[prev_idx],
            };
        } else if event::is_space(&key) || event::is_enter(&key) {
            // Toggle/action the current filter
            match filter_field {
                FilterField::ModeOsu => filter.toggle_mode(GameMode::Osu),
                FilterField::ModeTaiko => filter.toggle_mode(GameMode::Taiko),
                FilterField::ModeCatch => filter.toggle_mode(GameMode::Catch),
                FilterField::ModeMania => filter.toggle_mode(GameMode::Mania),
                FilterField::StatusRanked => filter.toggle_status(RankedStatus::Ranked),
                FilterField::StatusApproved => filter.toggle_status(RankedStatus::Approved),
                FilterField::StatusQualified => filter.toggle_status(RankedStatus::Qualified),
                FilterField::StatusLoved => filter.toggle_status(RankedStatus::Loved),
                FilterField::StatusPending => filter.toggle_status(RankedStatus::Pending),
                // Text fields and star ratings are handled differently
                _ => {}
            }
            self.state = AppState::SyncConfig {
                selected,
                stable_count,
                lazer_count,
                filter,
                filter_panel_open: true,
                filter_field,
            };
        } else {
            // Handle star rating adjustments
            match (filter_field, key.code) {
                (FilterField::StarMin, KeyCode::Char('+'))
                | (FilterField::StarMin, KeyCode::Char('=')) => {
                    let current = filter.star_rating_min.unwrap_or(0.0);
                    filter.star_rating_min = Some((current + 0.5).min(10.0));
                }
                (FilterField::StarMin, KeyCode::Char('-')) => {
                    if let Some(current) = filter.star_rating_min {
                        if current <= 0.5 {
                            filter.star_rating_min = None;
                        } else {
                            filter.star_rating_min = Some(current - 0.5);
                        }
                    }
                }
                (FilterField::StarMax, KeyCode::Char('+'))
                | (FilterField::StarMax, KeyCode::Char('=')) => {
                    let current = filter.star_rating_max.unwrap_or(0.0);
                    filter.star_rating_max = Some((current + 0.5).min(15.0));
                }
                (FilterField::StarMax, KeyCode::Char('-')) => {
                    if let Some(current) = filter.star_rating_max {
                        if current <= 0.5 {
                            filter.star_rating_max = None;
                        } else {
                            filter.star_rating_max = Some(current - 0.5);
                        }
                    }
                }
                _ => return, // No state change needed
            }
            self.state = AppState::SyncConfig {
                selected,
                stable_count,
                lazer_count,
                filter,
                filter_panel_open: true,
                filter_field,
            };
        }
    }

    fn handle_syncing_key(&mut self, key: KeyEvent) {
        if event::is_escape(&key) {
            self.request_cancel();
            // Will wait for SyncComplete message
        } else if event::is_space(&key) {
            // Toggle pause state
            if let AppState::Syncing {
                progress,
                logs,
                stats,
                is_paused,
            } = &self.state
            {
                self.state = AppState::Syncing {
                    progress: progress.clone(),
                    logs: logs.clone(),
                    stats: stats.clone(),
                    is_paused: !is_paused,
                };
                // Note: The worker would need to check this pause state
                // For now this just updates the UI state
            }
        }
    }

    fn handle_duplicate_dialog_key(&mut self, key: KeyEvent, selected: usize, apply_to_all: bool) {
        const OPTIONS: usize = 3;

        if event::is_down(&key) {
            if let AppState::DuplicateDialog {
                info, apply_to_all, ..
            } = &self.state
            {
                self.state = AppState::DuplicateDialog {
                    info: info.clone(),
                    selected: (selected + 1) % OPTIONS,
                    apply_to_all: *apply_to_all,
                };
            }
        } else if event::is_up(&key) {
            if let AppState::DuplicateDialog {
                info, apply_to_all, ..
            } = &self.state
            {
                self.state = AppState::DuplicateDialog {
                    info: info.clone(),
                    selected: selected.checked_sub(1).unwrap_or(OPTIONS - 1),
                    apply_to_all: *apply_to_all,
                };
            }
        } else if event::is_space(&key) {
            if let AppState::DuplicateDialog { info, selected, .. } = &self.state {
                self.state = AppState::DuplicateDialog {
                    info: info.clone(),
                    selected: *selected,
                    apply_to_all: !apply_to_all,
                };
            }
        } else if event::is_enter(&key) {
            self.resolve_duplicate(selected, apply_to_all);
        }
    }

    fn handle_sync_complete_key(&mut self, key: KeyEvent) {
        if event::is_enter(&key) || event::is_escape(&key) {
            self.state = AppState::MainMenu { selected: 0 };
        }
    }

    fn handle_config_key(&mut self, key: KeyEvent, selected: usize) {
        use crossterm::event::KeyCode;
        const OPTIONS: usize = 4; // stable path, lazer path, theme, rescan

        // Extract current state
        let (stable_path, lazer_path, status_message, editing) = if let AppState::Config {
            stable_path,
            lazer_path,
            status_message,
            editing,
            ..
        } = &self.state
        {
            (
                stable_path.clone(),
                lazer_path.clone(),
                status_message.clone(),
                editing.clone(),
            )
        } else {
            return;
        };

        // If we're in editing mode, handle text input
        if let Some(mut buffer) = editing {
            match key.code {
                KeyCode::Enter => {
                    // Save the edited path
                    let new_path = if buffer.is_empty() {
                        None
                    } else {
                        Some(buffer.clone())
                    };

                    let (new_stable, new_lazer) = if selected == 0 {
                        (new_path.clone(), lazer_path)
                    } else {
                        (stable_path, new_path.clone())
                    };

                    // Update cached scans
                    if selected == 0 {
                        self.cached_stable_scan = new_path.as_ref().map(|p| ScanResult {
                            path: Some(p.clone()),
                            detected: true,
                            beatmap_sets: 0,
                            total_beatmaps: 0,
                            timing_report: None,
                        });
                    } else {
                        self.cached_lazer_scan = new_path.as_ref().map(|p| ScanResult {
                            path: Some(p.clone()),
                            detected: true,
                            beatmap_sets: 0,
                            total_beatmaps: 0,
                            timing_report: None,
                        });
                    }

                    // Save config to disk
                    let config = osu_sync_core::config::Config {
                        stable_path: new_stable.clone().map(std::path::PathBuf::from),
                        lazer_path: new_lazer.clone().map(std::path::PathBuf::from),
                        duplicate_strategy: osu_sync_core::config::DuplicateStrategy::Ask,
                        theme: theme::current_theme_name(),
                        unified_storage: None,
                    };
                    let save_result = config.save();

                    self.state = AppState::Config {
                        selected,
                        stable_path: new_stable,
                        lazer_path: new_lazer,
                        status_message: if save_result.is_ok() {
                            "Path saved!".to_string()
                        } else {
                            "Path updated (failed to save)".to_string()
                        },
                        editing: None,
                    };
                }
                KeyCode::Esc => {
                    // Cancel editing
                    self.state = AppState::Config {
                        selected,
                        stable_path,
                        lazer_path,
                        status_message: "Edit cancelled".to_string(),
                        editing: None,
                    };
                }
                KeyCode::Backspace => {
                    buffer.pop();
                    self.state = AppState::Config {
                        selected,
                        stable_path,
                        lazer_path,
                        status_message,
                        editing: Some(buffer),
                    };
                }
                KeyCode::Char(c) => {
                    buffer.push(c);
                    self.state = AppState::Config {
                        selected,
                        stable_path,
                        lazer_path,
                        status_message,
                        editing: Some(buffer),
                    };
                }
                _ => {}
            }
            return;
        }

        // Normal mode (not editing)
        if event::is_escape(&key) {
            self.state = AppState::MainMenu { selected: 7 }; // Config is at index 7
        } else if event::is_down(&key) {
            self.state = AppState::Config {
                selected: (selected + 1) % OPTIONS,
                stable_path,
                lazer_path,
                status_message,
                editing: None,
            };
        } else if event::is_up(&key) {
            self.state = AppState::Config {
                selected: selected.checked_sub(1).unwrap_or(OPTIONS - 1),
                stable_path,
                lazer_path,
                status_message,
                editing: None,
            };
        } else if event::is_key(&key, 'd') {
            // Auto-detect paths (shortcut)
            self.start_scan();
        } else if event::is_enter(&key) || event::is_left(&key) || event::is_right(&key) {
            if selected == 2 {
                // Theme selection - cycle theme
                self.cycle_theme();
            } else if selected == 3 && event::is_enter(&key) {
                // Rescan installations
                self.start_scan();
            } else if event::is_enter(&key) && selected < 2 {
                // Start editing the selected path (only for path options)
                let current_value = if selected == 0 {
                    stable_path.clone().unwrap_or_default()
                } else {
                    lazer_path.clone().unwrap_or_default()
                };
                self.state = AppState::Config {
                    selected,
                    stable_path,
                    lazer_path,
                    status_message: "Type path, Enter to save, Esc to cancel".to_string(),
                    editing: Some(current_value),
                };
            }
        }
    }

    /// Cycle to the next theme and save the preference
    fn cycle_theme(&mut self) {
        let current = theme::current_theme_name();
        let new_theme = current.next();

        // Apply the new theme immediately
        theme::set_theme(new_theme);

        // Save to config
        let mut config = osu_sync_core::config::Config::load();
        config.theme = new_theme;
        let save_result = config.save();

        // Update state with success message
        if let AppState::Config {
            selected,
            stable_path,
            lazer_path,
            ..
        } = &self.state
        {
            self.state = AppState::Config {
                selected: *selected,
                stable_path: stable_path.clone(),
                lazer_path: lazer_path.clone(),
                status_message: if save_result.is_ok() {
                    format!("Theme '{}' applied and saved!", new_theme.display_name())
                } else {
                    format!("Theme '{}' applied (save failed)", new_theme.display_name())
                },
                editing: None,
            };
        }
    }

    /// Auto-detect osu! installation paths
    fn auto_detect_paths(&mut self) {
        use osu_sync_core::config::{detect_lazer_path, detect_stable_path};

        let stable_path = detect_stable_path().map(|p| p.to_string_lossy().to_string());
        let lazer_path = detect_lazer_path().map(|p| p.to_string_lossy().to_string());

        // Build status message
        let status = match (&stable_path, &lazer_path) {
            (Some(_), Some(_)) => "Both installations detected!".to_string(),
            (Some(_), None) => "osu!stable detected, osu!lazer not found".to_string(),
            (None, Some(_)) => "osu!lazer detected, osu!stable not found".to_string(),
            (None, None) => "No installations detected".to_string(),
        };

        // Update cached scans if paths were found
        if stable_path.is_some() {
            self.cached_stable_scan = Some(ScanResult {
                path: stable_path.clone(),
                detected: true,
                beatmap_sets: 0,
                total_beatmaps: 0,
                timing_report: None,
            });
        }
        if lazer_path.is_some() {
            self.cached_lazer_scan = Some(ScanResult {
                path: lazer_path.clone(),
                detected: true,
                beatmap_sets: 0,
                total_beatmaps: 0,
                timing_report: None,
            });
        }

        if let AppState::Config { selected, .. } = &self.state {
            self.state = AppState::Config {
                selected: *selected,
                stable_path,
                lazer_path,
                status_message: status,
                editing: None,
            };
        }
    }

    fn handle_statistics_key(&mut self, key: KeyEvent) {
        // Extract current state
        let (stats, loading, tab, status_message, export_state) = if let AppState::Statistics {
            stats,
            loading,
            tab,
            status_message,
            export_state,
        } = &self.state
        {
            (
                stats.clone(),
                *loading,
                *tab,
                status_message.clone(),
                export_state.clone(),
            )
        } else {
            return;
        };

        // Handle export dialog if open
        if export_state.dialog_open {
            self.handle_export_dialog_key(key, stats, loading, tab, status_message, export_state);
            return;
        }

        if event::is_escape(&key) {
            self.state = AppState::MainMenu { selected: 2 }; // Statistics is at index 2
        } else if event::is_key(&key, 'e') && stats.is_some() && !loading {
            // Open export dialog
            self.state = AppState::Statistics {
                stats,
                loading,
                tab,
                status_message,
                export_state: ExportState {
                    dialog_open: true,
                    selected_format: 0,
                    result_message: None,
                    export_success: false,
                },
            };
        } else if event::is_tab(&key) || event::is_right(&key) {
            // Cycle through tabs
            let next_tab = match tab {
                StatisticsTab::Overview => StatisticsTab::Stable,
                StatisticsTab::Stable => StatisticsTab::Lazer,
                StatisticsTab::Lazer => StatisticsTab::Duplicates,
                StatisticsTab::Duplicates => StatisticsTab::Recommendations,
                StatisticsTab::Recommendations => StatisticsTab::Overview,
            };
            self.state = AppState::Statistics {
                stats,
                loading,
                tab: next_tab,
                status_message,
                export_state,
            };
        } else if event::is_left(&key) {
            // Cycle backwards
            let prev_tab = match tab {
                StatisticsTab::Overview => StatisticsTab::Recommendations,
                StatisticsTab::Stable => StatisticsTab::Overview,
                StatisticsTab::Lazer => StatisticsTab::Stable,
                StatisticsTab::Duplicates => StatisticsTab::Lazer,
                StatisticsTab::Recommendations => StatisticsTab::Duplicates,
            };
            self.state = AppState::Statistics {
                stats,
                loading,
                tab: prev_tab,
                status_message,
                export_state,
            };
        }
    }

    fn handle_export_dialog_key(
        &mut self,
        key: KeyEvent,
        stats: Option<ComparisonStats>,
        loading: bool,
        tab: StatisticsTab,
        status_message: String,
        mut export_state: ExportState,
    ) {
        use osu_sync_core::ExportFormat;

        if event::is_escape(&key) {
            // Close export dialog
            self.state = AppState::Statistics {
                stats,
                loading,
                tab,
                status_message,
                export_state: ExportState::default(),
            };
        } else if event::is_up(&key) {
            // Navigate up in format selection
            export_state.selected_format = export_state.selected_format.saturating_sub(1);
            export_state.result_message = None;
            self.state = AppState::Statistics {
                stats,
                loading,
                tab,
                status_message,
                export_state,
            };
        } else if event::is_down(&key) {
            // Navigate down in format selection (now supports 3 formats: JSON, CSV, HTML)
            export_state.selected_format = (export_state.selected_format + 1).min(2);
            export_state.result_message = None;
            self.state = AppState::Statistics {
                stats,
                loading,
                tab,
                status_message,
                export_state,
            };
        } else if event::is_enter(&key) {
            // Perform export
            if let Some(ref comparison_stats) = stats {
                let format = match export_state.selected_format {
                    0 => ExportFormat::Json,
                    1 => ExportFormat::Csv,
                    _ => ExportFormat::Html,
                };

                // Determine export path (use current directory or documents folder)
                let filename = format!("osu-sync-stats.{}", format.extension());
                let export_path = std::env::current_dir()
                    .map(|p| p.join(&filename))
                    .unwrap_or_else(|_| std::path::PathBuf::from(&filename));

                match format.export(comparison_stats, &export_path) {
                    Ok(_) => {
                        export_state.result_message =
                            Some(format!("Exported to {}", export_path.display()));
                        export_state.export_success = true;
                    }
                    Err(e) => {
                        export_state.result_message = Some(format!("Export failed: {}", e));
                        export_state.export_success = false;
                    }
                }

                self.state = AppState::Statistics {
                    stats,
                    loading,
                    tab,
                    status_message,
                    export_state,
                };
            }
        }
    }

    fn handle_collection_config_key(&mut self, key: KeyEvent, selected: usize) {
        if event::is_escape(&key) {
            self.state = AppState::MainMenu { selected: 1 }; // Collection Sync is at index 1
        } else if let AppState::CollectionConfig {
            collections,
            strategy,
            loading,
            status_message,
            ..
        } = &self.state
        {
            let collections = collections.clone();
            let strategy = *strategy;
            let loading = *loading;
            let status_message = status_message.clone();
            let num_collections = collections.len().max(1); // At least 1 for strategy option

            if event::is_down(&key) {
                // Navigate down (collections + 1 for strategy)
                self.state = AppState::CollectionConfig {
                    collections,
                    selected: (selected + 1) % (num_collections + 1),
                    strategy,
                    loading,
                    status_message,
                };
            } else if event::is_up(&key) {
                // Navigate up
                self.state = AppState::CollectionConfig {
                    collections,
                    selected: selected.checked_sub(1).unwrap_or(num_collections),
                    strategy,
                    loading,
                    status_message,
                };
            } else if event::is_enter(&key) {
                // If on strategy line, toggle strategy, else start sync
                if selected == num_collections {
                    // Toggle strategy
                    let new_strategy = match strategy {
                        CollectionSyncStrategy::Merge => CollectionSyncStrategy::Replace,
                        CollectionSyncStrategy::Replace => CollectionSyncStrategy::Merge,
                    };
                    self.state = AppState::CollectionConfig {
                        collections,
                        selected,
                        strategy: new_strategy,
                        loading,
                        status_message,
                    };
                } else if !loading && !collections.is_empty() {
                    self.start_collection_sync(strategy);
                }
            } else if event::is_space(&key) && selected == num_collections {
                // Toggle strategy with space
                let new_strategy = match strategy {
                    CollectionSyncStrategy::Merge => CollectionSyncStrategy::Replace,
                    CollectionSyncStrategy::Replace => CollectionSyncStrategy::Merge,
                };
                self.state = AppState::CollectionConfig {
                    collections,
                    selected,
                    strategy: new_strategy,
                    loading,
                    status_message,
                };
            }
        }
    }

    fn handle_collection_sync_key(&mut self, key: KeyEvent) {
        if event::is_escape(&key) {
            self.request_cancel();
            self.state = AppState::MainMenu { selected: 1 };
        }
    }

    fn handle_collection_summary_key(&mut self, key: KeyEvent) {
        if event::is_enter(&key) || event::is_escape(&key) {
            self.state = AppState::MainMenu { selected: 1 };
        }
    }

    fn handle_dry_run_preview_key(&mut self, key: KeyEvent) {
        if let AppState::DryRunPreview {
            result,
            direction,
            selected_item,
            scroll_offset,
            checked_items,
            filter_text,
            filter_mode,
        } = &self.state
        {
            let result = result.clone();
            let direction = *direction;
            let selected_item = *selected_item;
            let scroll_offset = *scroll_offset;
            let mut checked_items = checked_items.clone();
            let mut filter_text = filter_text.clone();
            let mut filter_mode = *filter_mode;
            let total_items = result.items.len();

            if filter_mode {
                // Handle filter mode input
                match key.code {
                    KeyCode::Esc => {
                        filter_mode = false;
                        filter_text.clear();
                    }
                    KeyCode::Enter => {
                        filter_mode = false;
                    }
                    KeyCode::Backspace => {
                        filter_text.pop();
                    }
                    KeyCode::Char(c) => {
                        filter_text.push(c);
                    }
                    _ => {}
                }
                self.state = AppState::DryRunPreview {
                    result,
                    direction,
                    selected_item,
                    scroll_offset,
                    checked_items,
                    filter_text,
                    filter_mode,
                };
            } else if event::is_escape(&key) {
                // Return to sync config
                self.go_to_sync_config();
            } else if event::is_enter(&key) {
                use osu_sync_core::sync::DryRunAction;
                // Start sync with checked items, or sync just the current item if nothing checked
                if !checked_items.is_empty() {
                    // Extract set IDs from checked items
                    let selected_set_ids: HashSet<i32> = checked_items
                        .iter()
                        .filter_map(|&idx| result.items.get(idx).and_then(|item| item.set_id))
                        .collect();
                    let selected = if selected_set_ids.is_empty() {
                        None
                    } else {
                        Some(selected_set_ids)
                    };
                    self.start_sync(direction, selected);
                } else {
                    // Get filtered indices to map display index to actual index
                    let visible_indices =
                        screens::dry_run_preview::filter_items(&result.items, &filter_text);
                    if let Some(&actual_idx) = visible_indices.get(selected_item) {
                        if let Some(item) = result.items.get(actual_idx) {
                            // If current item is importable, sync just this one
                            if matches!(item.action, DryRunAction::Import) {
                                // Get the set ID for this single item
                                let selected_set_ids = item.set_id.map(|id| {
                                    let mut set = HashSet::new();
                                    set.insert(id);
                                    set
                                });
                                // Add just this item to checked and sync
                                checked_items.insert(actual_idx);
                                self.state = AppState::DryRunPreview {
                                    result,
                                    direction,
                                    selected_item,
                                    scroll_offset,
                                    checked_items,
                                    filter_text,
                                    filter_mode,
                                };
                                self.start_sync(direction, selected_set_ids);
                                return;
                            } else {
                                // Item not importable, go back
                                self.go_to_sync_config();
                            }
                        } else {
                            // Item not found, go back
                            self.go_to_sync_config();
                        }
                    } else {
                        // Nothing selected, go back
                        self.go_to_sync_config();
                    }
                }
            } else if key.code == KeyCode::Char(' ') {
                // Toggle selection on current item
                use osu_sync_core::sync::DryRunAction;
                // Get filtered indices to map display index to actual index
                let visible_indices =
                    screens::dry_run_preview::filter_items(&result.items, &filter_text);
                if let Some(&actual_idx) = visible_indices.get(selected_item) {
                    if let Some(item) = result.items.get(actual_idx) {
                        if matches!(item.action, DryRunAction::Import) {
                            if checked_items.contains(&actual_idx) {
                                checked_items.remove(&actual_idx);
                            } else {
                                checked_items.insert(actual_idx);
                            }
                        }
                    }
                }
                self.state = AppState::DryRunPreview {
                    result,
                    direction,
                    selected_item,
                    scroll_offset,
                    checked_items,
                    filter_text,
                    filter_mode,
                };
            } else if event::is_ctrl_a(&key) {
                // Select all Import items
                use osu_sync_core::sync::DryRunAction;
                checked_items = result
                    .items
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| matches!(item.action, DryRunAction::Import))
                    .map(|(idx, _)| idx)
                    .collect();
                self.state = AppState::DryRunPreview {
                    result,
                    direction,
                    selected_item,
                    scroll_offset,
                    checked_items,
                    filter_text,
                    filter_mode,
                };
            } else if event::is_ctrl_d(&key) {
                // Deselect all
                checked_items.clear();
                self.state = AppState::DryRunPreview {
                    result,
                    direction,
                    selected_item,
                    scroll_offset,
                    checked_items,
                    filter_text,
                    filter_mode,
                };
            } else if event::is_ctrl_i(&key) {
                // Invert selection
                use osu_sync_core::sync::DryRunAction;
                let all_import_indices: HashSet<usize> = result
                    .items
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| matches!(item.action, DryRunAction::Import))
                    .map(|(idx, _)| idx)
                    .collect();
                checked_items = all_import_indices
                    .symmetric_difference(&checked_items)
                    .copied()
                    .collect();
                self.state = AppState::DryRunPreview {
                    result,
                    direction,
                    selected_item,
                    scroll_offset,
                    checked_items,
                    filter_text,
                    filter_mode,
                };
            } else if key.code == KeyCode::Char('/') {
                // Enter filter mode
                filter_mode = true;
                self.state = AppState::DryRunPreview {
                    result,
                    direction,
                    selected_item,
                    scroll_offset,
                    checked_items,
                    filter_text,
                    filter_mode,
                };
            } else if event::is_down(&key) {
                // Move selection down
                let new_selected = if selected_item + 1 < total_items {
                    selected_item + 1
                } else {
                    selected_item
                };
                // Adjust scroll if needed (assume ~20 visible lines)
                let visible_lines = 20usize;
                let new_scroll = if new_selected >= scroll_offset + visible_lines {
                    new_selected - visible_lines + 1
                } else {
                    scroll_offset
                };
                self.state = AppState::DryRunPreview {
                    result,
                    direction,
                    selected_item: new_selected,
                    scroll_offset: new_scroll,
                    checked_items,
                    filter_text,
                    filter_mode,
                };
            } else if event::is_up(&key) {
                // Move selection up
                let new_selected = selected_item.saturating_sub(1);
                let new_scroll = if new_selected < scroll_offset {
                    new_selected
                } else {
                    scroll_offset
                };
                self.state = AppState::DryRunPreview {
                    result,
                    direction,
                    selected_item: new_selected,
                    scroll_offset: new_scroll,
                    checked_items,
                    filter_text,
                    filter_mode,
                };
            } else if event::is_page_down(&key) {
                // Page down
                let page_size = 20usize;
                let new_selected = (selected_item + page_size).min(total_items.saturating_sub(1));
                let new_scroll = new_selected.saturating_sub(page_size / 2);
                self.state = AppState::DryRunPreview {
                    result,
                    direction,
                    selected_item: new_selected,
                    scroll_offset: new_scroll.min(total_items.saturating_sub(page_size)),
                    checked_items,
                    filter_text,
                    filter_mode,
                };
            } else if event::is_page_up(&key) {
                // Page up
                let page_size = 20usize;
                let new_selected = selected_item.saturating_sub(page_size);
                let new_scroll = new_selected.saturating_sub(page_size / 2).max(0);
                self.state = AppState::DryRunPreview {
                    result,
                    direction,
                    selected_item: new_selected,
                    scroll_offset: new_scroll,
                    checked_items,
                    filter_text,
                    filter_mode,
                };
            }
        }
    }

    /// Start scanning installations
    pub fn start_scan(&mut self) {
        self.state = AppState::Scanning {
            in_progress: true,
            stable_result: None,
            lazer_result: None,
            status_message: "Starting scan...".to_string(),
        };
        let _ = self.worker_tx.send(WorkerMessage::StartScan {
            stable: true,
            lazer: true,
        });
    }

    /// Go to sync configuration screen
    fn go_to_sync_config(&mut self) {
        let stable_count = self
            .cached_stable_scan
            .as_ref()
            .map(|s| s.beatmap_sets)
            .unwrap_or(0);
        let lazer_count = self
            .cached_lazer_scan
            .as_ref()
            .map(|s| s.beatmap_sets)
            .unwrap_or(0);

        self.state = AppState::SyncConfig {
            selected: 0,
            stable_count,
            lazer_count,
            filter: FilterCriteria::default(),
            filter_panel_open: false,
            filter_field: FilterField::default(),
        };
    }

    /// Go to configuration screen
    fn go_to_config(&mut self) {
        // Load saved config first, fall back to cached scans
        let saved_config = osu_sync_core::config::Config::load();

        let stable_path = saved_config
            .stable_path
            .map(|p| p.to_string_lossy().to_string())
            .or_else(|| {
                self.cached_stable_scan
                    .as_ref()
                    .and_then(|s| s.path.clone())
            });

        let lazer_path = saved_config
            .lazer_path
            .map(|p| p.to_string_lossy().to_string())
            .or_else(|| self.cached_lazer_scan.as_ref().and_then(|s| s.path.clone()));

        self.state = AppState::Config {
            selected: 0,
            stable_path,
            lazer_path,
            status_message: "Enter to edit, 'd' to auto-detect".to_string(),
            editing: None,
        };
    }

    /// Go to statistics screen
    fn go_to_statistics(&mut self) {
        // Use cached stats if available, otherwise calculate
        if let Some(stats) = &self.cached_stats {
            self.state = AppState::Statistics {
                stats: Some(stats.clone()),
                loading: false,
                tab: StatisticsTab::default(),
                status_message: "Statistics loaded".to_string(),
                export_state: ExportState::default(),
            };
        } else {
            self.state = AppState::Statistics {
                stats: None,
                loading: true,
                tab: StatisticsTab::default(),
                status_message: "Calculating statistics...".to_string(),
                export_state: ExportState::default(),
            };
            let _ = self.worker_tx.send(WorkerMessage::CalculateStats);
        }
    }

    /// Go to collection configuration screen
    fn go_to_collection_config(&mut self) {
        self.state = AppState::CollectionConfig {
            collections: Vec::new(),
            selected: 0,
            strategy: CollectionSyncStrategy::default(),
            loading: true,
            status_message: "Loading collections...".to_string(),
        };
        let _ = self.worker_tx.send(WorkerMessage::LoadCollections);
    }

    /// Go to backup configuration screen
    fn go_to_backup_config(&mut self) {
        self.state = AppState::BackupConfig {
            selected: 0,
            status_message: "Select what to backup".to_string(),
        };
    }

    /// Go to restore configuration screen
    fn go_to_restore_config(&mut self) {
        self.state = AppState::RestoreConfig {
            backups: Vec::new(),
            selected: 0,
            loading: true,
            status_message: "Loading backups...".to_string(),
        };
        let _ = self.worker_tx.send(WorkerMessage::LoadBackups);
    }

    /// Go to media extraction configuration screen
    fn go_to_media_config(&mut self) {
        self.state = AppState::MediaConfig {
            selected: 0,
            media_type: MediaType::Both,
            organization: OutputOrganization::Flat,
            output_path: "extracted_media".to_string(),
            skip_duplicates: true,
            include_metadata: false,
            status_message: None,
        };
    }

    /// Go to replay export configuration screen
    fn go_to_replay_config(&mut self) {
        self.state = AppState::ReplayConfig {
            selected: 0,
            organization: ExportOrganization::Flat,
            output_path: "exported_replays".to_string(),
            replays: Vec::new(),
            loading: true,
            status_message: Some("Loading replays...".to_string()),
            filter: ReplayFilter::new(),
            rename_pattern: String::new(),
            filter_panel_open: false,
            filter_field: 0,
        };
        let _ = self.worker_tx.send(WorkerMessage::LoadReplays);
    }

    /// Go to unified storage configuration screen
    fn go_to_unified_config(&mut self) {
        use crate::screens::unified_config::UnifiedConfigScreen;
        self.state = AppState::UnifiedConfig {
            screen: UnifiedConfigScreen::new(),
        };
    }

    /// Handle key events for unified config screen
    fn handle_unified_config_key(&mut self, key: KeyEvent) {
        use crate::screens::unified_config::{ConfigAction, StorageMode};

        if let AppState::UnifiedConfig { screen } = &mut self.state {
            if let Some(action) = screen.handle_key(key.code) {
                match action {
                    ConfigAction::Apply => {
                        // Convert CLI StorageMode to core UnifiedStorageMode
                        let mode = match screen.mode {
                            StorageMode::Disabled => UnifiedStorageMode::Disabled,
                            StorageMode::StableMaster => UnifiedStorageMode::StableMaster,
                            StorageMode::LazerMaster => UnifiedStorageMode::LazerMaster,
                            StorageMode::TrueUnified => UnifiedStorageMode::TrueUnified,
                        };

                        // Get shared path for TrueUnified mode
                        let shared_path = if mode == UnifiedStorageMode::TrueUnified {
                            if screen.shared_path.is_empty() {
                                screen.status_message =
                                    Some("Please enter a shared path for True Unified mode".into());
                                return;
                            }
                            Some(std::path::PathBuf::from(&screen.shared_path))
                        } else {
                            None
                        };

                        // Convert resources
                        let resources: Vec<SharedResourceType> = screen
                            .shared_resources
                            .iter()
                            .map(|r| match r {
                                crate::screens::unified_config::ResourceType::Beatmaps => {
                                    SharedResourceType::Beatmaps
                                }
                                crate::screens::unified_config::ResourceType::Skins => {
                                    SharedResourceType::Skins
                                }
                                crate::screens::unified_config::ResourceType::Replays => {
                                    SharedResourceType::Replays
                                }
                                crate::screens::unified_config::ResourceType::Screenshots => {
                                    SharedResourceType::Screenshots
                                }
                                crate::screens::unified_config::ResourceType::Exports => {
                                    SharedResourceType::Exports
                                }
                                crate::screens::unified_config::ResourceType::Backgrounds => {
                                    SharedResourceType::Backgrounds
                                }
                            })
                            .collect();

                        // Send message to worker
                        let _ = self.worker_tx.send(WorkerMessage::StartUnifiedSetup {
                            mode,
                            shared_path,
                            resources,
                        });

                        // Transition to setup screen
                        self.state = AppState::UnifiedSetup {
                            screen: crate::screens::unified_setup::UnifiedSetupScreen::new(),
                        };
                    }
                    ConfigAction::Cancel => {
                        self.state = AppState::MainMenu { selected: 8 };
                    }
                }
            }
        }
    }

    /// Handle key events for unified setup screen
    fn handle_unified_setup_key(&mut self, key: KeyEvent) {
        if event::is_escape(&key) {
            // Cancel setup and return to config
            self.go_to_unified_config();
        }
    }

    /// Handle key events for unified status screen
    fn handle_unified_status_key(&mut self, key: KeyEvent) {
        use crate::screens::unified_status::StatusAction;

        if let AppState::UnifiedStatus { screen } = &mut self.state {
            if let Some(action) = screen.handle_key(key.code) {
                match action {
                    StatusAction::Verify => {
                        screen.loading = true;
                        let _ = self.worker_tx.send(WorkerMessage::VerifyUnifiedLinks);
                    }
                    StatusAction::Repair => {
                        screen.loading = true;
                        let _ = self.worker_tx.send(WorkerMessage::RepairUnifiedLinks);
                    }
                    StatusAction::SyncNow => {
                        // Re-run unified setup with current config
                        let _ = self.worker_tx.send(WorkerMessage::GetUnifiedStatus);
                    }
                    StatusAction::Configure => {
                        self.go_to_unified_config();
                    }
                    StatusAction::Back => {
                        self.state = AppState::MainMenu { selected: 8 };
                    }
                }
            }
        }
    }

    /// Start a backup operation
    fn start_backup(&mut self, selected: usize) {
        let target = match selected {
            0 => BackupTarget::StableSongs,
            1 => BackupTarget::StableCollections,
            2 => BackupTarget::StableScores,
            3 => BackupTarget::LazerData,
            4 => BackupTarget::All,
            _ => return,
        };

        self.state = AppState::BackupProgress {
            target,
            progress: osu_sync_core::backup::BackupProgress {
                phase: osu_sync_core::backup::BackupPhase::Scanning,
                files_processed: 0,
                total_files: None,
                bytes_written: 0,
                current_file: None,
            },
        };
        let _ = self.worker_tx.send(WorkerMessage::CreateBackup {
            target,
            compression: CompressionLevel::default(),
            mode: BackupMode::Full,
        });
    }

    /// Start a restore operation
    fn start_restore(&mut self, backup_path: &PathBuf) {
        let backup_name = backup_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("backup")
            .to_string();

        self.state = AppState::RestoreProgress {
            backup_name,
            progress: osu_sync_core::backup::BackupProgress {
                phase: osu_sync_core::backup::BackupPhase::Scanning,
                files_processed: 0,
                total_files: None,
                bytes_written: 0,
                current_file: None,
            },
        };
        let _ = self.worker_tx.send(WorkerMessage::RestoreBackup {
            backup_path: backup_path.clone(),
        });
    }

    /// Get the restore destination path for a backup target
    fn get_restore_dest_path(&self, target: &BackupTarget) -> PathBuf {
        use osu_sync_core::config::Config;
        let config = Config::load();

        match target {
            BackupTarget::StableSongs => config
                .stable_path
                .map(|p| p.join("Songs"))
                .unwrap_or_else(|| PathBuf::from("Songs")),
            BackupTarget::StableCollections | BackupTarget::StableScores => {
                config.stable_path.unwrap_or_else(|| PathBuf::from("."))
            }
            BackupTarget::LazerData => config.lazer_path.unwrap_or_else(|| PathBuf::from(".")),
            BackupTarget::All => PathBuf::from("."),
        }
    }

    /// Start collection sync operation
    fn start_collection_sync(&mut self, strategy: CollectionSyncStrategy) {
        self.state = AppState::CollectionSync {
            progress: 0.0,
            current_collection: "Starting...".to_string(),
            logs: vec![LogEntry {
                message: format!("Starting collection sync with {} strategy", strategy),
                level: LogLevel::Info,
            }],
        };
        let _ = self
            .worker_tx
            .send(WorkerMessage::SyncCollections { strategy });
    }

    /// Start sync operation
    fn start_sync(&mut self, direction: SyncDirection, selected_set_ids: Option<HashSet<i32>>) {
        // Reset cancellation flag before starting
        self.reset_cancel();

        let count_msg = if let Some(ref ids) = selected_set_ids {
            format!(" ({} selected)", ids.len())
        } else {
            String::new()
        };

        self.state = AppState::Syncing {
            progress: None,
            logs: vec![LogEntry {
                message: format!("Starting sync: {}{}", direction, count_msg),
                level: LogLevel::Info,
            }],
            stats: SyncStats::default(),
            is_paused: false,
        };
        let _ = self
            .worker_tx
            .send(WorkerMessage::StartSync { direction, selected_set_ids });
    }

    /// Start dry run operation
    fn start_dry_run(&mut self, direction: SyncDirection) {
        // Reset cancellation flag before starting
        self.reset_cancel();

        // Use syncing state to show progress during analysis
        self.state = AppState::Syncing {
            progress: None,
            logs: vec![LogEntry {
                message: format!("Analyzing what would be synced ({})...", direction),
                level: LogLevel::Info,
            }],
            stats: SyncStats::default(),
            is_paused: false,
        };
        let _ = self
            .worker_tx
            .send(WorkerMessage::StartDryRun { direction });
    }

    fn handle_backup_config_key(&mut self, key: KeyEvent, selected: usize) {
        const BACKUP_OPTIONS: usize = 5; // 5 backup targets

        if event::is_escape(&key) {
            self.state = AppState::MainMenu { selected: 5 };
        } else if event::is_down(&key) {
            if let AppState::BackupConfig { status_message, .. } = &self.state {
                self.state = AppState::BackupConfig {
                    selected: (selected + 1) % BACKUP_OPTIONS,
                    status_message: status_message.clone(),
                };
            }
        } else if event::is_up(&key) {
            if let AppState::BackupConfig { status_message, .. } = &self.state {
                self.state = AppState::BackupConfig {
                    selected: selected.checked_sub(1).unwrap_or(BACKUP_OPTIONS - 1),
                    status_message: status_message.clone(),
                };
            }
        } else if event::is_enter(&key) {
            self.start_backup(selected);
        }
    }

    fn handle_backup_progress_key(&mut self, key: KeyEvent) {
        if event::is_escape(&key) {
            self.request_cancel();
            self.state = AppState::BackupConfig {
                selected: 0,
                status_message: "Backup cancelled".to_string(),
            };
        }
    }

    fn handle_backup_complete_key(&mut self, key: KeyEvent) {
        if event::is_enter(&key) || event::is_escape(&key) {
            self.state = AppState::MainMenu { selected: 5 };
        }
    }

    fn handle_restore_config_key(&mut self, key: KeyEvent, selected: usize) {
        if event::is_escape(&key) {
            self.state = AppState::MainMenu { selected: 6 };
        } else if let AppState::RestoreConfig {
            backups,
            loading,
            status_message,
            ..
        } = &self.state
        {
            let backups = backups.clone();
            let loading = *loading;
            let status_message = status_message.clone();
            let num_backups = backups.len();

            if num_backups == 0 || loading {
                return;
            }

            if event::is_down(&key) {
                self.state = AppState::RestoreConfig {
                    backups,
                    selected: (selected + 1) % num_backups,
                    loading,
                    status_message,
                };
            } else if event::is_up(&key) {
                self.state = AppState::RestoreConfig {
                    backups,
                    selected: selected.checked_sub(1).unwrap_or(num_backups - 1),
                    loading,
                    status_message,
                };
            } else if event::is_enter(&key) && selected < num_backups {
                // Go to confirm screen
                let backup = backups[selected].clone();
                let dest_path = self.get_restore_dest_path(&backup.target);
                self.state = AppState::RestoreConfirm {
                    backup,
                    dest_path,
                    selected: 0,
                };
            }
        }
    }

    fn handle_restore_confirm_key(&mut self, key: KeyEvent, selected: usize) {
        if event::is_escape(&key) {
            self.go_to_restore_config();
        } else if event::is_left(&key) || event::is_right(&key) {
            if let AppState::RestoreConfirm {
                backup, dest_path, ..
            } = &self.state
            {
                self.state = AppState::RestoreConfirm {
                    backup: backup.clone(),
                    dest_path: dest_path.clone(),
                    selected: if selected == 0 { 1 } else { 0 },
                };
            }
        } else if event::is_enter(&key) {
            if selected == 0 {
                // Cancel
                self.go_to_restore_config();
            } else {
                // Confirm restore
                if let AppState::RestoreConfirm { backup, .. } = &self.state {
                    self.start_restore(&backup.path.clone());
                }
            }
        }
    }

    fn handle_restore_progress_key(&mut self, key: KeyEvent) {
        if event::is_escape(&key) {
            self.request_cancel();
            self.go_to_restore_config();
        }
    }

    fn handle_restore_complete_key(&mut self, key: KeyEvent) {
        if event::is_enter(&key) || event::is_escape(&key) {
            self.state = AppState::MainMenu { selected: 6 };
        }
    }

    fn handle_media_config_key(&mut self, key: KeyEvent, selected: usize) {
        const MEDIA_OPTIONS: usize = 6; // media type, organization, skip duplicates, include metadata, output path, start

        if let AppState::MediaConfig {
            media_type,
            organization,
            output_path,
            skip_duplicates,
            include_metadata,
            status_message,
            ..
        } = &self.state
        {
            let media_type = *media_type;
            let organization = *organization;
            let output_path = output_path.clone();
            let skip_duplicates = *skip_duplicates;
            let include_metadata = *include_metadata;
            let status_message = status_message.clone();

            if event::is_escape(&key) {
                self.state = AppState::MainMenu { selected: 3 };
            } else if event::is_down(&key) {
                self.state = AppState::MediaConfig {
                    selected: (selected + 1) % MEDIA_OPTIONS,
                    media_type,
                    organization,
                    output_path,
                    skip_duplicates,
                    include_metadata,
                    status_message,
                };
            } else if event::is_up(&key) {
                self.state = AppState::MediaConfig {
                    selected: selected.checked_sub(1).unwrap_or(MEDIA_OPTIONS - 1),
                    media_type,
                    organization,
                    output_path,
                    skip_duplicates,
                    include_metadata,
                    status_message,
                };
            } else if event::is_enter(&key) || event::is_space(&key) {
                match selected {
                    0 => {
                        // Toggle media type
                        let new_type = match media_type {
                            MediaType::Audio => MediaType::Backgrounds,
                            MediaType::Backgrounds => MediaType::Both,
                            MediaType::Both => MediaType::Audio,
                        };
                        self.state = AppState::MediaConfig {
                            selected,
                            media_type: new_type,
                            organization,
                            output_path,
                            skip_duplicates,
                            include_metadata,
                            status_message,
                        };
                    }
                    1 => {
                        // Toggle organization
                        let new_org = match organization {
                            OutputOrganization::Flat => OutputOrganization::ByArtist,
                            OutputOrganization::ByArtist => OutputOrganization::ByBeatmap,
                            OutputOrganization::ByBeatmap => OutputOrganization::Flat,
                        };
                        self.state = AppState::MediaConfig {
                            selected,
                            media_type,
                            organization: new_org,
                            output_path,
                            skip_duplicates,
                            include_metadata,
                            status_message,
                        };
                    }
                    2 => {
                        // Toggle skip duplicates
                        self.state = AppState::MediaConfig {
                            selected,
                            media_type,
                            organization,
                            output_path,
                            skip_duplicates: !skip_duplicates,
                            include_metadata,
                            status_message,
                        };
                    }
                    3 => {
                        // Toggle include metadata
                        self.state = AppState::MediaConfig {
                            selected,
                            media_type,
                            organization,
                            output_path,
                            skip_duplicates,
                            include_metadata: !include_metadata,
                            status_message,
                        };
                    }
                    4 => {
                        // Output path - could add editing, for now just toggle
                    }
                    5 => {
                        // Start extraction
                        self.start_media_extraction(
                            media_type,
                            organization,
                            &output_path,
                            skip_duplicates,
                            include_metadata,
                        );
                    }
                    _ => {}
                }
            }
        }
    }

    fn handle_media_progress_key(&mut self, key: KeyEvent) {
        if event::is_escape(&key) {
            self.request_cancel();
            self.go_to_media_config();
        }
    }

    fn handle_media_complete_key(&mut self, key: KeyEvent) {
        if event::is_enter(&key) || event::is_escape(&key) {
            self.state = AppState::MainMenu { selected: 3 };
        }
    }

    fn handle_replay_config_key(&mut self, key: KeyEvent, selected: usize) {
        const REPLAY_OPTIONS: usize = 5; // organization, output path, filter, rename pattern, start

        if let AppState::ReplayConfig {
            organization,
            output_path,
            replays,
            loading,
            status_message,
            filter,
            rename_pattern,
            filter_panel_open,
            filter_field,
            ..
        } = &self.state
        {
            let organization = *organization;
            let output_path = output_path.clone();
            let replays = replays.clone();
            let loading = *loading;
            let status_message = status_message.clone();
            let filter = filter.clone();
            let rename_pattern = rename_pattern.clone();
            let filter_panel_open = *filter_panel_open;
            let filter_field = *filter_field;

            if event::is_escape(&key) {
                if filter_panel_open {
                    // Close filter panel
                    self.state = AppState::ReplayConfig {
                        selected,
                        organization,
                        output_path,
                        replays,
                        loading,
                        status_message,
                        filter,
                        rename_pattern,
                        filter_panel_open: false,
                        filter_field: 0,
                    };
                } else {
                    self.state = AppState::MainMenu { selected: 4 };
                }
            } else if loading {
                return; // Don't process navigation while loading
            } else if filter_panel_open {
                // Handle filter panel navigation
                self.handle_replay_filter_panel_key(
                    key,
                    selected,
                    organization,
                    output_path,
                    replays,
                    status_message,
                    filter,
                    rename_pattern,
                    filter_field,
                );
            } else if event::is_down(&key) {
                self.state = AppState::ReplayConfig {
                    selected: (selected + 1) % REPLAY_OPTIONS,
                    organization,
                    output_path,
                    replays,
                    loading,
                    status_message,
                    filter,
                    rename_pattern,
                    filter_panel_open: false,
                    filter_field,
                };
            } else if event::is_up(&key) {
                self.state = AppState::ReplayConfig {
                    selected: selected.checked_sub(1).unwrap_or(REPLAY_OPTIONS - 1),
                    organization,
                    output_path,
                    replays,
                    loading,
                    status_message,
                    filter,
                    rename_pattern,
                    filter_panel_open: false,
                    filter_field,
                };
            } else if event::is_enter(&key) || event::is_space(&key) {
                match selected {
                    0 => {
                        // Toggle organization
                        let new_org = match organization {
                            ExportOrganization::Flat => ExportOrganization::ByBeatmap,
                            ExportOrganization::ByBeatmap => ExportOrganization::ByDate,
                            ExportOrganization::ByDate => ExportOrganization::ByPlayer,
                            ExportOrganization::ByPlayer => ExportOrganization::ByGrade,
                            ExportOrganization::ByGrade => ExportOrganization::Flat,
                        };
                        self.state = AppState::ReplayConfig {
                            selected,
                            organization: new_org,
                            output_path,
                            replays,
                            loading,
                            status_message,
                            filter,
                            rename_pattern,
                            filter_panel_open: false,
                            filter_field,
                        };
                    }
                    1 => {
                        // Output path - could add editing
                    }
                    2 => {
                        // Open filter panel
                        self.state = AppState::ReplayConfig {
                            selected,
                            organization,
                            output_path,
                            replays,
                            loading,
                            status_message,
                            filter,
                            rename_pattern,
                            filter_panel_open: true,
                            filter_field: 0,
                        };
                    }
                    3 => {
                        // Toggle between rename pattern presets
                        let new_pattern = if rename_pattern.is_empty() {
                            "{artist} - {title} [{grade}]".to_string()
                        } else if rename_pattern == "{artist} - {title} [{grade}]" {
                            "{player}_{date}_{title}_{grade}".to_string()
                        } else if rename_pattern == "{player}_{date}_{title}_{grade}" {
                            "{date}_{mode}_{title}".to_string()
                        } else {
                            String::new()
                        };
                        self.state = AppState::ReplayConfig {
                            selected,
                            organization,
                            output_path,
                            replays,
                            loading,
                            status_message,
                            filter,
                            rename_pattern: new_pattern,
                            filter_panel_open: false,
                            filter_field,
                        };
                    }
                    4 => {
                        // Start export
                        let exportable: Vec<_> = replays
                            .iter()
                            .filter(|r| r.has_replay_file)
                            .cloned()
                            .collect();
                        if !exportable.is_empty() {
                            self.start_replay_export(
                                organization,
                                &output_path,
                                filter,
                                &rename_pattern,
                            );
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn handle_replay_filter_panel_key(
        &mut self,
        key: KeyEvent,
        selected: usize,
        organization: ExportOrganization,
        output_path: String,
        replays: Vec<ReplayInfo>,
        status_message: Option<String>,
        mut filter: ReplayFilter,
        rename_pattern: String,
        filter_field: usize,
    ) {
        const FILTER_FIELDS: usize = 5; // grade, osu, taiko, catch, mania

        if event::is_down(&key) || event::is_right(&key) {
            let next_field = (filter_field + 1) % FILTER_FIELDS;
            self.state = AppState::ReplayConfig {
                selected,
                organization,
                output_path,
                replays,
                loading: false,
                status_message,
                filter,
                rename_pattern,
                filter_panel_open: true,
                filter_field: next_field,
            };
        } else if event::is_up(&key) || event::is_left(&key) {
            let prev_field = if filter_field == 0 {
                FILTER_FIELDS - 1
            } else {
                filter_field - 1
            };
            self.state = AppState::ReplayConfig {
                selected,
                organization,
                output_path,
                replays,
                loading: false,
                status_message,
                filter,
                rename_pattern,
                filter_panel_open: true,
                filter_field: prev_field,
            };
        } else if event::is_space(&key) || event::is_enter(&key) {
            // Toggle the selected filter
            match filter_field {
                0 => {
                    // Cycle grade threshold: None -> SS -> S -> A -> B -> C -> D -> None
                    filter.min_grade = match filter.min_grade {
                        None => Some(Grade::SS),
                        Some(Grade::SS) | Some(Grade::SSilver) => Some(Grade::S),
                        Some(Grade::S) | Some(Grade::SSilver2) => Some(Grade::A),
                        Some(Grade::A) => Some(Grade::B),
                        Some(Grade::B) => Some(Grade::C),
                        Some(Grade::C) => Some(Grade::D),
                        Some(Grade::D) | Some(Grade::F) => None,
                    };
                }
                1 => {
                    // Toggle osu! mode
                    if filter.modes.contains(&GameMode::Osu) {
                        filter.modes.retain(|m| *m != GameMode::Osu);
                    } else {
                        filter.modes.push(GameMode::Osu);
                    }
                }
                2 => {
                    // Toggle taiko mode
                    if filter.modes.contains(&GameMode::Taiko) {
                        filter.modes.retain(|m| *m != GameMode::Taiko);
                    } else {
                        filter.modes.push(GameMode::Taiko);
                    }
                }
                3 => {
                    // Toggle catch mode
                    if filter.modes.contains(&GameMode::Catch) {
                        filter.modes.retain(|m| *m != GameMode::Catch);
                    } else {
                        filter.modes.push(GameMode::Catch);
                    }
                }
                4 => {
                    // Toggle mania mode
                    if filter.modes.contains(&GameMode::Mania) {
                        filter.modes.retain(|m| *m != GameMode::Mania);
                    } else {
                        filter.modes.push(GameMode::Mania);
                    }
                }
                _ => {}
            }
            self.state = AppState::ReplayConfig {
                selected,
                organization,
                output_path,
                replays,
                loading: false,
                status_message,
                filter,
                rename_pattern,
                filter_panel_open: true,
                filter_field,
            };
        }
    }

    fn handle_replay_progress_key(&mut self, key: KeyEvent) {
        if event::is_escape(&key) {
            self.request_cancel();
            self.go_to_replay_config();
        }
    }

    fn handle_replay_complete_key(&mut self, key: KeyEvent) {
        if event::is_enter(&key) || event::is_escape(&key) {
            self.state = AppState::MainMenu { selected: 4 };
        }
    }

    fn start_media_extraction(
        &mut self,
        media_type: MediaType,
        organization: OutputOrganization,
        output_path: &str,
        skip_duplicates: bool,
        include_metadata: bool,
    ) {
        self.state = AppState::MediaProgress {
            progress: None,
            current_set: "Starting...".to_string(),
        };
        let _ = self.worker_tx.send(WorkerMessage::StartMediaExtraction {
            media_type,
            organization,
            output_path: PathBuf::from(output_path),
            skip_duplicates,
            include_metadata,
        });
    }

    fn start_replay_export(
        &mut self,
        organization: ExportOrganization,
        output_path: &str,
        filter: ReplayFilter,
        rename_pattern: &str,
    ) {
        self.state = AppState::ReplayProgress {
            progress: None,
            current_replay: "Starting...".to_string(),
        };
        let _ = self.worker_tx.send(WorkerMessage::StartReplayExport {
            organization,
            output_path: PathBuf::from(output_path),
            filter,
            rename_pattern: if rename_pattern.is_empty() {
                None
            } else {
                Some(rename_pattern.to_string())
            },
        });
    }

    /// Resolve a duplicate with the selected action
    fn resolve_duplicate(&mut self, selected: usize, apply_to_all: bool) {
        use osu_sync_core::dedup::{DuplicateAction, DuplicateResolution};

        let action = match selected {
            0 => DuplicateAction::Skip,
            1 => DuplicateAction::Replace,
            2 => DuplicateAction::KeepBoth,
            _ => DuplicateAction::Skip,
        };

        let resolution = DuplicateResolution {
            action,
            apply_to_all,
        };
        let _ = self
            .worker_tx
            .send(WorkerMessage::ResolveDuplicate(resolution));

        // Return to syncing state (will be updated by worker messages)
        self.state = AppState::Syncing {
            progress: None,
            logs: Vec::new(),
            stats: SyncStats::default(),
            is_paused: false,
        };
    }

    /// Process messages from the worker thread
    pub fn process_worker_messages(&mut self) {
        while let Ok(msg) = self.worker_rx.try_recv() {
            match msg {
                AppMessage::ScanProgress { stable: _, message } => {
                    if let AppState::Scanning { status_message, .. } = &mut self.state {
                        *status_message = message;
                    }
                }
                AppMessage::ScanComplete { stable, lazer } => {
                    self.cached_stable_scan = stable.clone();
                    self.cached_lazer_scan = lazer.clone();
                    self.state = AppState::Scanning {
                        in_progress: false,
                        stable_result: stable,
                        lazer_result: lazer,
                        status_message: "Scan complete".to_string(),
                    };
                }
                AppMessage::SyncProgress(progress) => {
                    if let AppState::Syncing {
                        progress: p,
                        logs,
                        stats,
                        is_paused: _,
                    } = &mut self.state
                    {
                        // Add log entry
                        logs.push(LogEntry {
                            message: format!("Processing: {}", progress.current_name),
                            level: LogLevel::Info,
                        });
                        // Keep only last 100 entries
                        if logs.len() > 100 {
                            logs.remove(0);
                        }
                        *p = Some(progress);
                        let _ = stats; // stats updated separately
                    }
                }
                AppMessage::DuplicateFound(info) => {
                    self.state = AppState::DuplicateDialog {
                        info,
                        selected: 0,
                        apply_to_all: false,
                    };
                }
                AppMessage::SyncComplete(result) => {
                    self.state = AppState::SyncComplete { result };
                }
                AppMessage::SyncCancelled => {
                    // Return to main menu when cancelled
                    self.state = AppState::MainMenu { selected: 0 };
                }
                AppMessage::StatsProgress(message) => {
                    if let AppState::Statistics { status_message, .. } = &mut self.state {
                        *status_message = message;
                    }
                }
                AppMessage::StatsComplete(stats) => {
                    self.cached_stats = Some(stats.clone());
                    if let AppState::Statistics {
                        stats: s,
                        loading,
                        status_message,
                        ..
                    } = &mut self.state
                    {
                        *s = Some(stats);
                        *loading = false;
                        *status_message = "Statistics ready".to_string();
                    }
                }
                AppMessage::CollectionsLoaded(collections) => {
                    let count = collections.len();
                    let total_beatmaps: usize = collections.iter().map(|c| c.len()).sum();
                    self.state = AppState::CollectionConfig {
                        collections,
                        selected: 0,
                        strategy: CollectionSyncStrategy::default(),
                        loading: false,
                        status_message: format!(
                            "Found {} collections with {} beatmaps",
                            count, total_beatmaps
                        ),
                    };
                }
                AppMessage::CollectionSyncProgress {
                    collection,
                    progress,
                } => {
                    if let AppState::CollectionSync {
                        progress: p,
                        current_collection,
                        logs,
                    } = &mut self.state
                    {
                        *p = progress;
                        *current_collection = collection.clone();
                        logs.push(LogEntry {
                            message: format!("Processing: {}", collection),
                            level: LogLevel::Info,
                        });
                        if logs.len() > 50 {
                            logs.remove(0);
                        }
                    }
                }
                AppMessage::CollectionSyncComplete(result) => {
                    self.state = AppState::CollectionSummary { result };
                }
                AppMessage::DryRunComplete { result, direction } => {
                    // Default: check all items that have Import action
                    use osu_sync_core::sync::DryRunAction;
                    let checked_items: HashSet<usize> = result
                        .items
                        .iter()
                        .enumerate()
                        .filter(|(_, item)| matches!(item.action, DryRunAction::Import))
                        .map(|(idx, _)| idx)
                        .collect();

                    self.state = AppState::DryRunPreview {
                        result,
                        direction,
                        selected_item: 0,
                        scroll_offset: 0,
                        checked_items,
                        filter_text: String::new(),
                        filter_mode: false,
                    };
                }
                AppMessage::BackupProgress(progress) => {
                    if let AppState::BackupProgress {
                        progress: p,
                        target: _,
                    } = &mut self.state
                    {
                        *p = progress;
                    }
                }
                AppMessage::BackupComplete {
                    path,
                    size_bytes,
                    is_incremental,
                } => {
                    self.state = AppState::BackupComplete {
                        backup_path: path,
                        size_bytes,
                        is_incremental,
                    };
                }
                AppMessage::BackupsLoaded(backups) => {
                    let count = backups.len();
                    self.state = AppState::RestoreConfig {
                        backups,
                        selected: 0,
                        loading: false,
                        status_message: format!("Found {} backups", count),
                    };
                }
                AppMessage::RestoreProgress(progress) => {
                    if let AppState::RestoreProgress { progress: p, .. } = &mut self.state {
                        *p = progress;
                    }
                }
                AppMessage::RestoreComplete {
                    dest_path,
                    files_restored,
                } => {
                    self.state = AppState::RestoreComplete {
                        dest_path,
                        files_restored,
                    };
                }
                AppMessage::MediaProgress(progress) => {
                    if let AppState::MediaProgress {
                        progress: p,
                        current_set,
                    } = &mut self.state
                    {
                        *current_set = progress.current_set.clone();
                        *p = Some(progress);
                    }
                }
                AppMessage::MediaComplete(result) => {
                    self.state = AppState::MediaComplete { result };
                }
                AppMessage::ReplaysLoaded {
                    replays,
                    exportable_count,
                } => {
                    if let AppState::ReplayConfig {
                        selected,
                        organization,
                        output_path,
                        filter,
                        rename_pattern,
                        ..
                    } = &self.state
                    {
                        self.state = AppState::ReplayConfig {
                            selected: *selected,
                            organization: *organization,
                            output_path: output_path.clone(),
                            replays,
                            loading: false,
                            status_message: Some(format!(
                                "Found {} replays with .osr files",
                                exportable_count
                            )),
                            filter: filter.clone(),
                            rename_pattern: rename_pattern.clone(),
                            filter_panel_open: false,
                            filter_field: 0,
                        };
                    }
                }
                AppMessage::ReplayProgress(progress) => {
                    if let AppState::ReplayProgress {
                        progress: p,
                        current_replay,
                    } = &mut self.state
                    {
                        *current_replay = progress.current_replay.clone();
                        *p = Some(progress);
                    }
                }
                AppMessage::ReplayComplete(result) => {
                    let stats = result.stats.clone();
                    self.state = AppState::ReplayComplete { result, stats };
                }
                AppMessage::UnifiedStorageProgress {
                    phase,
                    current,
                    total,
                    message,
                } => {
                    if let AppState::UnifiedSetup { screen } = &mut self.state {
                        screen.current_operation = format!("{}: {}", phase, message);
                        if total > 0 {
                            screen.progress = Some(
                                crate::screens::unified_setup::MigrationProgress {
                                    phase: crate::screens::unified_setup::MigrationPhase::Preparing,
                                    current,
                                    total,
                                    current_item: message,
                                    bytes_processed: 0,
                                    bytes_total: 0,
                                },
                            );
                        }
                    }
                }
                AppMessage::UnifiedStorageComplete {
                    success,
                    message,
                    links_created: _,
                    space_saved: _,
                } => {
                    if let AppState::UnifiedSetup { screen } = &mut self.state {
                        screen.set_complete(success, Some(message));
                    }
                }
                AppMessage::UnifiedStorageStatus {
                    mode,
                    active_links,
                    broken_links,
                    space_saved,
                } => {
                    if let AppState::UnifiedStatus { screen } = &mut self.state {
                        screen.mode = mode;
                        screen.health.total = active_links + broken_links;
                        screen.health.active = active_links;
                        screen.health.broken = broken_links;
                        screen.stats.space_saved = space_saved;
                    }
                }
                AppMessage::UnifiedStorageVerifyComplete {
                    healthy,
                    broken,
                    repaired: _,
                } => {
                    if let AppState::UnifiedStatus { screen } = &mut self.state {
                        screen.health.active = healthy;
                        screen.health.broken = broken;
                        screen.health.total = healthy + broken;
                        screen.loading = false;
                    }
                }
                AppMessage::Error(error) => {
                    self.last_error = Some(error);
                }
            }
        }
    }

    /// Check if help screen can be shown from current state
    fn can_show_help(&self) -> bool {
        // Help can be shown from most states except during active operations
        // and from the help screen itself
        !matches!(
            self.state,
            AppState::Syncing { .. }
                | AppState::CollectionSync { .. }
                | AppState::BackupProgress { .. }
                | AppState::RestoreProgress { .. }
                | AppState::MediaProgress { .. }
                | AppState::ReplayProgress { .. }
                | AppState::Help { .. }
                | AppState::Exiting
        )
    }

    /// Show the help screen
    fn show_help(&mut self) {
        let previous_state = Box::new(self.state.clone());
        self.state = AppState::Help { previous_state };
    }

    /// Render the application
    pub fn render(&self, frame: &mut Frame) {
        screens::render(frame, self);
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}
