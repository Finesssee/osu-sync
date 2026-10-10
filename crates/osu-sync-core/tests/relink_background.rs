//! Relink must leave the process priority alone, so this test has a binary of its own.
#![cfg(windows)]

use std::fs;

use osu_sync_core::linkstore::{BlobHash, Relinker};
use sha2::{Digest, Sha256};
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentThread, GetPriorityClass, GetProcessInformation,
    GetThreadPriority, ProcessMemoryPriority, SetPriorityClass, IDLE_PRIORITY_CLASS,
    MEMORY_PRIORITY_INFORMATION, NORMAL_PRIORITY_CLASS,
};

fn priorities() -> (u32, u32, i32) {
    let mut info = MEMORY_PRIORITY_INFORMATION::default();
    // SAFETY: `info` is a valid out pointer of the size passed.
    unsafe {
        GetProcessInformation(
            GetCurrentProcess(),
            ProcessMemoryPriority,
            (&mut info as *mut MEMORY_PRIORITY_INFORMATION).cast(),
            std::mem::size_of::<MEMORY_PRIORITY_INFORMATION>() as u32,
        )
    }
    .unwrap();
    // SAFETY: pseudo handles of the current process and thread are always valid.
    let (class, thread) = unsafe {
        (
            GetPriorityClass(GetCurrentProcess()),
            GetThreadPriority(GetCurrentThread()),
        )
    };
    (class, info.MemoryPriority.0, thread)
}

#[test]
fn relink_leaves_the_process_priority_alone() {
    let dir = tempfile::tempdir().unwrap();
    let content = b"ID3 not really audio";
    let files = dir.path().join("lazer").join("files");
    let blob = BlobHash::parse(&format!("{:x}", Sha256::digest(content)))
        .unwrap()
        .path_in(&files);
    fs::create_dir_all(blob.parent().unwrap()).unwrap();
    fs::write(&blob, content).unwrap();
    let songs = dir.path().join("stable").join("Songs");
    fs::create_dir_all(songs.join("1 A - B")).unwrap();
    fs::write(songs.join("1 A - B").join("audio.mp3"), content).unwrap();

    // SAFETY: the pseudo handle of the current process is always valid.
    unsafe { SetPriorityClass(GetCurrentProcess(), IDLE_PRIORITY_CLASS) }.unwrap();
    let before = priorities();
    let mut during = Vec::new();
    let report = Relinker::new(&songs, &files, None)
        .run(&mut |_, _| during.push(priorities()))
        .unwrap();
    let after = priorities();
    // SAFETY: as above.
    unsafe { SetPriorityClass(GetCurrentProcess(), NORMAL_PRIORITY_CLASS) }.unwrap();

    assert_eq!(report.relinked, 1);
    assert_eq!(
        (before, during, after),
        ((64, 5, 0), vec![(64, 5, -1)], (64, 5, 0))
    );
}
