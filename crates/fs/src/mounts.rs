//! Staying on one file system, the way `du -x` does, with what each platform needs on top:
//!
//! - **Linux:** the mount table (`/proc/self/mountinfo`) decides what a boundary is, not the
//!   device number alone. A Btrfs subvolume that is not a mount point has a device number of
//!   its own but is part of the same file system, so it is scanned. Pseudo file systems
//!   (`proc`, `sysfs`, `devtmpfs`, cgroups and the like) are never entered, even when crossing
//!   file systems is allowed: their "files" take no space and some never end.
//! - **macOS:** the system volume and the data volume are mounted as one (`/` and
//!   `/System/Volumes/Data`, joined by firmlinks such as `/Users` and `/Applications`), so they
//!   count as one file system, and `/System/Volumes/Data` itself is skipped when scanning `/`,
//!   because everything in it is already reached through the firmlinks.
//! - **FreeBSD and other Unix systems:** a different device number is a different file system.
//!   ZFS datasets are each a mount.
//! - **Windows:** volumes mounted in folders, junctions and symbolic links are reparse points,
//!   which a scan never follows, so a scan never leaves its volume and there is nothing to check.

use std::path::{Path, PathBuf};

use crate::volumes::Volume;

/// The rules of which platform to apply. [`Platform::current`] in real use; tests try them all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Linux,
    MacOs,
    OtherUnix,
    Windows,
}

impl Platform {
    pub const fn current() -> Platform {
        if cfg!(any(target_os = "linux", target_os = "android")) {
            Platform::Linux
        } else if cfg!(target_os = "macos") {
            Platform::MacOs
        } else if cfg!(windows) {
            Platform::Windows
        } else {
            Platform::OtherUnix
        }
    }
}

/// One mount: where, and which file system type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mount {
    pub point: PathBuf,
    pub fs_type: String,
}

/// The system's mount table, as far as it is known.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MountTable {
    mounts: Vec<Mount>,
}

impl MountTable {
    /// This system's mount table: `/proc/self/mountinfo` on Linux, the volume list elsewhere.
    pub fn load() -> MountTable {
        if Platform::current() == Platform::Linux
            && let Ok(text) = std::fs::read_to_string("/proc/self/mountinfo")
        {
            return MountTable::from_mountinfo(&text);
        }
        MountTable::from_volumes(&crate::volumes::list())
    }

    /// Parse Linux's `/proc/<pid>/mountinfo`. Lines that don't parse are left out.
    pub fn from_mountinfo(text: &str) -> MountTable {
        MountTable { mounts: text.lines().filter_map(parse_mountinfo_line).collect() }
    }

    pub fn from_volumes(volumes: &[Volume]) -> MountTable {
        MountTable { mounts: volumes.iter().map(|v| Mount { point: v.mount_point.clone(), fs_type: v.file_system.clone() }).collect() }
    }

    pub fn is_empty(&self) -> bool {
        self.mounts.is_empty()
    }

    pub fn mounts(&self) -> &[Mount] {
        &self.mounts
    }

    /// The type of the file system mounted exactly at `path` (the topmost one, when several are
    /// stacked there).
    pub fn mounted_at(&self, path: &Path) -> Option<&str> {
        self.mounts.iter().rev().find(|m| m.point == path).map(|m| m.fs_type.as_str())
    }

    /// The type of the file system `path` is on: the mount with the longest mount point above it.
    pub fn fs_type_of(&self, path: &Path) -> Option<&str> {
        self.mounts.iter().rev().filter(|m| path.starts_with(&m.point)).max_by_key(|m| m.point.as_os_str().len()).map(|m| m.fs_type.as_str())
    }
}

/// One line of mountinfo: `36 35 98:0 /mnt1 /mnt2 rw,noatime master:1 - ext3 /dev/root rw`.
/// The fifth field is the mount point; the first field after ` - ` is the file system type.
fn parse_mountinfo_line(line: &str) -> Option<Mount> {
    let (left, right) = line.split_once(" - ")?;
    let point = left.split(' ').nth(4)?;
    let fs_type = right.split(' ').next().filter(|t| !t.is_empty())?;
    Some(Mount { point: PathBuf::from(unescape_octal(point)), fs_type: fs_type.to_string() })
}

/// Undo mountinfo's escapes: a space is `\040`, a tab `\011`, a newline `\012`, a backslash
/// `\134`. Anything that isn't a well-formed escape stays as it is.
pub fn unescape_octal(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        if b == b'\\'
            && let Some(digits) = bytes.get(i + 1..i + 4)
            && digits.iter().all(|d| (b'0'..=b'7').contains(d))
        {
            let value = digits.iter().fold(0u32, |acc, d| acc * 8 + u32::from(d - b'0'));
            if let Ok(byte) = u8::try_from(value) {
                out.push(byte);
                i += 4;
                continue;
            }
        }
        out.push(b);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// File systems that hold no files that take space: never entered.
pub fn is_pseudo(fs_type: &str) -> bool {
    matches!(
        fs_type,
        "proc"
            | "sysfs"
            | "devtmpfs"
            | "devpts"
            | "devfs"
            | "cgroup"
            | "cgroup2"
            | "pstore"
            | "bpf"
            | "debugfs"
            | "tracefs"
            | "securityfs"
            | "configfs"
            | "fusectl"
            | "mqueue"
            | "hugetlbfs"
            | "autofs"
            | "binfmt_misc"
            | "efivarfs"
            | "rpc_pipefs"
            | "nsfs"
            | "procfs"
            | "fdescfs"
            | "linprocfs"
            | "linsysfs"
            | "selinuxfs"
    )
}

/// What to do with a directory that might be on another file system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Crossing {
    /// Same file system (or crossing is allowed): scan it.
    Enter,
    /// Another file system, and the scan stays on one.
    OtherFileSystem,
    /// A pseudo file system (`/proc`, `/sys`, `/dev`): never scanned.
    Pseudo,
    /// Its content is already reached another way (macOS's data volume under `/`).
    Duplicate,
}

/// Decides, for each directory a scan meets, whether to go in.
#[derive(Clone, Debug)]
pub struct Boundary {
    platform: Platform,
    root: PathBuf,
    /// Devices that count as the scanned file system.
    devices: Vec<u64>,
    mounts: MountTable,
    one_file_system: bool,
}

/// Places that are pseudo file systems on every Unix, whether or not the mount table says so.
const PSEUDO_PATHS: [&str; 3] = ["/proc", "/sys", "/dev"];

/// macOS: where the data volume is mounted, its content joined into `/` by firmlinks.
const MACOS_DATA: &str = "/System/Volumes/Data";

impl Boundary {
    /// The boundary for a scan of `root` (on device `root_device`) on this system.
    pub fn new(root: &Path, root_device: u64, one_file_system: bool) -> Boundary {
        let mut devices = vec![root_device];
        if Platform::current() == Platform::MacOs {
            // The system and data volumes are one file system as far as the owner can tell.
            let dev = |p: &str| std::fs::symlink_metadata(p).ok().map(|m| crate::entry::device(&m));
            if let (Some(system), Some(data)) = (dev("/"), dev(MACOS_DATA))
                && (root_device == system || root_device == data)
            {
                devices = vec![system, data];
            }
        }
        Boundary::with(Platform::current(), root, devices, MountTable::load(), one_file_system)
    }

    /// A boundary with every part given (tests, and callers that already have the mount table).
    pub fn with(platform: Platform, root: &Path, devices: Vec<u64>, mounts: MountTable, one_file_system: bool) -> Boundary {
        Boundary { platform, root: root.to_path_buf(), devices, mounts, one_file_system }
    }

    /// Whether to scan the directory at `path`, which is on `device`.
    pub fn check(&self, path: &Path, device: u64) -> Crossing {
        if self.platform == Platform::Windows {
            return Crossing::Enter;
        }
        if self.platform == Platform::MacOs && self.root == Path::new("/") && path == Path::new(MACOS_DATA) {
            return Crossing::Duplicate;
        }
        if self.devices.contains(&device) {
            return Crossing::Enter;
        }
        let pseudo = self.mounts.mounted_at(path).is_some_and(is_pseudo) || PSEUDO_PATHS.iter().any(|p| path == Path::new(p));
        if pseudo {
            return Crossing::Pseudo;
        }
        // Linux: another device that is not a mount point is a Btrfs subvolume of the same file
        // system (it has a device number of its own all the same).
        if self.platform == Platform::Linux && !self.mounts.is_empty() && self.mounts.mounted_at(path).is_none() {
            return Crossing::Enter;
        }
        if self.one_file_system { Crossing::OtherFileSystem } else { Crossing::Enter }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const MOUNTINFO: &str = "\
22 1 8:2 / / rw,relatime shared:1 - ext4 /dev/sda2 rw
23 22 0:21 / /proc rw,nosuid shared:5 - proc proc rw
24 22 0:22 / /sys rw,nosuid shared:6 - sysfs sysfs rw
25 22 0:5 / /dev rw,nosuid shared:2 - devtmpfs udev rw,size=8g
40 22 0:35 /@home /home rw,relatime shared:30 - btrfs /dev/sda3 rw,space_cache=v2
41 22 8:17 / /media/My\\040Disk rw,relatime shared:31 - exfat /dev/sdb1 rw
42 22 0:40 / /mnt/share rw,relatime shared:32 - cifs //server/share rw
not a mountinfo line at all
";

    #[test]
    fn parses_mountinfo() {
        let t = MountTable::from_mountinfo(MOUNTINFO);
        assert_eq!(t.mounts().len(), 7);
        assert_eq!(t.mounted_at(Path::new("/proc")), Some("proc"));
        assert_eq!(t.mounted_at(Path::new("/media/My Disk")), Some("exfat"), "\\040 is a space");
        assert_eq!(t.fs_type_of(Path::new("/home/ada/notes.txt")), Some("btrfs"));
        assert_eq!(t.fs_type_of(Path::new("/etc/hosts")), Some("ext4"));
        assert_eq!(t.mounted_at(Path::new("/etc")), None);
    }

    #[test]
    fn unescapes_only_real_escapes() {
        assert_eq!(unescape_octal("a\\040b"), "a b");
        assert_eq!(unescape_octal("back\\134slash"), "back\\slash");
        assert_eq!(unescape_octal("tail\\04"), "tail\\04");
        assert_eq!(unescape_octal("\\999"), "\\999");
        assert_eq!(unescape_octal("\\777"), "\\777", "511 is not a byte");
        assert_eq!(unescape_octal(""), "");
    }

    #[test]
    fn linux_rules() {
        let t = MountTable::from_mountinfo(MOUNTINFO);
        let b = Boundary::with(Platform::Linux, Path::new("/"), vec![1], t.clone(), true);
        assert_eq!(b.check(Path::new("/usr"), 1), Crossing::Enter);
        assert_eq!(b.check(Path::new("/proc"), 21), Crossing::Pseudo);
        assert_eq!(b.check(Path::new("/dev"), 5), Crossing::Pseudo);
        assert_eq!(b.check(Path::new("/home"), 35), Crossing::OtherFileSystem, "a mounted subvolume is a mount, like du -x");
        assert_eq!(b.check(Path::new("/srv/snapshots"), 36), Crossing::Enter, "a nested Btrfs subvolume is not a mount point");
        assert_eq!(b.check(Path::new("/mnt/share"), 40), Crossing::OtherFileSystem);
        // Crossing allowed: real file systems are entered, pseudo ones still aren't.
        let cross = Boundary::with(Platform::Linux, Path::new("/"), vec![1], t, false);
        assert_eq!(cross.check(Path::new("/home"), 35), Crossing::Enter);
        assert_eq!(cross.check(Path::new("/media/My Disk"), 17), Crossing::Enter);
        assert_eq!(cross.check(Path::new("/sys"), 22), Crossing::Pseudo);
    }

    #[test]
    fn macos_rules() {
        let b = Boundary::with(Platform::MacOs, Path::new("/"), vec![10, 11], MountTable::default(), true);
        assert_eq!(b.check(Path::new("/Users"), 11), Crossing::Enter, "a firmlink into the data volume");
        assert_eq!(b.check(Path::new("/System/Volumes/Data"), 11), Crossing::Duplicate);
        assert_eq!(b.check(Path::new("/Volumes/Backup"), 12), Crossing::OtherFileSystem);
        assert_eq!(b.check(Path::new("/dev"), 13), Crossing::Pseudo);
        // Scanning the data volume itself is fine.
        let data = Boundary::with(Platform::MacOs, Path::new("/System/Volumes/Data"), vec![11], MountTable::default(), true);
        assert_eq!(data.check(Path::new("/System/Volumes/Data/Users"), 11), Crossing::Enter);
    }

    #[test]
    fn other_unix_and_windows_rules() {
        let bsd = Boundary::with(Platform::OtherUnix, Path::new("/usr"), vec![3], MountTable::default(), true);
        assert_eq!(bsd.check(Path::new("/usr/home"), 3), Crossing::Enter);
        assert_eq!(bsd.check(Path::new("/usr/ports"), 4), Crossing::OtherFileSystem, "another ZFS dataset");
        let win = Boundary::with(Platform::Windows, Path::new("C:\\"), vec![0], MountTable::default(), true);
        assert_eq!(win.check(Path::new("C:\\Users"), 0), Crossing::Enter);
    }

    #[test]
    fn pseudo_types() {
        assert!(is_pseudo("proc") && is_pseudo("sysfs") && is_pseudo("devfs"));
        assert!(!is_pseudo("tmpfs") && !is_pseudo("ext4") && !is_pseudo("zfs"));
    }

    proptest! {
        #[test]
        fn hostile_mountinfo_never_panics(text in ".{0,400}") {
            let t = MountTable::from_mountinfo(&text);
            let _ = t.fs_type_of(Path::new("/x"));
            let _ = unescape_octal(&text);
        }

        #[test]
        fn hostile_bytes_unescape(bytes in proptest::collection::vec(any::<u8>(), 0..200)) {
            let s = String::from_utf8_lossy(&bytes);
            let _ = unescape_octal(&s);
        }
    }
}
