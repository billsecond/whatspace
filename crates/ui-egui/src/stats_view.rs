//! Extensions, Largest files and File ages: what the selected folder holds, by type, by size
//! and by how long ago it changed.

use egui::{Rect, Sense, Ui, pos2, vec2};
use whatspace_model::stats::{self, AgeBucket, ExtensionStat};
use whatspace_model::units::{format_percent, group, percent};
use whatspace_model::{Category, NodeId, Tree};

use crate::rows::{self, Column, ROW_H};
use crate::theme::{Palette, category_color};
use crate::tree_view::draw_icon;
use crate::{Action, App, StatsCache};

/// How many of the largest files are listed.
const LARGEST: usize = 500;

impl App {
    /// The statistics of the focused folder, worked out again when the folder, the measure or
    /// the tree changed.
    fn stats(&mut self, tree: &Tree, now: f64) -> Option<&StatsCache> {
        let node = self.focus_folder(tree);
        let metric = self.prefs.metric;
        let stale = !self.caches.stats.as_ref().is_some_and(|s| s.node == node && s.metric == metric);
        if stale {
            self.caches.stats = Some(StatsCache {
                node,
                metric,
                made_at: now,
                extensions: stats::extensions(tree, node, metric),
                largest: stats::largest_files(tree, node, metric, LARGEST),
                ages: stats::ages(tree, node, whatspace_model::time::now()),
            });
        }
        self.caches.stats.as_ref()
    }

    fn stats_heading(&self, ui: &mut Ui, tree: &Tree, node: NodeId) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(tree.path_of(node).display().to_string()).strong());
            if self.is_scanning() {
                ui.label(egui::RichText::new("(still scanning: updated every few seconds)").weak());
            }
        });
    }

    pub(crate) fn extensions(&mut self, ui: &mut Ui, tree: &Tree, now: f64) {
        let pal = Palette::of(ui);
        let metric = self.prefs.metric;
        let fmt = self.prefs.size_format();
        let Some(cache) = self.stats(tree, now) else { return };
        let node = cache.node;
        let list: Vec<ExtensionStat> = cache.extensions.iter().take(2000).cloned().collect();
        self.stats_heading(ui, tree, node);
        let total: u64 = list.iter().map(|e| e.metric(metric)).sum();

        // Space by kind of file, as one bar.
        let mut by_category: Vec<(Category, u64)> = Category::ALL.iter().map(|c| (*c, 0)).collect();
        for e in &list {
            if let Some(slot) = by_category.iter_mut().find(|(c, _)| *c == e.category) {
                slot.1 = slot.1.saturating_add(e.metric(metric));
            }
        }
        by_category.sort_by_key(|c| std::cmp::Reverse(c.1));
        let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::hover());
        let p = ui.painter();
        p.rect_filled(bar, 3.0, pal.track);
        let mut x = bar.min.x;
        for (c, v) in &by_category {
            let w = if total == 0 { 0.0 } else { bar.width() * (*v as f32 / total as f32) };
            if w > 0.0 {
                let r = Rect::from_min_size(pos2(x, bar.min.y), vec2(w, bar.height()));
                p.rect_filled(r, 0.0, category_color(*c));
                x += w;
            }
        }
        ui.horizontal_wrapped(|ui| {
            for (c, v) in by_category.iter().filter(|(_, v)| *v > 0) {
                let (r, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
                ui.painter().rect_filled(r, 2.0, category_color(*c));
                ui.label(egui::RichText::new(format!("{} {} ({})", c.label(), fmt.format(*v), format_percent(percent(*v, total)))).size(11.5));
                ui.add_space(6.0);
            }
        });
        ui.add_space(4.0);

        let columns = [Column::left("Type", 190.0), Column::right("Size", 90.0), Column::left("Share", 120.0), Column::right("Files", 80.0)];
        rows::header(ui, "Extension", &columns, None, pal.weak, pal.grid);
        ui.spacing_mut().item_spacing.y = 0.0;
        egui::ScrollArea::vertical().id_salt("ext-rows").auto_shrink(false).show_rows(ui, ROW_H, list.len(), |ui, range| {
            for e in list.get(range).unwrap_or(&[]) {
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::hover());
                let p = ui.painter();
                if resp.hovered() {
                    p.rect_filled(rect, 0.0, pal.hover);
                }
                let (name, cols) = rows::split(rect, &columns, 120.0);
                let swatch = Rect::from_center_size(pos2(name.min.x + 12.0, rect.center().y), vec2(10.0, 10.0));
                p.rect_filled(swatch, 2.0, category_color(e.category));
                rows::text_left(p, Rect::from_min_max(pos2(swatch.max.x + 2.0, rect.min.y), name.right_bottom()), &e.label(), pal.text, rows::font());
                let share = percent(e.metric(metric), total);
                if let [kind, size, bar, files] = cols.as_slice() {
                    rows::text_left(p, *kind, e.category.label(), pal.weak, rows::font());
                    rows::text_right(p, *size, &fmt.format(e.metric(metric)), pal.text, rows::font());
                    rows::share_bar(p, *bar, (share / 100.0) as f32, category_color(e.category), pal.track, &format_percent(share), pal.text);
                    rows::text_right(p, *files, &group(e.files), pal.weak, rows::font());
                }
            }
        });
    }

    pub(crate) fn largest(&mut self, ui: &mut Ui, tree: &Tree, now: f64) {
        let pal = Palette::of(ui);
        let metric = self.prefs.metric;
        let Some(cache) = self.stats(tree, now) else { return };
        let node = cache.node;
        let largest = cache.largest.clone();
        ui.horizontal(|ui| {
            self.stats_heading(ui, tree, node);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("Filter by name").desired_width(180.0));
            });
        });
        let filter = self.filter.to_lowercase();
        let shown: Vec<NodeId> = largest.into_iter().filter(|id| filter.is_empty() || tree.name(*id).to_lowercase().contains(&filter)).collect();
        let columns =
            [Column::right("Size", 90.0), Column::right("Allocated", 90.0), Column::right("Modified", 122.0), Column::left("Folder", 320.0)];
        rows::header(ui, "File", &columns, None, pal.weak, pal.grid);
        let fmt = self.prefs.size_format();
        ui.spacing_mut().item_spacing.y = 0.0;
        egui::ScrollArea::vertical().id_salt("largest-rows").auto_shrink(false).show_rows(ui, ROW_H, shown.len(), |ui, range| {
            for (i, id) in shown.iter().enumerate().skip(range.start).take(range.len()) {
                let Some(n) = tree.node(*id) else { continue };
                let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H), Sense::hover());
                let resp = ui.interact(rect, ui.id().with(("largest-row", *id)), Sense::click());
                let p = ui.painter();
                if self.selected == Some(*id) {
                    p.rect_filled(rect, 0.0, pal.selected);
                } else if resp.hovered() {
                    p.rect_filled(rect, 0.0, pal.hover);
                }
                let (name, cols) = rows::split(rect, &columns, 160.0);
                let rank = Rect::from_min_size(name.min, vec2(36.0, ROW_H));
                rows::text_right(p, rank, &format!("{}", i + 1), pal.weak, rows::small_font());
                let icon = pos2(rank.max.x + 8.0, rect.center().y);
                draw_icon(p, tree, *id, n.kind, icon, &pal);
                rows::text_left(p, Rect::from_min_max(pos2(icon.x + 8.0, rect.min.y), name.right_bottom()), tree.name(*id), pal.text, rows::font());
                if let [size, allocated, modified, folder] = cols.as_slice() {
                    rows::text_right(
                        p,
                        *size,
                        &fmt.format(n.size),
                        if metric == whatspace_model::Metric::Size { pal.text } else { pal.weak },
                        rows::font(),
                    );
                    rows::text_right(
                        p,
                        *allocated,
                        &fmt.format(n.allocated),
                        if metric == whatspace_model::Metric::Allocated { pal.text } else { pal.weak },
                        rows::font(),
                    );
                    rows::text_right(p, *modified, &whatspace_model::time::format_time(n.modified, &self.tz), pal.weak, rows::font());
                    let parent = tree.parent(*id).map(|pid| tree.path_of(pid).display().to_string()).unwrap_or_default();
                    rows::text_left(p, *folder, &parent, pal.weak, rows::font());
                }
                if resp.clicked() || resp.secondary_clicked() {
                    self.actions.push(Action::Reveal(*id));
                }
                if resp.double_clicked() {
                    self.actions.push(Action::ShowInFileManager(*id));
                }
                let resp = resp.on_hover_text_at_pointer(self.describe(tree, *id));
                resp.context_menu(|ui| self.node_menu(ui, tree, *id));
            }
        });
    }

    pub(crate) fn ages(&mut self, ui: &mut Ui, tree: &Tree, now: f64) {
        let pal = Palette::of(ui);
        let metric = self.prefs.metric;
        let fmt = self.prefs.size_format();
        let Some(cache) = self.stats(tree, now) else { return };
        let node = cache.node;
        let ages: Vec<AgeBucket> = cache.ages.clone();
        self.stats_heading(ui, tree, node);
        ui.label(egui::RichText::new("When the files were last changed").color(pal.weak));
        ui.add_space(4.0);
        let total: u64 = ages.iter().map(|a| a.metric(metric)).sum();
        let largest = ages.iter().map(|a| a.metric(metric)).max().unwrap_or(0).max(1);
        let columns = [Column::left("", 260.0), Column::right("Size", 90.0), Column::right("Share", 70.0), Column::right("Files", 90.0)];
        rows::header(ui, "Last changed", &columns, None, pal.weak, pal.grid);
        for a in &ages {
            let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), ROW_H + 4.0), Sense::hover());
            let p = ui.painter();
            let (name, cols) = rows::split(rect, &columns, 120.0);
            rows::text_left(p, name, a.label, pal.text, rows::font());
            if let [bar, size, share, files] = cols.as_slice() {
                let inner = bar.shrink2(vec2(4.0, 6.0));
                p.rect_filled(inner, 2.0, pal.track);
                let mut fill = inner;
                fill.set_width(inner.width() * (a.metric(metric) as f32 / largest as f32));
                p.rect_filled(fill, 2.0, pal.bar);
                rows::text_right(p, *size, &fmt.format(a.metric(metric)), pal.text, rows::font());
                rows::text_right(p, *share, &format_percent(percent(a.metric(metric), total)), pal.weak, rows::font());
                rows::text_right(p, *files, &group(a.files), pal.weak, rows::font());
            }
        }
    }
}
