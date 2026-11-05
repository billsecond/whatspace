# How WhatSpace measures space on each file system

A file has two sizes: its **length** (what a program reading it gets) and the **space it takes**
on its volume (allocated). They differ because space is handed out in whole clusters or blocks,
because some files are stored compressed or with holes (sparse), and because some "files" are only
placeholders for content kept elsewhere. WhatSpace shows both and lets you sort and measure by
either (Size / Allocated in the toolbar). All of this is decided in `crates/fs`.

## Windows: NTFS, ReFS, FAT32, exFAT

| Case | Allocated space |
|---|---|
| An ordinary file | Its length rounded up to the volume's cluster size (usually 4 KB; up to 128 KB or more on large exFAT and FAT32 volumes). This is what Explorer calls "Size on disk". |
| Compressed (NTFS compression), sparse, deduplicated | What the system says it really takes (`GetCompressedFileSizeW`), rounded up to the cluster size. |
| Online-only cloud file (OneDrive, iCloud for Windows, Dropbox, …) | Nothing: the content isn't on this disk. The file is never opened, because opening it can start a download. Files you keep "always on this device" are measured like any other. |
| Hard links (`C:\Windows\WinSxS` is full of them) | With **Count hard links once** on (Options), the second and later names count as nothing and are marked. It is off by default on Windows because it means opening every file, which makes a scan slower; Explorer counts every name too. |
| Junctions, symbolic links, volumes mounted in folders | Listed, never followed, so a scan never loops and never leaves its volume. |

Very small NTFS files can live inside the Master File Table and take no cluster at all; WhatSpace
still counts one cluster for them, as most tools do.

Some system folders (`System Volume Information`, other users' profiles) can only be read by an
administrator; they are marked as unreadable and counted in the status bar.

## macOS: APFS, HFS+

| Case | Allocated space |
|---|---|
| Any file | The blocks the file system reports for it (`st_blocks`), which is exact for sparse and compressed files. |
| Cloned files (APFS copies made in the Finder) | Each clone is counted in full, though the clones share their blocks: macOS offers no public way to tell. Totals can be higher than what the Finder reports as used. |
| Snapshots (Time Machine local snapshots) | Not visible as files, so not counted. |

The startup disk is two volumes, System and Data, joined by "firmlinks" (`/Users`,
`/Applications` and others live on Data but appear under `/`). WhatSpace treats them as one file
system, and when scanning `/` it skips `/System/Volumes/Data` itself, because everything in it is
already reached through the firmlinks.

macOS protects some folders (Mail, Messages, Safari, other apps' containers). To scan them, give
WhatSpace **Full Disk Access** in System Settings › Privacy & Security; without it they are marked
as unreadable.

## Linux and the BSDs: ext4, XFS, Btrfs, ZFS, UFS, F2FS

| Case | Allocated space |
|---|---|
| Any file | The blocks it really uses (`st_blocks`), so sparse files (disk images, databases) and files compressed by Btrfs or ZFS show their true footprint. |
| Hard links | Counted once, always (the link count is in the metadata, so it costs nothing). |
| Reflinks and snapshots (Btrfs, XFS, ZFS) | Shared blocks are counted for every file that uses them; snapshots aren't visible as files. |

**Staying on one file system** (Options, on by default) works like `du -x`, with the mount table
(`/proc/self/mountinfo`) deciding what a boundary is:

- Another mount (a second disk, `/home` on its own partition, a USB drive, a network share) is
  shown but not scanned. Turn the option off to scan into them.
- A **Btrfs subvolume that is not a mount point** has a device number of its own, but it is part of
  the same file system and is scanned.
- **Pseudo file systems** (`/proc`, `/sys`, `/dev`, cgroups, debugfs and the like) are never
  scanned, even with crossing allowed: they hold no real files and some never end.
- **ZFS datasets** are each their own mount, so a dataset mounted inside another counts as another
  file system.

Folders you can't read (other users' homes, `/root`) are marked and counted.

## Network shares: SMB, NFS, sshfs

The server decides what a file takes, and some servers report nothing useful for allocated space.
Reading a large share over the network is slow; WhatSpace never goes into one from a local drive
unless crossing file systems is allowed, but you can always scan a share directly (a mapped drive,
`\\server\share`, or its mount point).

## Units

Sizes are written the way the platform's file manager writes them unless you choose otherwise
(Units): KB, MB and GB of 1024 on Windows, kB, MB and GB of 1000 on macOS, KiB, MiB and GiB of
1024 on Linux and the BSDs.
