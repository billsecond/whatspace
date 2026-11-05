//! The folder tree: every folder (and, if wanted, file) with its size, its share of the folder
//! above it as a bar, its file and folder counts and when it last changed. Largest first, like
//! TreeSize's main view. Only the open folders' rows exist, and only those on screen are drawn.

use egui::{Key, Rect, Sense, Ui, pos2, vec2};
use whatspace_model::units::{format_percent, group, percent};
use whatspace_model::{Category, Flags, Kind, NodeId, Tree};

use crate::rows::{self, Column, ROW_H};
use crate::theme::{Palette, category_color};
use crate::{Action, App};

/// One visible row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub id: NodeId,
    pub depth: u16,
    pub has_children: bool,
}

const COLUMNS: [Column; 5] = [
    Column::right("Size", 86.0),
    Column::left("Share of folder", 120.0),
    Column::right("Files", 78.0),
    Column::right("Folders", 66.0),
    Column::right("Modified", 122.0),
];

/// More rows than this are not shown (the rest of a folder with millions of files open).
const MAX_ROWS: usize = 500_000;

impl App {
    fn build_rows(&mut self, tree: &Tree) -> Vec<Row> {
        let metric = self.prefs.metric;
        let show_files = self.prefs.show_files;
        let mut rows = Vec::new();
        let mut stack = vec![(NodeId::ROOT, 0u16)];
        while let Some((id, depth)) = stack.pop() {
            if tree.live(id).is_none() {
                continue;
            }
            let kids: Vec<NodeId> =
                self.caches.sorted(tree, id, metric).iter().copied().filter(|k| show_files || tree.node(*k).is_some_and(|n| n.is_dir())).collect();
            rows.push(Row { id, depth, has_children: !kids.is_empty() });
            if rows.len() >= MAX_ROWS {
                break;
            }
            if self.expanded.contains(&id) {
                for k in kids.into_iter().rev() {
                    stack.push((k, depth.saturating_add(1)));
                }
            }
        }
        rows
    }

    fn rows(&mut self, tree: &Tree) -> Vec<Row> {
        if let Some(rows) = &self.caches.rows {
            return rows.clone();
        }
        let rows = self.build_rows(tree);
        self.caches.rows = Some(rows.clone());
        rows
    }

    pub(crate) fn tree_panel(&mut self, ui: &mut Ui, tree: &Tree) {
        let pal = Palette::of(ui);
        let metric_title = match self.prefs.metric {
            whatspace_model::Metric::Size => "Size",
            whatspace_model::Metric::Allocated => "Allocated",
        };
        let mut columns = COLUMNS;
        columns[0].title = metric_title;
        rows::header(ui, "Name", &columns, None, pal.weak, pal.grid);
        let rows = self.rows(tree);
        let selected_index = self.selected.and_then(|s| rows.iter().position(|r| r.id == s));
        let mut area = egui::ScrollArea::vertical().id_salt("tree-rows").auto_shrink(false);
        if self.scroll_to_selected
            && let Some(i) = selected_index
        {
            let target = (i as f32 * ROW_H - ui.available_height() / 2.0).max(0.0);
            area = area.vertical_scroll_offset(target);
            self.scroll_to_selected = false;
        }
        ui.spacing_mut().item_spacing.y = 0.0;
        area.show_rows(ui, ROW_H, rows.len(), |ui, range| {
            for i in range {
                if let Some(row) = rows.get(i) {
                    self.tree_row(ui, tree, row, &columns, &pal);
                }
            }
        });
    }

    fn tree_row(&mut self, ui: &mut Ui, tree: &Tree, row: &Row, columns: &[Column], pal: &Palette) {
        let Some(node) = tree.node(row.id) else { return };
        // A stable id per node, so a menu stays open while the rows scroll or the tree grows.
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::hover());
        let resp = ui.interact(rect, ui.id().with(("tree-row", row.id)), Sense::click());
        let selected = self.selected == Some(row.id);
        let p = ui.painter();
        if selected {
            p.rect_filled(rect, 0.0, pal.selected);
        } else if resp.hovered() {
            p.rect_filled(rect, 0.0, pal.hover);
        }
        let (name_rect, cols) = rows::split(rect, columns, 140.0);
        let indent = 6.0 + f32::from(row.depth.min(60)) * 16.0;
        let expander_center = pos2(name_rect.min.x + indent + 5.0, rect.center().y);
        let open = self.expanded.contains(&row.id);
        if row.has_children {
            rows::expander(p, expander_center, open, pal.weak);
        }
        let icon_center = pos2(expander_center.x + 15.0, rect.center().y);
        draw_icon(p, tree, row.id, node.kind, icon_center, pal);
        let text_rect = Rect::from_min_max(pos2(icon_center.x + 9.0, rect.min.y), name_rect.right_bottom());
        let name = tree.name(row.id);
        let problem = node.flags.intersects(Flags::PROBLEMS) || (node.is_dir() && node.errors > 0);
        let color = if node.flags.intersects(Flags::NOT_ENTERED) || node.kind == Kind::Link {
            pal.weak
        } else if problem {
            pal.warn
        } else {
            pal.text
        };
        let mut label = name.to_string();
        if node.flags.intersects(Flags::NOT_ENTERED) {
            label.push_str("  (not scanned)");
        } else if node.flags.intersects(Flags::PROBLEMS) {
            label.push_str("  (could not be read)");
        } else if node.is_dir() && !node.flags.contains(Flags::SCANNED) && self.is_scanning() {
            label.push_str("  …");
        }
        rows::text_left(p, text_rect, &label, color, rows::font());

        let metric = self.prefs.metric;
        let value = node.metric(metric);
        let parent_value = tree.parent(row.id).and_then(|pid| tree.node(pid)).map_or(value, |pn| pn.metric(metric));
        let share = percent(value, parent_value);
        let fmt = self.prefs.size_format();
        if let [size, bar, files, folders, modified] = cols.as_slice() {
            rows::text_right(p, *size, &fmt.format(value), pal.text, rows::font());
            rows::share_bar(p, *bar, (share / 100.0) as f32, pal.bar, pal.track, &format_percent(share), pal.text);
            if node.is_dir() {
                rows::text_right(p, *files, &group(node.files), pal.weak, rows::font());
                rows::text_right(p, *folders, &group(node.dirs), pal.weak, rows::font());
            }
            rows::text_right(p, *modified, &whatspace_model::time::format_time(node.modified, &self.tz), pal.weak, rows::font());
        }

        let on_expander = row.has_children && resp.interact_pointer_pos().is_some_and(|pos| (pos.x - expander_center.x).abs() <= 9.0);
        if resp.clicked() {
            if on_expander {
                self.actions.push(Action::Toggle(row.id));
            } else {
                self.actions.push(Action::Select(row.id));
            }
        }
        if resp.double_clicked() && row.has_children && !on_expander {
            self.actions.push(Action::Toggle(row.id));
        }
        if resp.secondary_clicked() {
            self.actions.push(Action::Select(row.id));
        }
        let resp = resp.on_hover_text_at_pointer(self.describe(tree, row.id));
        resp.context_menu(|ui| self.node_menu(ui, tree, row.id));
    }

    /// Up and down move the selection, right opens a folder (or goes into it), left closes it (or
    /// goes up), Home and End go to the ends.
    pub(crate) fn tree_keys(&mut self, ctx: &egui::Context, tree: &Tree) {
        let (up, down, left, right, home, end, enter) = ctx.input(|i| {
            (
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::ArrowLeft),
                i.key_pressed(Key::ArrowRight),
                i.key_pressed(Key::Home),
                i.key_pressed(Key::End),
                i.key_pressed(Key::Enter),
            )
        });
        if !(up || down || left || right || home || end || enter) {
            return;
        }
        let rows = self.rows(tree);
        if rows.is_empty() {
            return;
        }
        let current = self.selected.and_then(|s| rows.iter().position(|r| r.id == s)).unwrap_or(0);
        let target = if up {
            current.saturating_sub(1)
        } else if down {
            (current + 1).min(rows.len() - 1)
        } else if home {
            0
        } else if end {
            rows.len() - 1
        } else {
            current
        };
        let Some(row) = rows.get(current) else { return };
        if right && row.has_children {
            if self.expanded.contains(&row.id) {
                if let Some(next) = rows.get(current + 1) {
                    self.actions.push(Action::Select(next.id));
                }
            } else {
                self.actions.push(Action::SetOpen(row.id, true));
            }
        } else if left {
            if self.expanded.contains(&row.id) && row.has_children {
                self.actions.push(Action::SetOpen(row.id, false));
            } else if let Some(parent) = tree.parent(row.id) {
                self.actions.push(Action::Select(parent));
            }
        } else if enter && row.has_children {
            self.actions.push(Action::Toggle(row.id));
        } else if let Some(t) = rows.get(target)
            && target != current
        {
            self.actions.push(Action::Select(t.id));
        }
        self.scroll_to_selected = true;
    }
}

/// The icon for a node: a folder, a link, or a page in its file type's colour.
pub(crate) fn draw_icon(p: &egui::Painter, tree: &Tree, id: NodeId, kind: Kind, center: egui::Pos2, pal: &Palette) {
    match kind {
        Kind::Dir => rows::folder_icon(p, center, pal.folder),
        Kind::Link => rows::link_icon(p, center, pal.weak),
        Kind::File | Kind::Other => {
            let c = category_color(Category::of_name(tree.name(id)));
            rows::file_icon(p, center, c, crate::theme::shade(c, -0.35));
        }
    }
}
