# Unified storage

Unified storage lets osu!stable and osu!lazer share one copy of each beatmap file.
osu-sync builds stable's `Songs` folder out of hard links into lazer's `files` store.
This is the linked store, and it is the only unified storage mode. The other choice
is disabled.

## Requirements

- Songs and lazer's `files` folder must be on the same NTFS volume. Hard links cannot
  cross volumes. On different volumes, setup still writes missing sets, but as plain
  copies, and relinking skips every file.
- osu!stable must be closed while the step runs. Stable rewrites Songs and `osu!.db`
  while it is open, so the step refuses to start and the watcher waits.
- Songs and `files` must be real folders. If an older osu-sync version replaced either
  with a junction or symbolic link, the step refuses to run and changes nothing.

## What the step does

Setup, `unified watch` and "sync now" all run the same step, and a rerun with nothing
new changes nothing.

1. Read lazer's library through the realm export helper.
2. Write every lazer set stable lacks into Songs as `{OnlineID} Artist - Title`.
   Audio, images, video and storyboard assets become hard links to lazer's blobs.
   `.osu` and `.osb` files are copied, because stable rewrites them in place.
3. Relink: stable files that are byte-for-byte copies of a lazer blob become hard
   links to that blob. `.osu`, `.osb` and other files stable writes stay copies.

The step never changes lazer's store, lazer's realm or `osu!.db`.

## What each game sees

- osu!stable sees normal folders in Songs. New sets show up once stable reloads
  Songs. Editing a `.osu` file changes only stable's copy.
- osu!lazer sees no change. Its blobs keep their names and content. A linked blob just
  has a second name in Songs.
- Deleting a set in either game removes only that game's name for the files. The data
  stays on disk while the other game still links to it.

## Commands

```bash
osu-sync --cli unified setup     # Run the step once, then save the linked-store mode
osu-sync --cli unified status    # Count linked and copied files in Songs
osu-sync --cli unified watch     # Run the step when lazer's realm or Songs changes
osu-sync --cli unified disable   # Save the disabled mode; changes no files
```

`--threads <n>` sets relink threads for setup and watch. `--json` prints JSON.
With `--stable-path` or `--lazer-path` set, the mode is not saved to the config file.

In the TUI, the Unified Storage screen offers the linked store and disabled. Its status
screen shows linked files, copied files and bytes saved.

## Status

Status reads the NTFS link count of every file in Songs. There is no manifest.

- Linked files have two or more links.
- Copied files have one link: `.osu` and `.osb` files, stable-only sets and fallbacks.
- Bytes saved is the total size of linked files, which are stored once instead of twice.

A link count of two or more shows that the file shares its data with another name. It
does not prove the other name is in lazer's store.

## Watcher

`unified watch` runs one catch-up step, then watches Songs recursively and the folder
that holds lazer's `client.realm`. Lazer commits a new set to the realm after writing its
blobs, so the watcher follows the realm, not `files`. After a change, the step runs once
changes stop for a quiet second, and at most `watcher_interval_secs` (default 5) after
the first change.

The watcher ignores temp files and realm events without a new realm commit. It drops
changes that arrive while a step runs, and treats Songs changes in the two seconds after
a step as the step's own writes. Each step stamps the realm's modified time and size right
before it reads the realm, so a commit made during a step still gets its own step. While
osu!stable runs, the watcher logs why it waits and retries after the interval. The
watcher runs only from the CLI; the TUI has no watcher.

## Disable

Disable saves the disabled mode and touches no files. Linked files stay readable from
both games. To stop sharing a file's data, delete one of its names.

## Upgrading from the junction modes

Older versions had StableMaster, LazerMaster and TrueUnified modes, which moved folders
and made junctions. Configs with those modes, or a mode osu-sync does not know, load as
disabled with a notice. The junctions and the records those versions wrote
(`.osu-sync-migration.json` in the stable folder and `unified-manifest.json` in the
osu-sync config folder) are left in place, and status and setup name them. Sharing
skins, replays and screenshots ended with those modes.
