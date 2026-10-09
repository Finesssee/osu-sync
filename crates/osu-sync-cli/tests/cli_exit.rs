//! Runs the real `osu-sync` binary on a temp stable folder and a temp lazer store,
//! with a stand-in realm-export helper that prints a fixed library.

use std::fs;
use std::path::Path;
use std::process::Command;

const OSU: &str = "osu file format v14\n\n[General]\nAudioFilename: audio.mp3\n";
const OSU_SHA256: &str = "d8da81b65826dbd581118a638920d8210cdf4dda6f592b163d2a90e318a11565";
const OSU_MD5: &str = "27ce3bbb14207aeb035570fd0f1c0b9f";
/// audio.mp3 is listed in realm but its blob is not in the store.
const MISSING: &str = "cdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcdcd";

fn export_json() -> String {
    format!(
        r#"{{"sets":[{{"id":"1d0c5b0e-0000-0000-0000-000000000000","online_id":1001,"protected":false,"delete_pending":false,"date_added":"2024-01-02T03:04:05+00:00","artist":"Artist","title":"Title","creator":"Mapper","beatmaps":[{{"id":"b1","online_id":0,"hash":"{OSU_SHA256}","md5_hash":"{OSU_MD5}","difficulty_name":"Easy","ruleset":0,"length_ms":null,"bpm":null,"star_rating":null,"status":1,"hidden":false,"title":"Title","title_unicode":null,"artist":"Artist","artist_unicode":null,"author":"Mapper","source":null,"tags":null,"drain_rate":null,"circle_size":4,"overall_difficulty":null,"approach_rate":null,"slider_multiplier":null,"slider_tick_rate":null}}],"files":[{{"filename":"Artist - Title (Mapper) [Easy].osu","hash":"{OSU_SHA256}"}},{{"filename":"audio.mp3","hash":"{MISSING}"}}]}}],"skipped":null}}"#
    )
}

/// Writes a helper that ignores its arguments and prints `export.json` next to it.
fn write_helper(dir: &Path) -> std::path::PathBuf {
    fs::write(dir.join("export.json"), export_json()).unwrap();
    #[cfg(windows)]
    {
        let helper = dir.join("realm-export.cmd");
        fs::write(&helper, "@type \"%~dp0export.json\"\r\n").unwrap();
        helper
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let helper = dir.join("realm-export");
        fs::write(
            &helper,
            "#!/bin/sh\ncat \"$(dirname \"$0\")/export.json\"\n",
        )
        .unwrap();
        fs::set_permissions(&helper, fs::Permissions::from_mode(0o755)).unwrap();
        helper
    }
}

#[test]
fn sync_with_a_failed_set_exits_nonzero() {
    let dir = tempfile::tempdir().unwrap();
    let stable = dir.path().join("stable");
    let lazer = dir.path().join("lazer");
    fs::create_dir_all(stable.join("Songs")).unwrap();
    let blob = lazer
        .join("files")
        .join(&OSU_SHA256[..1])
        .join(&OSU_SHA256[..2])
        .join(OSU_SHA256);
    fs::create_dir_all(blob.parent().unwrap()).unwrap();
    fs::write(&blob, OSU).unwrap();
    fs::write(lazer.join("client.realm"), b"").unwrap();
    let helper = write_helper(dir.path());

    let output = Command::new(env!("CARGO_BIN_EXE_osu-sync"))
        .arg("--stable-path")
        .arg(&stable)
        .arg("--lazer-path")
        .arg(&lazer)
        .args(["--cli", "sync", "l2s", "--json"])
        .env("OSU_SYNC_REALM_EXPORT", &helper)
        .env_remove("OSU_SYNC_LAZER_DIR")
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let json = &stdout[stdout.find('{').unwrap_or(0)..];
    let result: serde_json::Value = serde_json::from_str(json).unwrap_or_else(|e| {
        panic!("stdout is not the sync JSON ({e}): {stdout}\nstderr: {stderr}")
    });
    assert_eq!(result["failed"], 1, "stdout: {stdout}\nstderr: {stderr}");
    assert_eq!(
        result["errors"][0]["message"],
        "audio.mp3 is missing from the lazer store or cannot be read"
    );
    assert!(
        stderr.contains("Error: 1 beatmap sets failed"),
        "stderr: {stderr}"
    );
    assert_eq!(output.status.code(), Some(1));
    assert!(!stable.join("Songs").join("1001 Artist - Title").exists());
}
