//! Restoring a Songs backup over a Songs folder whose assets are hard links to lazer's
//! blobs replaces the Songs names and leaves the blobs as they were.

use std::fs;
use std::path::{Path, PathBuf};

use osu_sync_core::backup::{create_backup_archive, BackupManager, BackupTarget, RestoreOptions};

struct Linked {
    _dir: tempfile::TempDir,
    root: PathBuf,
    blob: PathBuf,
    songs: PathBuf,
    zip: PathBuf,
}

/// A lazer blob linked into Songs, and a Songs backup holding other bytes under the
/// same name.
fn linked() -> Linked {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let store = root.join("lazer").join("files").join("a").join("ab");
    fs::create_dir_all(&store).unwrap();
    let blob = store.join("abcdef");
    fs::write(&blob, b"LAZER-ORIGINAL-AUDIO").unwrap();

    let songs = root.join("stable").join("Songs");
    fs::create_dir_all(songs.join("1 A - B")).unwrap();
    fs::hard_link(&blob, songs.join("1 A - B").join("audio.mp3")).unwrap();

    let other = root.join("other");
    fs::create_dir_all(other.join("1 A - B")).unwrap();
    fs::write(other.join("1 A - B").join("audio.mp3"), b"OLD").unwrap();
    let zip = root.join("songs_backup.zip");
    create_backup_archive(&other, &zip, BackupTarget::StableSongs, None).unwrap();

    Linked {
        _dir: dir,
        root,
        blob,
        songs,
        zip,
    }
}

fn names(folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(folder)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn assert_restored_without_touching_the_blob(l: &Linked) {
    assert_eq!(fs::read(&l.blob).unwrap(), b"LAZER-ORIGINAL-AUDIO");
    assert_eq!(
        fs::read(l.songs.join("1 A - B").join("audio.mp3")).unwrap(),
        b"OLD"
    );
    assert_eq!(names(&l.songs.join("1 A - B")), ["audio.mp3"]);
}

#[test]
fn restoring_over_a_linked_songs_file_keeps_lazers_blob() {
    let l = linked();
    BackupManager::new(l.root.join("bk"))
        .restore_backup(&l.zip, &l.songs)
        .unwrap();
    assert_restored_without_touching_the_blob(&l);
}

#[test]
fn restoring_selected_files_over_a_linked_songs_file_keeps_lazers_blob() {
    let l = linked();
    let restored = BackupManager::new(l.root.join("bk"))
        .restore_backup_with_options(&l.zip, &l.songs, &RestoreOptions::all(), None)
        .unwrap();
    assert_eq!(restored, 1);
    assert_restored_without_touching_the_blob(&l);
}
