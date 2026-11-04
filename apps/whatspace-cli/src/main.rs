//! WhatSpace on the command line: scan a folder and print where the space went, list the
//! volumes, or export a scan as CSV or JSON.
//!
//! Output goes through `writeln!` to a locked stdout, so a closed pipe (`| head`) ends the
//! program quietly instead of panicking.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::ffi::OsString;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use whatspace_fs::volumes;
use whatspace_model::export;
use whatspace_model::units::{format_percent, group, percent};
use whatspace_model::{Kind, Metric, NodeId, Scale, SizeFormat, Tree, UnitStyle};
use whatspace_scan::{Options, Snapshot, VolumeInfo};

const VERSION: &str = env!("CARGO_PKG_VERSION");

const USAGE: &str = "\
WhatSpace: see what is taking up space on your disks.

usage:
  whatspace-cli scan <folder> [options]   scan a folder and print the largest folders and files
  whatspace-cli volumes                   list the mounted volumes with their file systems
  whatspace-cli --version                 print the version

scan options:
  --depth <n>             levels of folders to print or export (default 1)
  --top <n>               largest files to print (default 10; 0 for none)
  --files                 list files beside folders
  --allocated             measure and sort by space on disk instead of size
  --cross-file-systems    go into other mounted file systems (default: stay on one)
  --hard-links            count a file with several names once (default on Unix)
  --no-hard-links         count every name of a file
  --threads <n>           worker threads (default: from the processor count)
  --units <style>         windows (KB of 1024), iec (KiB of 1024) or si (kB of 1000)
  --csv <file|->          write the scan as CSV
  --json <file|->         write the scan as JSON
  --quiet                 print nothing but the export
";

/// What went wrong, and the exit code it gets.
enum Failure {
    /// Bad arguments: say so with the usage, exit 2.
    Usage(String),
    /// Exit 1 with the message.
    Error(String),
    /// Output was closed early (`| head`): stop quietly.
    Closed,
}

impl From<io::Error> for Failure {
    fn from(e: io::Error) -> Failure {
        if e.kind() == io::ErrorKind::BrokenPipe { Failure::Closed } else { Failure::Error(e.to_string()) }
    }
}

fn main() -> ExitCode {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    match run(&args) {
        Ok(()) | Err(Failure::Closed) => ExitCode::SUCCESS,
        Err(Failure::Usage(msg)) => {
            let _ = writeln!(io::stderr(), "error: {msg}\n\n{USAGE}");
            ExitCode::from(2)
        }
        Err(Failure::Error(msg)) => {
            let _ = writeln!(io::stderr(), "error: {msg}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[OsString]) -> Result<(), Failure> {
    let mut out = io::stdout().lock();
    let first = args.first().map(|a| a.to_string_lossy().into_owned());
    match first.as_deref() {
        None | Some("help" | "-h" | "--help") => {
            write!(out, "{USAGE}")?;
            Ok(())
        }
        Some("--version" | "-V" | "version") => {
            writeln!(out, "whatspace-cli {VERSION}")?;
            Ok(())
        }
        Some("volumes") => list_volumes(&mut out),
        Some("scan") => scan(args.get(1..).unwrap_or(&[]), &mut out),
        Some(other) if !other.starts_with('-') && args.first().is_some_and(|a| PathBuf::from(a).is_dir()) => scan(args, &mut out),
        Some(other) => Err(Failure::Usage(format!("unknown command `{other}`"))),
    }
}

fn list_volumes(out: &mut impl Write) -> Result<(), Failure> {
    let fmt = SizeFormat::default();
    let all = volumes::list();
    if all.is_empty() {
        writeln!(out, "The system did not list any volumes.")?;
        return Ok(());
    }
    writeln!(out, "{:<28} {:<10} {:>10} {:>10} {:>10}  {:>7}  label", "mount point", "file sys", "size", "used", "free", "used %")?;
    for v in &all {
        writeln!(
            out,
            "{:<28} {:<10} {:>10} {:>10} {:>10}  {:>7}  {}",
            v.mount_point.display(),
            v.file_system_name(),
            fmt.format(v.total),
            fmt.format(v.used()),
            fmt.format(v.available),
            format_percent(percent(v.used(), v.total)),
            v.label
        )?;
    }
    Ok(())
}

/// Parsed `scan` arguments.
struct ScanArgs {
    folder: PathBuf,
    depth: usize,
    top: usize,
    files: bool,
    metric: Metric,
    options: Options,
    units: UnitStyle,
    csv: Option<String>,
    json: Option<String>,
    quiet: bool,
}

fn parse_scan(args: &[OsString]) -> Result<ScanArgs, Failure> {
    let mut a = ScanArgs {
        folder: PathBuf::new(),
        depth: 1,
        top: 10,
        files: false,
        metric: Metric::Size,
        options: Options::default(),
        units: UnitStyle::platform_default(),
        csv: None,
        json: None,
        quiet: false,
    };
    let mut folder = None;
    let mut it = args.iter();
    let number = |v: Option<&OsString>, what: &str| -> Result<usize, Failure> {
        v.and_then(|v| v.to_str()).and_then(|v| v.parse().ok()).ok_or_else(|| Failure::Usage(format!("{what} needs a number")))
    };
    while let Some(arg) = it.next() {
        match arg.to_string_lossy().as_ref() {
            "--depth" => a.depth = number(it.next(), "--depth")?.min(export::MAX_JSON_DEPTH),
            "--top" => a.top = number(it.next(), "--top")?.min(100_000),
            "--threads" => a.options.threads = number(it.next(), "--threads")?.min(64),
            "--files" => a.files = true,
            "--allocated" => a.metric = Metric::Allocated,
            "--cross-file-systems" => a.options.one_file_system = false,
            "--hard-links" => a.options.hard_links = true,
            "--no-hard-links" => a.options.hard_links = false,
            "--quiet" | "-q" => a.quiet = true,
            "--units" => {
                a.units = match it.next().map(|v| v.to_string_lossy().to_lowercase()).as_deref() {
                    Some("windows") => UnitStyle::Windows,
                    Some("iec") => UnitStyle::Iec,
                    Some("si") => UnitStyle::Si,
                    _ => return Err(Failure::Usage("--units is windows, iec or si".into())),
                }
            }
            "--csv" => {
                a.csv = Some(it.next().map(|v| v.to_string_lossy().into_owned()).ok_or_else(|| Failure::Usage("--csv needs a file or -".into()))?)
            }
            "--json" => {
                a.json = Some(it.next().map(|v| v.to_string_lossy().into_owned()).ok_or_else(|| Failure::Usage("--json needs a file or -".into()))?)
            }
            flag if flag.starts_with("--") => return Err(Failure::Usage(format!("unknown option `{flag}`"))),
            _ if folder.is_none() => folder = Some(PathBuf::from(arg)),
            extra => return Err(Failure::Usage(format!("one folder at a time (also got `{extra}`)"))),
        }
    }
    a.folder = folder.ok_or_else(|| Failure::Usage("which folder? `whatspace-cli scan <folder>`".into()))?;
    Ok(a)
}

fn scan(args: &[OsString], out: &mut impl Write) -> Result<(), Failure> {
    let a = parse_scan(args)?;
    let (tree, snap, volume, problems) = whatspace_scan::scan(&a.folder, a.options).map_err(|e| Failure::Error(e.to_string()))?;
    let fmt = SizeFormat::new(a.units, Scale::Auto);
    let export_opts = export::Options { max_depth: a.depth, include_files: a.files, metric: a.metric };
    let to_stdout = a.csv.as_deref() == Some("-") || a.json.as_deref() == Some("-");
    if let Some(target) = &a.csv {
        write_export(target, out, |w| export::write_csv(&tree, NodeId::ROOT, &export_opts, w))?;
    }
    if let Some(target) = &a.json {
        write_export(target, out, |w| export::write_json(&tree, NodeId::ROOT, &export_opts, w))?;
    }
    if a.quiet || to_stdout {
        return Ok(());
    }
    print_header(out, &tree, &volume, &fmt)?;
    print_tree(out, &tree, &a, &fmt)?;
    if a.top > 0 {
        let top = whatspace_model::stats::largest_files(&tree, NodeId::ROOT, a.metric, a.top);
        if !top.is_empty() {
            writeln!(out, "\nLargest files:")?;
            for id in top {
                let n = tree.node(id).map_or(0, |n| n.metric(a.metric));
                writeln!(out, "  {:>10}  {}", fmt.format(n), tree.path_of(id).display())?;
            }
        }
    }
    print_summary(out, &snap, problems.len())?;
    Ok(())
}

fn write_export(target: &str, out: &mut impl Write, write: impl Fn(&mut dyn Write) -> io::Result<()>) -> Result<(), Failure> {
    if target == "-" {
        write(out)?;
        return Ok(());
    }
    let file = File::create(target).map_err(|e| Failure::Error(format!("{target}: {e}")))?;
    let mut w = BufWriter::new(file);
    write(&mut w).and_then(|()| w.flush()).map_err(|e| Failure::Error(format!("{target}: {e}")))
}

fn print_header(out: &mut impl Write, tree: &Tree, v: &VolumeInfo, fmt: &SizeFormat) -> io::Result<()> {
    let fs = if v.file_system.is_empty() { "unknown file system".to_string() } else { volumes::pretty_file_system(&v.file_system) };
    write!(out, "{}  ({fs}", tree.root_path().display())?;
    if v.cluster > 0 {
        write!(out, ", {} allocation unit", fmt.format(v.cluster))?;
    }
    if v.total > 0 {
        write!(out, ", {} free of {}", fmt.format(v.available), fmt.format(v.total))?;
    }
    writeln!(out, ")")?;
    let note = volumes::notes(&v.file_system);
    if !note.is_empty() {
        writeln!(out, "  note: {note}")?;
    }
    writeln!(out)
}

fn print_tree(out: &mut impl Write, tree: &Tree, a: &ScanArgs, fmt: &SizeFormat) -> io::Result<()> {
    writeln!(
        out,
        "{:>10}  {:<22} {:>8}  {:>10}  {:>9}  name",
        if a.metric == Metric::Size { "size" } else { "allocated" },
        "",
        "share",
        "files",
        "folders"
    )?;
    let mut stack = vec![(NodeId::ROOT, 0usize)];
    while let Some((id, depth)) = stack.pop() {
        let Some(node) = tree.live(id) else { continue };
        let parent = tree.parent(id).and_then(|p| tree.node(p)).map_or(node.metric(a.metric), |p| p.metric(a.metric));
        let share = percent(node.metric(a.metric), parent);
        let filled = (share / 5.0).round().clamp(0.0, 20.0) as usize;
        let bar = format!("[{}{}]", "#".repeat(filled), "-".repeat(20 - filled));
        let name = if depth == 0 { tree.root_path().display().to_string() } else { tree.name(id).to_string() };
        let marker = if node.flags.intersects(whatspace_model::Flags::PROBLEMS) {
            "  (could not be read)"
        } else if node.flags.intersects(whatspace_model::Flags::NOT_ENTERED) {
            "  (another file system, not scanned)"
        } else {
            ""
        };
        let (files, dirs) = if node.kind == Kind::Dir { (group(node.files), group(node.dirs)) } else { (String::new(), String::new()) };
        writeln!(
            out,
            "{:>10}  {bar} {:>8}  {files:>10}  {dirs:>9}  {}{name}{marker}",
            fmt.format(node.metric(a.metric)),
            format_percent(share),
            "  ".repeat(depth)
        )?;
        if depth < a.depth {
            for kid in tree.sorted_children(id, a.metric).into_iter().rev() {
                if a.files || tree.node(kid).is_some_and(|n| n.kind == Kind::Dir) {
                    stack.push((kid, depth + 1));
                }
            }
        }
    }
    Ok(())
}

fn print_summary(out: &mut impl Write, snap: &Snapshot, problems: usize) -> io::Result<()> {
    let secs = snap.elapsed.as_secs_f64();
    write!(out, "\nScanned {} files and {} folders in {secs:.1} s", group(snap.files), group(snap.dirs))?;
    if snap.errors > 0 {
        write!(out, "; {} could not be read", group(snap.errors))?;
        if problems > 0 && problems < snap.errors as usize {
            write!(out, " (the first {problems} are listed in a JSON export's \"unreadable\" counts)")?;
        }
    }
    writeln!(out, ".")
}
