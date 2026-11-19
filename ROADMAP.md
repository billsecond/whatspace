# WhatSpace roadmap

**Stage: 0.1, early.** The scanner, the views and the release pipeline are in place and tested on
Windows; the code builds for macOS, Linux and FreeBSD, and CI tests it on all of them.

## What works (0.1)

- Parallel scanning with live progress, Stop, and Scan this folder again.
- Size and allocated space, measured per file system (`docs/filesystems.md`): cluster rounding and
  compressed/sparse files on Windows, block counts on Unix, cloud placeholders, hard links counted
  once, links and junctions never followed, staying on one file system with Linux's mount table,
  Btrfs subvolumes, pseudo file systems and the macOS system/data volume pair handled.
- Folder tree with share bars, Details, Treemap, Extensions, Largest files and File ages.
- Drive list with file system, size and free space; folder picker, typed paths, drag and drop.
- Open, show in file manager, copy path, move to Recycle Bin/Trash with confirmation.
- CSV and JSON export; units per platform convention or chosen; light and dark.
- `whatspace-cli`: `volumes`, `scan` with depth, top files, CSV/JSON export.
- Packaging: MSI (with Explorer's "Scan with WhatSpace") and portable zip for Windows x64/x86/ARM64,
  universal DMG, AppImage/deb/rpm/Flatpak/tar.gz for Linux x86_64/aarch64, FreeBSD tarball.

## Next

| Id | What | Why |
|---|---|---|
| M1.1 | Fast NTFS scan from the Master File Table when running as administrator | TreeSize's speed on whole Windows drives |
| M1.2 | Owners: space by file owner (Windows security IDs, Unix uids) | TreeSize's "Users" view |
| M1.3 | Filters: include or exclude by name pattern, size or date, for all views | Find e.g. every `node_modules` or `.iso` |
| M1.4 | Find duplicate files (by size, then content hash) | Reclaiming space |
| M1.5 | Save a scan and compare two scans (what grew) | Tracking growth over time |
| M1.6 | Treemap: zoom animation, folder borders by depth, label every large tile | Easier reading |
| M1.7 | Export to HTML and Excel; print | Reports for others |
| M1.8 | Pie and bar charts of the selected folder | TreeSize's chart view |
| M1.9 | APFS clone and snapshot awareness where public interfaces allow; Btrfs/ZFS snapshot usage | Totals that match the file system's own |
| M1.10 | Scan several roots at once (all drives) | One view of the whole machine |
| M1.11 | Translations | Reach |
| M2 | Signed releases (Windows Authenticode, Apple notarization) once certificates exist | No SmartScreen or Gatekeeper prompt |

## Progress log

- **0.1.0** (2026-11): first version: the workspace on MailCraft's setup, scanner, views, CLI,
  packaging and CI/CD.
