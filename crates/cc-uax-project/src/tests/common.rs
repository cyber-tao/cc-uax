use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static COUNTER: AtomicU64 = AtomicU64::new(0);

pub fn temp_project(prefix: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "cc_uax_project_{prefix}_{}_{}_{}",
        std::process::id(),
        nanos,
        counter
    ));
    std::fs::create_dir_all(root.join("Content")).unwrap();
    root
}

pub fn minimal_package() -> Vec<u8> {
    let mut data = Vec::new();
    push_u32(&mut data, 0x9E2A_83C1);
    push_i32(&mut data, -8);
    push_i32(&mut data, 0);
    push_i32(&mut data, 522);
    push_i32(&mut data, 1018);
    push_i32(&mut data, 0);
    data.extend_from_slice(&[0u8; 20]);
    push_i32(&mut data, 0);
    push_i32(&mut data, 0);
    push_fstring(&mut data, "TestPkg");
    push_u32(&mut data, 0x8000_0000);
    for _ in 0..23 {
        push_i32(&mut data, 0);
    }
    push_u16(&mut data, 5);
    push_u16(&mut data, 7);
    push_u16(&mut data, 0);
    push_u32(&mut data, 0);
    push_fstring(&mut data, "");
    push_u16(&mut data, 5);
    push_u16(&mut data, 7);
    push_u16(&mut data, 0);
    push_u32(&mut data, 0);
    push_fstring(&mut data, "");
    push_u32(&mut data, 0);
    push_i32(&mut data, 0);
    push_u32(&mut data, 0);
    push_i32(&mut data, 0);
    push_i32(&mut data, 0);
    push_i64(&mut data, 0);
    for _ in 0..5 {
        push_i32(&mut data, 0);
    }
    push_i64(&mut data, 0);
    push_i32(&mut data, 0);
    data
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
