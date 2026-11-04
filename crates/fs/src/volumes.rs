//! The mounted volumes: where they are, what file system they carry, how big they are and how
//! much is free. The list comes from the operating system (through `sysinfo`), so it is what
//! Explorer, Finder or `df` would show.

use std::path::{Path, PathBuf};

/// What sort of device a volume is on, as far as the system says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VolumeKind {
    Ssd,
    Hdd,
    Removable,
    Network,
    Unknown,
}

/// One mounted volume.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Volume {
    /// Where it is mounted: `C:\`, `/`, `/Volumes/Backup`, `/home`.
    pub mount_point: PathBuf,
    /// The volume's label or device name, when the system gives one.
    pub label: String,
    /// The file system as the system names it (`NTFS`, `apfs`, `ext4`, `zfs`).
    pub file_system: String,
    /// Size in bytes.
    pub total: u64,
    /// Bytes available to the current user.
    pub available: u64,
    pub read_only: bool,
    pub kind: VolumeKind,
}

impl Volume {
    /// Bytes in use (or otherwise not available to the current user).
    pub fn used(&self) -> u64 {
        self.total.saturating_sub(self.available)
    }

    /// The file system's name as people write it.
    pub fn file_system_name(&self) -> String {
        pretty_file_system(&self.file_system)
    }
}

/// All mounted volumes with a size, sorted by mount point. Empty when the system won't say.
pub fn list() -> Vec<Volume> {
    let Ok(disks) = sysinfo::Disks::new_with_refreshed_list() else {
        return Vec::new();
    };
    let mut out: Vec<Volume> = disks
        .iter()
        .filter(|d| d.total_space() > 0)
        .map(|d| {
            let file_system = d.file_system().to_string_lossy().into_owned();
            let kind = if is_network(&file_system) {
                VolumeKind::Network
            } else if d.is_removable() {
                VolumeKind::Removable
            } else {
                match d.kind() {
                    sysinfo::DiskKind::SSD => VolumeKind::Ssd,
                    sysinfo::DiskKind::HDD => VolumeKind::Hdd,
                    sysinfo::DiskKind::Unknown(_) => VolumeKind::Unknown,
                }
            };
            Volume {
                mount_point: d.mount_point().to_path_buf(),
                label: d.name().to_string_lossy().into_owned(),
                file_system,
                total: d.total_space(),
                available: d.available_space().min(d.total_space()),
                read_only: d.is_read_only(),
                kind,
            }
        })
        .collect();
    out.sort_by(|a, b| a.mount_point.cmp(&b.mount_point));
    out.dedup_by(|a, b| a.mount_point == b.mount_point);
    out
}

/// The volume `path` is on: the one with the longest mount point that `path` is under.
pub fn containing<'a>(volumes: &'a [Volume], path: &Path) -> Option<&'a Volume> {
    volumes.iter().filter(|v| is_under(path, &v.mount_point)).max_by_key(|v| v.mount_point.as_os_str().len())
}

/// Whether `path` is `base` or inside it. Drive letters and names compare without regard to
/// case on Windows, as the file system does.
pub fn is_under(path: &Path, base: &Path) -> bool {
    if cfg!(windows) {
        let lower = |p: &Path| p.to_string_lossy().replace('/', "\\").to_lowercase();
        let (p, b) = (lower(path), lower(base));
        let b = b.trim_end_matches('\\');
        p == b || p.strip_prefix(b).is_some_and(|rest| rest.starts_with('\\'))
    } else {
        path.starts_with(base)
    }
}

/// The allocation unit (cluster size on Windows, block size elsewhere) of the volume holding
/// `path`, when the system says and it is a sensible number.
pub fn allocation_unit(path: &Path) -> Option<u64> {
    fs4::allocation_granularity(path).ok().filter(|n| (512..=64 * 1024 * 1024).contains(n))
}

/// Network file systems: reaching into them can be slow, and they are never entered by
/// accident when staying on one file system.
pub fn is_network(file_system: &str) -> bool {
    let fs = file_system.to_ascii_lowercase();
    matches!(
        fs.as_str(),
        "nfs" | "nfs4" | "cifs" | "smb" | "smbfs" | "smb2" | "smb3" | "afpfs" | "webdav" | "davfs" | "9p" | "ncpfs" | "afs" | "glusterfs" | "ceph"
    ) || fs.starts_with("fuse.sshfs")
}

/// A file system's name as people write it (`apfs` → `APFS`, `ext4` stays `ext4`).
pub fn pretty_file_system(name: &str) -> String {
    let lower = name.to_ascii_lowercase();
    let pretty = match lower.as_str() {
        "ntfs" => "NTFS",
        "refs" => "ReFS",
        "fat" | "fat12" | "fat16" | "msdos" | "vfat" => "FAT",
        "fat32" => "FAT32",
        "exfat" => "exFAT",
        "apfs" => "APFS",
        "hfs" | "hfs+" | "hfsplus" => "HFS+",
        "btrfs" => "Btrfs",
        "xfs" => "XFS",
        "zfs" => "ZFS",
        "ufs" => "UFS",
        "f2fs" => "F2FS",
        "nfs" | "nfs4" => "NFS",
        "cifs" | "smb" | "smbfs" | "smb2" | "smb3" => "SMB",
        "cdfs" | "iso9660" => "ISO 9660",
        "udf" => "UDF",
        "" => "unknown",
        _ => return name.to_string(),
    };
    pretty.to_string()
}

/// What to know about how sizes are measured on a file system, in a sentence or two for the
/// owner. Empty when there is nothing particular to say.
pub fn notes(file_system: &str) -> &'static str {
    match file_system.to_ascii_lowercase().as_str() {
        "ntfs" => {
            "Allocated space is the size rounded up to the cluster size; compressed and sparse files show what they really take. \
             Online-only cloud files (OneDrive and the like) take no space on this disk. Hard links are counted once when that option is on."
        }
        "refs" => "Allocated space is the size rounded up to the cluster size; block-cloned files are each counted in full.",
        "fat" | "fat12" | "fat16" | "fat32" | "msdos" | "vfat" | "exfat" => {
            "Allocated space is the size rounded up to the cluster size, which can be large on big FAT and exFAT volumes."
        }
        "apfs" => {
            "Cloned files share their space but are each counted in full, and snapshots are not visible as files, \
             so the total can differ from what Finder reports as used. Some folders need Full Disk Access to be read."
        }
        "hfs" | "hfs+" | "hfsplus" => "Allocated space is what the file system reports for each file.",
        "btrfs" => {
            "Compressed files show less allocated than their size. Shared extents (reflinks, snapshots) are counted for every file that uses them; \
             subvolumes that are not mount points are scanned as part of the file system."
        }
        "zfs" => "Compression shows as allocated less than size. Snapshots and reservations are not visible as files; each dataset is its own mount.",
        "xfs" => "Allocated space is what the file system reports; reflinked files are each counted in full.",
        "ext2" | "ext3" | "ext4" => "Allocated space is what the file system reports for each file, sparse files included.",
        fs if is_network(fs) => "A network share: the server decides what files take, and scanning can be slow.",
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vol(mount: &str, fs: &str) -> Volume {
        Volume {
            mount_point: PathBuf::from(mount),
            label: String::new(),
            file_system: fs.into(),
            total: 100,
            available: 40,
            read_only: false,
            kind: VolumeKind::Unknown,
        }
    }

    #[test]
    fn finds_the_innermost_volume() {
        if cfg!(windows) {
            let v = [vol("C:\\", "NTFS"), vol("D:\\", "exFAT"), vol("C:\\mnt\\data\\", "ReFS")];
            assert_eq!(containing(&v, Path::new("c:\\Users\\x")).map(|v| v.file_system.as_str()), Some("NTFS"));
            assert_eq!(containing(&v, Path::new("C:\\mnt\\data\\big")).map(|v| v.file_system.as_str()), Some("ReFS"));
            assert_eq!(containing(&v, Path::new("C:\\mnt\\database")).map(|v| v.file_system.as_str()), Some("NTFS"));
            assert_eq!(containing(&v, Path::new("D:\\")).map(|v| v.file_system.as_str()), Some("exFAT"));
            assert!(containing(&v, Path::new("E:\\x")).is_none());
        } else {
            let v = [vol("/", "ext4"), vol("/home", "btrfs"), vol("/media/usb", "exfat")];
            assert_eq!(containing(&v, Path::new("/home/ada/x")).map(|v| v.file_system.as_str()), Some("btrfs"));
            assert_eq!(containing(&v, Path::new("/homework")).map(|v| v.file_system.as_str()), Some("ext4"));
            assert_eq!(containing(&v, Path::new("/")).map(|v| v.file_system.as_str()), Some("ext4"));
        }
    }

    #[test]
    fn used_never_goes_negative() {
        let mut v = vol("/", "ext4");
        assert_eq!(v.used(), 60);
        v.available = 500;
        assert_eq!(v.used(), 0);
    }

    #[test]
    fn file_system_names() {
        assert_eq!(pretty_file_system("apfs"), "APFS");
        assert_eq!(pretty_file_system("NTFS"), "NTFS");
        assert_eq!(pretty_file_system("ext4"), "ext4");
        assert_eq!(pretty_file_system("cifs"), "SMB");
        assert_eq!(pretty_file_system(""), "unknown");
        assert!(is_network("nfs4") && is_network("fuse.sshfs") && !is_network("ext4"));
        assert!(!notes("NTFS").is_empty() && !notes("apfs").is_empty() && !notes("zfs").is_empty());
        assert!(notes("nothing-known").is_empty());
    }

    #[test]
    fn listing_the_volumes_does_not_fail() {
        // Whatever the machine has; on a normal desktop or CI runner there is at least one.
        for v in list() {
            assert!(v.total > 0);
            assert!(v.available <= v.total);
        }
    }

    #[test]
    fn the_allocation_unit_of_the_temp_folder_is_sensible() {
        if let Some(unit) = allocation_unit(&std::env::temp_dir()) {
            assert!(unit >= 512 && unit.is_power_of_two(), "{unit}");
        }
    }
}
