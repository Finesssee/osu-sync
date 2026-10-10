//! The linked-store step against small stable and lazer folders, and configs and
//! records that versions with junction modes left behind.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{TimeZone, Utc};
use osu_sync_core::config::Config;
use osu_sync_core::linkstore::BlobHash;
use osu_sync_core::unified::{
    legacy_notes, retired_mode_notice, LinkedStoreStatus, StepReport, UnifiedStorageEngine,
    UnifiedStorageMode,
};
use osu_sync_core::{
    BeatmapDifficulty, BeatmapMetadata, GameMode, LazerBeatmapInfo, LazerBeatmapSet, LazerNamedFile,
};
use sha2::{Digest, Sha256};

const OSU: &[u8] = b"osu file format v14\n\n[General]\nAudioFilename: audio.mp3\n";
const OSU_MD5: &str = "27ce3bbb14207aeb035570fd0f1c0b9f";
const OSU_NAME: &str = "Artist - Title (Mapper) [Easy].osu";
const AUDIO: &[u8] = b"ID3 not really audio, thirty-two";

struct Fixture {
    _dir: tempfile::TempDir,
    stable: PathBuf,
    lazer: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let stable = dir.path().join("stable");
        let lazer = dir.path().join("lazer");
        fs::create_dir_all(stable.join("Songs")).unwrap();
        fs::create_dir_all(lazer.join("files")).unwrap();
        Self {
            _dir: dir,
            stable,
            lazer,
        }
    }

    fn engine(&self) -> UnifiedStorageEngine {
        UnifiedStorageEngine::new(&self.stable, &self.lazer).relink_cache(None)
    }

    fn songs(&self) -> PathBuf {
        self.stable.join("Songs")
    }

    fn blob(&self, content: &[u8]) -> String {
        let hash = format!("{:x}", Sha256::digest(content));
        let path = BlobHash::parse(&hash)
            .unwrap()
            .path_in(&self.lazer.join("files"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, content).unwrap();
        hash
    }

    /// A lazer set with one `.osu` and an audio file.
    fn set(&self) -> LazerBeatmapSet {
        let osu = self.blob(OSU);
        let audio = self.blob(AUDIO);
        let metadata = BeatmapMetadata {
            artist: "Artist".to_string(),
            title: "Title".to_string(),
            ..Default::default()
        };
        LazerBeatmapSet {
            id: "1d0c5b0e-7a2f-4c11-9e3b-2f6a8d4c0b17".to_string(),
            online_id: Some(1001),
            beatmaps: vec![LazerBeatmapInfo {
                id: "b1".to_string(),
                online_id: None,
                hash: osu.clone(),
                md5_hash: OSU_MD5.to_string(),
                metadata,
                difficulty: BeatmapDifficulty::default(),
                version: "Easy".to_string(),
                mode: GameMode::Osu,
                length_ms: 0,
                bpm: 0.0,
                star_rating: None,
                ranked_status: None,
            }],
            files: vec![
                LazerNamedFile {
                    filename: OSU_NAME.to_string(),
                    hash: osu,
                },
                LazerNamedFile {
                    filename: "audio.mp3".to_string(),
                    hash: audio,
                },
            ],
            date_added: Utc.with_ymd_and_hms(2024, 3, 5, 7, 7, 9).unwrap(),
        }
    }
}

/// Sets, sets written, sets complete, files linked, files copied, relinked, errors.
fn counts(report: &StepReport) -> [usize; 7] {
    [
        report.lazer_sets,
        report.sets_written,
        report.sets_complete,
        report.files_linked,
        report.files_copied,
        report.relinked,
        report.errors.len(),
    ]
}

#[test]
fn setup_links_a_lazer_set_and_a_rerun_changes_nothing() {
    let fx = Fixture::new();
    let sets = [fx.set()];
    let engine = fx.engine();

    let first = engine.sync_sets(&sets, &mut |_, _, _| {}).unwrap();
    assert_eq!(counts(&first), [1, 1, 0, 1, 1, 0, 0]);
    let folder = fx.songs().join("1001 Artist - Title");
    assert_eq!(fs::read(folder.join("audio.mp3")).unwrap(), AUDIO);
    assert_eq!(fs::read(folder.join(OSU_NAME)).unwrap(), OSU);

    let second = engine.sync_sets(&sets, &mut |_, _, _| {}).unwrap();
    assert_eq!(counts(&second), [1, 0, 1, 0, 0, 0, 0]);
    assert_eq!(second.changed_files(), 0);

    assert_eq!(
        engine.status().unwrap(),
        LinkedStoreStatus {
            linked_files: 1,
            copied_files: 1,
            bytes_saved: 32,
            unreadable_files: 0,
        }
    );
}

#[test]
fn a_stable_copy_of_a_lazer_blob_becomes_a_link() {
    let fx = Fixture::new();
    fx.blob(AUDIO);
    let folder = fx.songs().join("2 Other - Song");
    fs::create_dir_all(&folder).unwrap();
    fs::write(folder.join("song.mp3"), AUDIO).unwrap();

    let report = fx.engine().sync_sets(&[], &mut |_, _, _| {}).unwrap();
    assert_eq!(counts(&report), [0, 0, 0, 0, 0, 1, 0]);
    assert_eq!(report.bytes_reclaimed, 32);
    assert_eq!(
        fx.engine().status().unwrap(),
        LinkedStoreStatus {
            linked_files: 1,
            copied_files: 0,
            bytes_saved: 32,
            unreadable_files: 0,
        }
    );
}

/// Makes `link` a directory junction to `target` with the system's own tool.
#[cfg(windows)]
fn junction(link: &Path, target: &Path) {
    let status = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(link)
        .arg(target)
        .stdout(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
}

#[cfg(windows)]
#[test]
fn a_junctioned_songs_folder_is_refused_and_left_alone() {
    let fx = Fixture::new();
    let sets = [fx.set()];
    let shared = fx.stable.parent().unwrap().join("shared-songs");
    fs::create_dir_all(&shared).unwrap();
    let songs = fx.songs();
    fs::remove_dir(&songs).unwrap();
    junction(&songs, &shared);

    let message = fx
        .engine()
        .sync_sets(&sets, &mut |_, _, _| {})
        .unwrap_err()
        .to_string();
    assert_eq!(
        message,
        format!(
            "Unified storage error: the stable Songs folder {} is a junction or symbolic link, \
             which an older unified storage mode makes. The linked store needs the real folder \
             there. Restore it, then run setup again.",
            songs.display()
        )
    );
    assert!(fx.engine().status().is_err());
    assert_eq!(fs::read_dir(&shared).unwrap().count(), 0);
    assert!(fs::symlink_metadata(&songs)
        .unwrap()
        .file_type()
        .is_symlink());
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("legacy")
        .join(name)
}

#[test]
fn configs_with_junction_modes_load_as_disabled() {
    for (file, mode) in [
        ("config-stable-master.json", "StableMaster"),
        ("config-lazer-master.json", "LazerMaster"),
        ("config-true-unified.json", "TrueUnified"),
    ] {
        let text = fs::read_to_string(fixture(file)).unwrap();
        let config: Config = serde_json::from_str(&text).unwrap();
        let unified = config.unified_storage.unwrap();
        assert_eq!(unified.mode, UnifiedStorageMode::Disabled);
        assert_eq!(unified.retired_mode.as_deref(), Some(mode));
        assert_eq!(
            unified.retired_mode_notice(),
            Some(retired_mode_notice(mode))
        );
        assert_eq!(unified.triggers.watcher_interval_secs, 5);
        assert_eq!(config.stable_path.as_deref(), Some(Path::new(r"D:\osu!")));
    }
}

#[test]
fn an_old_migration_record_is_noted_and_kept() {
    let fx = Fixture::new();
    let record = fx.stable.join(".osu-sync-migration.json");
    fs::copy(fixture("osu-sync-migration.json"), &record).unwrap();
    let before = fs::read(&record).unwrap();

    let report = fx.engine().sync_sets(&[], &mut |_, _, _| {}).unwrap();
    assert_eq!(report.notes, Vec::<String>::new());
    let config = Config {
        stable_path: Some(fx.stable.clone()),
        ..Config::default()
    };
    let notes = legacy_notes(&config);
    let prefix = format!(
        "{} is the record of unified storage mode \"LazerMaster\"",
        record.display()
    );
    assert!(notes.iter().any(|n| n.starts_with(&prefix)), "{notes:?}");
    assert_eq!(fs::read(&record).unwrap(), before);
}
