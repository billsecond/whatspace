//! WhatSpace's scanner: reads a folder and everything beneath it into a [`Tree`], on worker
//! threads, while whoever started it watches the tree grow.
//!
//! Each worker takes a folder from a shared queue, reads its listing, measures each file
//! through [`whatspace_fs`] (so allocated space, cloud placeholders, hard links and file system
//! boundaries follow each platform's rules), adds the whole listing to the tree under one short
//! lock, and puts the subfolders it found back on the queue. Folders are taken newest first, so
//! the walk goes deep before wide and the queue stays small.
//!
//! Symbolic links, junctions and other reparse points are listed but never followed, so a scan
//! can't loop and never leaves its volume through one. A folder that can't be read is marked and
//! counted, and the scan goes on. [`Scan::cancel`] stops it within a folder or two; the tree keeps
//! what was read. A bug that panics while reading one folder costs that folder, not the scan.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::collections::HashSet;
use std::fs;
use std::io;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use whatspace_fs::{Boundary, Crossing, FileId, Probe, device, modified_secs, volumes};
pub use whatspace_model::{Entry, Flags, Kind, NodeId, Tree};

/// A tree shared between the scan's workers and whoever shows it.
pub type SharedTree = Arc<Mutex<Tree>>;

/// The tree behind `shared`, even if a thread panicked while holding it (the tree's totals are
/// only ever changed whole, so it is still consistent).
pub fn lock(shared: &SharedTree) -> MutexGuard<'_, Tree> {
    shared.lock().unwrap_or_else(PoisonError::into_inner)
}

/// How a scan goes about it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// Stay on the file system the scan starts on (like `du -x`).
    pub one_file_system: bool,
    /// Count a file with several names (hard links) once. Free on Unix; on Windows it costs
    /// opening each file, which makes a scan slower.
    pub hard_links: bool,
    /// Worker threads; 0 picks a number from the processor count.
    pub threads: usize,
    /// Folders nested deeper than this are marked, not read.
    pub max_depth: u32,
}

impl Default for Options {
    fn default() -> Self {
        Options { one_file_system: true, hard_links: cfg!(unix), threads: 0, max_depth: 2048 }
    }
}

/// Why a scan couldn't start.
#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("{}: {source}", path.display())]
    Open { path: PathBuf, source: io::Error },
    #[error("{} is not a folder", .0.display())]
    NotAFolder(PathBuf),
    #[error("the folder is no longer in the tree")]
    Gone,
    #[error("no worker thread could be started: {0}")]
    Threads(io::Error),
}

/// The volume a scan is on, as far as the system says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VolumeInfo {
    pub mount_point: Option<PathBuf>,
    pub label: String,
    /// The file system as the system names it (`NTFS`, `apfs`, `ext4`).
    pub file_system: String,
    pub total: u64,
    pub available: u64,
    /// The allocation unit (cluster or block size).
    pub cluster: u64,
}

/// A place that couldn't be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    pub path: PathBuf,
    pub message: String,
}

/// At most this many problems are kept with their paths; the rest are only counted.
pub const MAX_PROBLEMS: usize = 1000;

/// How far a scan has got.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Snapshot {
    pub files: u64,
    pub dirs: u64,
    pub bytes: u64,
    pub errors: u64,
    pub done: bool,
    pub cancelled: bool,
    pub elapsed: Duration,
    /// A folder being read just now.
    pub current: PathBuf,
}

struct Progress {
    files: AtomicU64,
    dirs: AtomicU64,
    bytes: AtomicU64,
    errors: AtomicU64,
    done: AtomicBool,
    cancelled: AtomicBool,
    started: Instant,
    elapsed_ms: AtomicU64,
    current: Mutex<PathBuf>,
    problems: Mutex<Vec<Problem>>,
}

impl Progress {
    fn new() -> Progress {
        Progress {
            files: AtomicU64::new(0),
            dirs: AtomicU64::new(0),
            bytes: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            done: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            started: Instant::now(),
            elapsed_ms: AtomicU64::new(0),
            current: Mutex::new(PathBuf::new()),
            problems: Mutex::new(Vec::new()),
        }
    }

    fn elapsed(&self) -> Duration {
        if self.done.load(Ordering::Acquire) { Duration::from_millis(self.elapsed_ms.load(Ordering::Acquire)) } else { self.started.elapsed() }
    }

    fn finish(&self) {
        let ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.elapsed_ms.store(ms, Ordering::Release);
        self.done.store(true, Ordering::Release);
    }

    fn problem(&self, path: &Path, message: String) {
        self.errors.fetch_add(1, Ordering::Relaxed);
        let mut problems = self.problems.lock().unwrap_or_else(PoisonError::into_inner);
        if problems.len() < MAX_PROBLEMS {
            problems.push(Problem { path: path.to_path_buf(), message });
        }
    }
}

/// One folder to read.
struct Task {
    node: NodeId,
    path: PathBuf,
    depth: u32,
    /// The device the folder is on (Unix), for the folders inside it whose metadata fails.
    device: u64,
}

/// The folders waiting to be read, and how many are being read.
struct Queue {
    state: Mutex<QueueState>,
    ready: Condvar,
}

struct QueueState {
    tasks: Vec<Task>,
    active: usize,
    closed: bool,
}

impl Queue {
    fn new() -> Queue {
        Queue { state: Mutex::new(QueueState { tasks: Vec::new(), active: 0, closed: false }), ready: Condvar::new() }
    }

    fn state(&self) -> MutexGuard<'_, QueueState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn push(&self, tasks: Vec<Task>) {
        if tasks.is_empty() {
            return;
        }
        self.state().tasks.extend(tasks);
        self.ready.notify_all();
    }

    /// The next folder, or `None` once there is nothing left and nothing being read that could
    /// add more.
    fn pop(&self) -> Option<Task> {
        let mut s = self.state();
        loop {
            if s.closed {
                return None;
            }
            if let Some(task) = s.tasks.pop() {
                s.active += 1;
                return Some(task);
            }
            if s.active == 0 {
                s.closed = true;
                self.ready.notify_all();
                return None;
            }
            s = self.ready.wait(s).unwrap_or_else(PoisonError::into_inner);
        }
    }

    /// A folder popped earlier is finished.
    fn done(&self) {
        let mut s = self.state();
        s.active = s.active.saturating_sub(1);
        if s.active == 0 && s.tasks.is_empty() {
            s.closed = true;
            drop(s);
            self.ready.notify_all();
        }
    }

    fn drain(&self) {
        self.state().tasks.clear();
        self.ready.notify_all();
    }
}

struct Shared {
    tree: SharedTree,
    progress: Progress,
    queue: Queue,
    seen: Mutex<HashSet<FileId>>,
    boundary: Boundary,
    probe: Probe,
    max_depth: u32,
    workers: AtomicUsize,
}

impl Shared {
    fn cancelled(&self) -> bool {
        self.progress.cancelled.load(Ordering::Relaxed)
    }

    /// Whether this is the first name of a hard-linked file seen in this scan.
    fn first_sighting(&self, id: FileId) -> bool {
        self.seen.lock().unwrap_or_else(PoisonError::into_inner).insert(id)
    }

    /// The folder `node` at `path` couldn't be read.
    fn unreadable(&self, node: NodeId, path: &Path, err: &io::Error) {
        let flag = if err.kind() == io::ErrorKind::PermissionDenied { Flags::DENIED } else { Flags::ERROR };
        lock(&self.tree).mark(node, flag | Flags::SCANNED);
        self.progress.problem(path, err.to_string());
    }
}

/// A scan, running or finished.
pub struct Scan {
    shared: Arc<Shared>,
    handles: Vec<JoinHandle<()>>,
    root: PathBuf,
    node: NodeId,
    volume: VolumeInfo,
}

impl Scan {
    /// Start scanning the folder at `path` into a new tree.
    pub fn start(path: &Path, options: Options) -> Result<Scan, ScanError> {
        let root = normalize_root(path);
        let meta = fs::metadata(&root).map_err(|source| ScanError::Open { path: root.clone(), source })?;
        if !meta.is_dir() {
            return Err(ScanError::NotAFolder(root));
        }
        let tree = Arc::new(Mutex::new(Tree::new(root.clone(), modified_secs(&meta))));
        Scan::begin(tree, NodeId::ROOT, root, 0, &meta, options)
    }

    /// Scan the folder `node` of `tree` again: what was beneath it is taken out, then read anew.
    pub fn rescan(tree: SharedTree, node: NodeId, options: Options) -> Result<Scan, ScanError> {
        let (path, depth) = {
            let t = lock(&tree);
            if !t.live(node).is_some_and(|n| n.is_dir()) {
                return Err(ScanError::Gone);
            }
            (t.path_of(node), u32::try_from(t.ancestry(node).len().saturating_sub(1)).unwrap_or(u32::MAX))
        };
        let meta = fs::metadata(&path).map_err(|source| ScanError::Open { path: path.clone(), source })?;
        if !meta.is_dir() {
            return Err(ScanError::NotAFolder(path));
        }
        lock(&tree).clear(node);
        Scan::begin(tree, node, path, depth, &meta, options)
    }

    fn begin(tree: SharedTree, node: NodeId, path: PathBuf, depth: u32, meta: &fs::Metadata, options: Options) -> Result<Scan, ScanError> {
        let all = volumes::list();
        let found = volumes::containing(&all, &path);
        let cluster = volumes::allocation_unit(&path).unwrap_or(4096);
        let volume = VolumeInfo {
            mount_point: found.map(|v| v.mount_point.clone()),
            label: found.map(|v| v.label.clone()).unwrap_or_default(),
            file_system: found.map(|v| v.file_system.clone()).unwrap_or_default(),
            total: found.map_or(0, |v| v.total),
            available: found.map_or(0, |v| v.available),
            cluster,
        };
        let root_device = device(meta);
        let shared = Arc::new(Shared {
            tree,
            progress: Progress::new(),
            queue: Queue::new(),
            seen: Mutex::new(HashSet::new()),
            boundary: Boundary::new(&path, root_device, options.one_file_system),
            probe: Probe { cluster, hard_links: options.hard_links },
            max_depth: options.max_depth.max(1),
            workers: AtomicUsize::new(0),
        });
        shared.queue.push(vec![Task { node, path: path.clone(), depth, device: root_device }]);
        let threads = match options.threads {
            0 => std::thread::available_parallelism().map_or(4, |n| n.get()).clamp(4, 16),
            n => n.clamp(1, 64),
        };
        let mut handles = Vec::with_capacity(threads);
        let mut last_error = None;
        for i in 0..threads {
            shared.workers.fetch_add(1, Ordering::AcqRel);
            let worker = Arc::clone(&shared);
            match std::thread::Builder::new().name(format!("whatspace-scan-{i}")).spawn(move || work(&worker)) {
                Ok(h) => handles.push(h),
                Err(e) => {
                    shared.workers.fetch_sub(1, Ordering::AcqRel);
                    last_error = Some(e);
                }
            }
        }
        if handles.is_empty() {
            return Err(ScanError::Threads(last_error.unwrap_or_else(|| io::Error::other("no threads"))));
        }
        Ok(Scan { shared, handles, root: path, node, volume })
    }

    /// The tree being filled in.
    pub fn tree(&self) -> SharedTree {
        Arc::clone(&self.shared.tree)
    }

    /// The folder this scan reads (the tree's root, or the folder being scanned again).
    pub fn node(&self) -> NodeId {
        self.node
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn volume(&self) -> &VolumeInfo {
        &self.volume
    }

    pub fn snapshot(&self) -> Snapshot {
        let p = &self.shared.progress;
        Snapshot {
            files: p.files.load(Ordering::Relaxed),
            dirs: p.dirs.load(Ordering::Relaxed),
            bytes: p.bytes.load(Ordering::Relaxed),
            errors: p.errors.load(Ordering::Relaxed),
            done: p.done.load(Ordering::Acquire),
            cancelled: p.cancelled.load(Ordering::Relaxed),
            elapsed: p.elapsed(),
            current: p.current.lock().map(|c| c.clone()).unwrap_or_default(),
        }
    }

    /// The places that couldn't be read (the first [`MAX_PROBLEMS`]).
    pub fn problems(&self) -> Vec<Problem> {
        self.shared.progress.problems.lock().map(|p| p.clone()).unwrap_or_default()
    }

    pub fn is_done(&self) -> bool {
        self.shared.progress.done.load(Ordering::Acquire)
    }

    /// Stop soon. What was read stays in the tree.
    pub fn cancel(&self) {
        self.shared.progress.cancelled.store(true, Ordering::Relaxed);
        self.shared.queue.drain();
    }

    /// Wait for the scan to finish.
    pub fn wait(mut self) -> Snapshot {
        for h in std::mem::take(&mut self.handles) {
            let _ = h.join();
        }
        self.snapshot()
    }
}

impl Drop for Scan {
    fn drop(&mut self) {
        // A scan nobody is watching any more stops; its threads finish on their own.
        if !self.handles.is_empty() && !self.is_done() {
            self.cancel();
        }
    }
}

/// Scan `path` and wait for it: the tree, how it went, the volume, and what couldn't be read.
pub fn scan(path: &Path, options: Options) -> Result<(Tree, Snapshot, VolumeInfo, Vec<Problem>), ScanError> {
    let running = Scan::start(path, options)?;
    let tree = running.tree();
    let volume = running.volume().clone();
    let problems_of = Arc::clone(&running.shared);
    let snapshot = running.wait();
    let problems = problems_of.progress.problems.lock().map(|p| p.clone()).unwrap_or_default();
    drop(problems_of);
    let tree = match Arc::try_unwrap(tree) {
        Ok(m) => m.into_inner().unwrap_or_else(PoisonError::into_inner),
        Err(shared) => lock(&shared).clone(),
    };
    Ok((tree, snapshot, volume, problems))
}

/// An absolute path for the folder to scan: `C:` becomes `C:\`, a relative path is taken from
/// the current folder. Links in it are left as they are.
pub fn normalize_root(path: &Path) -> PathBuf {
    let mut p = path.to_path_buf();
    if cfg!(windows) {
        let s = path.as_os_str().to_string_lossy();
        let b = s.as_bytes();
        if b.len() == 2 && b.get(1) == Some(&b':') && b.first().is_some_and(u8::is_ascii_alphabetic) {
            p = PathBuf::from(format!("{s}\\"));
        }
    }
    std::path::absolute(&p).unwrap_or(p)
}

fn work(shared: &Shared) {
    while let Some(task) = shared.queue.pop() {
        if !shared.cancelled() {
            let (node, path) = (task.node, task.path.clone());
            // The last-resort guard: a panic while reading one folder costs that folder only.
            if catch_unwind(AssertUnwindSafe(|| read_folder(shared, task))).is_err() {
                shared.unreadable(node, &path, &io::Error::other("WhatSpace hit an internal error reading this folder"));
            }
        }
        shared.queue.done();
    }
    if shared.workers.fetch_sub(1, Ordering::AcqRel) == 1 {
        shared.progress.finish();
    }
}

/// What to do with each entry of a listing once it is in the tree.
enum Next {
    Nothing,
    Enter(PathBuf, u64),
}

fn read_folder(shared: &Shared, task: Task) {
    if let Ok(mut current) = shared.progress.current.try_lock() {
        current.clone_from(&task.path);
    }
    let listing = match fs::read_dir(&task.path) {
        Ok(l) => l,
        Err(e) => return shared.unreadable(task.node, &task.path, &e),
    };
    let mut entries = Vec::new();
    let mut next = Vec::new();
    let (mut files, mut dirs, mut bytes) = (0u64, 0u64, 0u64);
    for (i, item) in listing.enumerate() {
        if i % 256 == 255 && shared.cancelled() {
            break;
        }
        let entry = match item {
            Ok(e) => e,
            Err(e) => {
                shared.progress.problem(&task.path, e.to_string());
                continue;
            }
        };
        let name = entry.file_name();
        let file_type = match entry.file_type() {
            Ok(t) => t,
            Err(e) => {
                shared.progress.problem(&entry.path(), e.to_string());
                entries.push(Entry { name, kind: Kind::Other, flags: Flags::ERROR, size: 0, allocated: 0, modified: 0 });
                next.push(Next::Nothing);
                continue;
            }
        };
        if file_type.is_dir() {
            let meta = entry.metadata().ok();
            let dev = meta.as_ref().map_or(task.device, device);
            let path = entry.path();
            let (flags, step) = match shared.boundary.check(&path, dev) {
                Crossing::Enter => (Flags::NONE, Next::Enter(path, dev)),
                Crossing::OtherFileSystem => (Flags::OTHER_FS, Next::Nothing),
                Crossing::Pseudo => (Flags::PSEUDO, Next::Nothing),
                Crossing::Duplicate => (Flags::DUPLICATE, Next::Nothing),
            };
            entries.push(Entry::dir(name, meta.as_ref().map_or(0, modified_secs)).with_flags(flags));
            next.push(step);
            dirs += 1;
        } else if file_type.is_file() {
            match entry.metadata() {
                Ok(meta) => {
                    let facts = shared.probe.file(&entry.path(), &meta);
                    let mut flags = Flags::NONE;
                    if facts.cloud {
                        flags.insert(Flags::CLOUD);
                    }
                    if facts.packed {
                        flags.insert(Flags::PACKED);
                    }
                    let (size, allocated) = match facts.hard_link {
                        Some(id) if !shared.first_sighting(id) => {
                            flags.insert(Flags::HARD_LINK);
                            (0, 0)
                        }
                        _ => (facts.size, facts.allocated),
                    };
                    bytes = bytes.saturating_add(size);
                    entries.push(Entry { name, kind: Kind::File, flags, size, allocated, modified: facts.modified });
                }
                Err(e) => {
                    shared.progress.problem(&entry.path(), e.to_string());
                    entries.push(Entry { name, kind: Kind::File, flags: Flags::ERROR, size: 0, allocated: 0, modified: 0 });
                }
            }
            next.push(Next::Nothing);
            files += 1;
        } else {
            // A symbolic link or junction (never followed), or a socket, pipe or device.
            let kind = if file_type.is_symlink() { Kind::Link } else { Kind::Other };
            let modified = entry.metadata().map_or(0, |m| modified_secs(&m));
            entries.push(Entry { name, kind, flags: Flags::NONE, size: 0, allocated: 0, modified });
            next.push(Next::Nothing);
            files += 1;
        }
    }
    let ids = {
        let mut tree = lock(&shared.tree);
        let ids = tree.add_children(task.node, entries);
        tree.mark(task.node, Flags::SCANNED);
        ids
    };
    let p = &shared.progress;
    p.files.fetch_add(files, Ordering::Relaxed);
    p.dirs.fetch_add(dirs, Ordering::Relaxed);
    p.bytes.fetch_add(bytes, Ordering::Relaxed);

    let depth = task.depth.saturating_add(1);
    let mut tasks = Vec::new();
    let mut too_deep = Vec::new();
    for (id, step) in ids.into_iter().zip(next) {
        if let Next::Enter(path, device) = step {
            if depth > shared.max_depth {
                too_deep.push((id, path));
            } else {
                tasks.push(Task { node: id, path, depth, device });
            }
        }
    }
    if !too_deep.is_empty() {
        let mut tree = lock(&shared.tree);
        for (id, path) in too_deep {
            tree.mark(id, Flags::TOO_DEEP);
            p.problem(&path, "nested too deep to follow".to_string());
        }
    }
    shared.queue.push(tasks);
}
