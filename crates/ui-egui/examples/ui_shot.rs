//! Render WhatSpace's window to PNG files without showing it on screen (no focus stealing,
//! works with a locked screen). One picture per view.
//!
//!   cargo run --release -p whatspace-ui-egui --example ui_shot -- <out-dir> [--dark] [folder]
//!
//! Without a folder it shows a made-up demo disk (invented names and sizes, the same every time),
//! which is what the pictures in the README are of. With a folder it scans it first.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use whatspace_model::{Entry, NodeId, Tree};
use whatspace_ui_egui::{App, NoHost, Prefs, Tab, ThemeChoice};

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out = PathBuf::from(args.first().ok_or("usage: ui_shot <out-dir> [--dark] [folder]")?);
    let dark = args.iter().any(|a| a == "--dark");
    let folder = args.iter().skip(1).find(|a| !a.starts_with("--")).map(PathBuf::from);
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;

    let theme = if dark { ThemeChoice::Dark } else { ThemeChoice::Light };
    let mut app = App::new(Prefs { theme, recent: Vec::new(), ..Prefs::default() }, Box::new(NoHost));
    match &folder {
        Some(f) => {
            app.start_scan(f);
            let started = Instant::now();
            while app.is_scanning() && started.elapsed() < Duration::from_secs(600) {
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        None => app.show_tree(demo()),
    }
    // Open the largest folder and the largest folder in it, and select the second.
    if let Some(tree) = app.tree() {
        let (first, second) = {
            let t = whatspace_scan::lock(&tree);
            let first = t.sorted_children(NodeId::ROOT, whatspace_model::Metric::Size).into_iter().find(|c| t.node(*c).is_some_and(|n| n.is_dir()));
            let second =
                first.and_then(|f| t.sorted_children(f, whatspace_model::Metric::Size).into_iter().find(|c| t.node(*c).is_some_and(|n| n.is_dir())));
            (first, second)
        };
        if let Some(f) = first {
            app.set_open(f, true);
        }
        if let Some(s) = second.or(first) {
            app.reveal(s);
        }
    }
    let mut h = Harness::builder().with_size([1360.0, 820.0]).wgpu().build_ui_state(|ui, app: &mut App| app.ui(ui), app);
    let suffix = if dark { "-dark" } else { "" };
    for (tab, name) in
        [(Tab::Details, "details"), (Tab::Treemap, "treemap"), (Tab::Extensions, "extensions"), (Tab::Largest, "largest"), (Tab::Ages, "ages")]
    {
        h.state_mut().set_tab(tab);
        h.run_steps(6);
        let image = h.render()?;
        let path = out.join(format!("{name}{suffix}.png"));
        image.save(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        println!("wrote {}", path.display());
    }
    Ok(())
}

/// A made-up disk: invented folders and files with sizes from a fixed sequence.
fn demo() -> Tree {
    let root: &Path = if cfg!(windows) { Path::new("D:\\") } else { Path::new("/Volumes/Studio") };
    let mut t = Tree::new(root, 1_760_000_000);
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    let mut next = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed
    };
    let now = 1_762_000_000i64;
    let tops = t.add_children(
        NodeId::ROOT,
        ["Video projects", "Photo library", "Games", "Music", "Backups", "Documents", "Downloads", "Code"]
            .iter()
            .map(|n| Entry::dir(*n, now))
            .collect(),
    );
    let kinds: [(&str, &[&str], u64); 8] = [
        ("Video projects", &["mov", "mp4", "braw", "wav", "xml"], 9_000_000_000),
        ("Photo library", &["jpg", "cr3", "heic", "png", "xmp"], 40_000_000),
        ("Games", &["pak", "dll", "exe", "bin", "dat"], 3_000_000_000),
        ("Music", &["flac", "mp3", "m4a", "jpg"], 60_000_000),
        ("Backups", &["zip", "7z", "img", "vhdx"], 20_000_000_000),
        ("Documents", &["pdf", "docx", "xlsx", "pptx", "txt"], 8_000_000),
        ("Downloads", &["iso", "zip", "msi", "dmg", "pdf", "mp4"], 2_000_000_000),
        ("Code", &["rs", "js", "rlib", "json", "o", "pdb"], 3_000_000),
    ];
    for (dir, (name, exts, scale)) in tops.iter().zip(kinds) {
        let subs: Vec<Entry> =
            (0..(3 + next() % 5)).map(|i| Entry::dir(format!("{} {}", name.split(' ').next().unwrap_or(name), 2019 + i), now)).collect();
        let sub_ids = t.add_children(*dir, subs);
        for sub in sub_ids {
            let count = 6 + next() % 40;
            let files: Vec<Entry> = (0..count)
                .map(|i| {
                    let ext = exts[(next() % exts.len() as u64) as usize];
                    let size = (next() % scale).max(1000);
                    let age = (next() % 2500) as i64 * 86_400;
                    Entry::file(format!("file-{i:03}.{ext}"), size, size.div_ceil(4096) * 4096, now - age)
                })
                .collect();
            t.add_children(sub, files);
        }
    }
    t
}
