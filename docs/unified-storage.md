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
- Editing a linked Songs asset in place (an audio file, background or video) also
  changes lazer's copy, because both names point at the same data. To change an asset
  for stable only, save the edited file under a new name, then rename it over the old
  one. That replaces the Songs name and leaves lazer's blob as it was. Restoring a Songs
  backup with osu-sync works this way.

## Commands

```bash
osu-sync --cli unified setup     # Run the step once, then save the linked-store mode
osu-sync --cli unified status    # Count linked and copied files in Songs
osu-sync --cli unified watch     # Run the step when lazer's realm or Songs changes
osu-sync --cli unified disable   # Save the disabled mode; changes no files
```

`--threads <n>` sets relink threads for setup and watch. `--json` prints JSON.
With `--stable-path` or `--lazer-path` set, the mode is not saved to the config file.
`unified watch` runs whatever mode is saved, disabled included, and does not save a
mode. Each step it runs writes to Songs just as setup does.

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

`unified watch` runs one catch-up step, then watches Songs and lazer's `files` store
recursively, plus the folder that holds lazer's `client.realm`. Lazer writes a new set's
blobs into `files` as ordinary files, which raise change events. It then commits the set
to the realm through a memory map, which changes neither the realm's modified time nor
its size and raises no change event. Every commit rewrites the realm's 24-byte header, so
while idle the watcher reads the realm's size, modified time and header once per interval
on a fixed schedule that other events in lazer's folder do not delay (a shared read that
never locks or writes the file), and a new header runs the step. After a change, the step
runs once changes stop for a quiet second, and at most `watcher_interval_secs` (default 5)
after the first change. A step never starts sooner than one interval after the previous
step ended, so a long copy into Songs runs at most one step per interval.

The watcher ignores temp files and realm events without a new realm commit. A step's
own writes to Songs and `files` raise change events while it runs and in the two seconds
after it. The watcher cannot tell those from a change made by a game, so after any Songs
or `files` change in that window it runs one confirming step once the two seconds pass
and one interval has passed since the step ended.
That step finds nothing new when the changes were its own, writes nothing and raises no
events, so the watcher goes back to idle. Each step stamps the realm right before it
reads the realm, so a commit made during a step still gets its own step. While
osu!stable runs, the watcher logs why it waits and retries after the interval. A step
that fails for another reason does not mark the realm as seen, so the next realm read
retries it when the realm changed since the last step that succeeded. The watcher runs
only from the CLI; the TUI has no watcher.

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

osu-sync no longer removes those junctions. Setup and watch refuse to run while Songs
or lazer's `files` is one, and status refuses while Songs is one. Junctions on other
folders (`Skins`, `Replays`, `Screenshots`, `Exports`, `Backgrounds`) stay until you
remove them by hand. For each one, with both games closed:

1. Run `dir /AL` in the folder that holds it (the stable or lazer folder) to list the
   junctions and the folders they point to.
2. Remove the junction with `rmdir "<folder>"`. Without `/s`, `rmdir` removes only the
   link and leaves the folder it points to as it was.
3. Rename `<folder>_backup`, the copy the old mode kept next to the junction, back to
   `<folder>`. If you added files through the junction since then, copy them over from
   the folder it pointed to.

These are Command Prompt commands. Do not delete the files inside a junction, for
example with `del /s "<folder>\*"` or by selecting its contents in Explorer. Those are
the files of the folder it points to, which is the other game's copy or the shared copy.
