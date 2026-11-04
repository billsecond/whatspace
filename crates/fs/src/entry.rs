//! What a file takes: its logical size, the space it occupies on its volume, its modification
//! time and, for files with more than one name, its identity.
//!
//! The logical size is the same everywhere (the file's length). The space on disk is not:
//!
//! - **Unix** (Linux, macOS, the BSDs): the block count the file system reports (`st_blocks`,
//!   always in 512-byte units). It is exact for sparse files and for what Btrfs, ZFS and APFS
//!   compress, and it includes the last partial block. APFS clones share their blocks but each
//!   reports them in full; nothing short of private interfaces tells them apart.
//! - **Windows** (NTFS, ReFS, FAT, exFAT): the length rounded up to the volume's cluster size,
//!   which is what Explorer's "Size on disk" does. Compressed, sparse and deduplicated files ask
//!   the system for what they really take (`GetCompressedFileSizeW`). Cloud placeholders whose
//!   content is online only (OneDrive, iCloud for Windows, Dropbox and the like) take nothing on
//!   this disk and are never opened, so a scan never downloads anything.

use std::fs::Metadata;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// A file's identity on its volume: two names with the same identity are hard links to one file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FileId {
    /// The device (Unix) or volume serial number (Windows).
    pub volume: u64,
    /// The inode (Unix) or file index (Windows).
    pub index: u64,
}

/// What a scan learns about one file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FileFacts {
    /// Logical length in bytes.
    pub size: u64,
    /// Bytes the file occupies on its volume.
    pub allocated: u64,
    /// Last modification, in seconds since 1970 (negative before; 0 when unknown).
    pub modified: i64,
    /// The file's identity when it has more than one name and hard links are being looked for.
    pub hard_link: Option<FileId>,
    /// A cloud placeholder whose content is not on this disk.
    pub cloud: bool,
    /// Stored compressed or sparse, so it takes less than its length.
    pub packed: bool,
}

/// How files are measured during one scan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Probe {
    /// The allocation unit (cluster or block size) of the volume being scanned. Where the system
    /// doesn't say what a file takes (Windows), its length is rounded up to this.
    pub cluster: u64,
    /// Look for hard links. Free on Unix (the link count is in the metadata already); on Windows
    /// it costs opening each file, so it is a choice there.
    pub hard_links: bool,
}

impl Default for Probe {
    fn default() -> Self {
        Probe { cluster: 4096, hard_links: cfg!(unix) }
    }
}

impl Probe {
    /// Measure the file at `path`, whose metadata (not following links) is `meta`.
    pub fn file(&self, path: &Path, meta: &Metadata) -> FileFacts {
        imp::file(self, path, meta)
    }
}

/// The modification time in `meta`, in seconds since 1970 (0 when the system doesn't say).
pub fn modified_secs(meta: &Metadata) -> i64 {
    meta.modified().map(seconds).unwrap_or(0)
}

/// Seconds since 1970, negative before, saturating far out.
pub fn seconds(t: SystemTime) -> i64 {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => i64::try_from(d.as_secs()).unwrap_or(i64::MAX),
        Err(e) => i64::try_from(e.duration().as_secs()).map(|s| -s).unwrap_or(i64::MIN),
    }
}

/// The device a directory is on (Unix); 0 on Windows, where a scan never leaves its volume
/// anyway (mounted folders are reparse points, which are not followed).
pub fn device(meta: &Metadata) -> u64 {
    imp::device(meta)
}

/// `size` rounded up to a whole number of `unit`s (unchanged for a unit of 0 or 1).
pub fn round_up(size: u64, unit: u64) -> u64 {
    if unit <= 1 {
        return size;
    }
    match size % unit {
        0 => size,
        rem => size.saturating_add(unit - rem),
    }
}

#[cfg(unix)]
mod imp {
    use super::{FileFacts, FileId, Probe, modified_secs, round_up};
    use std::fs::Metadata;
    use std::os::unix::fs::MetadataExt;
    use std::path::Path;

    pub fn file(probe: &Probe, _path: &Path, meta: &Metadata) -> FileFacts {
        let size = meta.len();
        // `st_blocks` is in 512-byte units on every Unix, whatever the file system's block size.
        let allocated = meta.blocks().saturating_mul(512);
        let hard_link = (probe.hard_links && meta.nlink() > 1).then(|| FileId { volume: meta.dev(), index: meta.ino() });
        let packed = allocated.saturating_add(probe.cluster.max(512)) <= round_up(size, 512);
        FileFacts { size, allocated, modified: modified_secs(meta), hard_link, cloud: false, packed }
    }

    pub fn device(meta: &Metadata) -> u64 {
        meta.dev()
    }
}

#[cfg(windows)]
mod imp {
    use super::{FileFacts, FileId, Probe, modified_secs, round_up};
    use std::fs::Metadata;
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use std::path::Path;

    const FILE_ATTRIBUTE_SPARSE_FILE: u32 = 0x200;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    const FILE_ATTRIBUTE_COMPRESSED: u32 = 0x800;
    const FILE_ATTRIBUTE_OFFLINE: u32 = 0x1000;
    const FILE_ATTRIBUTE_RECALL_ON_OPEN: u32 = 0x4_0000;
    const FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS: u32 = 0x40_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    /// FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE: never get in anybody's way.
    const SHARE_ALL: u32 = 0x7;

    pub fn file(probe: &Probe, path: &Path, meta: &Metadata) -> FileFacts {
        let attrs = meta.file_attributes();
        let size = meta.file_size();
        // Online-only cloud files: their content is somewhere else, and opening them could
        // start a download, so they are measured as taking nothing and never opened.
        let cloud = attrs & (FILE_ATTRIBUTE_OFFLINE | FILE_ATTRIBUTE_RECALL_ON_OPEN | FILE_ATTRIBUTE_RECALL_ON_DATA_ACCESS) != 0;
        let packed = attrs & (FILE_ATTRIBUTE_SPARSE_FILE | FILE_ATTRIBUTE_COMPRESSED) != 0;
        let allocated = if cloud {
            0
        } else if packed || attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            // Compressed, sparse, or another kind of reparse point (deduplicated files): the
            // system knows what it really takes.
            match filesize::file_real_size(path) {
                Ok(real) => round_up(real, probe.cluster),
                Err(_) => round_up(size, probe.cluster),
            }
        } else {
            round_up(size, probe.cluster)
        };
        let hard_link = if probe.hard_links && !cloud { identity(path) } else { None };
        FileFacts { size, allocated, modified: modified_secs(meta), hard_link, cloud, packed }
    }

    /// The volume and file index of a file with more than one name. The file is opened without
    /// asking to read it (access 0), sharing everything, and without following a reparse point.
    fn identity(path: &Path) -> Option<FileId> {
        let file = std::fs::OpenOptions::new()
            .access_mode(0)
            .share_mode(SHARE_ALL)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .ok()?;
        let info = winapi_util::file::information(&file).ok()?;
        (info.number_of_links() > 1).then(|| FileId { volume: info.volume_serial_number(), index: info.file_index() })
    }

    pub fn device(_meta: &Metadata) -> u64 {
        0
    }
}

#[cfg(not(any(unix, windows)))]
mod imp {
    use super::{FileFacts, Probe, modified_secs, round_up};
    use std::fs::Metadata;
    use std::path::Path;

    pub fn file(probe: &Probe, _path: &Path, meta: &Metadata) -> FileFacts {
        let size = meta.len();
        FileFacts { size, allocated: round_up(size, probe.cluster), modified: modified_secs(meta), hard_link: None, cloud: false, packed: false }
    }

    pub fn device(_meta: &Metadata) -> u64 {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn rounds_up_to_the_unit() {
        assert_eq!(round_up(0, 4096), 0);
        assert_eq!(round_up(1, 4096), 4096);
        assert_eq!(round_up(4096, 4096), 4096);
        assert_eq!(round_up(4097, 4096), 8192);
        assert_eq!(round_up(10, 0), 10);
        assert_eq!(round_up(10, 1), 10);
        assert_eq!(round_up(u64::MAX, 4096), u64::MAX, "saturates instead of overflowing");
    }

    #[test]
    fn seconds_either_side_of_1970() {
        assert_eq!(seconds(UNIX_EPOCH), 0);
        assert_eq!(seconds(UNIX_EPOCH + Duration::from_secs(90)), 90);
        if let Some(before) = UNIX_EPOCH.checked_sub(Duration::from_secs(60)) {
            assert_eq!(seconds(before), -60);
        }
    }

    #[test]
    fn measures_a_real_file() {
        let dir = std::env::temp_dir().join(format!("whatspace-fs-entry-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("ten-thousand.bin");
        std::fs::write(&path, vec![7u8; 10_000]).unwrap();
        let meta = std::fs::symlink_metadata(&path).unwrap();
        let facts = Probe { cluster: 4096, hard_links: true }.file(&path, &meta);
        assert_eq!(facts.size, 10_000);
        assert!(facts.allocated >= 10_000, "a dense file takes at least its length: {facts:?}");
        assert!(facts.modified > 1_600_000_000, "{facts:?}");
        assert!(!facts.cloud);
        assert_eq!(facts.hard_link, None, "one name only");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_second_name_is_a_hard_link() {
        let dir = std::env::temp_dir().join(format!("whatspace-fs-link-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("a.bin");
        let b = dir.join("b.bin");
        std::fs::write(&a, b"shared").unwrap();
        if std::fs::hard_link(&a, &b).is_ok() {
            let probe = Probe { cluster: 4096, hard_links: true };
            let fa = probe.file(&a, &std::fs::symlink_metadata(&a).unwrap());
            let fb = probe.file(&b, &std::fs::symlink_metadata(&b).unwrap());
            assert!(fa.hard_link.is_some(), "{fa:?}");
            assert_eq!(fa.hard_link, fb.hard_link, "both names are one file");
            let off = Probe { cluster: 4096, hard_links: false };
            assert_eq!(off.file(&a, &std::fs::symlink_metadata(&a).unwrap()).hard_link, None);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
