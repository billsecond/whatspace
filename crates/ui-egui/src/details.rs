//! Details: what is in the selected folder, folders and files together, in a table that sorts
//! by any column.

use egui::{Rect, Sense, Ui, pos2, vec2};
use whatspace_model::units::{format_percent, group, percent};
use whatspace_model::{Category, Kind, NodeId, Tree};

use crate::rows::{self, Column, ROW_H};
use crate::theme::Palette;
use crate::tree_view::draw_icon;
use crate::{Action, App};

const COLUMNS: [Column; 7] = [
    Column::right("Size", 86.0),
    Column::right("Allocated", 86.0),
    Column::left("Share", 100.0),
    Column::right("Modified", 122.0),
    Column::right("Files", 76.0),
    Column::right("Folders", 64.0),
    Column::left("Type", 150.0),
];

impl App {
    pub(crate) fn details(&mut self, ui: &mut Ui, tree: &Tree) {
        let pal = Palette::of(ui);
        let folder = self.focus_folder(tree);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(tree.path_of(folder).display().to_string()).strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("Filter by name").desired_width(180.0));
            });
        });
        let filter = self.filter.to_lowercase();
        let mut kids: Vec<NodeId> =
            tree.children(folder).iter().copied().filter(|k| filter.is_empty() || tree.name(*k).to_lowercase().contains(&filter)).collect();
        let (col, desc) = self.details_sort;
        sort(tree, &mut kids, col, desc);
        if let Some(clicked) = rows::header(ui, "Name", &COLUMNS, Some(self.details_sort), pal.weak, pal.grid) {
            self.details_sort = if clicked == col { (col, !desc) } else { (clicked, clicked != 0 && clicked != 7) };
        }
        let total = tree.node(folder).map_or(0, |n| n.metric(self.prefs.metric));
        ui.spacing_mut().item_spacing.y = 0.0;
        egui::ScrollArea::vertical().id_salt("details-rows").auto_shrink(false).show_rows(ui, ROW_H, kids.len(), |ui, range| {
            for i in range {
                if let Some(id) = kids.get(i) {
                    self.details_row(ui, tree, *id, total, &pal);
                }
            }
        });
        if kids.is_empty() {
            ui.label(
                egui::RichText::new(if filter.is_empty() { "This folder is empty." } else { "Nothing here matches the filter." }).color(pal.weak),
            );
        }
    }

    fn details_row(&mut self, ui: &mut Ui, tree: &Tree, id: NodeId, total: u64, pal: &Palette) {
        let Some(node) = tree.node(id) else { return };
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::hover());
        let resp = ui.interact(rect, ui.id().with(("details-row", id)), Sense::click());
        let p = ui.painter();
        if self.selected == Some(id) {
            p.rect_filled(rect, 0.0, pal.selected);
        } else if resp.hovered() {
            p.rect_filled(rect, 0.0, pal.hover);
        }
        let (name_rect, cols) = rows::split(rect, &COLUMNS, 140.0);
        let icon = pos2(name_rect.min.x + 12.0, rect.center().y);
        draw_icon(p, tree, id, node.kind, icon, pal);
        let text_rect = Rect::from_min_max(pos2(icon.x + 9.0, rect.min.y), name_rect.right_bottom());
        rows::text_left(p, text_rect, tree.name(id), pal.text, rows::font());
        let fmt = self.prefs.size_format();
        let share = percent(node.metric(self.prefs.metric), total);
        if let [size, allocated, bar, modified, files, folders, kind] = cols.as_slice() {
            rows::text_right(p, *size, &fmt.format(node.size), pal.text, rows::font());
            rows::text_right(p, *allocated, &fmt.format(node.allocated), pal.weak, rows::font());
            rows::share_bar(p, *bar, (share / 100.0) as f32, pal.bar, pal.track, &format_percent(share), pal.text);
            if node.is_dir() {
                rows::text_right(p, *files, &group(node.files), pal.weak, rows::font());
                rows::text_right(p, *folders, &group(node.dirs), pal.weak, rows::font());
            }
            rows::text_right(p, *modified, &whatspace_model::time::format_time(node.modified, &self.tz), pal.weak, rows::font());
            rows::text_left(p, *kind, &type_label(tree, id, node.kind), pal.weak, rows::font());
        }
        if resp.clicked() || resp.secondary_clicked() {
            self.actions.push(Action::Reveal(id));
        }
        if resp.double_clicked() {
            if node.is_dir() {
                self.actions.push(Action::Select(id));
                self.actions.push(Action::SetOpen(id, true));
            } else {
                self.actions.push(Action::Open(id));
            }
        }
        let resp = resp.on_hover_text_at_pointer(self.describe(tree, id));
        resp.context_menu(|ui| self.node_menu(ui, tree, id));
    }
}

/// What kind of thing a row is: "Folder", "Link", or the file's type and extension.
fn type_label(tree: &Tree, id: NodeId, kind: Kind) -> String {
    match kind {
        Kind::Dir => "Folder".into(),
        Kind::Link => "Link".into(),
        Kind::Other => "Special file".into(),
        Kind::File => {
            let name = tree.name(id);
            match whatspace_model::stats::extension_of(name) {
                Some(ext) => format!("{} (.{ext})", Category::of_extension(&ext).label()),
                None => "File".into(),
            }
        }
    }
}

/// Sort `kids` by column `col` (0 name, 1 size, 2 allocated, 3 share, 4 modified, 5 files,
/// 6 folders, 7 type).
fn sort(tree: &Tree, kids: &mut [NodeId], col: usize, desc: bool) {
    kids.sort_by(|a, b| {
        let (na, nb) = (tree.node(*a), tree.node(*b));
        let key = |n: Option<&whatspace_model::Node>, f: fn(&whatspace_model::Node) -> i128| n.map_or(0, f);
        let ord = match col {
            0 => tree.name(*a).to_lowercase().cmp(&tree.name(*b).to_lowercase()),
            1 | 3 => key(na, |n| i128::from(n.size)).cmp(&key(nb, |n| i128::from(n.size))),
            2 => key(na, |n| i128::from(n.allocated)).cmp(&key(nb, |n| i128::from(n.allocated))),
            4 => key(na, |n| i128::from(n.modified)).cmp(&key(nb, |n| i128::from(n.modified))),
            5 => key(na, |n| i128::from(n.files)).cmp(&key(nb, |n| i128::from(n.files))),
            6 => key(na, |n| i128::from(n.dirs)).cmp(&key(nb, |n| i128::from(n.dirs))),
            _ => type_label(tree, *a, na.map_or(Kind::Other, |n| n.kind)).cmp(&type_label(tree, *b, nb.map_or(Kind::Other, |n| n.kind))),
        };
        let ord = if desc { ord.reverse() } else { ord };
        ord.then_with(|| tree.name(*a).cmp(tree.name(*b)))
    });
}
