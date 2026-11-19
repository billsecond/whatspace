//! The treemap: the folder as a rectangle, divided among what is in it by size, files coloured
//! by type. Click a tile to select it; double-click a folder to go into it; Up goes back out.

use egui::{Align2, Color32, Mesh, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use whatspace_model::treemap::{self, Tile};
use whatspace_model::{Category, Kind, Metric, NodeId, Tree};

use crate::rows;
use crate::theme::{Palette, category_color, folder_fill, shade};
use crate::{Action, App};

/// A treemap laid out for one folder, measure and size.
pub struct Layout {
    pub root: NodeId,
    pub metric: Metric,
    pub size: egui::Vec2,
    pub tiles: Vec<Tile>,
}

impl App {
    pub(crate) fn treemap(&mut self, ui: &mut Ui, tree: &Tree) {
        let pal = Palette::of(ui);
        let root = self.treemap_root.filter(|r| tree.live(*r).is_some_and(|n| n.is_dir())).unwrap_or(NodeId::ROOT);
        ui.horizontal(|ui| {
            let parent = tree.parent(root);
            if ui.add_enabled(parent.is_some(), egui::Button::new("⬆ Up")).on_hover_text("Show the folder above").clicked()
                && let Some(p) = parent
            {
                self.actions.push(Action::ZoomTreemap(p));
            }
            let fmt = self.prefs.size_format();
            let total = tree.node(root).map_or(0, |n| n.metric(self.prefs.metric));
            ui.label(egui::RichText::new(tree.path_of(root).display().to_string()).strong());
            ui.label(egui::RichText::new(fmt.format(total)).color(pal.weak));
        });
        // Room for the legend on two lines.
        let legend_h = 46.0;
        let avail = ui.available_size();
        let (rect, resp) = ui.allocate_exact_size(vec2(avail.x, (avail.y - legend_h).max(40.0)), Sense::click());
        let fresh =
            self.caches.treemap.as_ref().is_some_and(|l| l.root == root && l.metric == self.prefs.metric && (l.size - rect.size()).length() < 0.5);
        if !fresh {
            let opts = treemap::Options { metric: self.prefs.metric, max_tiles: 25_000, ..treemap::Options::default() };
            let tiles = treemap::layout(tree, root, treemap::Rect::new(0.0, 0.0, rect.width(), rect.height()), &opts);
            self.caches.treemap = Some(Layout { root, metric: self.prefs.metric, size: rect.size(), tiles });
        }
        // Taken out while it is drawn, so the views can ask for things meanwhile; put back below.
        let Some(layout) = self.caches.treemap.take() else { return };
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, pal.canvas);
        let to_screen = |r: &treemap::Rect| Rect::from_min_size(pos2(rect.min.x + r.x, rect.min.y + r.y), vec2(r.w, r.h));
        let mut mesh = Mesh::default();
        for tile in &layout.tiles {
            let r = to_screen(&tile.rect);
            match tile.kind {
                Kind::Dir => {
                    painter.rect_filled(r, 0.0, folder_fill(tile.depth, pal.dark));
                    painter.rect_stroke(r, 0.0, Stroke::new(1.0, pal.grid), StrokeKind::Inside);
                    if r.height() >= 48.0 && r.width() >= 48.0 && tile.depth > 0 {
                        let label = format!(
                            "{}  {}",
                            tree.name(tile.node),
                            self.prefs.size_format().format(tree.node(tile.node).map_or(0, |n| n.metric(self.prefs.metric)))
                        );
                        let header = Rect::from_min_size(r.min, vec2(r.width(), 16.0)).shrink2(vec2(3.0, 0.0));
                        painter.with_clip_rect(header).text(
                            pos2(header.min.x, header.center().y + 1.0),
                            Align2::LEFT_CENTER,
                            label,
                            rows::small_font(),
                            pal.text,
                        );
                    }
                }
                _ => cushion(&mut mesh, r, category_color(Category::of_name(tree.name(tile.node)))),
            }
        }
        painter.add(egui::Shape::mesh(mesh));
        // Outlines between files, where there is room for them.
        for tile in layout.tiles.iter().filter(|t| t.kind != Kind::Dir) {
            let r = to_screen(&tile.rect);
            if r.width() >= 4.0 && r.height() >= 4.0 {
                painter.rect_stroke(r, 0.0, Stroke::new(0.6, Color32::from_black_alpha(70)), StrokeKind::Inside);
            }
        }
        if let Some(sel) = self.selected
            && let Some(t) = layout.tiles.iter().find(|t| t.node == sel && t.node != root)
        {
            painter.rect_stroke(to_screen(&t.rect), 0.0, Stroke::new(2.5, pal.accent), StrokeKind::Inside);
        }
        let hovered = resp.hover_pos().and_then(|pos| treemap::hit(&layout.tiles, pos.x - rect.min.x, pos.y - rect.min.y)).copied();
        if let Some(t) = hovered {
            painter.rect_stroke(to_screen(&t.rect), 0.0, Stroke::new(1.5, pal.text), StrokeKind::Inside);
        }
        let clicked_tile = resp.interact_pointer_pos().and_then(|pos| treemap::hit(&layout.tiles, pos.x - rect.min.x, pos.y - rect.min.y)).copied();
        if let Some(t) = clicked_tile {
            if resp.double_clicked() {
                // Into the folder under the pointer (the file's folder, for a file).
                let folder = if t.kind == Kind::Dir { t.node } else { tree.parent(t.node).unwrap_or(root) };
                if folder != root {
                    self.actions.push(Action::ZoomTreemap(folder));
                }
            } else if resp.clicked() || resp.secondary_clicked() {
                self.actions.push(Action::Pick(t.node));
            }
        }
        if let Some(t) = hovered {
            let text = self.describe(tree, t.node);
            let resp = resp.on_hover_text_at_pointer(text);
            resp.context_menu(|ui| self.node_menu(ui, tree, t.node));
        } else if let Some(sel) = self.selected {
            resp.context_menu(|ui| self.node_menu(ui, tree, sel));
        }
        self.caches.treemap = Some(layout);
        legend(ui, &pal);
    }
}

/// A file's tile with a soft highlight, lighter at the top left and darker at the bottom right.
fn cushion(mesh: &mut Mesh, r: Rect, base: Color32) {
    if r.width() < 0.5 || r.height() < 0.5 {
        return;
    }
    let i = u32::try_from(mesh.vertices.len()).unwrap_or(u32::MAX);
    if i > u32::MAX - 4 {
        return;
    }
    let corners: [(Pos2, f32); 4] = [(r.left_top(), 0.28), (r.right_top(), 0.05), (r.right_bottom(), -0.22), (r.left_bottom(), 0.05)];
    for (pos, amount) in corners {
        mesh.colored_vertex(pos, shade(base, amount));
    }
    mesh.add_triangle(i, i + 1, i + 2);
    mesh.add_triangle(i, i + 2, i + 3);
}

fn legend(ui: &mut Ui, pal: &Palette) {
    ui.horizontal_wrapped(|ui| {
        for c in Category::ALL {
            let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
            ui.painter().rect_filled(r, 2.0, category_color(c));
            ui.label(egui::RichText::new(c.label()).color(pal.weak).size(11.5));
            ui.add_space(6.0);
        }
    });
}
