use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_cc-uax"))
}

fn temp_dir(label: &str) -> PathBuf {
    let id = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("cc-uax-cli-{label}-{}-{id}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn write_package(path: &Path) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, minimal_package()).unwrap();
}

#[test]
fn asset_summary_uses_the_new_subcommand_and_typed_schema() {
    let root = temp_dir("asset");
    let package = root.join("Test.uasset");
    write_package(&package);
    let output = bin()
        .args(["asset", package.to_str().unwrap(), "--view", "summary"])
        .output()
        .unwrap();
    std::fs::remove_dir_all(&root).unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["view"], "summary");
    assert_eq!(report["status"], "complete");
    assert_eq!(report["summary"]["package_name"], "TestPkg");
}

#[test]
fn strict_project_scan_emits_a_partial_report_and_fails() {
    let root = temp_dir("strict");
    let content = root.join("Content");
    write_package(&content.join("Good.uasset"));
    std::fs::write(content.join("Broken.uasset"), b"not a package").unwrap();
    let output = bin()
        .args(["project", root.to_str().unwrap(), "--no-cache"])
        .output()
        .unwrap();
    std::fs::remove_dir_all(&root).unwrap();

    assert_eq!(output.status.code(), Some(2));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "partial");
    assert_eq!(report["stats"]["discovered"], 2);
    assert_eq!(report["stats"]["indexed"], 1);
    assert_eq!(report["stats"]["failed"], 1);
    let path = report["failures"][0]["path"].as_str().unwrap();
    assert!(!path.contains(root.to_str().unwrap()));
}

#[test]
fn allow_partial_is_an_explicit_zero_exit_override() {
    let root = temp_dir("partial");
    let content = root.join("Content");
    std::fs::create_dir_all(&content).unwrap();
    std::fs::write(content.join("Broken.uasset"), b"not a package").unwrap();
    let output = bin()
        .args([
            "project",
            root.to_str().unwrap(),
            "--no-cache",
            "--allow-partial",
        ])
        .output()
        .unwrap();
    std::fs::remove_dir_all(&root).unwrap();

    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "partial");
    assert_eq!(report["stats"]["failed"], 1);
}

fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_i32(bytes: &mut Vec<u8>, value: i32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_i64(bytes: &mut Vec<u8>, value: i64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_fstring(bytes: &mut Vec<u8>, value: &str) {
    if value.is_empty() {
        push_i32(bytes, 0);
    } else {
        push_i32(bytes, (value.len() + 1) as i32);
        bytes.extend_from_slice(value.as_bytes());
        bytes.push(0);
    }
}

fn minimal_package() -> Vec<u8> {
    let mut bytes = Vec::new();
    push_u32(&mut bytes, 0x9E2A_83C1);
    push_i32(&mut bytes, -8);
    push_i32(&mut bytes, 0);
    push_i32(&mut bytes, 522);
    push_i32(&mut bytes, 1018);
    push_i32(&mut bytes, 0);
    bytes.extend_from_slice(&[0; 20]);
    push_i32(&mut bytes, 0);
    push_i32(&mut bytes, 0);
    push_fstring(&mut bytes, "TestPkg");
    push_u32(&mut bytes, 0x8000_0000);
    for _ in 0..23 {
        push_i32(&mut bytes, 0);
    }
    push_u16(&mut bytes, 5);
    push_u16(&mut bytes, 7);
    push_u16(&mut bytes, 0);
    push_u32(&mut bytes, 0);
    push_fstring(&mut bytes, "");
    push_u16(&mut bytes, 5);
    push_u16(&mut bytes, 7);
    push_u16(&mut bytes, 0);
    push_u32(&mut bytes, 0);
    push_fstring(&mut bytes, "");
    push_u32(&mut bytes, 0);
    push_i32(&mut bytes, 0);
    push_u32(&mut bytes, 0);
    for _ in 0..2 {
        push_i32(&mut bytes, 0);
    }
    push_i64(&mut bytes, 0);
    for _ in 0..5 {
        push_i32(&mut bytes, 0);
    }
    push_i64(&mut bytes, 0);
    push_i32(&mut bytes, 0);
    bytes
}
