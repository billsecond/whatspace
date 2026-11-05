//! The page shown before anything is scanned: every drive with how full it is, the folders
//! scanned lately, and a way to pick any folder.

use std::path::PathBuf;

use egui::{RichText, Sense, Ui, vec2};
use whatspace_fs::VolumeKind;
use whatspace_model::units::{format_percent, percent};

use crate::App;
use crate::theme::{Palette, usage_color};

impl App {
    pub(crate) fn start_page(&mut self, ui: &mut Ui) {
        let pal = Palette::of(ui);
        let fmt = self.prefs.size_format();
        let mut scan: Option<PathBuf> = None;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.add_space(18.0);
            ui.heading(RichText::new("WhatSpace").size(26.0).strong());
            ui.label(RichText::new("See what is taking up space. Choose a drive, or a folder, to scan.").color(pal.weak));
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if ui.button("Choose a folder…").clicked()
                    && let Some(folder) = self.host_pick()
                {
                    scan = Some(folder);
                }
                ui.label(RichText::new("or drop a folder on this window").color(pal.weak));
                if ui.button("Refresh drives").clicked() {
                    self.volumes = whatspace_fs::volumes::list();
                }
            });
            ui.add_space(14.0);
            if self.volumes.is_empty() {
                ui.label("The system did not list any drives. Choose a folder instead.");
            }
            let volumes = self.volumes.clone();
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(12.0, 12.0);
                for v in &volumes {
                    let card = egui::Frame::group(ui.style()).inner_margin(12.0).corner_radius(8.0).show(ui, |ui| {
                        ui.set_width(250.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(v.mount_point.display().to_string()).strong().size(15.0));
                            if !v.label.is_empty() && v.label != v.mount_point.display().to_string() {
                                ui.label(RichText::new(&v.label).color(pal.weak));
                            }
                        });
                        let kind = match v.kind {
                            VolumeKind::Ssd => "SSD",
                            VolumeKind::Hdd => "hard disk",
                            VolumeKind::Removable => "removable",
                            VolumeKind::Network => "network",
                            VolumeKind::Unknown => "",
                        };
                        let mut info = v.file_system_name();
                        if !kind.is_empty() {
                            info.push_str(" · ");
                            info.push_str(kind);
                        }
                        if v.read_only {
                            info.push_str(" · read only");
                        }
                        ui.label(RichText::new(info).color(pal.weak).size(11.5));
                        let used = percent(v.used(), v.total);
                        let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 10.0), Sense::hover());
                        ui.painter().rect_filled(bar, 3.0, pal.track);
                        let mut fill = bar;
                        fill.set_width(bar.width() * (used / 100.0) as f32);
                        ui.painter().rect_filled(fill, 3.0, usage_color((used / 100.0) as f32));
                        ui.label(
                            RichText::new(format!("{} free of {}  ·  {} used", fmt.format(v.available), fmt.format(v.total), format_percent(used)))
                                .size(11.5),
                        );
                    });
                    let resp = ui.interact(card.response.rect, ui.id().with(("volume", &v.mount_point)), Sense::click());
                    if resp.clicked() {
                        scan = Some(v.mount_point.clone());
                    }
                    resp.on_hover_text(format!("Scan {}", v.mount_point.display()));
                }
            });
            if !self.prefs.recent.is_empty() {
                ui.add_space(18.0);
                ui.label(RichText::new("Scanned lately").strong());
                for p in self.prefs.recent.clone() {
                    if ui.link(&p).clicked() {
                        scan = Some(PathBuf::from(p));
                    }
                }
            }
        });
        if let Some(path) = scan {
            self.start_scan(&path);
        }
    }

    fn host_pick(&mut self) -> Option<PathBuf> {
        self.host.pick_folder()
    }
}
