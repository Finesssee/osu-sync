//! Records older osu-sync versions left behind.
//!
//! The junction modes wrote `.osu-sync-migration.json` into the stable folder when
//! they moved folders and made junctions, and `unified-manifest.json` into the
//! osu-sync config folder. The linked store keeps no record of its own, since its
//! state is the link count of each file. It reads an old record only to tell the
//! user about it, and never changes or removes it.

use std::path::Path;

use crate::config::Config;

/// Notes about what older versions left behind: a retired mode in `config`, the
/// manifest in the osu-sync config folder, and the record in the stable folder.
pub fn legacy_notes(config: &Config) -> Vec<String> {
    let manifest = dirs::config_dir().map(|p| p.join("osu-sync").join("unified-manifest.json"));
    let record = config
        .stable_path
        .as_ref()
        .map(|p| p.join(".osu-sync-migration.json"));
    let mut notes: Vec<String> = config.unified().retired_mode_notice().into_iter().collect();
    notes.extend(manifest.and_then(|p| legacy_note(&p, Legacy::Manifest)));
    notes.extend(record.and_then(|p| legacy_note(&p, Legacy::Record)));
    notes
}

/// The kinds of file the junction modes wrote.
#[derive(Clone, Copy)]
enum Legacy {
    /// `unified-manifest.json`, listing shared folders under `resources`.
    Manifest,
    /// `.osu-sync-migration.json`, listing junctions under `created_links`.
    Record,
}

/// A note about the file at `path` an older version wrote, or `None` when there is
/// none. A file that cannot be read gets a note too, never an error.
fn legacy_note(path: &Path, kind: Legacy) -> Option<String> {
    let (noun, key, made) = match kind {
        Legacy::Manifest => ("manifest", "resources", "shared {n} folder(s)"),
        Legacy::Record => ("record", "created_links", "made {n} junction(s)"),
    };
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(e) => {
            return Some(format!(
                "{} is from an older osu-sync version and could not be read ({e}). It was left in place.",
                path.display()
            ))
        }
    };
    let record: serde_json::Value = match serde_json::from_str(&text) {
        Ok(record) => record,
        Err(e) => {
            return Some(format!(
                "{} is from an older osu-sync version and is not valid JSON ({e}). It was left in place.",
                path.display()
            ))
        }
    };
    let mode = record["mode"].as_str().unwrap_or("unknown");
    let n = record[key].as_array().map_or(0, Vec::len);
    Some(format!(
        "{} is the {noun} of unified storage mode \"{mode}\", which an older osu-sync version \
         used and this one removed. That mode {}. The {noun} and what it lists were left in place.",
        path.display(),
        made.replace("{n}", &n.to_string())
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// A file the junction modes wrote, captured from a version that had them.
    fn fixture(name: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join("legacy")
            .join(name)
    }

    #[test]
    fn no_file_means_no_note() {
        let dir = TempDir::new().unwrap();
        assert_eq!(
            legacy_note(&dir.path().join("none.json"), Legacy::Record),
            None
        );
    }

    #[test]
    fn an_old_record_is_named_with_its_mode_and_junctions() {
        let path = fixture("osu-sync-migration.json");
        assert_eq!(
            legacy_note(&path, Legacy::Record).unwrap(),
            format!(
                "{} is the record of unified storage mode \"LazerMaster\", which an older \
                 osu-sync version used and this one removed. That mode made 1 junction(s). The \
                 record and what it lists were left in place.",
                path.display()
            )
        );
    }

    #[test]
    fn an_old_manifest_is_named_with_its_mode_and_folders() {
        let note = legacy_note(&fixture("unified-manifest.json"), Legacy::Manifest).unwrap();
        assert!(
            note.contains("manifest of unified storage mode \"TrueUnified\""),
            "{note}"
        );
        assert!(note.contains("That mode shared 1 folder(s)."), "{note}");
    }

    #[test]
    fn a_broken_record_gets_a_note_not_an_error() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join(".osu-sync-migration.json");
        std::fs::write(&path, "{not json").unwrap();
        let note = legacy_note(&path, Legacy::Record).unwrap();
        assert!(note.contains("is not valid JSON"), "{note}");
        assert!(note.ends_with("It was left in place."), "{note}");
    }
}
