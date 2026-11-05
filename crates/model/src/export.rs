//! Writing a part of the tree out: CSV for spreadsheets, JSON for programs.

use std::borrow::Cow;
use std::io::{self, Write};

use crate::time::format_utc;
use crate::tree::{Kind, Metric, NodeId, Tree};
use crate::units::percent;

/// What an export includes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    /// Levels below the starting folder (0: the folder alone).
    pub max_depth: usize,
    /// Files as well as folders.
    pub include_files: bool,
    /// What children are sorted by.
    pub metric: Metric,
}

impl Default for Options {
    fn default() -> Self {
        Options { max_depth: 3, include_files: false, metric: Metric::Size }
    }
}

/// The deepest a JSON export nests, whatever is asked: deeper folders are left out.
pub const MAX_JSON_DEPTH: usize = 128;

/// A CSV field, quoted when it has to be. A field a spreadsheet would read as a formula
/// (starting with `=`, `+`, `-` or `@`) gets a leading apostrophe.
pub fn csv_field(s: &str) -> Cow<'_, str> {
    let formula = s.starts_with(['=', '+', '-', '@']);
    let s: Cow<'_, str> = if formula { Cow::Owned(format!("'{s}")) } else { Cow::Borrowed(s) };
    if s.contains([',', '"', '\n', '\r']) { Cow::Owned(format!("\"{}\"", s.replace('"', "\"\""))) } else { s }
}

/// The folder `root` and what is beneath it as CSV, one line per folder (and file), each folder
/// followed by its contents, largest first.
pub fn write_csv(tree: &Tree, root: NodeId, opts: &Options, out: &mut (impl Write + ?Sized)) -> io::Result<()> {
    writeln!(out, "Path,Type,Size,Allocated,Files,Folders,Modified (UTC),Percent of parent,Unreadable")?;
    let mut stack = vec![(root, 0usize)];
    while let Some((id, depth)) = stack.pop() {
        let Some(node) = tree.live(id) else { continue };
        let parent_total = tree.parent(id).and_then(|p| tree.node(p)).map_or(node.metric(opts.metric), |p| p.metric(opts.metric));
        let path = tree.path_of(id).display().to_string();
        let kind = match node.kind {
            Kind::Dir => "Folder",
            Kind::File => "File",
            Kind::Link => "Link",
            Kind::Other => "Other",
        };
        writeln!(
            out,
            "{},{kind},{},{},{},{},{},{:.2},{}",
            csv_field(&path),
            node.size,
            node.allocated,
            node.files,
            node.dirs,
            format_utc(node.modified),
            percent(node.metric(opts.metric), parent_total),
            node.errors
        )?;
        if depth < opts.max_depth {
            for kid in tree.sorted_children(id, opts.metric).into_iter().rev() {
                if opts.include_files || tree.node(kid).is_some_and(|n| n.is_dir()) {
                    stack.push((kid, depth + 1));
                }
            }
        }
    }
    Ok(())
}

/// The folder `root` and what is beneath it as one JSON object, children nested and largest
/// first.
pub fn write_json(tree: &Tree, root: NodeId, opts: &Options, out: &mut (impl Write + ?Sized)) -> io::Result<()> {
    let depth = opts.max_depth.min(MAX_JSON_DEPTH);
    write_json_node(tree, root, opts, depth, true, out)?;
    writeln!(out)
}

fn write_json_node(tree: &Tree, id: NodeId, opts: &Options, depth_left: usize, top: bool, out: &mut (impl Write + ?Sized)) -> io::Result<()> {
    let Some(node) = tree.live(id) else { return write!(out, "null") };
    let name = if top { tree.path_of(id).display().to_string() } else { tree.name(id).to_string() };
    let kind = match node.kind {
        Kind::Dir => "folder",
        Kind::File => "file",
        Kind::Link => "link",
        Kind::Other => "other",
    };
    write!(
        out,
        "{{\"name\":{},\"type\":\"{kind}\",\"size\":{},\"allocated\":{},\"files\":{},\"folders\":{},\"modified\":{},\"unreadable\":{}",
        serde_json::to_string(&name).unwrap_or_else(|_| "\"\"".into()),
        node.size,
        node.allocated,
        node.files,
        node.dirs,
        serde_json::to_string(&format_utc(node.modified)).unwrap_or_else(|_| "\"\"".into()),
        node.errors
    )?;
    if node.is_dir() && depth_left > 0 {
        write!(out, ",\"children\":[")?;
        let mut first = true;
        for kid in tree.sorted_children(id, opts.metric) {
            if !opts.include_files && !tree.node(kid).is_some_and(|n| n.is_dir()) {
                continue;
            }
            if !first {
                write!(out, ",")?;
            }
            first = false;
            write_json_node(tree, kid, opts, depth_left - 1, false, out)?;
        }
        write!(out, "]")?;
    }
    write!(out, "}}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tree::Entry;

    fn tree() -> Tree {
        let mut t = Tree::new("/d", 0);
        let ids = t.add_children(NodeId::ROOT, vec![Entry::dir("a, \"quoted\"", 0), Entry::file("=sum.csv", 10, 4096, 1_762_268_580)]);
        let sub = t.add_children(ids[0], vec![Entry::dir("deeper", 0), Entry::file("big.bin", 90, 4096, 0)]);
        t.add_children(sub[0], vec![Entry::file("tiny", 1, 4096, 0)]);
        t
    }

    #[test]
    fn csv_quotes_and_defuses() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_field("=1+1"), "'=1+1");
        assert_eq!(csv_field("-x,y"), "\"'-x,y\"");
    }

    #[test]
    fn csv_folders_then_their_contents() {
        let t = tree();
        let mut out = Vec::new();
        write_csv(&t, NodeId::ROOT, &Options { max_depth: 5, include_files: true, metric: Metric::Size }, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 7, "{text}");
        assert!(lines[1].starts_with("/d,Folder,101,"), "{}", lines[1]);
        assert!(lines[2].contains("\"\"quoted\"\""), "{}", lines[2]);
        assert!(lines[3].contains("big.bin\",File,90"), "largest first, quoted for its folder's comma: {}", lines[3]);
        assert!(text.contains("'=sum.csv") || text.contains("=sum.csv,File"), "{text}");
        let mut folders = Vec::new();
        write_csv(&t, NodeId::ROOT, &Options { max_depth: 1, include_files: false, metric: Metric::Size }, &mut folders).unwrap();
        assert_eq!(String::from_utf8(folders).unwrap().lines().count(), 3, "header, root, one folder");
    }

    #[test]
    fn json_nests_and_parses() {
        let t = tree();
        let mut out = Vec::new();
        write_json(&t, NodeId::ROOT, &Options { max_depth: 10, include_files: true, metric: Metric::Size }, &mut out).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["name"], "/d");
        assert_eq!(v["size"], 101);
        assert_eq!(v["children"][0]["name"], "a, \"quoted\"");
        assert_eq!(v["children"][0]["children"][0]["name"], "big.bin");
        assert_eq!(v["children"][1]["modified"], "2025-11-04T15:03:00Z");
        let mut shallow = Vec::new();
        write_json(&t, NodeId::ROOT, &Options { max_depth: 0, include_files: true, metric: Metric::Size }, &mut shallow).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&shallow).unwrap();
        assert!(v.get("children").is_none());
    }
}
