//! Export functionality for statistics data

use std::fs::File;
use std::io::Write;
use std::path::Path;

use serde::Serialize;

use super::model::{ComparisonStats, DuplicateStats, InstallationStats};
use crate::error::{Error, Result};

/// Serializable version of ComparisonStats for JSON export
#[derive(Serialize)]
struct ExportStats<'a> {
    stable: &'a InstallationStats,
    lazer: &'a InstallationStats,
    duplicates: &'a DuplicateStats,
    unique_to_stable: usize,
    unique_to_lazer: usize,
    common_beatmaps: usize,
    total_unique: usize,
}

impl<'a> From<&'a ComparisonStats> for ExportStats<'a> {
    fn from(stats: &'a ComparisonStats) -> Self {
        Self {
            stable: &stats.stable,
            lazer: &stats.lazer,
            duplicates: &stats.duplicates,
            unique_to_stable: stats.unique_to_stable,
            unique_to_lazer: stats.unique_to_lazer,
            common_beatmaps: stats.common_beatmaps,
            total_unique: stats.total_unique(),
        }
    }
}

/// Export statistics to JSON format
pub fn export_json(stats: &ComparisonStats, path: &Path) -> Result<()> {
    let export_data = ExportStats::from(stats);
    let json = serde_json::to_string_pretty(&export_data)
        .map_err(|e| Error::Other(format!("Failed to serialize stats: {}", e)))?;

    let mut file = File::create(path)?;
    file.write_all(json.as_bytes())?;

    Ok(())
}

/// Export statistics to CSV format
pub fn export_csv(stats: &ComparisonStats, path: &Path) -> Result<()> {
    let mut writer = csv::Writer::from_path(path)
        .map_err(|e| Error::Other(format!("Failed to create CSV file: {}", e)))?;

    // Write header
    writer
        .write_record(["Category", "Metric", "Stable", "Lazer", "Notes"])
        .map_err(|e| Error::Other(format!("CSV write error: {}", e)))?;

    // Overview statistics
    writer
        .write_record([
            "Overview",
            "Beatmap Sets",
            &stats.stable.total_beatmap_sets.to_string(),
            &stats.lazer.total_beatmap_sets.to_string(),
            "",
        ])
        .map_err(|e| Error::Other(format!("CSV write error: {}", e)))?;

    writer
        .write_record([
            "Overview",
            "Total Beatmaps",
            &stats.stable.total_beatmaps.to_string(),
            &stats.lazer.total_beatmaps.to_string(),
            "",
        ])
        .map_err(|e| Error::Other(format!("CSV write error: {}", e)))?;

    writer
        .write_record([
            "Overview",
            "Storage (bytes)",
            &stats.stable.storage_bytes.to_string(),
            &stats.lazer.storage_bytes.to_string(),
            "",
        ])
        .map_err(|e| Error::Other(format!("CSV write error: {}", e)))?;

    writer
        .write_record([
            "Overview",
            "Storage (formatted)",
            &stats.stable.storage_display(),
            &stats.lazer.storage_display(),
            "",
        ])
        .map_err(|e| Error::Other(format!("CSV write error: {}", e)))?;

    // Comparison stats
    writer
        .write_record([
            "Comparison",
            "Common Beatmaps",
            &stats.common_beatmaps.to_string(),
            "",
            "Present in both installations",
        ])
        .map_err(|e| Error::Other(format!("CSV write error: {}", e)))?;

    writer
        .write_record([
            "Comparison",
            "Unique to Stable",
            &stats.unique_to_stable.to_string(),
            "",
            "Only in osu!stable",
        ])
        .map_err(|e| Error::Other(format!("CSV write error: {}", e)))?;

    writer
        .write_record([
            "Comparison",
            "Unique to Lazer",
            "",
            &stats.unique_to_lazer.to_string(),
            "Only in osu!lazer",
        ])
        .map_err(|e| Error::Other(format!("CSV write error: {}", e)))?;

    writer
        .write_record([
            "Comparison",
            "Total Unique",
            &stats.total_unique().to_string(),
            "",
            "All unique beatmap sets",
        ])
        .map_err(|e| Error::Other(format!("CSV write error: {}", e)))?;

    // Duplicate stats
    writer
        .write_record([
            "Duplicates",
            "Count",
            &stats.duplicates.count.to_string(),
            "",
            "Number of duplicate sets",
        ])
        .map_err(|e| Error::Other(format!("CSV write error: {}", e)))?;

    writer
        .write_record([
            "Duplicates",
            "Wasted Space (bytes)",
            &stats.duplicates.wasted_bytes.to_string(),
            "",
            "",
        ])
        .map_err(|e| Error::Other(format!("CSV write error: {}", e)))?;

    writer
        .write_record([
            "Duplicates",
            "Wasted Space (formatted)",
            &stats.duplicates.wasted_display(),
            "",
            "",
        ])
        .map_err(|e| Error::Other(format!("CSV write error: {}", e)))?;

    // Match types breakdown
    for (match_type, count) in &stats.duplicates.by_match_type {
        writer
            .write_record([
                "Duplicates",
                &format!("Match Type: {}", match_type),
                &count.to_string(),
                "",
                "",
            ])
            .map_err(|e| Error::Other(format!("CSV write error: {}", e)))?;
    }

    // Game mode breakdown
    for (mode, count) in &stats.stable.by_mode {
        writer
            .write_record([
                "Game Mode",
                &format!("{:?}", mode),
                &count.to_string(),
                &stats.lazer.by_mode.get(mode).unwrap_or(&0).to_string(),
                "",
            ])
            .map_err(|e| Error::Other(format!("CSV write error: {}", e)))?;
    }

    // Add any lazer modes not in stable
    for (mode, count) in &stats.lazer.by_mode {
        if !stats.stable.by_mode.contains_key(mode) {
            writer
                .write_record([
                    "Game Mode",
                    &format!("{:?}", mode),
                    "0",
                    &count.to_string(),
                    "",
                ])
                .map_err(|e| Error::Other(format!("CSV write error: {}", e)))?;
        }
    }

    writer
        .flush()
        .map_err(|e| Error::Other(format!("CSV flush error: {}", e)))?;

    Ok(())
}

/// Export format options
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportFormat {
    #[default]
    Json,
    Csv,
}

impl std::fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportFormat::Json => write!(f, "JSON"),
            ExportFormat::Csv => write!(f, "CSV"),
        }
    }
}

impl ExportFormat {
    /// Get the file extension for this format
    pub fn extension(&self) -> &'static str {
        match self {
            ExportFormat::Json => "json",
            ExportFormat::Csv => "csv",
        }
    }

    /// Export stats using this format
    pub fn export(&self, stats: &ComparisonStats, path: &Path) -> Result<()> {
        match self {
            ExportFormat::Json => export_json(stats, path),
            ExportFormat::Csv => export_csv(stats, path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(dead_code)]
    fn create_test_stats() -> ComparisonStats {
        ComparisonStats {
            stable: InstallationStats {
                total_beatmap_sets: 100,
                total_beatmaps: 500,
                storage_bytes: 1024 * 1024 * 1024, // 1 GB
                ..Default::default()
            },
            lazer: InstallationStats {
                total_beatmap_sets: 150,
                total_beatmaps: 750,
                storage_bytes: 2 * 1024 * 1024 * 1024, // 2 GB
                ..Default::default()
            },
            duplicates: DuplicateStats {
                count: 50,
                wasted_bytes: 512 * 1024 * 1024, // 512 MB
                ..Default::default()
            },
            unique_to_stable: 25,
            unique_to_lazer: 75,
            common_beatmaps: 75,
        }
    }

    #[test]
    fn test_export_format_extension() {
        assert_eq!(ExportFormat::Json.extension(), "json");
        assert_eq!(ExportFormat::Csv.extension(), "csv");
    }

    #[test]
    fn test_export_format_display() {
        assert_eq!(format!("{}", ExportFormat::Json), "JSON");
        assert_eq!(format!("{}", ExportFormat::Csv), "CSV");
    }
}
