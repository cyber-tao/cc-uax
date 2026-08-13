#![allow(dead_code)]

use crate::object::{ObjectExport, PackageIndex};
use crate::reader::RawName;

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
pub fn push_u64(v: &mut Vec<u8>, x: u64) {
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

// Minimal versioned UE5 package header (legacy=-8, ue4=522,
// FilterEditorOnly set to skip editor-only fields, all tables empty).
pub fn build_minimal_package() -> Vec<u8> {
    build_minimal_package_with_version(1018, 5, 7)
}

/// Build a minimal package for a specific UE5 file version and engine version.
/// `file_version_ue5` controls which summary fields are included; for 1017
/// (UE5.6) the ImportTypeHierarchies fields are omitted.
// Builds a minimal but *valid* versioned package summary for any supported
// FileVersionUE5. Every version-gated field is written only when the parser reads
// it for that version (mirroring PackageFileSummary::parse), so the bytes line up
// for UE5.1 (1008) through UE5.8 (1018) alike. ue4v is always 522 and
// FilterEditorOnly is set, which keeps the editor-only/localization fields absent.
pub fn build_minimal_package_with_version(
    file_version_ue5: i32,
    major: u16,
    minor: u16,
) -> Vec<u8> {
    build_minimal_package_header(file_version_ue5, major, minor, -8, true)
}

/// Unfiltered editor summary (localization id + PersistentGuid) for a given
/// FileVersionUE5. UE5.6+ editor packages write LegacyFileVersion -9.
pub fn build_minimal_editor_package_with_version(
    file_version_ue5: i32,
    major: u16,
    minor: u16,
) -> Vec<u8> {
    let legacy = if file_version_ue5 >= crate::version::ue5::PACKAGE_SAVED_HASH {
        -9
    } else {
        -8
    };
    build_minimal_package_header(file_version_ue5, major, minor, legacy, false)
}

fn build_minimal_package_header(
    file_version_ue5: i32,
    major: u16,
    minor: u16,
    legacy_file_version: i32,
    filter_editor_only: bool,
) -> Vec<u8> {
    use crate::version::ue5;
    let fv = file_version_ue5;
    let mut d = Vec::new();
    push_u32(&mut d, 0x9E2A_83C1); // PACKAGE_FILE_TAG
    push_i32(&mut d, legacy_file_version);
    push_i32(&mut d, 0); // legacy ue3 version (legacy != -4)
    push_i32(&mut d, 522); // file_version_ue4
    push_i32(&mut d, fv); // file_version_ue5 (legacy <= -8)
    push_i32(&mut d, 0); // file_version_licensee
    if fv >= ue5::PACKAGE_SAVED_HASH {
        d.extend_from_slice(&[0u8; 20]); // saved_hash
        push_i32(&mut d, 0); // total_header_size (hash position)
    }
    push_i32(&mut d, 0); // custom version count
    if fv < ue5::PACKAGE_SAVED_HASH {
        push_i32(&mut d, 0); // total_header_size (legacy position)
    }
    push_fstring(&mut d, "TestPkg"); // package_name
    push_u32(&mut d, if filter_editor_only { 0x8000_0000 } else { 0 });
    push_i32(&mut d, 0); // name_count
    push_i32(&mut d, 0); // name_offset
    if fv >= ue5::ADD_SOFTOBJECTPATH_LIST {
        push_i32(&mut d, 0); // soft_object_paths_count
        push_i32(&mut d, 0); // soft_object_paths_offset
    }
    if !filter_editor_only {
        push_fstring(&mut d, ""); // localization_id
    }
    push_i32(&mut d, 0); // gatherable_text_data_count (ue4 >= 459)
    push_i32(&mut d, 0); // gatherable_text_data_offset
    push_i32(&mut d, 0); // export_count
    push_i32(&mut d, 0); // export_offset
    push_i32(&mut d, 0); // import_count
    push_i32(&mut d, 0); // import_offset
    if fv >= ue5::VERSE_CELLS {
        push_i32(&mut d, 0); // cell_export_count
        push_i32(&mut d, 0); // cell_export_offset
        push_i32(&mut d, 0); // cell_import_count
        push_i32(&mut d, 0); // cell_import_offset
    }
    if fv >= ue5::METADATA_SERIALIZATION_OFFSET {
        push_i32(&mut d, 0); // metadata_offset
    }
    push_i32(&mut d, 0); // depends_offset
    push_i32(&mut d, 0); // soft_package_references_count (ue4 >= 384)
    push_i32(&mut d, 0); // soft_package_references_offset
    push_i32(&mut d, 0); // searchable_names_offset (ue4 >= 510)
    push_i32(&mut d, 0); // thumbnail_table_offset
    if fv >= ue5::IMPORT_TYPE_HIERARCHIES {
        push_i32(&mut d, 0); // import_type_hierarchies_count
        push_i32(&mut d, 0); // import_type_hierarchies_offset
    }
    if fv < ue5::PACKAGE_SAVED_HASH {
        push_guid(&mut d, 0, 0, 0, 0); // legacy_guid
    }
    if !filter_editor_only {
        push_guid(&mut d, 0, 0, 0, 0); // PersistentGuid
    }
    push_i32(&mut d, 0); // generation_count
    push_u16(&mut d, major); // engine_version.major (ue4 >= 336)
    push_u16(&mut d, minor); // .minor
    push_u16(&mut d, 0); // .patch
    push_u32(&mut d, 0); // .changelist
    push_fstring(&mut d, ""); // .branch
    push_u16(&mut d, major); // compatible_engine_version (ue4 >= 444)
    push_u16(&mut d, minor);
    push_u16(&mut d, 0);
    push_u32(&mut d, 0);
    push_fstring(&mut d, "");
    push_u32(&mut d, 0); // compression_flags
    push_i32(&mut d, 0); // compressed_chunks_count
    push_u32(&mut d, 0); // package_source
    push_i32(&mut d, 0); // additional_packages_to_cook count
    // num_texture_allocations skipped: legacy (-8) is not > -7.
    push_i32(&mut d, 0); // asset_registry_data_offset
    push_i64(&mut d, 0); // bulk_data_start_offset
    push_i32(&mut d, 0); // world_tile_info_data_offset (ue4 >= 224)
    push_i32(&mut d, 0); // chunk ids count (ue4 >= 392)
    push_i32(&mut d, 0); // preload_dependency_count (ue4 >= 507)
    push_i32(&mut d, 0); // preload_dependency_offset
    push_i32(&mut d, 0); // names_referenced_from_export_data_count (ue5 >= 1001)
    push_i64(&mut d, 0); // payload_toc_offset (ue5 >= 1002)
    if fv >= ue5::DATA_RESOURCES {
        push_i32(&mut d, 0); // data_resource_offset
    }
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

// Legacy (pre-`PROPERTY_TAG_COMPLETE_TYPE_NAME`) tag tail. UE serializes the
// `HasPropertyGuid` bool as a 4-byte legacy UBOOL, and only appends the
// PropertyTagExtensions byte when `file_version_ue5 >= 1011`.
pub fn push_legacy_tag_tail(v: &mut Vec<u8>, file_version_ue5: i32) {
    push_u32(v, 0); // HasPropertyGuid = false (4-byte UBOOL)
    if file_version_ue5 >= crate::version::ue5::PROPERTY_TAG_EXTENSION_AND_OVERRIDABLE_SERIALIZATION
    {
        v.push(0); // PropertyTagExtensions = NoExtension
    }
}

pub fn push_legacy_tag_tail_with_guid(v: &mut Vec<u8>, file_version_ue5: i32) {
    push_u32(v, 1); // HasPropertyGuid = true (4-byte UBOOL)
    push_guid(v, 1, 2, 3, 4);
    if file_version_ue5 >= crate::version::ue5::PROPERTY_TAG_EXTENSION_AND_OVERRIDABLE_SERIALIZATION
    {
        v.push(0); // PropertyTagExtensions = NoExtension
    }
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
