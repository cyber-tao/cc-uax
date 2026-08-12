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
    assert_eq!(report["schema_version"], 5);
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
    assert_eq!(report["schema_version"], 5);
    assert_eq!(report["status"], "partial");
    assert_eq!(report["layout"]["project_root"], ".");
    assert_eq!(report["layout"]["content_root"], "Content");
    assert_eq!(report["mounts"][0]["package_root"], "/Game");
    assert_eq!(report["mounts"][0]["relative_path"], "Content");
    assert!(report.get("project_file").is_none());
    assert_eq!(report["reachability"]["failed_assets"], 1);
    assert!(
        report["reachability"]["isolated_project_assets"]
            .as_array()
            .unwrap()
            .iter()
            .any(|package| package == "/Game/Good")
    );
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

#[test]
fn strict_project_scan_without_hard_failures_exits_zero_despite_partial() {
    // A package that parses cleanly but reports a non-complete status (here an
    // unsupported future version) is inherent partial evidence, not a hard scan
    // failure. Strict mode must exit 0 and keep the truthful non-complete status.
    let root = temp_dir("inherent");
    let content = root.join("Content");
    write_future_package(&content.join("Future.uasset"));
    let output = bin()
        .args(["project", root.to_str().unwrap(), "--no-cache"])
        .output()
        .unwrap();
    std::fs::remove_dir_all(&root).unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_ne!(report["status"], "complete");
    assert_eq!(report["stats"]["indexed"], 1);
    assert_eq!(report["stats"]["failed"], 0);
    assert!(
        report["failures"]
            .as_array()
            .is_none_or(|failures| failures.is_empty())
    );
}

#[test]
fn asset_with_inherent_partial_status_still_exits_zero() {
    // A cleanly parsed but unsupported package yields a truthful non-complete
    // status; producing that report is a success, not a process failure.
    let root = temp_dir("asset_partial");
    let package = root.join("Future.uasset");
    write_future_package(&package);
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
    assert_ne!(report["status"], "complete");
}

#[test]
fn max_output_bytes_adds_output_block_and_respects_budget() {
    let root = temp_dir("budget");
    let package = root.join("Test.uasset");
    write_package(&package);
    let output = bin()
        .args([
            "asset",
            package.to_str().unwrap(),
            "--view",
            "summary",
            "--compact",
            "--max-output-bytes",
            "100000",
        ])
        .output()
        .unwrap();
    std::fs::remove_dir_all(&root).unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.len() <= 100_000 + 1);
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    // Opting into a budget adds the truncation block; the skeleton is intact.
    assert_eq!(report["status"], "complete");
    assert_eq!(report["summary"]["package_name"], "TestPkg");
    assert_eq!(report["output"]["truncated"], false);
}

#[test]
fn max_output_bytes_below_skeleton_still_emits_valid_json() {
    // A budget too small for even the skeleton must never produce invalid JSON.
    let root = temp_dir("budget_tiny");
    let package = root.join("Test.uasset");
    write_package(&package);
    let output = bin()
        .args([
            "asset",
            package.to_str().unwrap(),
            "--view",
            "full",
            "--compact",
            "--max-output-bytes",
            "20",
        ])
        .output()
        .unwrap();
    std::fs::remove_dir_all(&root).unwrap();

    assert!(output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["output"]["truncated"], true);
    assert_eq!(report["schema_version"], 5);
    assert!(report["status"].is_string());
    assert!(report["coverage"].is_object());
    assert!(report["summary"].is_object());
}

#[test]
fn focus_miss_records_a_failure_but_still_writes_the_report() {
    let root = temp_dir("focus-miss");
    let content = root.join("Content");
    write_package(&content.join("Good.uasset"));
    let out_path = root.join("focus.json");
    let output = bin()
        .args([
            "project",
            root.to_str().unwrap(),
            "--no-cache",
            "--compact",
            "--focus",
            "/Game/Good",
            "--focus",
            "/Game/Typo/DoesNotExist",
            "--output",
            out_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&out_path).unwrap()).unwrap();
    let exit = output.status.code();
    std::fs::remove_dir_all(&root).unwrap();

    // A bad pattern is a strict hard failure, but the full scan is still written.
    assert_eq!(exit, Some(2));
    assert_eq!(report["status"], "partial");
    assert!(!report["inventory"].as_array().unwrap().is_empty());
    assert!(report["focused"]["/Game/Good"].is_object());
    assert!(
        report["failures"]
            .as_array()
            .unwrap()
            .iter()
            .any(|failure| failure["stage"] == "focus")
    );
}

#[test]
fn focus_miss_with_allow_partial_exits_zero() {
    let root = temp_dir("focus-allow");
    let content = root.join("Content");
    write_package(&content.join("Good.uasset"));
    let out_path = root.join("focus.json");
    let output = bin()
        .args([
            "project",
            root.to_str().unwrap(),
            "--no-cache",
            "--compact",
            "--allow-partial",
            "--focus",
            "/Game/Typo/DoesNotExist",
            "--output",
            out_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let report: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&out_path).unwrap()).unwrap();
    let exit = output.status.code();
    std::fs::remove_dir_all(&root).unwrap();

    assert_eq!(exit, Some(0));
    assert!(
        report["failures"]
            .as_array()
            .unwrap()
            .iter()
            .any(|failure| failure["stage"] == "focus")
    );
}

#[test]
fn failed_run_overwrites_a_stale_output_file() {
    let root = temp_dir("stale");
    let content = root.join("Content");
    write_package(&content.join("Good.uasset"));
    let out_path = root.join("report.json");

    // A successful scan writes a full report to --output.
    let ok = bin()
        .args([
            "project",
            root.to_str().unwrap(),
            "--no-cache",
            "--compact",
            "--output",
            out_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(ok.status.success());
    let first = std::fs::read_to_string(&out_path).unwrap();
    assert!(first.contains("\"layout\""));

    // A later failing command targeting the same file must replace its contents,
    // so the stale success report is never read back.
    let missing = root.join("Nope.uasset");
    let fail = bin()
        .args([
            "asset",
            missing.to_str().unwrap(),
            "--view",
            "summary",
            "--compact",
            "--output",
            out_path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let second = std::fs::read_to_string(&out_path).unwrap();
    std::fs::remove_dir_all(&root).unwrap();

    assert!(!fail.status.success());
    assert!(second.contains("\"status\":\"error\""));
    assert!(!second.contains("\"layout\""));
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

fn write_future_package(path: &Path) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, future_version_package()).unwrap();
}

fn future_version_package() -> Vec<u8> {
    // minimal_package targets the highest supported FileVersionUE5 (1018) at
    // bytes 16..20. Bumping it past the supported ceiling makes analysis report
    // `unsupported` while the package still parses cleanly (no hard failure).
    let mut bytes = minimal_package();
    bytes[16..20].copy_from_slice(&1019_i32.to_le_bytes());
    bytes
}
