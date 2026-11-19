//! The scanner against real folders on this machine's disk.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use whatspace_scan::{Flags, Kind, NodeId, Options, Scan, ScanError, lock, scan};

/// A fresh folder for one test, removed when it goes out of scope.
struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Scratch {
        static N: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!("whatspace-scan-{name}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }

    fn file(&self, rel: &str, len: usize) -> PathBuf {
        let p = self.0.join(rel);
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, vec![b'x'; len]).unwrap();
        p
    }

    fn dir(&self, rel: &str) -> PathBuf {
        let p = self.0.join(rel);
        fs::create_dir_all(&p).unwrap();
        p
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn opts() -> Options {
    Options { threads: 4, ..Options::default() }
}

#[test]
fn totals_match_what_is_on_disk() {
    let s = Scratch::new("totals");
    s.file("a/one.bin", 1000);
    s.file("a/two.bin", 3000);
    s.file("b/c/three.bin", 5000);
    s.dir("empty");
    s.file("top.txt", 10);
    let (tree, snap, volume, problems) = scan(&s.0, opts()).unwrap();
    assert!(snap.done && !snap.cancelled);
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(snap.files, 4);
    assert_eq!(snap.dirs, 4);
    assert_eq!(snap.bytes, 9010);
    let root = tree.node(NodeId::ROOT).unwrap();
    assert_eq!(root.size, 9010);
    assert_eq!(root.files, 4);
    assert_eq!(root.dirs, 4);
    assert_eq!(root.errors, 0);
    // The files take at least their sizes, unless the file system squeezed them (ZFS, compressed
    // Btrfs): then the files that take less are marked packed.
    let mut packed = false;
    tree.walk(NodeId::ROOT, |_, n| packed |= n.flags.contains(Flags::PACKED));
    assert!(root.allocated >= 9010 || packed, "allocated {} is less than the sizes and nothing is marked packed", root.allocated);
    assert!(root.flags.contains(Flags::SCANNED));
    let a = tree.find(&s.0.join("a")).unwrap();
    assert_eq!(tree.node(a).unwrap().size, 4000);
    let c = tree.find(&s.0.join("b").join("c")).unwrap();
    assert_eq!(tree.node(c).unwrap().files, 1);
    let empty = tree.find(&s.0.join("empty")).unwrap();
    assert_eq!(tree.node(empty).unwrap().size, 0);
    assert!(tree.node(empty).unwrap().flags.contains(Flags::SCANNED));
    let names: Vec<&str> = tree.sorted_children(NodeId::ROOT, whatspace_model::Metric::Size).iter().map(|id| tree.name(*id)).collect();
    assert_eq!(names, ["b", "a", "top.txt", "empty"]);
    assert!(volume.cluster >= 512, "{volume:?}");
}

#[test]
fn a_hard_linked_file_counts_once() {
    let s = Scratch::new("links");
    let original = s.file("one/data.bin", 50_000);
    s.dir("two");
    if fs::hard_link(&original, s.0.join("two").join("same.bin")).is_err() {
        return; // A file system without hard links.
    }
    let (tree, ..) = scan(&s.0, Options { hard_links: true, ..opts() }).unwrap();
    assert_eq!(tree.node(NodeId::ROOT).unwrap().size, 50_000);
    let mut flagged = 0;
    tree.walk(NodeId::ROOT, |_, n| flagged += usize::from(n.flags.contains(Flags::HARD_LINK)));
    assert_eq!(flagged, 1, "the second name is marked");
    let (counted_twice, ..) = scan(&s.0, Options { hard_links: false, ..opts() }).unwrap();
    assert_eq!(counted_twice.node(NodeId::ROOT).unwrap().size, 100_000);
}

#[test]
fn links_are_listed_but_never_followed() {
    let s = Scratch::new("symlink");
    let big = s.dir("big");
    s.file("big/inside.bin", 20_000);
    #[cfg(unix)]
    let made = std::os::unix::fs::symlink(&big, s.0.join("shortcut")).is_ok();
    #[cfg(windows)]
    let made = std::os::windows::fs::symlink_dir(&big, s.0.join("shortcut")).is_ok();
    if !made {
        return; // Windows without the right to make symbolic links.
    }
    let (tree, ..) = scan(&s.0, opts()).unwrap();
    assert_eq!(tree.node(NodeId::ROOT).unwrap().size, 20_000, "the link's target is counted once, where it is");
    let link = tree.find(&s.0.join("shortcut")).unwrap();
    assert_eq!(tree.node(link).unwrap().kind, Kind::Link);
    assert!(tree.children(link).is_empty());
}

#[cfg(unix)]
#[test]
fn a_sparse_file_takes_less_than_its_length() {
    let s = Scratch::new("sparse");
    let f = fs::File::create(s.0.join("sparse.img")).unwrap();
    f.set_len(256 * 1024 * 1024).unwrap();
    drop(f);
    let (tree, ..) = scan(&s.0, opts()).unwrap();
    let id = tree.find(&s.0.join("sparse.img")).unwrap();
    let node = tree.node(id).unwrap();
    assert_eq!(node.size, 256 * 1024 * 1024);
    if node.allocated < node.size / 2 {
        assert!(node.flags.contains(Flags::PACKED));
    }
}

#[cfg(unix)]
#[test]
fn an_unreadable_folder_is_marked_and_the_scan_goes_on() {
    use std::os::unix::fs::PermissionsExt;
    let s = Scratch::new("denied");
    s.file("open/fine.bin", 100);
    let locked = s.dir("locked");
    s.file("locked/secret.bin", 100);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
    let readable_anyway = fs::read_dir(&locked).is_ok(); // running as root
    let (tree, snap, _, problems) = scan(&s.0, opts()).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    if readable_anyway {
        return;
    }
    let id = tree.find(&locked).unwrap();
    assert!(tree.node(id).unwrap().flags.contains(Flags::DENIED));
    assert_eq!(tree.node(NodeId::ROOT).unwrap().errors, 1);
    assert_eq!(snap.errors, 1);
    assert_eq!(problems.len(), 1);
    assert_eq!(tree.node(NodeId::ROOT).unwrap().size, 100, "what could be read is counted");
}

#[test]
fn folders_too_deep_are_marked_not_read() {
    let s = Scratch::new("deep");
    s.file("1/2/3/4/5/6/bottom.bin", 7);
    let (tree, snap, _, problems) = scan(&s.0, Options { max_depth: 3, ..opts() }).unwrap();
    let four = tree.find(&s.0.join("1").join("2").join("3").join("4")).unwrap();
    assert!(tree.node(four).unwrap().flags.contains(Flags::TOO_DEEP));
    assert_eq!(tree.node(NodeId::ROOT).unwrap().size, 0);
    assert_eq!(snap.errors, 1);
    assert_eq!(problems.len(), 1);
}

#[test]
fn cancelling_stops_and_keeps_what_was_read() {
    let s = Scratch::new("cancel");
    for d in 0..60 {
        for f in 0..20 {
            s.file(&format!("d{d}/e{}/f{f}.bin", d % 7), 10);
        }
    }
    let running = Scan::start(&s.0, opts()).unwrap();
    running.cancel();
    let snap = running.wait();
    assert!(snap.done, "the workers all stopped");
    assert!(snap.cancelled);
    // And a full scan of the same folder for comparison.
    let (tree, full, ..) = scan(&s.0, opts()).unwrap();
    assert!(full.files >= snap.files);
    assert_eq!(tree.node(NodeId::ROOT).unwrap().files, 1200);
}

#[test]
fn a_folder_can_be_scanned_again() {
    let s = Scratch::new("rescan");
    s.file("keep/a.bin", 100);
    s.file("change/b.bin", 200);
    let running = Scan::start(&s.0, opts()).unwrap();
    let tree = running.tree();
    running.wait();
    let change = lock(&tree).find(&s.0.join("change")).unwrap();
    s.file("change/c.bin", 5000);
    let again = Scan::rescan(tree.clone(), change, opts()).unwrap();
    again.wait();
    let t = lock(&tree);
    assert_eq!(t.node(change).unwrap().size, 5200);
    assert_eq!(t.node(NodeId::ROOT).unwrap().size, 5300);
    assert_eq!(t.node(NodeId::ROOT).unwrap().files, 3);
    drop(t);
    let file = lock(&tree).find(&s.0.join("keep").join("a.bin")).unwrap();
    assert!(matches!(Scan::rescan(tree.clone(), file, opts()), Err(ScanError::Gone)), "a file is not a folder to rescan");
}

#[test]
fn starting_on_something_that_is_not_a_folder() {
    let s = Scratch::new("errors");
    let f = s.file("plain.txt", 3);
    assert!(matches!(Scan::start(&f, opts()), Err(ScanError::NotAFolder(_))));
    assert!(matches!(Scan::start(&s.0.join("missing"), opts()), Err(ScanError::Open { .. })));
}

#[test]
fn names_with_odd_characters() {
    let s = Scratch::new("names");
    for name in ["space in name.txt", "ünïcödé-文件.bin", "comma, and \"quote\".csv", "#hash%percent&amp.txt", "trailing.dot."] {
        // Windows allows neither a double quote nor a trailing dot in a name.
        let rel = if cfg!(windows) { name.trim_end_matches('.').replace('"', "'") } else { name.to_string() };
        s.file(&rel, 42);
    }
    let (tree, snap, _, problems) = scan(&s.0, opts()).unwrap();
    assert!(problems.is_empty(), "{problems:?}");
    assert_eq!(snap.files, 5);
    tree.walk(NodeId::ROOT, |id, n| {
        if n.kind == Kind::File {
            assert!(tree.path_of(id).exists(), "{}", tree.path_of(id).display());
        }
    });
}

#[cfg(unix)]
#[test]
fn a_name_that_is_not_unicode() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;
    let s = Scratch::new("bytes");
    let raw = OsStr::from_bytes(b"caf\xe9.txt");
    if fs::write(s.0.join(raw), b"hello").is_err() {
        return; // A file system that insists on Unicode names (APFS).
    }
    let (tree, ..) = scan(&s.0, opts()).unwrap();
    let id = tree.find(&s.0.join(raw)).unwrap();
    assert!(tree.path_of(id).exists());
    assert_eq!(tree.node(id).unwrap().size, 5);
}

#[test]
fn the_root_is_made_absolute() {
    let p = whatspace_scan::normalize_root(Path::new("relative-folder"));
    assert!(p.is_absolute(), "{}", p.display());
    if cfg!(windows) {
        assert_eq!(whatspace_scan::normalize_root(Path::new("C:")), Path::new("C:\\"));
    }
}
