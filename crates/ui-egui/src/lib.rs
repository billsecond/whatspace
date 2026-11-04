//! WhatSpace's window, in egui: the folder tree with its share bars on the left; Details,
//! Treemap, Extensions, Largest files and Ages on the right; the drives to start from when
//! nothing is scanned yet.
//!
//! The front end only reads the tree and asks for things to happen. Anything that reaches
//! outside the window (a file dialog, opening a file, the trash) goes through [`Host`], which the
//! desktop app provides and tests replace. Scans run on the scanner's own threads; the window
//! looks at the tree they fill in a few times a second, so it stays responsive however big the
//! disk is.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod details;
mod rows;
mod start;
mod stats_view;
mod theme;
mod tree_view;
mod treemap_view;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use egui::{Key, RichText, Ui};
use serde::{Deserialize, Serialize};
use whatspace_fs::{Volume, volumes};
use whatspace_model::stats::{AgeBucket, ExtensionStat};
use whatspace_model::units::group;
use whatspace_model::{Metric, NodeId, Scale, SizeFormat, Tree, UnitStyle};
use whatspace_scan::{Options, Problem, Scan, SharedTree, Snapshot, VolumeInfo, lock};

pub use theme::Palette;

/// What the window asks of the system around it.
pub trait Host {
    /// Ask the owner for a folder to scan.
    fn pick_folder(&mut self) -> Option<PathBuf>;
    /// Ask the owner where to save a file, suggesting `name`.
    fn save_file(&mut self, name: &str, extension: &str) -> Option<PathBuf>;
    /// Open a file or folder with the program the system uses for it.
    fn open(&mut self, path: &Path) -> Result<(), String>;
    /// Show a file or folder in the system's file manager.
    fn reveal(&mut self, path: &Path) -> Result<(), String>;
    /// Move a file or folder to the Recycle Bin or Trash.
    fn trash(&mut self, path: &Path) -> Result<(), String>;
}

/// A host that can do none of it (tests, and where nothing is available).
pub struct NoHost;

impl Host for NoHost {
    fn pick_folder(&mut self) -> Option<PathBuf> {
        None
    }
    fn save_file(&mut self, _name: &str, _extension: &str) -> Option<PathBuf> {
        None
    }
    fn open(&mut self, _path: &Path) -> Result<(), String> {
        Err("not available here".into())
    }
    fn reveal(&mut self, _path: &Path) -> Result<(), String> {
        Err("not available here".into())
    }
    fn trash(&mut self, _path: &Path) -> Result<(), String> {
        Err("not available here".into())
    }
}

/// Light, dark, or whatever the system uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

/// What the owner chose, kept between runs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub theme: ThemeChoice,
    pub units: UnitStyle,
    pub scale: Scale,
    pub metric: Metric,
    pub one_file_system: bool,
    pub hard_links: bool,
    pub show_files: bool,
    pub export_files: bool,
    /// Folders scanned lately, newest first.
    pub recent: Vec<String>,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            theme: ThemeChoice::System,
            units: UnitStyle::platform_default(),
            scale: Scale::Auto,
            metric: Metric::Size,
            one_file_system: true,
            hard_links: cfg!(unix),
            show_files: true,
            export_files: false,
            recent: Vec::new(),
        }
    }
}

impl Prefs {
    pub fn size_format(&self) -> SizeFormat {
        SizeFormat::new(self.units, self.scale)
    }

    fn remember(&mut self, path: &Path) {
        let text = path.display().to_string();
        self.recent.retain(|p| *p != text);
        self.recent.insert(0, text);
        self.recent.truncate(8);
    }
}

/// The views beside the tree.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Details,
    Treemap,
    Extensions,
    Largest,
    Ages,
}

impl Tab {
    const ALL: [Tab; 5] = [Tab::Details, Tab::Treemap, Tab::Extensions, Tab::Largest, Tab::Ages];

    fn label(self) -> &'static str {
        match self {
            Tab::Details => "Details",
            Tab::Treemap => "Treemap",
            Tab::Extensions => "Extensions",
            Tab::Largest => "Largest files",
            Tab::Ages => "File ages",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExportKind {
    Csv,
    Json,
}

/// Something a view asked for, done once the frame's look at the tree is over.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Action {
    /// Select (from the tree, Details or the keyboard): the treemap follows.
    Select(NodeId),
    /// Select from the treemap: the treemap stays where it is.
    Pick(NodeId),
    /// Select from a list: open the folders above it in the tree and scroll to it.
    Reveal(NodeId),
    Toggle(NodeId),
    SetOpen(NodeId, bool),
    ZoomTreemap(NodeId),
    Open(NodeId),
    ShowInFileManager(NodeId),
    CopyPath(NodeId),
    Rescan(NodeId),
    AskTrash(NodeId),
    Export(NodeId, ExportKind),
    Scan(PathBuf),
}

/// What was worked out from the tree, kept until the tree changes.
#[derive(Default)]
pub(crate) struct Caches {
    /// The tree generation these were made from.
    pub stamp: u64,
    /// When the caches last caught up with a growing tree.
    pub refreshed_at: f64,
    pub sorted: HashMap<NodeId, Vec<NodeId>>,
    pub rows: Option<Vec<tree_view::Row>>,
    pub treemap: Option<treemap_view::Layout>,
    pub stats: Option<StatsCache>,
}

pub(crate) struct StatsCache {
    pub node: NodeId,
    pub metric: Metric,
    pub made_at: f64,
    pub extensions: Vec<ExtensionStat>,
    pub largest: Vec<NodeId>,
    pub ages: Vec<AgeBucket>,
}

impl Caches {
    pub fn sorted(&mut self, tree: &Tree, id: NodeId, metric: Metric) -> &[NodeId] {
        self.sorted.entry(id).or_insert_with(|| tree.sorted_children(id, metric))
    }
}

/// The window.
pub struct App {
    pub prefs: Prefs,
    host: Box<dyn Host>,
    pub(crate) volumes: Vec<Volume>,
    path_text: String,
    scan: Option<Scan>,
    pub(crate) tree: Option<SharedTree>,
    pub(crate) volume: Option<VolumeInfo>,
    problems: Vec<Problem>,
    snapshot: Snapshot,
    pub(crate) selected: Option<NodeId>,
    pub(crate) expanded: HashSet<NodeId>,
    pub(crate) treemap_root: Option<NodeId>,
    tab: Tab,
    pub(crate) details_sort: (usize, bool),
    pub(crate) filter: String,
    pub(crate) caches: Caches,
    pub(crate) actions: Vec<Action>,
    pub(crate) scroll_to_selected: bool,
    pub(crate) tz: jiff::tz::TimeZone,
    confirm_trash: Option<NodeId>,
    show_problems: bool,
    notice: Option<(String, f64)>,
    applied_theme: Option<ThemeChoice>,
}

impl App {
    pub fn new(prefs: Prefs, host: Box<dyn Host>) -> App {
        App {
            prefs,
            host,
            volumes: volumes::list(),
            path_text: String::new(),
            scan: None,
            tree: None,
            volume: None,
            problems: Vec::new(),
            snapshot: Snapshot::default(),
            selected: None,
            expanded: HashSet::new(),
            treemap_root: None,
            tab: Tab::default(),
            details_sort: (1, true),
            filter: String::new(),
            caches: Caches::default(),
            actions: Vec::new(),
            scroll_to_selected: false,
            tz: whatspace_model::time::local_zone(),
            confirm_trash: None,
            show_problems: false,
            notice: None,
            applied_theme: None,
        }
    }

    /// The tree on screen, if anything has been scanned.
    pub fn tree(&self) -> Option<SharedTree> {
        self.tree.clone()
    }

    pub fn is_scanning(&self) -> bool {
        self.scan.as_ref().is_some_and(|s| !s.is_done())
    }

    pub fn selected(&self) -> Option<NodeId> {
        self.selected
    }

    pub fn set_tab(&mut self, tab: Tab) {
        self.tab = tab;
    }

    pub fn tab(&self) -> Tab {
        self.tab
    }

    /// Select `id` and open the folders above it, as a click in a list would.
    pub fn reveal(&mut self, id: NodeId) {
        self.actions.push(Action::Reveal(id));
    }

    /// Open or close the folder `id` in the tree.
    pub fn set_open(&mut self, id: NodeId, open: bool) {
        self.actions.push(Action::SetOpen(id, open));
    }

    /// Show a tree that was made elsewhere (tests, screenshots).
    pub fn show_tree(&mut self, tree: Tree) {
        if let Some(scan) = self.scan.take() {
            scan.cancel();
        }
        self.path_text = tree.root_path().display().to_string();
        self.tree = Some(std::sync::Arc::new(std::sync::Mutex::new(tree)));
        self.reset_view();
        self.snapshot = Snapshot { done: true, ..Snapshot::default() };
    }

    /// Start scanning `path`, replacing whatever is shown.
    pub fn start_scan(&mut self, path: &Path) {
        if let Some(old) = self.scan.take() {
            old.cancel();
        }
        let options = self.options();
        match Scan::start(path, options) {
            Ok(scan) => {
                self.path_text = scan.root().display().to_string();
                self.prefs.remember(scan.root());
                self.tree = Some(scan.tree());
                self.volume = Some(scan.volume().clone());
                self.problems.clear();
                self.scan = Some(scan);
                self.reset_view();
            }
            Err(e) => self.say(format!("Could not scan {}: {e}", path.display())),
        }
    }

    fn options(&self) -> Options {
        Options { one_file_system: self.prefs.one_file_system, hard_links: self.prefs.hard_links, ..Options::default() }
    }

    fn reset_view(&mut self) {
        self.selected = Some(NodeId::ROOT);
        self.treemap_root = Some(NodeId::ROOT);
        self.expanded = HashSet::from([NodeId::ROOT]);
        self.caches = Caches::default();
        self.filter.clear();
        self.confirm_trash = None;
    }

    /// Back to the drives.
    fn close_tree(&mut self) {
        if let Some(scan) = self.scan.take() {
            scan.cancel();
        }
        self.tree = None;
        self.volume = None;
        self.caches = Caches::default();
        self.volumes = volumes::list();
    }

    fn say(&mut self, text: impl Into<String>) {
        self.notice = Some((text.into(), f64::NAN));
    }

    /// Draw the window. Call once a frame with the root `Ui`.
    pub fn ui(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        self.apply_theme(&ctx);
        let now = ctx.input(|i| i.time);
        if let Some((_, until)) = &mut self.notice {
            if until.is_nan() {
                *until = now + 6.0;
            } else if now > *until {
                self.notice = None;
            }
        }
        self.dropped_folders(&ctx);
        self.poll_scan(&ctx, now);

        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));

        match self.tree.clone() {
            None => {
                egui::CentralPanel::default().show(ui, |ui| self.start_page(ui));
            }
            Some(shared) => {
                {
                    let tree = lock(&shared);
                    self.refresh_caches(&tree, now);
                    self.keyboard(&ctx, &tree);
                    let width = ui.available_width();
                    egui::Panel::left("tree")
                        .resizable(true)
                        .default_size((width * 0.5).max(360.0))
                        .min_size(300.0)
                        .show(ui, |ui| self.tree_panel(ui, &tree));
                    egui::CentralPanel::default().show(ui, |ui| self.tabs(ui, &tree, now));
                }
                self.run_actions(&ctx);
            }
        }
        self.dialogs(&ctx);
    }

    fn apply_theme(&mut self, ctx: &egui::Context) {
        if self.applied_theme != Some(self.prefs.theme) {
            ctx.set_theme(match self.prefs.theme {
                ThemeChoice::System => egui::ThemePreference::System,
                ThemeChoice::Light => egui::ThemePreference::Light,
                ThemeChoice::Dark => egui::ThemePreference::Dark,
            });
            self.applied_theme = Some(self.prefs.theme);
        }
    }

    fn dropped_folders(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if let Some(folder) = dropped.into_iter().find(|p| p.is_dir()) {
            self.start_scan(&folder);
        }
    }

    fn poll_scan(&mut self, ctx: &egui::Context, _now: f64) {
        let Some(scan) = &self.scan else { return };
        self.snapshot = scan.snapshot();
        if self.snapshot.done {
            if self.problems.is_empty() {
                self.problems = scan.problems();
            }
        } else {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
    }

    /// Catch the caches up with the tree: at once when it stopped changing, at most a few
    /// times a second while a scan is growing it.
    fn refresh_caches(&mut self, tree: &Tree, now: f64) {
        let generation = tree.generation();
        if generation == self.caches.stamp {
            return;
        }
        let scanning = self.is_scanning();
        if scanning && now - self.caches.refreshed_at < 0.4 && self.caches.rows.is_some() {
            return;
        }
        self.caches.stamp = generation;
        self.caches.refreshed_at = now;
        self.caches.sorted.clear();
        self.caches.rows = None;
        self.caches.treemap = None;
        // The statistics walk the whole tree: while scanning, only every few seconds.
        if !scanning || self.caches.stats.as_ref().is_some_and(|s| now - s.made_at > 3.0) {
            self.caches.stats = None;
        }
        // A selection that went away (moved to the trash) falls back to its folder.
        if let Some(sel) = self.selected
            && tree.live(sel).is_none()
        {
            self.selected = Some(NodeId::ROOT);
        }
        if let Some(root) = self.treemap_root
            && tree.live(root).is_none()
        {
            self.treemap_root = Some(NodeId::ROOT);
        }
    }

    /// Forget what was worked out from the tree (the measure or a display option changed).
    fn invalidate(&mut self) {
        self.caches.sorted.clear();
        self.caches.rows = None;
        self.caches.treemap = None;
        self.caches.stats = None;
    }

    fn toolbar(&mut self, ui: &mut Ui) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if self.tree.is_some() && ui.button("Drives").on_hover_text("Back to the list of drives").clicked() {
                self.close_tree();
            }
            let drives = self.volumes.clone();
            egui::ComboBox::from_id_salt("drives").selected_text("Drive").width(70.0).show_ui(ui, |ui| {
                for v in &drives {
                    let label = format!("{}  {}  ({})", v.mount_point.display(), v.label, v.file_system_name());
                    if ui.selectable_label(false, label).clicked() {
                        self.actions.push(Action::Scan(v.mount_point.clone()));
                    }
                }
            });
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.path_text).hint_text("Folder to scan").desired_width((ui.available_width() - 520.0).max(160.0)),
            );
            if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) && !self.path_text.trim().is_empty() {
                self.actions.push(Action::Scan(PathBuf::from(self.path_text.trim())));
            }
            if ui.button("Browse…").on_hover_text("Choose a folder to scan (Ctrl+O)").clicked() {
                self.browse();
            }
            if self.is_scanning() {
                if ui.button("■ Stop").on_hover_text("Stop scanning (Esc); what was read stays").clicked()
                    && let Some(scan) = &self.scan
                {
                    scan.cancel();
                }
            } else if ui.button("▶ Scan").on_hover_text("Scan the folder in the box (F5 scans again)").clicked() {
                let text = self.path_text.trim().to_string();
                if !text.is_empty() {
                    self.actions.push(Action::Scan(PathBuf::from(text)));
                }
            }
            ui.separator();
            let before = self.prefs.metric;
            ui.selectable_value(&mut self.prefs.metric, Metric::Size, "Size").on_hover_text("Measure by the files' lengths");
            ui.selectable_value(&mut self.prefs.metric, Metric::Allocated, "Allocated")
                .on_hover_text("Measure by the space the files take on disk (clusters, compression, sparse files, cloud placeholders)");
            if before != self.prefs.metric {
                self.invalidate();
            }
            ui.separator();
            self.menus(ui);
        });
        ui.add_space(2.0);
        let actions = std::mem::take(&mut self.actions);
        for a in actions {
            if let Action::Scan(p) = a {
                self.start_scan(&p);
            } else {
                self.actions.push(a);
            }
        }
    }

    fn menus(&mut self, ui: &mut Ui) {
        ui.menu_button("Units", |ui| {
            for style in UnitStyle::ALL {
                ui.radio_value(&mut self.prefs.units, style, style.label());
            }
            ui.separator();
            for scale in Scale::ALL {
                ui.radio_value(&mut self.prefs.scale, scale, scale.label(self.prefs.units));
            }
        });
        ui.menu_button("Options", |ui| {
            ui.checkbox(&mut self.prefs.one_file_system, "Stay on one file system")
                .on_hover_text("Don't go into other drives or mounts inside the folder (like du -x). Takes effect at the next scan.");
            let hint = if cfg!(windows) {
                "Count a file with several names once. On Windows this opens every file, which makes scans slower. Takes effect at the next scan."
            } else {
                "Count a file with several names once. Takes effect at the next scan."
            };
            ui.checkbox(&mut self.prefs.hard_links, "Count hard links once").on_hover_text(hint);
            if ui.checkbox(&mut self.prefs.show_files, "Show files in the tree").changed() {
                self.caches.rows = None;
            }
            ui.separator();
            ui.label("Theme");
            ui.radio_value(&mut self.prefs.theme, ThemeChoice::System, "Like the system");
            ui.radio_value(&mut self.prefs.theme, ThemeChoice::Light, "Light");
            ui.radio_value(&mut self.prefs.theme, ThemeChoice::Dark, "Dark");
        });
        ui.add_enabled_ui(self.tree.is_some(), |ui| {
            ui.menu_button("Export", |ui| {
                let target = self.selected.unwrap_or(NodeId::ROOT);
                ui.checkbox(&mut self.prefs.export_files, "Include files");
                if ui.button("Selected folder as CSV…").clicked() {
                    self.actions.push(Action::Export(target, ExportKind::Csv));
                    ui.close();
                }
                if ui.button("Selected folder as JSON…").clicked() {
                    self.actions.push(Action::Export(target, ExportKind::Json));
                    ui.close();
                }
            });
        });
    }

    fn browse(&mut self) {
        if let Some(folder) = self.host.pick_folder() {
            self.start_scan(&folder);
        }
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        let pal = Palette::of(ui);
        let fmt = self.prefs.size_format();
        ui.horizontal(|ui| {
            let s = &self.snapshot;
            if self.is_scanning() {
                ui.spinner();
                let secs = s.elapsed.as_secs_f64().max(0.001);
                ui.label(format!(
                    "Scanning… {} files, {} folders, {} ({} files a second)",
                    group(s.files),
                    group(s.dirs),
                    fmt.format(s.bytes),
                    group((s.files as f64 / secs) as u64)
                ));
                let current = s.current.display().to_string();
                ui.add(egui::Label::new(RichText::new(current).color(pal.weak)).truncate());
            } else if let Some((text, _)) = &self.notice {
                ui.label(text.clone());
            } else if self.tree.is_some() {
                let what = if s.cancelled { "Stopped: showing what was read" } else { "Scanned" };
                if s.files > 0 || s.dirs > 0 {
                    ui.label(format!("{what} {} files and {} folders in {:.1} s", group(s.files), group(s.dirs), s.elapsed.as_secs_f64()));
                } else {
                    ui.label(what);
                }
            } else {
                ui.label(RichText::new("Choose a drive or a folder to scan").color(pal.weak));
            }
            if s.errors > 0
                && ui
                    .add(egui::Button::new(RichText::new(format!("⚠ {} could not be read", group(s.errors))).color(pal.warn)).frame(false))
                    .on_hover_text("Show which")
                    .clicked()
            {
                self.show_problems = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if let Some(v) = &self.volume {
                    let mut text = String::new();
                    if let Some(m) = &v.mount_point {
                        text.push_str(&m.display().to_string());
                        text.push_str("  ·  ");
                    }
                    if !v.file_system.is_empty() {
                        text.push_str(&volumes::pretty_file_system(&v.file_system));
                        text.push_str("  ·  ");
                    }
                    if v.total > 0 {
                        text.push_str(&format!(
                            "{} used of {}, {} free  ·  ",
                            fmt.format(v.total.saturating_sub(v.available)),
                            fmt.format(v.total),
                            fmt.format(v.available)
                        ));
                    }
                    text.push_str(&format!("{} allocation unit", fmt.format(v.cluster)));
                    let note = volumes::notes(&v.file_system);
                    let label = ui.label(RichText::new(text).color(pal.weak));
                    if !note.is_empty() {
                        label.on_hover_text(note);
                    }
                }
            });
        });
    }

    fn tabs(&mut self, ui: &mut Ui, tree: &Tree, now: f64) {
        ui.horizontal(|ui| {
            for tab in Tab::ALL {
                ui.selectable_value(&mut self.tab, tab, tab.label());
            }
        });
        ui.separator();
        match self.tab {
            Tab::Details => self.details(ui, tree),
            Tab::Treemap => self.treemap(ui, tree),
            Tab::Extensions => self.extensions(ui, tree, now),
            Tab::Largest => self.largest(ui, tree, now),
            Tab::Ages => self.ages(ui, tree, now),
        }
    }

    /// The folder the views beside the tree show: the selection, or the folder of a selected file.
    pub(crate) fn focus_folder(&self, tree: &Tree) -> NodeId {
        let sel = self.selected.filter(|s| tree.live(*s).is_some()).unwrap_or(NodeId::ROOT);
        match tree.node(sel) {
            Some(n) if n.is_dir() => sel,
            _ => tree.parent(sel).unwrap_or(NodeId::ROOT),
        }
    }

    fn keyboard(&mut self, ctx: &egui::Context, tree: &Tree) {
        if ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        let (esc, f5, open, copy, delete) = ctx.input(|i| {
            (
                i.key_pressed(Key::Escape),
                i.key_pressed(Key::F5),
                i.modifiers.command && i.key_pressed(Key::O),
                i.modifiers.command && i.key_pressed(Key::C),
                i.key_pressed(Key::Delete),
            )
        });
        if esc && let Some(scan) = &self.scan {
            scan.cancel();
        }
        if f5 && !self.is_scanning() {
            self.actions.push(Action::Scan(tree.root_path().to_path_buf()));
        }
        if open {
            self.browse();
        }
        if let Some(sel) = self.selected {
            if copy {
                self.actions.push(Action::CopyPath(sel));
            }
            if delete && sel != NodeId::ROOT {
                self.actions.push(Action::AskTrash(sel));
            }
        }
        self.tree_keys(ctx, tree);
    }

    fn run_actions(&mut self, ctx: &egui::Context) {
        let actions = std::mem::take(&mut self.actions);
        let Some(shared) = self.tree.clone() else { return };
        for action in actions {
            match action {
                Action::Select(id) => {
                    self.selected = Some(id);
                    let tree = lock(&shared);
                    self.treemap_root = Some(match tree.node(id) {
                        Some(n) if n.is_dir() => id,
                        _ => tree.parent(id).unwrap_or(NodeId::ROOT),
                    });
                }
                Action::Pick(id) => {
                    self.selected = Some(id);
                    self.open_to(&lock(&shared), id);
                }
                Action::Reveal(id) => {
                    self.selected = Some(id);
                    self.open_to(&lock(&shared), id);
                    self.scroll_to_selected = true;
                }
                Action::Toggle(id) => {
                    if !self.expanded.remove(&id) {
                        self.expanded.insert(id);
                    }
                    self.caches.rows = None;
                }
                Action::SetOpen(id, open) => {
                    if open {
                        self.expanded.insert(id);
                    } else {
                        self.expanded.remove(&id);
                    }
                    self.caches.rows = None;
                }
                Action::ZoomTreemap(id) => {
                    self.treemap_root = Some(id);
                    self.selected = Some(id);
                    self.open_to(&lock(&shared), id);
                }
                Action::Open(id) => {
                    let path = lock(&shared).path_of(id);
                    if let Err(e) = self.host.open(&path) {
                        self.say(format!("Could not open {}: {e}", path.display()));
                    }
                }
                Action::ShowInFileManager(id) => {
                    let path = lock(&shared).path_of(id);
                    if let Err(e) = self.host.reveal(&path) {
                        self.say(format!("Could not show {}: {e}", path.display()));
                    }
                }
                Action::CopyPath(id) => {
                    let path = lock(&shared).path_of(id).display().to_string();
                    ctx.copy_text(path.clone());
                    self.say(format!("Copied {path}"));
                }
                Action::Rescan(id) => {
                    if self.is_scanning() {
                        self.say("Wait for the scan to finish, or stop it, before scanning a folder again");
                        continue;
                    }
                    match Scan::rescan(shared.clone(), id, self.options()) {
                        Ok(scan) => {
                            self.problems.clear();
                            self.scan = Some(scan);
                            self.caches.rows = None;
                        }
                        Err(e) => self.say(format!("Could not scan again: {e}")),
                    }
                }
                Action::AskTrash(id) => {
                    if self.is_scanning() {
                        self.say("Wait for the scan to finish, or stop it, before moving anything to the trash");
                    } else if id != NodeId::ROOT {
                        self.confirm_trash = Some(id);
                    }
                }
                Action::Export(id, kind) => self.export(&shared, id, kind),
                Action::Scan(path) => self.start_scan(&path),
            }
        }
    }

    /// Open every folder above `id` in the tree.
    fn open_to(&mut self, tree: &Tree, id: NodeId) {
        let ancestry = tree.ancestry(id);
        let before = self.expanded.len();
        self.expanded.extend(ancestry.iter().take(ancestry.len().saturating_sub(1)).copied());
        if self.expanded.len() != before {
            self.caches.rows = None;
        }
    }

    fn export(&mut self, shared: &SharedTree, id: NodeId, kind: ExportKind) {
        let (name, ext) = {
            let tree = lock(shared);
            let base = tree.path_of(id).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "whatspace".into());
            match kind {
                ExportKind::Csv => (format!("{base}.csv"), "csv"),
                ExportKind::Json => (format!("{base}.json"), "json"),
            }
        };
        let Some(target) = self.host.save_file(&name, ext) else { return };
        let opts = whatspace_model::export::Options { max_depth: 64, include_files: self.prefs.export_files, metric: self.prefs.metric };
        let result = std::fs::File::create(&target).and_then(|f| {
            let mut w = std::io::BufWriter::new(f);
            let tree = lock(shared);
            match kind {
                ExportKind::Csv => whatspace_model::export::write_csv(&tree, id, &opts, &mut w)?,
                ExportKind::Json => whatspace_model::export::write_json(&tree, id, &opts, &mut w)?,
            }
            std::io::Write::flush(&mut w)
        });
        match result {
            Ok(()) => self.say(format!("Exported to {}", target.display())),
            Err(e) => self.say(format!("Could not export to {}: {e}", target.display())),
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(id) = self.confirm_trash {
            self.trash_dialog(ctx, id);
        }
        if self.show_problems {
            let mut open = true;
            let problems = self.problems.clone();
            let total = self.snapshot.errors;
            egui::Window::new("Could not be read").open(&mut open).default_size([640.0, 360.0]).show(ctx, |ui| {
                if (problems.len() as u64) < total {
                    ui.label(format!("The first {} of {}:", problems.len(), group(total)));
                }
                if ui.button("Copy the list").clicked() {
                    let text: Vec<String> = problems.iter().map(|p| format!("{}\t{}", p.path.display(), p.message)).collect();
                    ui.ctx().copy_text(text.join("\n"));
                }
                if cfg!(target_os = "macos") {
                    ui.label("Some folders on macOS can only be read with Full Disk Access (System Settings › Privacy & Security).");
                } else if cfg!(windows) {
                    ui.label("Some system folders can only be read when WhatSpace runs as an administrator.");
                }
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    for p in &problems {
                        ui.horizontal(|ui| {
                            ui.label(p.path.display().to_string());
                            ui.label(RichText::new(&p.message).weak());
                        });
                    }
                });
            });
            self.show_problems = open;
        }
    }

    fn trash_dialog(&mut self, ctx: &egui::Context, id: NodeId) {
        let Some(shared) = self.tree.clone() else {
            self.confirm_trash = None;
            return;
        };
        let (name, path, size, files, is_dir) = {
            let tree = lock(&shared);
            let Some(node) = tree.live(id) else {
                self.confirm_trash = None;
                return;
            };
            (tree.name(id).to_string(), tree.path_of(id), node.size, node.files, node.is_dir())
        };
        let bin = if cfg!(windows) { "Recycle Bin" } else { "Trash" };
        let fmt = self.prefs.size_format();
        let mut choice = None;
        let modal = egui::Modal::new(egui::Id::new("confirm-trash")).show(ctx, |ui| {
            ui.set_max_width(460.0);
            ui.heading(format!("Move to the {bin}?"));
            ui.add_space(6.0);
            ui.label(RichText::new(&name).strong());
            ui.label(RichText::new(path.display().to_string()).weak());
            ui.add_space(4.0);
            if is_dir {
                ui.label(format!("{} in {} files.", fmt.format(size), group(files)));
            } else {
                ui.label(fmt.format(size));
            }
            ui.label(format!("You can put it back from the {bin}."));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(format!("Move to the {bin}")).clicked() {
                    choice = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    choice = Some(false);
                }
            });
        });
        if modal.should_close() && choice.is_none() {
            choice = Some(false);
        }
        match choice {
            Some(true) => {
                self.confirm_trash = None;
                match self.host.trash(&path) {
                    Ok(()) => {
                        lock(&shared).remove(id);
                        self.say(format!("Moved {name} to the {bin}"));
                    }
                    Err(e) => self.say(format!("Could not move {name} to the {bin}: {e}")),
                }
            }
            Some(false) => self.confirm_trash = None,
            None => {}
        }
    }

    /// The menu for a node, wherever it is shown.
    pub(crate) fn node_menu(&mut self, ui: &mut Ui, tree: &Tree, id: NodeId) {
        let is_dir = tree.node(id).is_some_and(|n| n.is_dir());
        let manager = if cfg!(windows) {
            "Show in Explorer"
        } else if cfg!(target_os = "macos") {
            "Show in Finder"
        } else {
            "Show in file manager"
        };
        if ui.button("Open").clicked() {
            self.actions.push(Action::Open(id));
            ui.close();
        }
        if ui.button(manager).clicked() {
            self.actions.push(Action::ShowInFileManager(id));
            ui.close();
        }
        if ui.button("Copy path").clicked() {
            self.actions.push(Action::CopyPath(id));
            ui.close();
        }
        if is_dir {
            ui.separator();
            if ui.button("Show in treemap").clicked() {
                self.actions.push(Action::ZoomTreemap(id));
                self.tab = Tab::Treemap;
                ui.close();
            }
            if ui.button("Scan this folder again").clicked() {
                self.actions.push(Action::Rescan(id));
                ui.close();
            }
            if ui.button("Export as CSV…").clicked() {
                self.actions.push(Action::Export(id, ExportKind::Csv));
                ui.close();
            }
        }
        if id != NodeId::ROOT {
            ui.separator();
            let bin = if cfg!(windows) { "Move to Recycle Bin…" } else { "Move to Trash…" };
            if ui.button(bin).clicked() {
                self.actions.push(Action::AskTrash(id));
                ui.close();
            }
        }
    }

    /// A short line about what a node is, for tooltips.
    pub(crate) fn describe(&self, tree: &Tree, id: NodeId) -> String {
        let Some(n) = tree.node(id) else { return String::new() };
        let fmt = self.prefs.size_format();
        let mut s = format!("{}\nSize {}  ·  allocated {}", tree.path_of(id).display(), fmt.format(n.size), fmt.format(n.allocated));
        if n.is_dir() {
            s.push_str(&format!("\n{} files, {} folders", group(n.files), group(n.dirs)));
        }
        let modified = whatspace_model::time::format_time(n.modified, &self.tz);
        if !modified.is_empty() {
            s.push_str(&format!("\nModified {modified}"));
        }
        if let Some(note) = flag_note(n.flags) {
            s.push('\n');
            s.push_str(note);
        }
        s
    }
}

/// What a node's flags mean, in words.
pub(crate) fn flag_note(flags: whatspace_model::Flags) -> Option<&'static str> {
    use whatspace_model::Flags;
    if flags.contains(Flags::DENIED) {
        Some("Could not be read: access denied")
    } else if flags.contains(Flags::ERROR) {
        Some("Could not be read")
    } else if flags.contains(Flags::TOO_DEEP) {
        Some("Nested too deep to follow")
    } else if flags.contains(Flags::OTHER_FS) {
        Some("Another file system: not scanned (see Options)")
    } else if flags.contains(Flags::PSEUDO) {
        Some("A system file system with no real files: not scanned")
    } else if flags.contains(Flags::DUPLICATE) {
        Some("Already counted through the folders it is joined to")
    } else if flags.contains(Flags::HARD_LINK) {
        Some("Another name for a file counted elsewhere (hard link)")
    } else if flags.contains(Flags::CLOUD) {
        Some("Kept in the cloud: takes no space on this disk")
    } else if flags.contains(Flags::PACKED) {
        Some("Compressed or sparse: takes less space than its size")
    } else {
        None
    }
}

#[cfg(test)]
mod tests;
