use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// Assert against the constants rather than copied literals, so a schema bump
/// cannot leave these tests pinning a version the CLI no longer emits.
fn asset_schema() -> serde_json::Value {
    cc_uax_core::ASSET_ANALYSIS_SCHEMA_VERSION.into()
}

fn project_schema() -> serde_json::Value {
    cc_uax_cli::PROJECT_REPORT_SCHEMA_VERSION.into()
}

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

// The exit code answers "did the run hold together", not "is the evidence
// complete". Pin every boundary so the two cannot be conflated again: a partial or
// unsupported report is still a successful run, only a project scan failure is 2,
// and 1 is reserved for not producing a report at all.
#[test]
fn exit_codes_separate_run_failure_from_evidence_gaps() {
    let root = temp_dir("exitcodes");
    let content = root.join("Content");
    write_package(&content.join("Good.uasset"));
    let project = root.to_str().unwrap().to_string();

    let code = |args: &[&str]| {
        bin()
            .args(args)
            .output()
            .unwrap()
            .status
            .code()
            .expect("the process must exit normally")
    };

    // 0: a report was produced and nothing the run was asked to do failed.
    assert_eq!(
        code(&["asset", content.join("Good.uasset").to_str().unwrap()]),
        0
    );
    assert_eq!(code(&["project", &project, "--no-cache"]), 0);

    // 0: an out-of-scope package is unsupported evidence, not a failure.
    std::fs::write(content.join("Legacy.uasset"), ue4_package()).unwrap();
    let output = bin()
        .args(["project", &project, "--no-cache"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "partial");
    assert_eq!(report["analysis"]["unsupported_assets"], 1);
    assert!(report.get("failures").is_none(), "{report}");

    // 2: a mapped package that is not readable at all is a hard scan failure, and
    // --allow-partial is the explicit override back to 0.
    std::fs::write(content.join("Broken.uasset"), b"not a package").unwrap();
    assert_eq!(code(&["project", &project, "--no-cache"]), 2);
    assert_eq!(
        code(&["project", &project, "--no-cache", "--allow-partial"]),
        0
    );
    std::fs::remove_file(content.join("Broken.uasset")).unwrap();

    // 2: a --focus pattern that selects nothing is a hard failure too.
    assert_eq!(
        code(&[
            "project",
            &project,
            "--no-cache",
            "--focus",
            "/Game/Nope/**"
        ]),
        2
    );

    // 1: no report could be produced. Each of these fails before analysis.
    assert_eq!(
        code(&["asset", root.join("Missing.uasset").to_str().unwrap()]),
        1
    );
    assert_eq!(
        code(&["project", root.join("NoSuchProject").to_str().unwrap()]),
        1
    );
    assert_eq!(
        code(&[
            "project",
            &project,
            "--no-cache",
            "--mount",
            "not-a-mapping"
        ]),
        1
    );

    // The exit-1 document is the only place `status` is "error"; it is not a report.
    let output = bin()
        .args(["asset", root.join("Missing.uasset").to_str().unwrap()])
        .output()
        .unwrap();
    let failure: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(failure["status"], "error");
    assert!(failure["message"].is_string());
    assert!(failure.get("coverage").is_none());

    std::fs::remove_dir_all(&root).unwrap();
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
    assert_eq!(report["schema_version"], asset_schema());
    assert_eq!(report["view"], "summary");
    assert_eq!(report["status"], "complete");
    assert_eq!(report["summary"]["package_name"], "TestPkg");
}

// `--view` restricts what is decoded, not just what is rendered, so each view has
// to be exercised through the binary rather than only at the core boundary.
#[test]
fn each_asset_view_renders_its_own_sections() {
    let root = temp_dir("views");
    let package = root.join("Test.uasset");
    write_package(&package);
    let file = package.to_str().unwrap().to_string();

    let view = |name: &str| -> serde_json::Value {
        let output = bin()
            .args(["asset", &file, "--view", name])
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(0),
            "view {name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    };

    for name in ["summary", "logic", "properties", "references", "full"] {
        let report = view(name);
        assert_eq!(report["view"], name);
        assert_eq!(report["schema_version"], asset_schema());
        assert!(
            report["summary"]["package_name"] == "TestPkg",
            "view {name}"
        );
    }
    // Only `full` carries export byte placement; the focused views omit it.
    assert!(view("full")["summary"].is_object());
    assert!(view("summary").get("exports").is_none());

    std::fs::remove_dir_all(&root).unwrap();
}

// The documented glob rules, exercised through the binary: `*` stays inside one
// path segment, `**` crosses separators, matching is case-insensitive, and an
// asset/object suffix is stripped.
#[test]
fn focus_glob_rules_hold_through_the_binary() {
    let root = temp_dir("focusglob");
    let content = root.join("Content");
    write_package(&content.join("Blueprints/BP_Top.uasset"));
    write_package(&content.join("Blueprints/Nested/BP_Deep.uasset"));
    write_package(&content.join("Maps/L_Main.umap"));
    let project = root.to_str().unwrap().to_string();

    let focused = |pattern: &str| -> Vec<String> {
        let output = bin()
            .args(["project", &project, "--no-cache", "--focus", pattern])
            .output()
            .unwrap();
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let mut keys = report["focused"]
            .as_object()
            .map(|map| map.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        keys.sort();
        keys
    };

    // `*` matches within one segment only.
    assert_eq!(focused("/Game/Blueprints/*"), ["/Game/Blueprints/BP_Top"]);
    // `**` crosses separators.
    assert_eq!(
        focused("/Game/Blueprints/**"),
        ["/Game/Blueprints/BP_Top", "/Game/Blueprints/Nested/BP_Deep"]
    );
    // Case-insensitive, and a `.uasset` suffix is stripped.
    assert_eq!(
        focused("/game/blueprints/bp_top.uasset"),
        ["/Game/Blueprints/BP_Top"]
    );
    // `.umap` too, and an object suffix after the package name.
    assert_eq!(focused("/Game/Maps/L_Main.umap"), ["/Game/Maps/L_Main"]);
    assert_eq!(focused("/Game/Maps/L_Main.L_Main"), ["/Game/Maps/L_Main"]);

    std::fs::remove_dir_all(&root).unwrap();
}

// A malformed command line is not a scan result. Exit 2 is reserved for a project
// hard scan failure that still wrote a report, so a usage error must not be
// mistaken for one, and it must never leave a stale `--output` behind.
#[test]
fn a_usage_error_is_not_a_scan_failure() {
    let root = temp_dir("usage");
    let out = root.join("stale.json");
    std::fs::write(&out, b"{\"status\":\"complete\"}").unwrap();

    let output = bin()
        .args([
            "asset",
            "--view",
            "not-a-view",
            "missing.uasset",
            "--output",
            out.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert_eq!(
        output.status.code(),
        Some(1),
        "usage errors must not share the project hard-failure code"
    );
    // No report was produced, so nothing should have been written.
    assert_eq!(
        std::fs::read_to_string(&out).unwrap().trim(),
        "{\"status\":\"complete\"}"
    );
    std::fs::remove_dir_all(&root).unwrap();
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
    assert_eq!(report["schema_version"], project_schema());
    assert_eq!(report["status"], "partial");
    assert_eq!(report["layout"]["project_root"], ".");
    assert_eq!(report["layout"]["content_root"], "Content");
    assert_eq!(report["mounts"][0]["package_root"], "/Game");
    assert_eq!(report["mounts"][0]["relative_path"], "Content");
    // `project_file` lives under `layout`, and this temp project has no
    // `.uproject`. Asserting the top level was tautological: nothing ever puts a
    // `project_file` key there.
    assert!(report["layout"].get("project_file").is_none());
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
    // failure. Strict mode must exit 0 and surface the truthful status — unsupported
    // when every scanned asset is unsupported.
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
    assert_eq!(report["status"], "unsupported");
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
    // A package that parses cleanly but carries an evidence gap yields a truthful
    // non-complete status; producing that report is a success, not a process
    // failure. A package this tool refuses outright is the other case and exits 1.
    let root = temp_dir("asset_partial");
    let package = root.join("BrokenRefs.uasset");
    std::fs::write(&package, broken_soft_path_table_package()).unwrap();
    let output = bin()
        .args(["asset", package.to_str().unwrap(), "--view", "references"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["status"], "partial", "{report}");

    // A file version past the highest known layout is out of scope, not partial:
    // UE itself stops reading such a package, so no report is produced.
    let future = root.join("Future.uasset");
    write_future_package(&future);
    let output = bin()
        .args(["asset", future.to_str().unwrap(), "--view", "summary"])
        .output()
        .unwrap();
    std::fs::remove_dir_all(&root).unwrap();

    assert_eq!(output.status.code(), Some(1));
    let failure: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(failure["status"], "error");
    assert!(
        failure["message"]
            .as_str()
            .is_some_and(|message| message.contains("newer than this parser understands")),
        "{failure}"
    );
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
    assert_eq!(report["schema_version"], asset_schema());
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
    build_minimal_package(0)
}

/// A package that parses cleanly but whose soft-object-path table cannot be read,
/// which `analyze` reports as a `reference_tables` gap. That is inherent partial
/// evidence, distinct from a package this tool refuses outright.
fn broken_soft_path_table_package() -> Vec<u8> {
    build_minimal_package(1)
}

/// `soft_object_path_count` is the only knob: a non-zero count with a zero offset
/// is exactly the shape UE never writes, so the table read fails while every other
/// summary field stays valid.
fn build_minimal_package(soft_object_path_count: i32) -> Vec<u8> {
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
    push_i32(&mut bytes, 0); // name_count
    push_i32(&mut bytes, 0); // name_offset
    push_i32(&mut bytes, soft_object_path_count);
    push_i32(&mut bytes, 0); // soft_object_paths_offset
    for _ in 0..19 {
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

/// A real UE4 package header (`FileVersionUE5` = 0). Readable, deliberately out of
/// scope, and therefore `unsupported` evidence rather than a scan failure.
fn ue4_package() -> Vec<u8> {
    let mut bytes = Vec::new();
    push_u32(&mut bytes, 0x9E2A_83C1);
    push_i32(&mut bytes, -7); // legacy_file_version: no FileVersionUE5 field follows
    push_i32(&mut bytes, 0); // legacy ue3
    push_i32(&mut bytes, 522); // file_version_ue4
    push_i32(&mut bytes, 0); // licensee
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
