# realm-export

osu!lazer stores its library in `client.realm`, a Realm file in format 24 that no Rust crate can read.
`realm-export` reads it through the `Realm.dll` that ships with osu!lazer and prints the beatmap sets as JSON.
osu-sync runs it from `LazerDatabase::open`.

## Requirements

- The .NET 8 runtime: https://dotnet.microsoft.com/download/dotnet/8.0
- An osu!lazer install. The helper loads `Realm.dll`, `MongoDB.Bson.dll`, `Remotion.Linq.dll` and
  `realm-wrappers.dll` from `%LOCALAPPDATA%\osulazer\current`, or from the folder given with `--lazer-dir`.

## Build

The .NET 8 SDK builds it. Cargo does not.

```
dotnet publish tools/realm-export -c Release -o target/release
```

This puts `realm-export.exe` next to `osu-sync.exe`, which is where osu-sync looks first.
To keep it elsewhere, set `OSU_SYNC_REALM_EXPORT` to the full path of `realm-export.exe` or `realm-export.dll`.

## Commands

```
realm-export export <client.realm> [--out <file>] [--lazer-dir <dir>]
realm-export trim <client.realm> --keep <ids-file> [--lazer-dir <dir>]
realm-export mark-delete-pending <client.realm> --id <set-id> [--lazer-dir <dir>]
```

- `export` copies `client.realm` to a temp folder, opens the copy read-only, writes JSON to stdout or `--out`,
  then deletes the temp folder. The file it was given is never opened by Realm.
  Sets are sorted by online ID then set ID, beatmaps the same way, and files by filename, so two exports of
  one realm are byte-identical.
- `trim` deletes every set whose ID is not listed in `<ids-file>` (one set GUID per line), with its beatmaps,
  metadata and scores, then compacts the file.
- `mark-delete-pending` sets `DeletePending` on one set.

`trim` and `mark-delete-pending` write to the realm in place, so they refuse any path outside
`D:\osu-sync-sandbox`.

Exit codes: 0 success, 1 failure, 2 usage, 3 missing lazer assembly, 4 refused path.
