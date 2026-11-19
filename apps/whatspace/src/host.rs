//! What the window asks of the system: the native folder and save dialogs, opening a file,
//! showing it in Explorer, Finder or the file manager, and the Recycle Bin or Trash.
//!
//! Programs are started directly, never through a shell, so nothing in a file's name is ever
//! read as a command.

use std::path::{Path, PathBuf};
use std::process::Command;

use whatspace_ui_egui::Host;

pub struct Desktop;

impl Host for Desktop {
    fn pick_folder(&mut self) -> Option<PathBuf> {
        rfd::FileDialog::new().set_title("Choose a folder to scan").pick_folder()
    }

    fn save_file(&mut self, name: &str, extension: &str) -> Option<PathBuf> {
        rfd::FileDialog::new().set_title("Export").set_file_name(name).add_filter(extension.to_uppercase(), &[extension]).save_file()
    }

    fn open(&mut self, path: &Path) -> Result<(), String> {
        let mut cmd = if cfg!(windows) {
            Command::new("explorer")
        } else if cfg!(target_os = "macos") {
            Command::new("open")
        } else {
            Command::new("xdg-open")
        };
        cmd.arg(path);
        spawn(cmd)
    }

    fn reveal(&mut self, path: &Path) -> Result<(), String> {
        spawn(reveal_command(path))
    }

    fn trash(&mut self, path: &Path) -> Result<(), String> {
        trash::delete(path).map_err(|e| e.to_string())
    }
}

#[cfg(windows)]
fn reveal_command(path: &Path) -> Command {
    use std::os::windows::process::CommandExt;
    let mut cmd = Command::new("explorer");
    // Explorer wants `/select,"path"` as one argument; a Windows path can't contain a quote.
    cmd.raw_arg(format!("/select,\"{}\"", path.display()));
    cmd
}

#[cfg(target_os = "macos")]
fn reveal_command(path: &Path) -> Command {
    let mut cmd = Command::new("open");
    cmd.arg("-R").arg(path);
    cmd
}

#[cfg(not(any(windows, target_os = "macos")))]
fn reveal_command(path: &Path) -> Command {
    // Not every file manager can select a file: open the folder it is in.
    let folder = if path.is_dir() { path } else { path.parent().unwrap_or(path) };
    let mut cmd = Command::new("xdg-open");
    cmd.arg(folder);
    cmd
}

/// Start a program and leave it running; its exit is collected on a thread of its own.
fn spawn(mut cmd: Command) -> Result<(), String> {
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    let _ = std::thread::Builder::new().name("whatspace-child".into()).spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
