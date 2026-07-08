//! CLI integration tests: spawn the built `cc-uax` binary and assert on exit
//! status and stderr/stdout.

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

fn push_u16(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_le_bytes());
}

fn push_u32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}

fn push_i32(v: &mut Vec<u8>, x: i32) {
    v.extend_from_slice(&x.to_le_bytes());
}

fn push_i64(v: &mut Vec<u8>, x: i64) {
    v.extend_from_slice(&x.to_le_bytes());
}

fn push_fstring(v: &mut Vec<u8>, s: &str) {
    if s.is_empty() {
        push_i32(v, 0);
        return;
    }
    push_i32(v, (s.len() + 1) as i32);
    v.extend_from_slice(s.as_bytes());
    v.push(0);
}

fn build_minimal_package() -> Vec<u8> {
    let mut d = Vec::new();
    push_u32(&mut d, 0x9E2A_83C1); // PACKAGE_FILE_TAG
    push_i32(&mut d, -8); // legacy_file_version
    push_i32(&mut d, 0); // legacy ue3 version
    push_i32(&mut d, 522); // file_version_ue4
    push_i32(&mut d, 1018); // file_version_ue5
    push_i32(&mut d, 0); // file_version_licensee
    d.extend_from_slice(&[0u8; 20]); // saved_hash
    push_i32(&mut d, 0); // total_header_size
    push_i32(&mut d, 0); // custom version count
    push_fstring(&mut d, "TestPkg"); // package_name
    push_u32(&mut d, 0x8000_0000); // package_flags = FilterEditorOnly
    push_i32(&mut d, 0); // name_count
    push_i32(&mut d, 0); // name_offset
    push_i32(&mut d, 0); // soft_object_paths_count
    push_i32(&mut d, 0); // soft_object_paths_offset
    push_i32(&mut d, 0); // gatherable_text_data_count
    push_i32(&mut d, 0); // gatherable_text_data_offset
    push_i32(&mut d, 0); // export_count
    push_i32(&mut d, 0); // export_offset
    push_i32(&mut d, 0); // import_count
    push_i32(&mut d, 0); // import_offset
    push_i32(&mut d, 0); // cell_export_count
    push_i32(&mut d, 0); // cell_export_offset
    push_i32(&mut d, 0); // cell_import_count
    push_i32(&mut d, 0); // cell_import_offset
    push_i32(&mut d, 0); // metadata_offset
    push_i32(&mut d, 0); // depends_offset
    push_i32(&mut d, 0); // soft_package_references_count
    push_i32(&mut d, 0); // soft_package_references_offset
    push_i32(&mut d, 0); // searchable_names_offset
    push_i32(&mut d, 0); // thumbnail_table_offset
    push_i32(&mut d, 0); // import_type_hierarchies_count
    push_i32(&mut d, 0); // import_type_hierarchies_offset
    push_i32(&mut d, 0); // generation_count
    push_u16(&mut d, 5); // engine_version.major
    push_u16(&mut d, 7); // .minor
    push_u16(&mut d, 0); // .patch
    push_u32(&mut d, 0); // .changelist
    push_fstring(&mut d, ""); // .branch
    push_u16(&mut d, 5); // compatible_engine_version
    push_u16(&mut d, 7);
    push_u16(&mut d, 0);
    push_u32(&mut d, 0);
    push_fstring(&mut d, "");
    push_u32(&mut d, 0);
    push_i32(&mut d, 0);
    push_u32(&mut d, 0);
    push_i32(&mut d, 0);
    push_i32(&mut d, 0);
    push_i64(&mut d, 0);
    push_i32(&mut d, 0);
    push_i32(&mut d, 0);
    push_i32(&mut d, 0);
    push_i32(&mut d, 0);
    push_i32(&mut d, 0);
    push_i64(&mut d, 0);
    push_i32(&mut d, 0);
    d
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
            "dump",
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

#[test]
fn cli_unmapped_input_mount_errors() {
    let path = write_temp_package();
    let dir = path.parent().unwrap().to_owned();
    let out = bin()
        .args([
            "-S",
            "refs",
            "-d",
            dir.to_str().unwrap(),
            "-m",
            "/Game=Content",
            path.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let _ = std::fs::remove_file(&path);

    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("not covered by --mount mapping"),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}
