# Architecture

WhatSpace is a Rust workspace in layers. A crate may use only crates in lower layers, never one
beside it or above it; `cargo xtask layers` checks this, and it is part of `cargo xtask ci`.

```
apps/whatspace       apps/whatspace-cli        (binaries; exempt from the rules)
        │                    │
crates/ui-egui (L2)          │                  egui front end: views, actions, the Host trait
        │                    │
crates/scan (L1) ────────────┘                  worker threads, progress, cancel, rescan
     │        │
crates/fs (L0)  crates/model (L0)               platform knowledge │ pure data
```

## `whatspace-fs` (L0): what the system says

Everything that differs between operating systems and file systems is here, so nothing above
has a `cfg(target_os)` that matters.

- `entry::Probe::file` measures one file from its metadata: logical size, allocated size,
  modification time, whether it is a cloud placeholder or stored compressed or sparse, and its
  identity (`FileId`) when it has several names. Unix uses `st_blocks`; Windows rounds to the
  cluster size and asks `GetCompressedFileSizeW` (through the `filesize` crate) for compressed,
  sparse and reparse-point files, and opens a file without read access (through `winapi-util`)
  only when hard links are being looked for.
- `volumes` lists the mounted volumes (through `sysinfo`), finds the volume a path is on, and
  reads the allocation unit (through `fs4`). It also names file systems the way people write them
  and says, per file system, what to know about its numbers.
- `mounts::Boundary` decides whether a scan goes into a directory on another device: Linux's
  mount table, Btrfs subvolumes that are not mounts, pseudo file systems, the macOS system and data
  volumes. Its rules take a `Platform`, so every platform's behaviour is tested on any machine.

## `whatspace-model` (L0): the data

- `Tree` is an arena of `Node`s addressed by `NodeId`. Every folder keeps the totals of
  everything beneath it (size, allocated, files, folders, newest modification, unreadable places).
  `add_children` adds a whole listing and updates every folder above in one pass, so a tree that
  is still being filled is always consistent and can be drawn. `remove` and `clear` take totals
  off again (moving to the trash, scanning a folder again).
- Names live in one string; a name that is not valid Unicode keeps its exact bytes on the side so
  `path_of` still finds the file.
- `stats` (extensions, largest files, ages), `treemap` (squarified layout, hit testing),
  `units` (sizes, counts, percentages), `time` (local dates), `export` (CSV, JSON), `category`
  (file types). All of it is pure and tested in memory.
- Nothing recurses over the tree: walks use explicit stacks, so a folder nested thousands deep
  can't overflow a thread's stack.

## `whatspace-scan` (L1): the scanner

`Scan::start` checks the folder, builds the boundary and probe for its volume, and starts worker
threads (4 to 16 by default). They share a queue of folders. A worker reads a folder's listing,
measures each entry, adds the listing to the tree under one short lock, and queues the
subfolders. Folders are taken newest first, so the walk goes deep before wide and the queue stays
small. Hard links are counted once through a shared set of `FileId`s.

A folder that can't be read is flagged and counted, and the scan goes on. `cancel` drains the
queue and the workers stop within a folder; the tree keeps what was read. A panic while reading
one folder is caught and costs only that folder. The last worker to stop records the time and
marks the scan done.

`Scan::rescan` clears one folder of the tree and scans it again into the same tree.

## `whatspace-ui-egui` (L2): the window

`App::ui` draws one frame: toolbar, status bar, and either the drive list or the tree with the
views beside it. It takes the tree's lock once for the frame's drawing and lets go before doing
anything the views asked for: views only push `Action`s (select, open a folder, rescan, trash,
export…), which run after the drawing. While a scan is running the window looks at the tree a
few times a second; what is worked out from the tree (sorted children, visible rows, the treemap
layout, statistics) is cached and worked out again only when the tree changed, and the
statistics, which walk everything, at most every few seconds during a scan.

Everything outside the window goes through the `Host` trait (folder and save dialogs, open,
reveal, trash). The desktop app implements it with `rfd`, the system's own programs and the
`trash` crate; the tests use a fake that records what it was asked.

Tables draw only the rows on screen (`ScrollArea::show_rows`) and paint each row's columns
directly, so a folder with a million files costs no more than one with ten.

## Apps

- `apps/whatspace`: the eframe window, the `Host`, the Windows GPU backend choice
  (`graphics.rs`), the icon and version resources (`build.rs`), and saving the preferences.
- `apps/whatspace-cli`: `volumes` and `scan` with text output and CSV/JSON export. It writes
  through a locked stdout and stops quietly when the pipe closes.

## Testing

- Unit tests in every crate; property tests feed arbitrary input to the mount-table parser and
  the treemap layout.
- `crates/scan/tests/scan.rs` scans real temporary folders: totals, hard links, symbolic links,
  sparse files, unreadable folders, depth limits, cancellation, rescans, odd names.
- `crates/ui-egui/src/tests.rs` drives the whole window without a screen (`egui_kittest`): the
  start page, every view in light and dark, selecting and opening, the trash confirmation, and a
  real scan through the window.
- `apps/whatspace-cli/tests/cli.rs` runs the command-line program.
