#![allow(dead_code)]

use crate::object::{ObjectExport, PackageIndex};
use crate::reader::RawName;

pub fn diagnostic_with_code<'a>(json: &'a serde_json::Value, code: &str) -> &'a serde_json::Value {
    json["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|diag| diag["code"].as_str() == Some(code))
        .unwrap_or_else(|| panic!("missing diagnostic code {code}: {json}"))
}

pub fn push_u16(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_le_bytes());
}
pub fn push_u32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}
pub fn push_i32(v: &mut Vec<u8>, x: i32) {
    v.extend_from_slice(&x.to_le_bytes());
}
pub fn push_i64(v: &mut Vec<u8>, x: i64) {
    v.extend_from_slice(&x.to_le_bytes());
}
pub fn put_i32(v: &mut [u8], offset: usize, x: i32) {
    v[offset..offset + 4].copy_from_slice(&x.to_le_bytes());
}
pub fn push_raw_name(v: &mut Vec<u8>, index: i32) {
    push_i32(v, index);
    push_i32(v, 0);
}
pub fn push_fstring(v: &mut Vec<u8>, s: &str) {
    if s.is_empty() {
        push_i32(v, 0);
        return;
    }
    push_i32(v, (s.len() + 1) as i32);
    v.extend_from_slice(s.as_bytes());
    v.push(0);
}

// Minimal versioned UE5 package header (legacy=-8, ue4=522, ue5=1018,
// FilterEditorOnly set to skip editor-only fields, all tables empty).
pub fn build_minimal_package() -> Vec<u8> {
    let mut d = Vec::new();
    push_u32(&mut d, 0x9E2A_83C1); // PACKAGE_FILE_TAG
    push_i32(&mut d, -8); // legacy_file_version
    push_i32(&mut d, 0); // legacy ue3 version (legacy != -4)
    push_i32(&mut d, 522); // file_version_ue4
    push_i32(&mut d, 1018); // file_version_ue5
    push_i32(&mut d, 0); // file_version_licensee
    d.extend_from_slice(&[0u8; 20]); // saved_hash (ue5 >= 1016)
    push_i32(&mut d, 0); // total_header_size
    push_i32(&mut d, 0); // custom version count
    push_fstring(&mut d, "TestPkg"); // package_name
    push_u32(&mut d, 0x8000_0000); // package_flags = FilterEditorOnly
    push_i32(&mut d, 0); // name_count
    push_i32(&mut d, 0); // name_offset
    push_i32(&mut d, 0); // soft_object_paths_count (ue5 >= 1008)
    push_i32(&mut d, 0); // soft_object_paths_offset
    push_i32(&mut d, 0); // gatherable_text_data_count (ue4 >= 459)
    push_i32(&mut d, 0); // gatherable_text_data_offset
    push_i32(&mut d, 0); // export_count
    push_i32(&mut d, 0); // export_offset
    push_i32(&mut d, 0); // import_count
    push_i32(&mut d, 0); // import_offset
    push_i32(&mut d, 0); // cell_export_count (ue5 >= 1015)
    push_i32(&mut d, 0); // cell_export_offset
    push_i32(&mut d, 0); // cell_import_count
    push_i32(&mut d, 0); // cell_import_offset
    push_i32(&mut d, 0); // metadata_offset (ue5 >= 1014)
    push_i32(&mut d, 0); // depends_offset
    push_i32(&mut d, 0); // soft_package_references_count (ue4 >= 384)
    push_i32(&mut d, 0); // soft_package_references_offset
    push_i32(&mut d, 0); // searchable_names_offset (ue4 >= 510)
    push_i32(&mut d, 0); // thumbnail_table_offset
    push_i32(&mut d, 0); // import_type_hierarchies_count (ue5 >= 1018)
    push_i32(&mut d, 0); // import_type_hierarchies_offset
    push_i32(&mut d, 0); // generation_count
    push_u16(&mut d, 5); // engine_version.major (ue4 >= 336)
    push_u16(&mut d, 7); // .minor
    push_u16(&mut d, 0); // .patch
    push_u32(&mut d, 0); // .changelist
    push_fstring(&mut d, ""); // .branch
    push_u16(&mut d, 5); // compatible_engine_version (ue4 >= 444)
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
    push_i32(&mut d, 0); // world_tile_info_data_offset (ue4 >= 224)
    push_i32(&mut d, 0); // chunk ids count (ue4 >= 392)
    push_i32(&mut d, 0); // preload_dependency_count (ue4 >= 507)
    push_i32(&mut d, 0); // preload_dependency_offset
    push_i32(&mut d, 0); // names_referenced_from_export_data_count (ue5 >= 1001)
    push_i64(&mut d, 0); // payload_toc_offset (ue5 >= 1002)
    push_i32(&mut d, 0); // data_resource_offset (ue5 >= 1009)
    d
}

pub fn test_export(
    object_name: i32,
    serial_size: i64,
    script_start: i64,
    script_end: i64,
) -> ObjectExport {
    ObjectExport {
        class_index: PackageIndex(0),
        super_index: PackageIndex(0),
        template_index: PackageIndex(0),
        outer_index: PackageIndex(0),
        object_name: RawName {
            index: object_name,
            number: 0,
        },
        object_flags: 0,
        serial_size,
        serial_offset: 0,
        is_asset: false,
        script_serialization_start_offset: script_start,
        script_serialization_end_offset: script_end,
    }
}

pub fn push_f32(v: &mut Vec<u8>, x: f32) {
    v.extend_from_slice(&x.to_le_bytes());
}
pub fn push_f64(v: &mut Vec<u8>, x: f64) {
    v.extend_from_slice(&x.to_le_bytes());
}
pub fn push_mesh_to_mesh_vert_data(v: &mut Vec<u8>, weight: f32) {
    for x in 0..12 {
        push_f32(v, x as f32);
    }
    for x in 0..4 {
        push_u16(v, x as u16);
    }
    push_f32(v, weight);
    push_u32(v, 0);
}

// Wrap pre-built `value` bytes as a single StructProperty named index 0 with a
// struct type name at `struct_idx`, then a trailing None (index `none_idx`).
pub fn build_struct_property(struct_idx: i32, none_idx: i32, value: &[u8]) -> Vec<u8> {
    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // property name
    push_raw_name(&mut d, 1); // "StructProperty"
    push_i32(&mut d, 1); // one type parameter
    push_raw_name(&mut d, struct_idx); // struct name
    push_i32(&mut d, 0);
    push_i32(&mut d, value.len() as i32);
    d.push(0x08); // HasBinaryOrNativeSerialize
    d.extend_from_slice(value);
    push_raw_name(&mut d, none_idx); // None
    d
}

pub fn push_legacy_tag_header(v: &mut Vec<u8>, name_idx: i32, type_idx: i32, size: i32) {
    push_raw_name(v, name_idx);
    push_raw_name(v, type_idx);
    push_i32(v, size);
    push_i32(v, 0); // ArrayIndex
}

pub fn push_legacy_tag_tail(v: &mut Vec<u8>) {
    v.push(0); // HasPropertyGuid
    v.push(0); // UE5 1011 PropertyExtensions = NoExtension
}

pub fn push_legacy_tag_tail_with_guid(v: &mut Vec<u8>) {
    v.push(1); // HasPropertyGuid
    push_guid(v, 1, 2, 3, 4);
    v.push(0); // UE5 1011 PropertyExtensions = NoExtension
}

pub fn push_guid(v: &mut Vec<u8>, a: u32, b: u32, c: u32, d: u32) {
    push_u32(v, a);
    push_u32(v, b);
    push_u32(v, c);
    push_u32(v, d);
}

// Empty FText: flags + history type -1 (None) + no culture-invariant string.
pub fn push_empty_ftext(v: &mut Vec<u8>) {
    push_u32(v, 0);
    v.push(0xFF);
    push_i32(v, 0);
}
