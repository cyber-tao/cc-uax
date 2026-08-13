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

fn put_i32(bytes: &mut [u8], offset: usize, value: i32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
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

/// Builds a valid FileVersionUE5=1018 package that carries one soft package
/// reference to each of `targets` (e.g. "/Game/Other"). The name table holds the
/// target paths and the soft-package-reference table points at them, so a project
/// scan sees real cross-package adjacency without any external assets.
pub fn package_with_soft_refs(targets: &[&str]) -> Vec<u8> {
    let mut d = Vec::new();
    push_u32(&mut d, 0x9E2A_83C1); // tag
    push_i32(&mut d, -8); // legacy_file_version
    push_i32(&mut d, 0); // legacy ue3
    push_i32(&mut d, 522); // file_version_ue4
    push_i32(&mut d, 1018); // file_version_ue5
    push_i32(&mut d, 0); // licensee
    d.extend_from_slice(&[0u8; 20]); // saved_hash (fv >= 1016)
    let total_header_size_pos = d.len();
    push_i32(&mut d, 0); // total_header_size (patched below)
    push_i32(&mut d, 0); // custom version count
    push_fstring(&mut d, "TestPkg"); // package_name
    push_u32(&mut d, 0x8000_0000); // package_flags = FilterEditorOnly
    let name_count_pos = d.len();
    push_i32(&mut d, 0); // name_count (patched)
    let name_offset_pos = d.len();
    push_i32(&mut d, 0); // name_offset (patched)
    push_i32(&mut d, 0); // soft_object_paths_count (fv >= 1008)
    push_i32(&mut d, 0); // soft_object_paths_offset
    push_i32(&mut d, 0); // gatherable_text_data_count
    push_i32(&mut d, 0); // gatherable_text_data_offset
    push_i32(&mut d, 0); // export_count
    push_i32(&mut d, 0); // export_offset
    push_i32(&mut d, 0); // import_count
    push_i32(&mut d, 0); // import_offset
    for _ in 0..4 {
        push_i32(&mut d, 0); // cell export/import counts+offsets (fv >= 1015)
    }
    push_i32(&mut d, 0); // metadata_offset (fv >= 1014)
    push_i32(&mut d, 0); // depends_offset
    let soft_ref_count_pos = d.len();
    push_i32(&mut d, 0); // soft_package_references_count (patched)
    let soft_ref_offset_pos = d.len();
    push_i32(&mut d, 0); // soft_package_references_offset (patched)
    push_i32(&mut d, 0); // searchable_names_offset
    push_i32(&mut d, 0); // thumbnail_table_offset
    push_i32(&mut d, 0); // import_type_hierarchies_count (fv >= 1018)
    push_i32(&mut d, 0); // import_type_hierarchies_offset
    push_i32(&mut d, 0); // generation_count
    push_u16(&mut d, 5); // engine_version major/minor/patch
    push_u16(&mut d, 7);
    push_u16(&mut d, 0);
    push_u32(&mut d, 0); // changelist
    push_fstring(&mut d, ""); // branch
    push_u16(&mut d, 5); // compatible_engine_version
    push_u16(&mut d, 7);
    push_u16(&mut d, 0);
    push_u32(&mut d, 0);
    push_fstring(&mut d, "");
    push_u32(&mut d, 0); // compression_flags
    push_i32(&mut d, 0); // compressed_chunks_count
    push_u32(&mut d, 0); // package_source
    push_i32(&mut d, 0); // additional_packages_to_cook count
    push_i32(&mut d, 0); // asset_registry_data_offset
    push_i64(&mut d, 0); // bulk_data_start_offset
    push_i32(&mut d, 0); // world_tile_info_data_offset
    push_i32(&mut d, 0); // chunk ids count
    push_i32(&mut d, 0); // preload_dependency_count
    push_i32(&mut d, 0); // preload_dependency_offset
    push_i32(&mut d, 0); // names_referenced_from_export_data_count
    push_i64(&mut d, 0); // payload_toc_offset
    push_i32(&mut d, 0); // data_resource_offset (fv >= 1009)

    let header_size = d.len();
    put_i32(&mut d, total_header_size_pos, header_size as i32);

    // Name table: one entry per target path, FString + 4-byte hash.
    put_i32(&mut d, name_count_pos, targets.len() as i32);
    let name_offset = d.len() as i32;
    put_i32(&mut d, name_offset_pos, name_offset);
    for target in targets {
        push_fstring(&mut d, target);
        push_u32(&mut d, 0); // name hash (skipped by the parser)
    }

    // Soft package reference table: one RawName (index, number) per target.
    put_i32(&mut d, soft_ref_count_pos, targets.len() as i32);
    let soft_ref_offset = d.len() as i32;
    put_i32(&mut d, soft_ref_offset_pos, soft_ref_offset);
    for index in 0..targets.len() {
        push_i32(&mut d, index as i32); // name index
        push_i32(&mut d, 0); // name number
    }
    d
}
