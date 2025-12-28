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
    Html,
}

impl std::fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportFormat::Json => write!(f, "JSON"),
            ExportFormat::Csv => write!(f, "CSV"),
            ExportFormat::Html => write!(f, "HTML"),
        }
    }
}

impl ExportFormat {
    /// Get the file extension for this format
    pub fn extension(&self) -> &'static str {
        match self {
            ExportFormat::Json => "json",
            ExportFormat::Csv => "csv",
            ExportFormat::Html => "html",
        }
    }

    /// Export stats using this format
    pub fn export(&self, stats: &ComparisonStats, path: &Path) -> Result<()> {
        match self {
            ExportFormat::Json => export_json(stats, path),
            ExportFormat::Csv => export_csv(stats, path),
            ExportFormat::Html => export_html(stats, path),
        }
    }
}

/// HTML export helper
pub struct HtmlExport;

impl HtmlExport {
    /// Generate a bar chart as HTML/CSS
    fn bar_chart(items: &[(String, usize)], max_width: u32) -> String {
        if items.is_empty() {
            return "<p>No data</p>".to_string();
        }

        let max_value = items.iter().map(|(_, v)| *v).max().unwrap_or(1) as f32;
        let mut html = String::from("<div class=\"chart\">\n");

        for (label, value) in items {
            let width_pct = (*value as f32 / max_value * max_width as f32) as u32;
            html.push_str(&format!(
                "  <div class=\"chart-row\">\n    <span class=\"label\">{}</span>\n    <div class=\"bar\" style=\"width: {}%\"></div>\n    <span class=\"value\">{}</span>\n  </div>\n",
                label, width_pct.max(1), value
            ));
        }

        html.push_str("</div>");
        html
    }

    /// Generate the CSS styles for the report
    fn css() -> &'static str {
        r#"
:root {
    --bg-primary: #1e1e2e;
    --bg-secondary: #313244;
    --bg-tertiary: #45475a;
    --text-primary: #cdd6f4;
    --text-secondary: #a6adc8;
    --accent: #f5c2e7;
    --accent-secondary: #cba6f7;
    --success: #a6e3a1;
    --warning: #f9e2af;
    --error: #f38ba8;
}

body {
    font-family: 'Segoe UI', system-ui, sans-serif;
    background: var(--bg-primary);
    color: var(--text-primary);
    margin: 0;
    padding: 2rem;
    line-height: 1.6;
}

.container {
    max-width: 1200px;
    margin: 0 auto;
}

h1 {
    color: var(--accent);
    border-bottom: 2px solid var(--accent);
    padding-bottom: 0.5rem;
}

h2 {
    color: var(--accent-secondary);
    margin-top: 2rem;
}

.stats-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(300px, 1fr));
    gap: 1.5rem;
    margin: 1.5rem 0;
}

.stat-card {
    background: var(--bg-secondary);
    border-radius: 8px;
    padding: 1.5rem;
    border-left: 4px solid var(--accent);
}

.stat-card h3 {
    margin: 0 0 1rem 0;
    color: var(--text-secondary);
    font-size: 0.9rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
}

.stat-value {
    font-size: 2rem;
    font-weight: bold;
    color: var(--accent);
}

.stat-detail {
    color: var(--text-secondary);
    font-size: 0.9rem;
    margin-top: 0.5rem;
}

table {
    width: 100%;
    border-collapse: collapse;
    margin: 1rem 0;
    background: var(--bg-secondary);
    border-radius: 8px;
    overflow: hidden;
}

th, td {
    padding: 1rem;
    text-align: left;
    border-bottom: 1px solid var(--bg-tertiary);
}

th {
    background: var(--bg-tertiary);
    color: var(--accent);
    font-weight: 600;
}

tr:last-child td {
    border-bottom: none;
}

tr:hover {
    background: var(--bg-tertiary);
}

.chart {
    background: var(--bg-secondary);
    border-radius: 8px;
    padding: 1rem;
}

.chart-row {
    display: flex;
    align-items: center;
    margin: 0.5rem 0;
    gap: 1rem;
}

.chart-row .label {
    min-width: 80px;
    color: var(--text-secondary);
}

.chart-row .bar {
    height: 24px;
    background: linear-gradient(90deg, var(--accent), var(--accent-secondary));
    border-radius: 4px;
    min-width: 4px;
}

.chart-row .value {
    min-width: 60px;
    text-align: right;
    font-weight: bold;
}

.comparison-highlight {
    color: var(--success);
}

.footer {
    margin-top: 3rem;
    padding-top: 1rem;
    border-top: 1px solid var(--bg-tertiary);
    color: var(--text-secondary);
    font-size: 0.85rem;
    text-align: center;
}
"#
    }
}

/// Export statistics to HTML format
pub fn export_html(stats: &ComparisonStats, path: &Path) -> Result<()> {
    let mut html = String::new();

    html.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n");
    html.push_str("  <meta charset=\"UTF-8\">\n");
    html.push_str("  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n");
    html.push_str("  <title>osu! Statistics Report</title>\n");
    html.push_str("  <style>");
    html.push_str(HtmlExport::css());
    html.push_str("</style>\n</head>\n<body>\n");
    html.push_str("<div class=\"container\">\n");

    // Header
    html.push_str("<h1>osu! Statistics Report</h1>\n");

    // Overview cards
    html.push_str("<div class=\"stats-grid\">\n");

    // Stable stats card
    html.push_str(&format!(
        "<div class=\"stat-card\">\n  <h3>osu!stable</h3>\n  <div class=\"stat-value\">{}</div>\n  <div class=\"stat-detail\">beatmap sets ({} beatmaps)</div>\n  <div class=\"stat-detail\">Storage: {}</div>\n</div>\n",
        stats.stable.total_beatmap_sets,
        stats.stable.total_beatmaps,
        stats.stable.storage_display()
    ));

    // Lazer stats card
    html.push_str(&format!(
        "<div class=\"stat-card\">\n  <h3>osu!lazer</h3>\n  <div class=\"stat-value\">{}</div>\n  <div class=\"stat-detail\">beatmap sets ({} beatmaps)</div>\n  <div class=\"stat-detail\">Storage: {}</div>\n</div>\n",
        stats.lazer.total_beatmap_sets,
        stats.lazer.total_beatmaps,
        stats.lazer.storage_display()
    ));

    // Comparison card
    html.push_str(&format!(
        "<div class=\"stat-card\">\n  <h3>Comparison</h3>\n  <div class=\"stat-value\">{}</div>\n  <div class=\"stat-detail\">common beatmap sets</div>\n  <div class=\"stat-detail\">Unique to stable: {} | Unique to lazer: {}</div>\n</div>\n",
        stats.common_beatmaps,
        stats.unique_to_stable,
        stats.unique_to_lazer
    ));

    // Duplicates card
    html.push_str(&format!(
        "<div class=\"stat-card\">\n  <h3>Duplicates</h3>\n  <div class=\"stat-value\">{}</div>\n  <div class=\"stat-detail\">duplicate sets detected</div>\n  <div class=\"stat-detail\">Wasted space: {}</div>\n</div>\n",
        stats.duplicates.count,
        stats.duplicates.wasted_display()
    ));

    html.push_str("</div>\n");

    // Mode breakdown section
    html.push_str("<h2>Game Mode Breakdown</h2>\n");
    html.push_str("<table>\n  <thead>\n    <tr>\n      <th>Mode</th>\n      <th>Stable</th>\n      <th>Lazer</th>\n    </tr>\n  </thead>\n  <tbody>\n");

    let modes = [
        (crate::beatmap::GameMode::Osu, "osu!"),
        (crate::beatmap::GameMode::Taiko, "Taiko"),
        (crate::beatmap::GameMode::Catch, "Catch"),
        (crate::beatmap::GameMode::Mania, "Mania"),
    ];

    for (mode, name) in modes {
        let stable_count = stats.stable.by_mode.get(&mode).unwrap_or(&0);
        let lazer_count = stats.lazer.by_mode.get(&mode).unwrap_or(&0);
        html.push_str(&format!(
            "    <tr>\n      <td>{}</td>\n      <td>{}</td>\n      <td>{}</td>\n    </tr>\n",
            name, stable_count, lazer_count
        ));
    }

    html.push_str("  </tbody>\n</table>\n");

    // Star rating distribution (if available)
    if !stats.stable.star_rating_distribution.is_empty() {
        html.push_str("<h2>Star Rating Distribution (Stable)</h2>\n");
        let chart_data: Vec<(String, usize)> = stats
            .stable
            .star_rating_distribution
            .iter()
            .map(|b| (format!("{:.0}-{:.0}★", b.min, b.max), b.count))
            .collect();
        html.push_str(&HtmlExport::bar_chart(&chart_data, 80));
    }

    // Footer - use std time instead of chrono
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| {
            let secs = d.as_secs();
            let days = secs / 86400;
            let (year, month, day) = super::model::days_to_ymd(days);
            let time_of_day = secs % 86400;
            let hours = time_of_day / 3600;
            let minutes = (time_of_day % 3600) / 60;
            let seconds = time_of_day % 60;
            format!(
                "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                year, month, day, hours, minutes, seconds
            )
        })
        .unwrap_or_else(|_| "Unknown".to_string());
    html.push_str(&format!(
        "<div class=\"footer\">Generated by osu-sync | {}</div>\n",
        now
    ));

    html.push_str("</div>\n</body>\n</html>");

    let mut file = File::create(path)?;
    file.write_all(html.as_bytes())?;

    Ok(())
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
            ..Default::default()
        }
    }

    #[test]
    fn test_export_format_extension() {
        assert_eq!(ExportFormat::Json.extension(), "json");
        assert_eq!(ExportFormat::Csv.extension(), "csv");
        assert_eq!(ExportFormat::Html.extension(), "html");
    }

    #[test]
    fn test_export_format_display() {
        assert_eq!(format!("{}", ExportFormat::Json), "JSON");
        assert_eq!(format!("{}", ExportFormat::Csv), "CSV");
        assert_eq!(format!("{}", ExportFormat::Html), "HTML");
    }
}
