//! What is worked out from a part of the tree: space by file type, the largest files, and how
//! old the files are.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use crate::category::Category;
use crate::tree::{Kind, Metric, NodeId, Tree};

/// Space taken by one file extension.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionStat {
    /// Lower case, without the dot; empty for files without one.
    pub extension: String,
    pub category: Category,
    pub size: u64,
    pub allocated: u64,
    pub files: u64,
}

impl ExtensionStat {
    pub fn metric(&self, metric: Metric) -> u64 {
        match metric {
            Metric::Size => self.size,
            Metric::Allocated => self.allocated,
        }
    }

    /// The extension as shown: ".jpg", or "(no extension)".
    pub fn label(&self) -> String {
        if self.extension.is_empty() { "(no extension)".to_string() } else { format!(".{}", self.extension) }
    }
}

/// The extension of a file name, lower case and without the dot. A leading dot alone
/// (`.bashrc`) is not an extension, nor is anything over 16 characters or with spaces in it.
pub fn extension_of(name: &str) -> Option<String> {
    let (stem, ext) = name.rsplit_once('.')?;
    if stem.is_empty() || ext.is_empty() || ext.chars().count() > 16 || ext.contains(char::is_whitespace) {
        return None;
    }
    Some(ext.to_lowercase())
}

/// Space by extension of every file beneath `under`, largest first by `metric`.
pub fn extensions(tree: &Tree, under: NodeId, metric: Metric) -> Vec<ExtensionStat> {
    let mut by_ext: HashMap<String, ExtensionStat> = HashMap::new();
    tree.walk(under, |id, node| {
        if node.kind != Kind::File {
            return;
        }
        let ext = extension_of(tree.name(id)).unwrap_or_default();
        let stat = by_ext.entry(ext).or_insert_with_key(|ext| ExtensionStat {
            extension: ext.clone(),
            category: if ext.is_empty() { Category::Other } else { Category::of_extension(ext) },
            size: 0,
            allocated: 0,
            files: 0,
        });
        stat.size = stat.size.saturating_add(node.size);
        stat.allocated = stat.allocated.saturating_add(node.allocated);
        stat.files += 1;
    });
    let mut out: Vec<ExtensionStat> = by_ext.into_values().collect();
    out.sort_by(|a, b| b.metric(metric).cmp(&a.metric(metric)).then_with(|| a.extension.cmp(&b.extension)));
    out
}

/// The `limit` largest files beneath `under` by `metric`, largest first.
pub fn largest_files(tree: &Tree, under: NodeId, metric: Metric, limit: usize) -> Vec<NodeId> {
    if limit == 0 {
        return Vec::new();
    }
    let mut heap: BinaryHeap<Reverse<(u64, NodeId)>> = BinaryHeap::with_capacity(limit + 1);
    tree.walk(under, |id, node| {
        if node.kind != Kind::File {
            return;
        }
        heap.push(Reverse((node.metric(metric), id)));
        if heap.len() > limit {
            heap.pop();
        }
    });
    let mut out: Vec<(u64, NodeId)> = heap.into_iter().map(|Reverse(x)| x).collect();
    out.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    out.into_iter().map(|(_, id)| id).collect()
}

/// Files last changed within one span of time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgeBucket {
    pub label: &'static str,
    pub size: u64,
    pub allocated: u64,
    pub files: u64,
}

impl AgeBucket {
    pub fn metric(&self, metric: Metric) -> u64 {
        match metric {
            Metric::Size => self.size,
            Metric::Allocated => self.allocated,
        }
    }
}

/// The spans, by the age in days they end at.
const SPANS: [(&str, i64); 7] = [
    ("Last 30 days", 30),
    ("1 to 3 months", 91),
    ("3 to 6 months", 182),
    ("6 to 12 months", 365),
    ("1 to 2 years", 730),
    ("2 to 5 years", 1826),
    ("Over 5 years", i64::MAX),
];

/// How old the files beneath `under` are, on `now` (seconds since 1970): one bucket per span,
/// newest first, then "Unknown" for files without a date.
pub fn ages(tree: &Tree, under: NodeId, now: i64) -> Vec<AgeBucket> {
    let mut buckets: Vec<AgeBucket> = SPANS.iter().map(|(label, _)| AgeBucket { label, size: 0, allocated: 0, files: 0 }).collect();
    buckets.push(AgeBucket { label: "Unknown", size: 0, allocated: 0, files: 0 });
    let unknown = buckets.len() - 1;
    tree.walk(under, |_, node| {
        if node.kind != Kind::File {
            return;
        }
        let slot = if node.modified == 0 {
            unknown
        } else {
            let days = now.saturating_sub(node.modified).max(0) / 86_400;
            SPANS.iter().position(|(_, end)| days <= *end).unwrap_or(unknown)
        };
        if let Some(b) = buckets.get_mut(slot) {
            b.size = b.size.saturating_add(node.size);
            b.allocated = b.allocated.saturating_add(node.allocated);
            b.files += 1;
        }
    });
    buckets
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::Entry;

    const DAY: i64 = 86_400;
    const NOW: i64 = 1_800_000_000;

    fn tree() -> Tree {
        let mut t = Tree::new("/d", 0);
        let ids = t.add_children(
            NodeId::ROOT,
            vec![
                Entry::dir("sub", 0),
                Entry::file("a.JPG", 5000, 8192, NOW - 2 * DAY),
                Entry::file("b.jpg", 1000, 4096, NOW - 100 * DAY),
                Entry::file("Makefile", 300, 4096, NOW - 3000 * DAY),
                Entry::file("unknown.bin", 10, 4096, 0),
            ],
        );
        t.add_children(ids[0], vec![Entry::file("song.flac", 9000, 12288, NOW + DAY), Entry::file(".hidden", 1, 4096, NOW - 400 * DAY)]);
        t
    }

    #[test]
    fn extensions_add_up() {
        let t = tree();
        let ext = extensions(&t, NodeId::ROOT, Metric::Size);
        let labels: Vec<String> = ext.iter().map(ExtensionStat::label).collect();
        assert_eq!(labels, [".flac", ".jpg", "(no extension)", ".bin"]);
        let jpg = &ext[1];
        assert_eq!((jpg.size, jpg.files, jpg.category), (6000, 2, Category::Image));
        let none = &ext[2];
        assert_eq!((none.size, none.files), (301, 2), "Makefile and .hidden");
        assert_eq!(extension_of("archive.tar.gz").as_deref(), Some("gz"));
        assert_eq!(extension_of("no extension here. really"), None);
        assert_eq!(extension_of("trailing."), None);
    }

    #[test]
    fn largest_files_first() {
        let t = tree();
        let top: Vec<&str> = largest_files(&t, NodeId::ROOT, Metric::Size, 3).iter().map(|id| t.name(*id)).collect();
        assert_eq!(top, ["song.flac", "a.JPG", "b.jpg"]);
        assert!(largest_files(&t, NodeId::ROOT, Metric::Size, 0).is_empty());
        assert_eq!(largest_files(&t, NodeId::ROOT, Metric::Size, 100).len(), 6);
    }

    #[test]
    fn ages_by_span() {
        let t = tree();
        let a = ages(&t, NodeId::ROOT, NOW);
        assert_eq!(a.len(), 8);
        assert_eq!(a[0].label, "Last 30 days");
        assert_eq!(a[0].files, 2, "two days old, and one dated in the future");
        assert_eq!(a[2].files, 1, "100 days");
        assert_eq!(a[4].files, 1, "400 days");
        assert_eq!(a[6].files, 1, "over five years");
        assert_eq!(a[7].files, 1, "no date");
        assert_eq!(a.iter().map(|b| b.files).sum::<u64>(), 6);
    }
}
