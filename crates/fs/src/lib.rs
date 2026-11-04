//! WhatSpace's platform layer: what each operating system and file system says about space.
//!
//! Everything that differs between Windows, macOS, Linux and the BSDs, or between NTFS, ReFS,
//! FAT, APFS, ext4, Btrfs, XFS, ZFS and the network file systems, is decided here, so the
//! scanner above it can stay the same everywhere:
//!
//! - [`entry`]: a file's logical size, the space it takes on disk (allocated), its modification
//!   time, and its identity for counting hard links once. On Unix the space comes from the
//!   file's block count; on Windows from the cluster size, and from the system itself for
//!   compressed, sparse and deduplicated files. Cloud placeholders (OneDrive and the like) whose
//!   content is not on this disk take no space here and are never opened.
//! - [`volumes`]: the mounted volumes with their file system, size and free space.
//! - [`mounts`]: the mount table and the rule for staying on one file system, which knows about
//!   pseudo file systems (`/proc`, `/sys`), Btrfs subvolumes that are not mount points, and the
//!   macOS system and data volumes that appear as one.
//!
//! Nothing here panics: every failure becomes `None`, `0` or an `Err`. No `unsafe` (the
//! workspace forbids it): the Windows calls go through small audited crates.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod entry;
pub mod mounts;
pub mod volumes;

pub use entry::{FileFacts, FileId, Probe, device, modified_secs, round_up};
pub use mounts::{Boundary, Crossing, MountTable, Platform};
pub use volumes::{Volume, VolumeKind};
