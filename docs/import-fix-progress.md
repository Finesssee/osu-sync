# Lazer Import Fix - Progress Documentation

**Date:** 2025-12-29  
**Status:** ✅ IMPLEMENTED (commit 4f8d8b5)

---

## Problem Summary

When syncing beatmaps from osu!stable to osu!lazer, the import process fails for files with special characters in their names (e.g., `!`, `[]`, `&`).

### Symptoms
- Windows shows error: "Windows cannot find 'filename.osz'. Make sure you typed the name correctly."
- Import works for normal filenames but fails for ~10-20% of beatmaps with special characters
- Example failing filename: `1018152 kyaru - Namaiwayo!.osz`

### Root Cause
The Rust `Command` API passes arguments that get misinterpreted by Windows when filenames contain special characters. The `!` character is particularly problematic as it has special meaning in Windows command processing.

---

## Session Accomplishments

### 1. Beatmap Selection Bug Fix
- **Issue:** Selecting beatmaps in dry run preview ignored unranked/unsubmitted maps (no `set_id`)
- **Fix:** Added `folder_name` field as fallback identifier
- **Files:** `dry_run.rs`, `engine.rs`, `app.rs`, `worker.rs`

### 2. Filter Mode Keyboard Bug Fix
- **Issue:** Pressing 'h' while typing in search/filter box triggered help screen
- **Fix:** Added `in_filter_mode` check before help handler
- **File:** `app.rs`

### 3. Lazer Import - Temporary PowerShell Fix
- **Issue:** Native Rust import fails for special character filenames
- **Workaround:** PowerShell script using .NET `ProcessStartInfo` with proper quoting
- **Result:** Successfully importing 5600+ beatmaps

---

## PowerShell Workaround (Current)

```powershell
$lazer = 'C:\Users\FSOS\AppData\Local\osulazer\current\osu!.exe'

foreach ($f in $files) { 
    # Use .NET Process to handle special characters properly
    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $lazer
    $psi.Arguments = "`"$($f.FullName)`""
    $psi.WindowStyle = 'Hidden'
    $psi.UseShellExecute = $true
    [System.Diagnostics.Process]::Start($psi) | Out-Null
}
```

**Why it works:** .NET's `ProcessStartInfo` properly handles the quoting of arguments with special characters.

---

## Native Rust Fix - Implementation Plan

### File: `crates/osu-sync-core/src/lazer/importer.rs`

### Change 1: Add Windows-specific imports

**Location:** After line 15

```rust
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;
```

### Change 2: Fix `trigger_single_import()`

**Location:** Replace lines 219-258

```rust
/// Trigger lazer to import a single .osz file
fn trigger_single_import(&self, osz_path: &Path) -> bool {
    if let Some(ref lazer_exe) = self.lazer_exe {
        #[cfg(target_os = "windows")]
        {
            // On Windows, use raw_arg with quoted path to handle special characters
            let quoted_path = format!("\"{}\"", osz_path.display());
            match Command::new(lazer_exe)
                .raw_arg(&quoted_path)
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
            {
                Ok(_) => {
                    tracing::debug!("Lazer import triggered for: {}", osz_path.display());
                    return true;
                }
                Err(e) => {
                    tracing::warn!("Failed to launch lazer for import: {}", e);
                }
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            // On Linux/macOS, standard arg passing works fine
            match Command::new(lazer_exe).arg(osz_path).spawn() {
                Ok(_) => {
                    tracing::debug!("Lazer import triggered for: {}", osz_path.display());
                    return true;
                }
                Err(e) => {
                    tracing::warn!("Failed to launch lazer for import: {}", e);
                }
            }
        }
    }
    false
}
```

### Change 3: Fix `trigger_batch_import()`

**Location:** Replace lines 264-330

```rust
/// Trigger lazer to process all pending imports
///
/// On Windows: Launches lazer once per file with proper quoting to handle special characters.
/// On Linux/macOS: Launches lazer with batch of files as arguments.
pub fn trigger_batch_import(&self) -> Result<bool> {
    if self.pending_imports.is_empty() {
        return Ok(false);
    }

    let Some(ref lazer_exe) = self.lazer_exe else {
        tracing::warn!(
            "Lazer executable not found. {} .osz files are waiting in: {}",
            self.pending_imports.len(),
            self.import_path.display()
        );
        tracing::warn!("Please start osu!lazer manually to import them.");
        return Ok(false);
    };

    let total = self.pending_imports.len();
    tracing::info!("Triggering lazer to import {} beatmaps", total);

    #[cfg(target_os = "windows")]
    {
        // On Windows, launch each file individually to handle special characters
        // This is slower but reliable for filenames with !, [], etc.
        let mut success_count = 0;
        let mut fail_count = 0;

        for (i, osz_path) in self.pending_imports.iter().enumerate() {
            let quoted_path = format!("\"{}\"", osz_path.display());
            
            match Command::new(lazer_exe)
                .raw_arg(&quoted_path)
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
            {
                Ok(_) => {
                    success_count += 1;
                }
                Err(e) => {
                    tracing::warn!("Failed to import {}: {}", osz_path.display(), e);
                    fail_count += 1;
                }
            }

            // Progress logging every 50 files
            if (i + 1) % 50 == 0 {
                tracing::info!("Import progress: {}/{} files sent to lazer", i + 1, total);
            }

            // Small delay to not overwhelm lazer (100ms between each)
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        if fail_count > 0 {
            tracing::warn!(
                "Import complete: {} succeeded, {} failed",
                success_count, fail_count
            );
        } else {
            tracing::info!("All {} files sent to lazer for import", success_count);
        }

        return Ok(success_count > 0);
    }

    #[cfg(not(target_os = "windows"))]
    {
        // On Linux/macOS, batch approach works fine
        const BATCH_SIZE: usize = 50;
        
        let batches: Vec<_> = self.pending_imports.chunks(BATCH_SIZE).collect();
        let batch_count = batches.len();
        
        if batch_count > 1 {
            tracing::info!(
                "Splitting into {} batches of up to {} files each",
                batch_count, BATCH_SIZE
            );
        }

        // Launch lazer for each batch
        for (batch_idx, batch) in batches.iter().enumerate() {
            match Command::new(lazer_exe)
                .args(batch.iter().map(|p| p.as_os_str()))
                .spawn()
            {
                Ok(_) => {
                    tracing::info!(
                        "Batch {}/{}: {} files sent to lazer",
                        batch_idx + 1, batch_count, batch.len()
                    );
                }
                Err(e) => {
                    tracing::warn!("Batch {}/{} failed: {}", batch_idx + 1, batch_count, e);
                }
            }

            // Wait between batches
            if batch_idx < batch_count - 1 {
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
        }

        return Ok(true);
    }
}
```

---

## Why This Fix Works

| Technique | Purpose |
|-----------|---------|
| `raw_arg()` | Passes argument without shell escaping, giving us control over exact string |
| Quoted path `"..."` | Properly interpreted by Windows even with special characters |
| `CREATE_NO_WINDOW` | Prevents console windows from flashing for each import |
| Individual launches | Avoids command line length limits and batching issues |
| 100ms delay | Gives lazer time to queue each import without overwhelming it |

---

## Files Modified This Session

| File | Changes |
|------|---------|
| `core/src/sync/dry_run.rs` | Added `folder_name` field |
| `core/src/sync/engine.rs` | Added `selected_folders` filtering logic |
| `core/src/lazer/importer.rs` | Windows special character fix with `raw_arg()` |
| `cli/src/app.rs` | Filter mode fix, selection extraction |
| `cli/src/worker.rs` | Pass `selected_folders` to sync |

---

## Commands

```bash
# Check import folder status
powershell -Command "(Get-ChildItem 'D:\osu!lazer\import\*.osz').Count"

# Run PowerShell import script
powershell -ExecutionPolicy Bypass -File "D:\code\osu-sync\import_to_lazer.ps1"

# Build osu-sync
cargo build --release -p osu-sync-cli

# Run tests
cargo test -p osu-sync-core
```

---

## Implementation Complete

Native Rust fix has been implemented in commit `4f8d8b5`.

### Testing Checklist

- [x] Build: `cargo build --release -p osu-sync-cli`
- [ ] Test with normal filename: `123456 Artist - Title.osz`
- [ ] Test with `!` in filename: `123456 Artist - Song!.osz`
- [ ] Test with `[]` in filename: `123456 Artist - Song [TV Size].osz`
- [ ] Test with `&` in filename: `123456 A & B - Song.osz`
- [ ] Test batch import of 100+ files
- [ ] Verify no error dialogs appear
- [ ] Verify beatmaps appear in lazer song select
