# realm-export

osu!lazer stores its library in `client.realm`, a Realm file in format 24 that no Rust crate can read.
`realm-export` reads it through the `Realm.dll` that ships with osu!lazer and prints the beatmap sets as JSON.
osu-sync runs it from `LazerDatabase::open`.

## Requirements

- The .NET 8 runtime: https://dotnet.microsoft.com/download/dotnet/8.0
- An osu!lazer install. The helper loads `Realm.dll`, `MongoDB.Bson.dll`, `Remotion.Linq.dll` and
  `realm-wrappers.dll` from `%LOCALAPPDATA%\osulazer\current`, or from the folder given with `--lazer-dir`.
  osu-sync passes `--lazer-dir` when the `OSU_SYNC_LAZER_DIR` environment variable is set, so set it to
  the install folder when osu!lazer is not in the default place.
- Windows. The helper is built for win-x64 and loads `realm-wrappers.dll`, so osu-sync reads lazer
  libraries only on Windows for now.

## Build

The .NET 8 SDK builds it. Cargo does not.

```
dotnet publish tools/realm-export -c Release -o target/release
```

This puts `realm-export.exe` next to `osu-sync.exe`.
`dotnet test tools/realm-export.Tests` runs the helper's unit tests; they do not need Realm or osu!lazer.
osu-sync looks for the helper in this order:

1. `OSU_SYNC_REALM_EXPORT`, the full path of `realm-export.exe` or `realm-export.dll`.
   If it is set but the file does not exist, osu-sync stops with an error naming the variable.
2. `realm-export.exe` or `realm-export.dll` next to the osu-sync executable.

Release builds from CI ship `realm-export.exe`, `.dll`, `.runtimeconfig.json` and `.deps.json`
next to `osu-sync.exe`.

## Commands

```
realm-export export <client.realm> [--out <file>] [--lazer-dir <dir>]
realm-export trim <client.realm> --keep <ids-file> [--lazer-dir <dir>]
realm-export mark-delete-pending <client.realm> --id <set-id> [--lazer-dir <dir>]
```

- `export` copies `client.realm` to a temp folder, opens the copy read-only, writes JSON to stdout or `--out`,
  then deletes the temp folder. The file it was given is never opened by Realm.
  A run that is killed before it finishes can leave a `%TEMP%\osu-sync-realm-export-*` folder holding a
  full copy of the realm. It is safe to delete. If the delete fails at the end of a normal run, the helper
  prints a warning to stderr and keeps the original result or error.
  `--out` refuses a path that ends in `.realm`, that is the input realm itself, or that names an alternate
  data stream (`file:stream`).
  A set that cannot be read or written is skipped, and one stderr warning gives the count and the first
  error. osu-sync logs that warning. If every set is skipped, `export` writes nothing and exits 1 with the
  count and the first error, so a systemic failure is not read as an empty library.
  Numbers that are infinite or NaN are written as `null`.
  Sets are sorted by online ID then set ID, beatmaps the same way, and files by filename, so two exports of
  one realm are byte-identical.
- `trim` deletes every set whose ID is not listed in `<ids-file>` (one set GUID per line), with its beatmaps,
  metadata and scores, then compacts the file.
- `mark-delete-pending` sets `DeletePending` on one set.

`trim` and `mark-delete-pending` write to the realm in place, so they refuse any path outside
`D:\osu-sync-sandbox`. They also refuse a realm that is a hard link, sits under a junction or symbolic
link, or has a `client.realm.lock`, `.management` or `.note` beside it that is a link.

## Limits

- Copying a realm while osu!lazer is writing it can give a torn snapshot. The export then fails or lists a
  library that is slightly out of date. Close osu!lazer for an exact read.
- The helper loads whatever `Realm.dll` the lazer install has. If its version does not match the one the
  helper was built against (Realm 20.1.0), `export` exits nonzero without writing anything: either the realm
  fails to open, or every set fails to read. If only some sets fail, `export` writes the rest and warns
  on stderr. `trim` can stop halfway and leave a sandbox realm partly trimmed. Rebuild the sandbox in that
  case.

Exit codes: 0 success, 1 failure, 2 usage (including an `--id` or `<ids-file>` line that is not a GUID),
3 missing lazer assembly, 4 refused path.
