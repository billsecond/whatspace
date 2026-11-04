//! The command-line program as somebody runs it.

use std::fs;
use std::process::{Command, Stdio};

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_whatspace-cli"))
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("whatspace-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("photos")).unwrap();
    fs::write(dir.join("photos").join("big.jpg"), vec![1u8; 30_000]).unwrap();
    fs::write(dir.join("notes.txt"), b"hello").unwrap();
    dir
}

#[test]
fn prints_its_version() {
    let out = cli().arg("--version").output().unwrap();
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), format!("whatspace-cli {}", env!("CARGO_PKG_VERSION")));
}

#[test]
fn scans_and_prints_the_largest() {
    let dir = scratch("print");
    let out = cli().args(["scan", "--depth", "2", "--files", "--units", "si"]).arg(&dir).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{text}\n{}", String::from_utf8_lossy(&out.stderr));
    assert!(text.contains("photos"), "{text}");
    assert!(text.contains("30.0 kB"), "{text}");
    assert!(text.contains("Largest files:"), "{text}");
    assert!(text.contains("Scanned 2 files and 1 folders"), "{text}");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn exports_json_to_stdout() {
    let dir = scratch("json");
    let out = cli().args(["scan", "--files", "--json", "-"]).arg(&dir).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["size"], 30_005);
    assert_eq!(v["files"], 2);
    assert_eq!(v["children"][0]["name"], "photos");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn bad_arguments_exit_2() {
    for args in [&["scan"][..], &["scan", "--depth"], &["scan", "--nope", "."], &["frobnicate"], &["scan", "--units", "furlongs", "."]] {
        let out = cli().args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(String::from_utf8_lossy(&out.stderr).contains("usage:"), "{args:?}");
    }
    let missing = cli().args(["scan", "/definitely/not/here/whatspace"]).output().unwrap();
    assert_eq!(missing.status.code(), Some(1));
}

#[test]
fn a_closed_output_ends_quietly() {
    let dir = scratch("closed");
    let mut child = cli().args(["scan", "--depth", "3", "--files"]).arg(&dir).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    drop(child.stdout.take());
    let out = child.wait_with_output().unwrap();
    assert!(!String::from_utf8_lossy(&out.stderr).contains("panicked"), "{}", String::from_utf8_lossy(&out.stderr));
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn lists_volumes() {
    let out = cli().arg("volumes").output().unwrap();
    assert!(out.status.success());
}
