//! CLI integration tests: spawn the built `cc-uax` binary and assert on exit
//! status and stderr/stdout. Gated on the `cli` feature so the binary exists.
#![cfg(feature = "cli")]

mod common;

use common::build_minimal_package;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_cc-uax"))
}

fn write_temp_package() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("cc-uax-it-{}-{}.uasset", std::process::id(), n));
    std::fs::write(&path, build_minimal_package()).unwrap();
    path
}

#[test]
fn cli_valid_summary_run_succeeds() {
    let path = write_temp_package();
    let out = bin()
        .args(["-S", "summary", path.to_str().unwrap()])
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&path);

    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(json["summary"]["package_name"], "TestPkg");
}

#[test]
fn cli_unknown_section_errors() {
    let path = write_temp_package();
    let out = bin()
        .args(["-S", "bogus", path.to_str().unwrap()])
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&path);

    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("unknown section"),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn cli_scan_dir_without_refs_errors() {
    let path = write_temp_package();
    let dir = path.parent().unwrap().to_owned();
    let out = bin()
        .args([
            "-S",
            "full",
            "-d",
            dir.to_str().unwrap(),
            path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&path);

    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("requires the references section"),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn cli_invalid_mount_errors() {
    let path = write_temp_package();
    let out = bin()
        .args(["-m", "C:/Game", "-S", "summary", path.to_str().unwrap()])
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&path);

    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("looks like a filesystem path"),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
