//! WhatSpace desktop app.
//!
//! Usage: `whatspace [folder]`. With a folder (Explorer's "Scan with WhatSpace", a file
//! manager's "Open with", or the command line) it starts scanning that folder at once; without
//! one it shows the drives to choose from. The owner's choices (units, theme, options, recent
//! folders) and the window's size and place are kept between runs.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

#[cfg(any(target_os = "windows", test))]
mod graphics;
mod host;

use std::ffi::OsString;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use whatspace_ui_egui::{App, Prefs};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const PREFS_KEY: &str = "whatspace.prefs";
const ICON: &[u8] = include_bytes!("../../../assets/app-icon/hicolor/256x256/apps/io.github.billsecond.whatspace.png");

struct Desktop(App);

impl eframe::App for Desktop {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.0.ui(ui);
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, PREFS_KEY, &self.0.prefs);
    }
}

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let first = args.first().map(|a| a.to_string_lossy().into_owned());
    match first.as_deref() {
        Some("--version" | "-V") => {
            let _ = writeln!(std::io::stdout(), "whatspace {VERSION}");
            return ExitCode::SUCCESS;
        }
        Some("--help" | "-h") => {
            let _ = writeln!(std::io::stdout(), "WhatSpace {VERSION}: see what is taking up space on your disks.\n\nusage: whatspace [folder]");
            return ExitCode::SUCCESS;
        }
        _ => {}
    }
    let folder = args.first().map(PathBuf::from);

    let mut options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("WhatSpace")
            .with_app_id("io.github.billsecond.whatspace")
            .with_inner_size([1360.0, 860.0])
            .with_min_inner_size([820.0, 520.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    if let Ok(icon) = eframe::icon_data::from_png_bytes(ICON) {
        options.viewport = options.viewport.with_icon(icon);
    }
    // Before eframe creates the wgpu instance: DirectX 12 where it has a real GPU, OpenGL where it
    // doesn't (see graphics.rs).
    #[cfg(target_os = "windows")]
    graphics::configure(&mut options, eframe::wgpu::Backends::from_env());

    let result = eframe::run_native(
        "WhatSpace",
        options,
        Box::new(move |cc| {
            let prefs: Prefs = cc.storage.and_then(|s| eframe::get_value(s, PREFS_KEY)).unwrap_or_default();
            let mut app = App::new(prefs, Box::new(host::Desktop));
            if let Some(folder) = &folder {
                app.start_scan(folder);
            }
            Ok(Box::new(Desktop(app)))
        }),
    );
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            // A window that can't open (no GPU driver, no display) says why instead of vanishing.
            let message = format!("WhatSpace could not open its window: {e}");
            let _ = writeln!(std::io::stderr(), "{message}");
            let _ = rfd::MessageDialog::new().set_title("WhatSpace").set_description(&message).set_level(rfd::MessageLevel::Error).show();
            ExitCode::FAILURE
        }
    }
}
