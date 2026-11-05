//! The size tree: every folder and file a scan found.
//!
//! Nodes live in one arena and are named by [`NodeId`]. Every folder keeps the totals of
//! everything beneath it (size, allocated space, files, folders, the newest modification time,
//! and how many places could not be read), and every change ([`Tree::add_children`],
//! [`Tree::mark`], [`Tree::remove`]) updates the totals of all the folders above it at once, so
//! the tree can be shown while a scan is still filling it in.
//!
//! Names are kept in one string; a name that is not valid Unicode (possible on Unix and, with
//! unpaired surrogates, on Windows) is shown with replacement characters and kept as it was on
//! the side, so the path built from it ([`Tree::path_of`]) still finds the file.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::ops::BitOr;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A node's place in the tree's arena.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(u32);

impl NodeId {
    /// The scanned folder itself.
    pub const ROOT: NodeId = NodeId(0);
    const NONE: u32 = u32::MAX;

    pub fn index(self) -> usize {
        self.0 as usize
    }

    pub fn from_index(i: usize) -> Option<NodeId> {
        u32::try_from(i).ok().filter(|v| *v != Self::NONE).map(NodeId)
    }
}

/// What a node is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Dir,
    File,
    /// A symbolic link or junction: shown, never followed.
    Link,
    /// A socket, pipe or device.
    Other,
}

/// Which number a view sorts and measures by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Metric {
    /// The files' lengths.
    #[default]
    Size,
    /// The space they take on disk.
    Allocated,
}

/// What a scan noted about a node.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Flags(u16);

impl Flags {
    pub const NONE: Flags = Flags(0);
    /// The folder could not be read: permission denied.
    pub const DENIED: Flags = Flags(1);
    /// Reading it failed for another reason.
    pub const ERROR: Flags = Flags(1 << 1);
    /// On another file system, and the scan stays on one.
    pub const OTHER_FS: Flags = Flags(1 << 2);
    /// A pseudo file system (`/proc`, `/sys`): never scanned.
    pub const PSEUDO: Flags = Flags(1 << 3);
    /// Its content is counted elsewhere (macOS's data volume under `/`).
    pub const DUPLICATE: Flags = Flags(1 << 4);
    /// Another name of a file already counted (a hard link): counts as nothing here.
    pub const HARD_LINK: Flags = Flags(1 << 5);
    /// A cloud placeholder whose content is not on this disk.
    pub const CLOUD: Flags = Flags(1 << 6);
    /// Stored compressed or sparse.
    pub const PACKED: Flags = Flags(1 << 7);
    /// Nested too deep to follow.
    pub const TOO_DEEP: Flags = Flags(1 << 8);
    /// The folder's own listing has been read.
    pub const SCANNED: Flags = Flags(1 << 9);
    /// Taken out of the tree (moved to the trash, or cleared for a rescan).
    pub const REMOVED: Flags = Flags(1 << 10);

    /// Places that could not be read: they count in a folder's `errors`.
    pub const PROBLEMS: Flags = Flags(Self::DENIED.0 | Self::ERROR.0 | Self::TOO_DEEP.0);
    /// Folders a scan deliberately didn't go into.
    pub const NOT_ENTERED: Flags = Flags(Self::OTHER_FS.0 | Self::PSEUDO.0 | Self::DUPLICATE.0);

    pub const fn contains(self, other: Flags) -> bool {
        self.0 & other.0 == other.0
    }

    pub const fn intersects(self, other: Flags) -> bool {
        self.0 & other.0 != 0
    }

    pub fn insert(&mut self, other: Flags) {
        self.0 |= other.0;
    }

    pub fn remove(&mut self, other: Flags) {
        self.0 &= !other.0;
    }

    pub const fn bits(self) -> u16 {
        self.0
    }
}

impl BitOr for Flags {
    type Output = Flags;
    fn bitor(self, rhs: Flags) -> Flags {
        Flags(self.0 | rhs.0)
    }
}

/// One folder or file.
#[derive(Clone, Debug)]
pub struct Node {
    name: (u32, u32),
    parent: u32,
    pub kind: Kind,
    pub flags: Flags,
    /// Length in bytes (a file), or of everything beneath (a folder).
    pub size: u64,
    /// Space on disk (a file), or of everything beneath (a folder).
    pub allocated: u64,
    /// Files beneath (a folder); 0 for a file.
    pub files: u64,
    /// Folders beneath, not counting itself (a folder); 0 for a file.
    pub dirs: u64,
    /// Last modification in seconds since 1970; for a folder the newest beneath it, itself included.
    pub modified: i64,
    /// Places that could not be read, beneath and including this one.
    pub errors: u32,
    children: Vec<NodeId>,
}

impl Node {
    pub fn parent(&self) -> Option<NodeId> {
        (self.parent != NodeId::NONE).then_some(NodeId(self.parent))
    }

    pub fn children(&self) -> &[NodeId] {
        &self.children
    }

    pub fn is_dir(&self) -> bool {
        self.kind == Kind::Dir
    }

    /// The number `metric` measures.
    pub fn metric(&self, metric: Metric) -> u64 {
        match metric {
            Metric::Size => self.size,
            Metric::Allocated => self.allocated,
        }
    }
}

/// A new child for [`Tree::add_children`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: OsString,
    pub kind: Kind,
    pub flags: Flags,
    pub size: u64,
    pub allocated: u64,
    pub modified: i64,
}

impl Entry {
    pub fn dir(name: impl Into<OsString>, modified: i64) -> Entry {
        Entry { name: name.into(), kind: Kind::Dir, flags: Flags::NONE, size: 0, allocated: 0, modified }
    }

    pub fn file(name: impl Into<OsString>, size: u64, allocated: u64, modified: i64) -> Entry {
        Entry { name: name.into(), kind: Kind::File, flags: Flags::NONE, size, allocated, modified }
    }

    pub fn with_flags(mut self, flags: Flags) -> Entry {
        self.flags.insert(flags);
        self
    }
}

/// What a change adds to (or takes from) the folders above it.
#[derive(Clone, Copy, Debug, Default)]
struct Totals {
    size: u64,
    allocated: u64,
    files: u64,
    dirs: u64,
    errors: u32,
    modified: i64,
}

/// The tree of one scan.
#[derive(Clone, Debug)]
pub struct Tree {
    nodes: Vec<Node>,
    names: String,
    raw_names: HashMap<u32, OsString>,
    root_path: PathBuf,
    generation: u64,
}

impl Tree {
    /// A tree holding only the scanned folder, at `root_path`, last modified at `modified`.
    pub fn new(root_path: impl Into<PathBuf>, modified: i64) -> Tree {
        let root_path = root_path.into();
        let mut tree = Tree { nodes: Vec::new(), names: String::new(), raw_names: HashMap::new(), root_path, generation: 0 };
        let label = tree.root_path.display().to_string();
        let name = tree.push_name(0, OsStr::new(&label));
        tree.nodes.push(Node {
            name,
            parent: NodeId::NONE,
            kind: Kind::Dir,
            flags: Flags::NONE,
            size: 0,
            allocated: 0,
            files: 0,
            dirs: 0,
            modified,
            errors: 0,
            children: Vec::new(),
        });
        tree
    }

    pub fn root(&self) -> NodeId {
        NodeId::ROOT
    }

    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    /// Goes up with every change: views compare it to know when to look again.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Nodes in the arena, removed ones included.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.len() <= 1
    }

    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(id.index())
    }

    /// The node, unless it has been removed.
    pub fn live(&self, id: NodeId) -> Option<&Node> {
        self.node(id).filter(|n| !n.flags.contains(Flags::REMOVED))
    }

    /// The name shown for a node (the full path for the root).
    pub fn name(&self, id: NodeId) -> &str {
        let Some(node) = self.node(id) else { return "" };
        let (start, len) = (node.name.0 as usize, node.name.1 as usize);
        self.names.get(start..start.saturating_add(len)).unwrap_or("")
    }

    pub fn children(&self, id: NodeId) -> &[NodeId] {
        self.node(id).map(Node::children).unwrap_or(&[])
    }

    pub fn parent(&self, id: NodeId) -> Option<NodeId> {
        self.node(id).and_then(Node::parent)
    }

    /// The node's children, largest first by `metric`, then by name.
    pub fn sorted_children(&self, id: NodeId, metric: Metric) -> Vec<NodeId> {
        let mut kids: Vec<NodeId> = self.children(id).to_vec();
        kids.sort_by(|a, b| {
            let (ma, mb) = (self.node(*a).map_or(0, |n| n.metric(metric)), self.node(*b).map_or(0, |n| n.metric(metric)));
            mb.cmp(&ma).then_with(|| self.name(*a).to_lowercase().cmp(&self.name(*b).to_lowercase()))
        });
        kids
    }

    /// The root, then each folder down to `id`, `id` included.
    pub fn ancestry(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut cur = self.node(id).map(|_| id);
        while let Some(n) = cur {
            out.push(n);
            if out.len() > self.nodes.len() {
                break;
            }
            cur = self.parent(n);
        }
        out.reverse();
        out
    }

    /// Where the node is on disk.
    pub fn path_of(&self, id: NodeId) -> PathBuf {
        let mut path = self.root_path.clone();
        for n in self.ancestry(id).into_iter().skip(1) {
            match self.raw_names.get(&n.0) {
                Some(raw) => path.push(raw),
                None => path.push(self.name(n)),
            }
        }
        path
    }

    /// The node at `path`, which must be the root's path or beneath it.
    pub fn find(&self, path: &Path) -> Option<NodeId> {
        let rest = path.strip_prefix(&self.root_path).ok()?;
        let mut cur = self.root();
        for part in rest.components() {
            let name = part.as_os_str();
            cur = *self
                .children(cur)
                .iter()
                .find(|c| self.raw_names.get(&c.0).map_or_else(|| OsStr::new(self.name(**c)) == name, |raw| raw == name))?;
        }
        Some(cur)
    }

    /// Visit `id` and everything beneath it that is still in the tree, parents before children.
    pub fn walk(&self, id: NodeId, mut visit: impl FnMut(NodeId, &Node)) {
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            let Some(node) = self.live(n) else { continue };
            visit(n, node);
            stack.extend(node.children.iter().rev().copied());
        }
    }

    /// Add `entries` as children of the folder `dir` and count them in every folder above.
    /// Returns their ids, in order. Nothing is added under a node that is not a folder still in
    /// the tree, nor beyond the arena's capacity (four billion nodes).
    pub fn add_children(&mut self, dir: NodeId, entries: Vec<Entry>) -> Vec<NodeId> {
        if !self.live(dir).is_some_and(Node::is_dir) {
            return Vec::new();
        }
        let mut ids = Vec::with_capacity(entries.len());
        let mut add = Totals { modified: i64::MIN, ..Totals::default() };
        for e in entries {
            let Some(id) = NodeId::from_index(self.nodes.len()) else { break };
            let name = self.push_name(id.0, &e.name);
            let is_dir = e.kind == Kind::Dir;
            let problem = e.flags.intersects(Flags::PROBLEMS);
            let (size, allocated) = if is_dir { (0, 0) } else { (e.size, e.allocated) };
            add.size = add.size.saturating_add(size);
            add.allocated = add.allocated.saturating_add(allocated);
            if is_dir {
                add.dirs += 1;
            } else {
                add.files += 1;
            }
            add.errors = add.errors.saturating_add(u32::from(problem));
            add.modified = add.modified.max(e.modified);
            self.nodes.push(Node {
                name,
                parent: dir.0,
                kind: e.kind,
                flags: e.flags,
                size,
                allocated,
                files: 0,
                dirs: 0,
                modified: e.modified,
                errors: u32::from(problem),
                children: Vec::new(),
            });
            ids.push(id);
        }
        if let Some(node) = self.nodes.get_mut(dir.index()) {
            node.children.extend_from_slice(&ids);
        }
        self.add_up(dir, add);
        self.generation += 1;
        ids
    }

    /// Note `flags` on a node. A problem (unreadable, too deep) counts once in the `errors` of
    /// the node and every folder above it.
    pub fn mark(&mut self, id: NodeId, flags: Flags) {
        let Some(node) = self.nodes.get_mut(id.index()) else { return };
        let new_problem = flags.intersects(Flags::PROBLEMS) && !node.flags.intersects(Flags::PROBLEMS);
        node.flags.insert(flags);
        if new_problem {
            self.add_up(id, Totals { errors: 1, modified: i64::MIN, ..Totals::default() });
        }
        self.generation += 1;
    }

    /// Take a node and everything beneath it out of the tree (after it was moved to the trash),
    /// taking its totals off every folder above. The root can't be removed.
    pub fn remove(&mut self, id: NodeId) -> bool {
        if id == self.root() {
            return false;
        }
        let Some(node) = self.live(id) else { return false };
        let Some(parent) = node.parent() else { return false };
        let take = Totals {
            size: node.size,
            allocated: node.allocated,
            files: if node.is_dir() { node.files } else { 1 },
            dirs: if node.is_dir() { node.dirs + 1 } else { 0 },
            errors: node.errors,
            modified: 0,
        };
        if let Some(p) = self.nodes.get_mut(parent.index()) {
            p.children.retain(|c| *c != id);
        }
        self.take_off(parent, take);
        self.tombstone(id);
        self.refresh_modified(parent);
        self.generation += 1;
        true
    }

    /// Take everything beneath the folder `dir` out of the tree and forget what was noted about
    /// reading it, ready for it to be scanned again.
    pub fn clear(&mut self, dir: NodeId) {
        for child in self.children(dir).to_vec() {
            self.remove(child);
        }
        if let Some(node) = self.nodes.get_mut(dir.index()) {
            let had_problem = node.flags.intersects(Flags::PROBLEMS);
            node.flags.remove(Flags::PROBLEMS | Flags::SCANNED);
            if had_problem {
                self.take_off(dir, Totals { errors: 1, ..Totals::default() });
            }
        }
        self.generation += 1;
    }

    fn tombstone(&mut self, id: NodeId) {
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            if let Some(node) = self.nodes.get_mut(n.index()) {
                node.flags.insert(Flags::REMOVED);
                stack.append(&mut node.children);
            }
        }
    }

    /// After a removal, a folder's newest modification may have gone with it.
    fn refresh_modified(&mut self, from: NodeId) {
        let mut cur = Some(from);
        let mut steps = 0;
        while let Some(id) = cur {
            let newest = self.children(id).iter().filter_map(|c| self.node(*c)).map(|n| n.modified).max();
            let Some(node) = self.nodes.get_mut(id.index()) else { break };
            if let Some(newest) = newest {
                node.modified = newest;
            }
            cur = node.parent();
            steps += 1;
            if steps > self.nodes.len() {
                break;
            }
        }
    }

    fn add_up(&mut self, from: NodeId, add: Totals) {
        let mut cur = Some(from);
        let mut steps = 0;
        while let Some(id) = cur {
            let Some(node) = self.nodes.get_mut(id.index()) else { break };
            node.size = node.size.saturating_add(add.size);
            node.allocated = node.allocated.saturating_add(add.allocated);
            node.files = node.files.saturating_add(add.files);
            node.dirs = node.dirs.saturating_add(add.dirs);
            node.errors = node.errors.saturating_add(add.errors);
            node.modified = node.modified.max(add.modified);
            cur = node.parent();
            steps += 1;
            if steps > self.nodes.len() {
                break;
            }
        }
    }

    fn take_off(&mut self, from: NodeId, take: Totals) {
        let mut cur = Some(from);
        let mut steps = 0;
        while let Some(id) = cur {
            let Some(node) = self.nodes.get_mut(id.index()) else { break };
            node.size = node.size.saturating_sub(take.size);
            node.allocated = node.allocated.saturating_sub(take.allocated);
            node.files = node.files.saturating_sub(take.files);
            node.dirs = node.dirs.saturating_sub(take.dirs);
            node.errors = node.errors.saturating_sub(take.errors);
            cur = node.parent();
            steps += 1;
            if steps > self.nodes.len() {
                break;
            }
        }
    }

    /// Keep a name in the arena; a name that isn't valid Unicode is also kept as it was.
    fn push_name(&mut self, id: u32, name: &OsStr) -> (u32, u32) {
        let text = match name.to_str() {
            Some(s) => std::borrow::Cow::Borrowed(s),
            None => {
                self.raw_names.insert(id, name.to_os_string());
                name.to_string_lossy()
            }
        };
        let (Ok(start), Ok(len)) = (u32::try_from(self.names.len()), u32::try_from(text.len())) else {
            self.raw_names.insert(id, name.to_os_string());
            return (0, 0);
        };
        if start.checked_add(len).is_none() {
            self.raw_names.insert(id, name.to_os_string());
            return (0, 0);
        }
        self.names.push_str(&text);
        (start, len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// root/
    ///   photos/ a.jpg (3000) b.jpg (1000)
    ///   music/  song.mp3 (500)
    ///   notes.txt (20)
    pub(crate) fn sample() -> Tree {
        let mut t = Tree::new("/data", 100);
        let ids = t.add_children(NodeId::ROOT, vec![Entry::dir("photos", 200), Entry::dir("music", 50), Entry::file("notes.txt", 20, 4096, 300)]);
        t.add_children(ids[0], vec![Entry::file("a.jpg", 3000, 4096, 150), Entry::file("b.jpg", 1000, 4096, 400)]);
        t.add_children(ids[1], vec![Entry::file("song.mp3", 500, 4096, 60)]);
        t
    }

    #[test]
    fn totals_add_up_the_whole_way() {
        let t = sample();
        let root = t.node(NodeId::ROOT).unwrap();
        assert_eq!(root.size, 4520);
        assert_eq!(root.allocated, 4 * 4096);
        assert_eq!(root.files, 4);
        assert_eq!(root.dirs, 2);
        assert_eq!(root.modified, 400, "the newest file beneath");
        let photos = t.find(Path::new("/data/photos")).unwrap();
        assert_eq!(t.node(photos).unwrap().size, 4000);
        assert_eq!(t.node(photos).unwrap().files, 2);
        assert_eq!(t.name(photos), "photos");
    }

    #[test]
    fn sorted_largest_first_then_by_name() {
        let t = sample();
        let names: Vec<&str> = t.sorted_children(NodeId::ROOT, Metric::Size).iter().map(|c| t.name(*c)).collect();
        assert_eq!(names, ["photos", "music", "notes.txt"]);
        let names: Vec<&str> = t.sorted_children(NodeId::ROOT, Metric::Allocated).iter().map(|c| t.name(*c)).collect();
        assert_eq!(names, ["photos", "music", "notes.txt"], "4096 each: music before notes by name");
    }

    #[test]
    fn paths_and_ancestry() {
        let t = sample();
        let b = t.find(Path::new("/data/photos/b.jpg")).unwrap();
        assert_eq!(t.path_of(b), Path::new("/data/photos/b.jpg"));
        assert_eq!(t.ancestry(b).len(), 3);
        assert_eq!(t.path_of(NodeId::ROOT), Path::new("/data"));
        assert!(t.find(Path::new("/elsewhere")).is_none());
        assert!(t.find(Path::new("/data/nope")).is_none());
    }

    #[test]
    fn removing_takes_the_totals_off() {
        let mut t = sample();
        let photos = t.find(Path::new("/data/photos")).unwrap();
        let before = t.generation();
        assert!(t.remove(photos));
        assert!(t.generation() > before);
        let root = t.node(NodeId::ROOT).unwrap();
        assert_eq!(root.size, 520);
        assert_eq!(root.files, 2);
        assert_eq!(root.dirs, 1);
        assert_eq!(root.modified, 300, "the newest that is left");
        assert!(t.live(photos).is_none());
        assert!(!t.remove(photos), "already gone");
        assert!(!t.remove(NodeId::ROOT), "the root stays");
        assert_eq!(t.children(NodeId::ROOT).len(), 2);
    }

    #[test]
    fn problems_count_once_up_the_chain() {
        let mut t = sample();
        let music = t.find(Path::new("/data/music")).unwrap();
        t.mark(music, Flags::DENIED);
        t.mark(music, Flags::ERROR);
        assert_eq!(t.node(music).unwrap().errors, 1);
        assert_eq!(t.node(NodeId::ROOT).unwrap().errors, 1);
        t.add_children(music, vec![Entry::dir("locked", 0).with_flags(Flags::DENIED)]);
        assert_eq!(t.node(NodeId::ROOT).unwrap().errors, 2);
        t.clear(music);
        assert_eq!(t.node(NodeId::ROOT).unwrap().errors, 0, "cleared for a rescan");
        assert_eq!(t.node(music).unwrap().size, 0);
        assert!(!t.node(music).unwrap().flags.contains(Flags::DENIED));
    }

    #[test]
    fn nothing_is_added_under_a_file_or_a_removed_folder() {
        let mut t = sample();
        let notes = t.find(Path::new("/data/notes.txt")).unwrap();
        assert!(t.add_children(notes, vec![Entry::file("x", 1, 1, 0)]).is_empty());
        let music = t.find(Path::new("/data/music")).unwrap();
        t.remove(music);
        assert!(t.add_children(music, vec![Entry::file("x", 1, 1, 0)]).is_empty());
        assert!(t.add_children(NodeId::from_index(999).unwrap(), vec![Entry::file("x", 1, 1, 0)]).is_empty());
        assert_eq!(t.node(NodeId::ROOT).unwrap().size, 4020);
    }

    #[test]
    fn walk_visits_live_nodes_parents_first() {
        let mut t = sample();
        let mut seen = Vec::new();
        t.walk(NodeId::ROOT, |id, _| seen.push(t.name(id).to_string()));
        assert_eq!(seen.len(), 7);
        assert_eq!(seen[0], "/data");
        let photos = t.find(Path::new("/data/photos")).unwrap();
        t.remove(photos);
        let mut count = 0;
        t.walk(NodeId::ROOT, |_, _| count += 1);
        assert_eq!(count, 4);
    }

    #[cfg(unix)]
    #[test]
    fn a_name_that_is_not_unicode_still_finds_its_file() {
        use std::os::unix::ffi::OsStrExt;
        let raw = OsStr::from_bytes(b"caf\xe9.txt");
        let mut t = Tree::new("/data", 0);
        let ids = t.add_children(NodeId::ROOT, vec![Entry::file(raw, 5, 5, 0)]);
        assert_eq!(t.name(ids[0]), "caf\u{fffd}.txt");
        assert_eq!(t.path_of(ids[0]).file_name(), Some(raw));
        assert_eq!(t.find(&Path::new("/data").join(raw)), Some(ids[0]));
    }
}
