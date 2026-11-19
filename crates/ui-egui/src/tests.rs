//! The window, driven frame by frame without a screen.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use whatspace_model::{Entry, Flags, NodeId, Tree};

use super::*;

/// A host that remembers what it was asked to put in the trash, and puts nothing there.
#[derive(Clone, Default)]
struct FakeHost {
    trashed: Arc<Mutex<Vec<PathBuf>>>,
}

impl Host for FakeHost {
    fn pick_folder(&mut self) -> Option<PathBuf> {
        None
    }
    fn save_file(&mut self, _name: &str, _extension: &str) -> Option<PathBuf> {
        None
    }
    fn open(&mut self, _path: &Path) -> Result<(), String> {
        Ok(())
    }
    fn reveal(&mut self, _path: &Path) -> Result<(), String> {
        Ok(())
    }
    fn trash(&mut self, path: &Path) -> Result<(), String> {
        self.trashed.lock().map_err(|e| e.to_string())?.push(path.to_path_buf());
        Ok(())
    }
}

fn sample_tree() -> Tree {
    let mut t = Tree::new(if cfg!(windows) { "C:\\sample" } else { "/sample" }, 1_700_000_000);
    let top = t.add_children(
        NodeId::ROOT,
        vec![
            Entry::dir("Videos", 1_700_000_000),
            Entry::dir("Photos", 1_690_000_000),
            Entry::dir("Locked", 0).with_flags(Flags::DENIED),
            Entry::dir("Other drive", 0).with_flags(Flags::OTHER_FS),
            Entry::file("notes.txt", 1200, 4096, 1_650_000_000),
        ],
    );
    t.add_children(
        top[0],
        vec![
            Entry::file("holiday.mp4", 4_000_000_000, 4_000_002_048, 1_700_000_000),
            Entry::file("clip.mov", 300_000_000, 300_003_328, 1_600_000_000),
        ],
    );
    let photos = t.add_children(top[1], vec![Entry::dir("2024", 0), Entry::file("cover.png", 2_000_000, 2_002_944, 1_500_000_000)]);
    let many: Vec<Entry> = (0..300)
        .map(|i| Entry::file(format!("IMG_{i:04}.jpg"), 3_000_000 + i * 1000, 3_002_368 + i * 1000, 1_710_000_000 - i as i64 * 86_400))
        .collect();
    t.add_children(photos[0], many);
    t
}

fn harness(app: App) -> Harness<'static, App> {
    Harness::builder().with_size([1280.0, 800.0]).build_ui_state(|ui, app: &mut App| app.ui(ui), app)
}

#[test]
fn the_start_page_draws() {
    let mut h = harness(App::new(Prefs::default(), Box::new(NoHost)));
    h.run_steps(3);
    assert!(h.state().tree().is_none());
    assert!(h.query_by_label("Choose a folder…").is_some());
}

#[test]
fn every_tab_draws_a_tree() {
    for theme in [ThemeChoice::Light, ThemeChoice::Dark] {
        let mut app = App::new(Prefs { theme, ..Prefs::default() }, Box::new(NoHost));
        app.show_tree(sample_tree());
        let mut h = harness(app);
        for tab in Tab::ALL {
            h.state_mut().set_tab(tab);
            h.run_steps(3);
        }
        // Allocated instead of size, files hidden, every unit style: still draws.
        h.state_mut().prefs.metric = Metric::Allocated;
        h.state_mut().prefs.show_files = false;
        for style in UnitStyle::ALL {
            h.state_mut().prefs.units = style;
            h.state_mut().set_tab(Tab::Treemap);
            h.run_steps(2);
        }
    }
}

#[test]
fn opening_folders_and_selecting() {
    let mut app = App::new(Prefs::default(), Box::new(NoHost));
    app.show_tree(sample_tree());
    let mut h = harness(app);
    h.run_steps(2);
    let photos = {
        let tree = h.state().tree().unwrap();
        let t = lock(&tree);
        t.find(&t.root_path().join("Photos")).unwrap()
    };
    h.state_mut().actions.push(Action::Toggle(photos));
    h.state_mut().actions.push(Action::Select(photos));
    h.run_steps(2);
    assert_eq!(h.state().selected(), Some(photos));
    assert!(h.state().expanded.contains(&photos));
    assert_eq!(h.state().treemap_root, Some(photos), "the treemap follows a folder selected in the tree");
    // A file picked deep down from a list opens the folders above it.
    let tree = h.state().tree().unwrap();
    let deep = {
        let t = lock(&tree);
        let year = t.find(&t.root_path().join("Photos").join("2024")).unwrap();
        t.children(year)[0]
    };
    h.state_mut().actions.push(Action::Reveal(deep));
    h.state_mut().set_tab(Tab::Largest);
    h.run_steps(3);
    assert_eq!(h.state().selected(), Some(deep));
    assert_eq!(h.state().expanded.len(), 3, "root, Photos and 2024");
}

#[test]
fn moving_to_the_trash_asks_first_and_takes_it_off_the_totals() {
    let host = FakeHost::default();
    let mut app = App::new(Prefs::default(), Box::new(host.clone()));
    app.show_tree(sample_tree());
    let mut h = harness(app);
    h.run_steps(2);
    let tree = h.state().tree().unwrap();
    let videos = {
        let t = lock(&tree);
        t.find(&t.root_path().join("Videos")).unwrap()
    };
    let before = lock(&tree).node(NodeId::ROOT).unwrap().size;
    h.state_mut().actions.push(Action::AskTrash(videos));
    h.run_steps(2);
    assert!(host.trashed.lock().unwrap().is_empty(), "nothing goes before the owner says so");
    let bin = if cfg!(windows) { "Move to the Recycle Bin" } else { "Move to the Trash" };
    h.get_by_label(bin).click();
    h.run_steps(3);
    assert_eq!(host.trashed.lock().unwrap().len(), 1);
    assert!(host.trashed.lock().unwrap()[0].ends_with("Videos"));
    let after = lock(&tree).node(NodeId::ROOT).unwrap().size;
    assert_eq!(before - after, 4_300_000_000);
    // The root itself is never offered.
    h.state_mut().actions.push(Action::AskTrash(NodeId::ROOT));
    h.run_steps(2);
    assert!(h.query_by_label(bin).is_none());
}

#[test]
fn a_real_scan_through_the_window() {
    let dir = std::env::temp_dir().join(format!("whatspace-ui-scan-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("a").join("b")).unwrap();
    std::fs::write(dir.join("a").join("one.bin"), vec![0u8; 10_000]).unwrap();
    std::fs::write(dir.join("a").join("b").join("two.bin"), vec![0u8; 20_000]).unwrap();
    let mut app = App::new(Prefs::default(), Box::new(NoHost));
    app.start_scan(&dir);
    let mut h = harness(app);
    let started = Instant::now();
    while h.state().is_scanning() && started.elapsed() < Duration::from_secs(30) {
        h.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!h.state().is_scanning());
    for tab in Tab::ALL {
        h.state_mut().set_tab(tab);
        h.run_steps(2);
    }
    let tree = h.state().tree().unwrap();
    assert_eq!(lock(&tree).node(NodeId::ROOT).unwrap().size, 30_000);
    // Scanning one folder again.
    std::fs::write(dir.join("a").join("b").join("three.bin"), vec![0u8; 5_000]).unwrap();
    let b = {
        let t = lock(&tree);
        t.find(&dir.join("a").join("b")).unwrap()
    };
    h.state_mut().actions.push(Action::Rescan(b));
    h.run_steps(1);
    let started = Instant::now();
    while h.state().is_scanning() && started.elapsed() < Duration::from_secs(30) {
        h.step();
        std::thread::sleep(Duration::from_millis(10));
    }
    h.run_steps(2);
    assert_eq!(lock(&tree).node(NodeId::ROOT).unwrap().size, 35_000);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn preferences_survive_a_round_trip() {
    let prefs = Prefs { units: UnitStyle::Si, metric: Metric::Allocated, recent: vec!["/x".into()], ..Prefs::default() };
    let text = serde_json::to_string(&prefs).unwrap();
    let back: Prefs = serde_json::from_str(&text).unwrap();
    assert_eq!(back, prefs);
    // Old or partial preferences fill in the rest.
    let partial: Prefs = serde_json::from_str("{\"show_files\": false}").unwrap();
    assert!(!partial.show_files);
    assert_eq!(partial.units, UnitStyle::platform_default());
}
