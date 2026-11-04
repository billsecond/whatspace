# WhatSpace — instructions for agents

WhatSpace is a clean-room, open-source, Rust-native disk space analyzer that works the way TreeSize users expect: scan a drive or folder, see a tree sorted by size with share bars, a treemap, space by file type, the largest files and file ages, and act on what you find. It runs natively on Windows, macOS, Linux and FreeBSD. It follows the setup of its sibling `../MailCraft` (workspace layout, quality gates, packaging, release workflows) and the shared rules in [`storytold/craftrules`](https://github.com/storytold/craftrules).

## Start every session here
1. Read `plan/STATUS.md` (current milestone, next task) if it exists. `plan/` is gitignored (local only); if it's missing, start from `ROADMAP.md`.
2. Don't stop to ask unless a decision is genuinely the owner's (licensing, publishing, pushing to new remotes, secrets, anything that deletes or moves the owner's files).

## Never crash
A disk analyzer reads millions of files written by anybody; a crash loses a long scan and the owner's trust. **This outranks feature work.**
- **No panics in non-test code:** no `unwrap()`, `expect()`, `panic!`, `unreachable!`, `todo!`, `unimplemented!`; no `unsafe` (`unsafe_code = "forbid"`). Platform calls that need `unsafe` go through small audited crates (`filesize`, `winapi-util`, `sysinfo`, `fs4`, `trash`).
- **Errors are values.** A folder that can't be read is marked (`Flags::DENIED`, `Flags::ERROR`) and counted, and the scan goes on.
- **Everything on disk is hostile:** names that aren't Unicode, names with any character a file system allows, folders nested thousands deep, millions of entries, files that change or vanish mid-scan. `get()` instead of `[i]`, saturating arithmetic, bounded loops (no recursion over the tree: explicit stacks).
- **Last-resort guard:** a panic while reading one folder is caught (`catch_unwind` in `whatspace-scan`), costs that folder only, and is reported. It's a safety net, not a licence.
- **Locks:** the tree is shared between the scan's workers and the window behind one `Mutex`; take it with `whatspace_scan::lock` (poison-tolerant), never twice in one expression, and never across a call that might take it again.
- Production crate roots carry `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]`; new crates start with it. Tests may `unwrap` (`clippy.toml`).

## The owner's files are the owner's
- **WhatSpace never deletes.** The only thing it changes is moving what the owner chose to the Recycle Bin or Trash, after a confirmation that says what and how much, through `Host::trash`. Nothing is ever moved without that click; no command line or automation path moves anything.
- **A scan never changes or downloads anything.** Files are measured from their metadata; online-only cloud placeholders are never opened (opening one can start a download). Links and junctions are never followed.
- **Nothing leaves the computer.** WhatSpace has no network code; keep it that way. Exports go only where the owner saves them.
- **Tests never touch real folders** beyond fresh temporary ones they make and remove. The README pictures are of an invented disk (`crates/ui-egui/examples/ui_shot.rs`), never a real one.

## Different file systems
How sizes are measured per platform and file system lives in one place, `crates/fs` (see `docs/filesystems.md`). Add knowledge of a file system there, with a test that runs on every platform (the rules take a `Platform` so Linux and macOS behaviour is tested on Windows too).

## Assets: no vendor iconography or images
- Never copy, trace, screenshot-crop or recreate icons, images, themes or any other asset from TreeSize, WinDirStat, Explorer, Finder or any other product. Not even "temporarily".
- Every asset must be **original** (drawn in code — prefer this: `xtask/src/icon.rs`, the icons in `crates/ui-egui/src/rows.rs`), **public domain / CC0**, **OSI open source**, or **Creative Commons that allows redistribution**.
- **Every asset file has a row in [`ATTRIBUTION.md`](ATTRIBUTION.md)** (author, source, licence). `cargo xtask assets` (part of `cargo xtask ci`) fails otherwise. Add the row in the same commit as the file.

## Clean room
TreeSize may be **observed** black-box: run it, use its UI, note how it behaves. Never commit screenshots of it, never read, disassemble or copy anything inside it, never copy its wording beyond feature names. The squarified treemap comes from the published paper (Bruls, Huizing and van Wijk, 2000).

## Architecture (see `docs/architecture.md`)
| Layer | Crate | Job |
|---|---|---|
| L0 | `fs` | platform: allocated sizes, file identity, cloud placeholders, volumes, mount boundaries |
| L0 | `model` | the size tree and totals, statistics, treemap layout, units and dates, export |
| L1 | `scan` | worker threads that fill the tree, progress, cancellation, rescans |
| L2 | `ui-egui` | the egui front end (swappable) |
| apps | `apps/whatspace`, `apps/whatspace-cli` | binaries |

- **Layering** is enforced by `cargo xtask layers`. Nothing below L2 depends on egui/eframe/winit/rfd; the two L0 crates don't depend on each other.
- **The UI only reads the tree and asks.** Views push `Action`s; they run after the frame's look at the tree. Anything outside the window (dialogs, open, reveal, trash) goes through the `Host` trait, which the desktop app implements and tests replace.
- **Rust only** (no handwritten JS/TS).

## Quality gates
Before every commit: `cargo xtask ci` (fmt, clippy -D warnings, tests, assets, layers). Commit after every arc of work that builds, with a task id in the message (`M1.2: owner statistics`).

## Running and looking at the app
- `cargo run --release -p whatspace -- <folder>` scans a folder at start.
- **Headless window screenshots** (no focus stealing, works with a locked screen): `cargo run --release -p whatspace-ui-egui --example ui_shot -- <out-dir> [--dark] [folder]`. Without a folder it shows the invented demo disk. **For UI work, look at the result** (read the PNG), in light and dark.
- CLI: `whatspace-cli volumes`, `whatspace-cli scan <folder> --depth 2 --top 20`, `--csv`/`--json` exports.
- Parallel agents: separate `CARGO_TARGET_DIR` per agent; edit only the crates (or files) you own; delete your target dir when done (disk).

## Roadmap
`ROADMAP.md` holds the stage, what works, and what's next. When work lands, update it.
