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
// for UE5.0 (1000) through UE5.8 (1018) alike. ue4v is always 522 and
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

/// Minimal package whose `CompressedChunks` count is `chunk_count`, so callers can
/// exercise the sign classification without hunting for the field's offset.
pub fn build_minimal_package_with_compressed_chunks(chunk_count: i32) -> Vec<u8> {
    build_minimal_package_header_with_chunks(1018, 5, 7, -8, true, chunk_count)
}

fn build_minimal_package_header(
    file_version_ue5: i32,
    major: u16,
    minor: u16,
    legacy_file_version: i32,
    filter_editor_only: bool,
) -> Vec<u8> {
    build_minimal_package_header_with_chunks(
        file_version_ue5,
        major,
        minor,
        legacy_file_version,
        filter_editor_only,
        0,
    )
}

fn build_minimal_package_header_with_chunks(
    file_version_ue5: i32,
    major: u16,
    minor: u16,
    legacy_file_version: i32,
    filter_editor_only: bool,
    compressed_chunks_count: i32,
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
    push_i32(&mut d, compressed_chunks_count);
    push_u32(&mut d, 0); // package_source
    push_i32(&mut d, 0); // additional_packages_to_cook count
    // num_texture_allocations skipped: legacy (-8) is not > -7.
    push_i32(&mut d, 0); // asset_registry_data_offset
    push_i64(&mut d, 0); // bulk_data_start_offset
    push_i32(&mut d, 0); // world_tile_info_data_offset (ue4 >= 224)
    push_i32(&mut d, 0); // chunk ids count (ue4 >= 392)
    push_i32(&mut d, 0); // preload_dependency_count (ue4 >= 507)
    push_i32(&mut d, 0); // preload_dependency_offset
    if fv >= ue5::NAMES_REFERENCED_FROM_EXPORT_DATA {
        push_i32(&mut d, 0); // names_referenced_from_export_data_count
    }
    if fv >= ue5::PAYLOAD_TOC {
        push_i64(&mut d, 0); // payload_toc_offset
    }
    if fv >= ue5::DATA_RESOURCES {
        push_i32(&mut d, 0); // data_resource_offset
    }
    d
}

/// One import row for [`PackageBuilder`], by name-table index.
pub struct ImportSpec {
    pub class_package: usize,
    pub class_name: usize,
    pub outer_index: i32,
    pub object_name: usize,
}

/// One export for [`PackageBuilder`]: its identity and the payload bytes that
/// become its serial window. `script_range` is the tagged-property range
/// relative to the payload, which is also how UE writes
/// `ScriptSerializationStart/EndOffset`: `FLinkerSave` records absolute `Tell()`
/// positions and `SavePackage2.cpp` subtracts `SerialOffset` before the export
/// table is written.
pub struct ExportSpec {
    pub class_index: i32,
    pub outer_index: i32,
    pub object_name: usize,
    pub payload: Vec<u8>,
    pub script_range: Option<(u64, u64)>,
}

/// Builds a complete package — summary, name table, import table, export table
/// and export payloads — for any supported `FileVersionUE5`, so a test can go
/// through `PackageView::parse` and exercise the header offsets, table readers
/// and export windows together instead of hand-assembling a `Package`.
///
/// The header fields are written zeroed by the summary builder and patched here,
/// so the summary layout lives in exactly one place. `FilterEditorOnly` is set,
/// the engine version is `major.minor`, and there are no custom versions.
pub struct PackageBuilder {
    pub file_version_ue5: i32,
    pub engine: (u16, u16),
    pub names: Vec<String>,
    pub imports: Vec<ImportSpec>,
    pub exports: Vec<ExportSpec>,
}

impl PackageBuilder {
    pub fn new(file_version_ue5: i32, engine: (u16, u16)) -> Self {
        Self {
            file_version_ue5,
            engine,
            names: Vec::new(),
            imports: Vec::new(),
            exports: Vec::new(),
        }
    }

    pub fn name(&mut self, name: &str) -> usize {
        if let Some(index) = self.names.iter().position(|existing| existing == name) {
            return index;
        }
        self.names.push(name.to_string());
        self.names.len() - 1
    }

    /// Adds a `/Script/<module>` package import and a class import under it,
    /// returning the class's `FPackageIndex` (negative).
    pub fn script_class(&mut self, module: &str, class: &str) -> i32 {
        let package_name = self.name("Package");
        let class_name = self.name("Class");
        let core = self.name("/Script/CoreUObject");
        let module_name = self.name(module);
        let class_object = self.name(class);
        self.imports.push(ImportSpec {
            class_package: core,
            class_name: package_name,
            outer_index: 0,
            object_name: module_name,
        });
        let package_index = -(self.imports.len() as i32);
        self.imports.push(ImportSpec {
            class_package: core,
            class_name,
            outer_index: package_index,
            object_name: class_object,
        });
        -(self.imports.len() as i32)
    }

    pub fn build(&self) -> Vec<u8> {
        use crate::version::{ue4, ue5};
        let fv = self.file_version_ue5;
        let ue4v = 522;
        let filter_editor_only = true;
        let mut d = build_minimal_package_with_version(fv, self.engine.0, self.engine.1);
        let header_len = d.len();

        // The header builder writes fixed-width zeros for every count/offset; find
        // each field by re-walking the prefix it wrote, which is the same walk the
        // parser does. `TestPkg` is the only variable-width field before them.
        let mut cursor = 4 + 4 + 4 + 4 + 4 + 4; // tag, legacy, ue3, ue4, ue5, licensee
        if fv >= ue5::PACKAGE_SAVED_HASH {
            cursor += 20 + 4; // saved hash, total header size
        }
        cursor += 4; // custom version count
        if fv < ue5::PACKAGE_SAVED_HASH {
            cursor += 4; // total header size
        }
        cursor += 4 + "TestPkg".len() + 1; // package name FString
        cursor += 4; // package flags
        let name_count_pos = cursor;
        let name_offset_pos = cursor + 4;
        cursor += 8;
        if fv >= ue5::ADD_SOFTOBJECTPATH_LIST {
            cursor += 8;
        }
        cursor += 8; // gatherable text data
        let export_count_pos = cursor;
        let export_offset_pos = cursor + 4;
        let import_count_pos = cursor + 8;
        let import_offset_pos = cursor + 12;

        // Name table: FString plus the two 16-bit hashes NAME_HASHES_SERIALIZED adds.
        let name_offset = d.len();
        for name in &self.names {
            push_fstring(&mut d, name);
            push_u32(&mut d, 0);
        }
        // Import table.
        let import_offset = d.len();
        let has_package_name =
            ue4v >= ue4::NON_OUTER_PACKAGE_IMPORT && (!filter_editor_only || self.engine >= (5, 8));
        for import in &self.imports {
            push_raw_name(&mut d, import.class_package as i32);
            push_raw_name(&mut d, import.class_name as i32);
            push_i32(&mut d, import.outer_index);
            push_raw_name(&mut d, import.object_name as i32);
            if has_package_name {
                push_raw_name(&mut d, 0);
            }
            if fv >= ue5::OPTIONAL_RESOURCES {
                push_i32(&mut d, 0); // bImportOptional
            }
        }
        // Export table, with payloads laid out right after it.
        let export_offset = d.len();
        let row_len = crate::object::ObjectExport::entry_bytes(ue4v, fv);
        let payload_base = export_offset + self.exports.len() * row_len as usize;
        let mut payload_cursor = payload_base as u64;
        for export in &self.exports {
            let serial_offset = payload_cursor;
            payload_cursor += export.payload.len() as u64;
            push_i32(&mut d, export.class_index);
            push_i32(&mut d, 0); // super
            push_i32(&mut d, 0); // template
            push_i32(&mut d, export.outer_index);
            push_raw_name(&mut d, export.object_name as i32);
            push_u32(&mut d, 0); // object flags
            push_i64(&mut d, export.payload.len() as i64);
            push_i64(&mut d, serial_offset as i64);
            push_i32(&mut d, 0); // forced export
            push_i32(&mut d, 0); // not for client
            push_i32(&mut d, 0); // not for server
            if fv < ue5::REMOVE_OBJECT_EXPORT_PACKAGE_GUID {
                push_guid(&mut d, 0, 0, 0, 0);
            }
            if fv >= ue5::TRACK_OBJECT_EXPORT_IS_INHERITED {
                push_i32(&mut d, 0);
            }
            push_u32(&mut d, 0); // package flags
            push_i32(&mut d, 0); // not always loaded for editor game
            push_i32(&mut d, 0); // is asset
            if fv >= ue5::OPTIONAL_RESOURCES {
                push_i32(&mut d, 0); // generate public hash
            }
            for _ in 0..5 {
                push_i32(&mut d, 0); // preload dependency fields
            }
            if fv >= ue5::SCRIPT_SERIALIZATION_OFFSET {
                let (start, end) = export.script_range.unwrap_or((0, 0));
                push_i64(&mut d, start as i64);
                push_i64(&mut d, end as i64);
            }
        }
        assert_eq!(
            d.len(),
            payload_base,
            "export row width drifted from the parser"
        );
        for export in &self.exports {
            d.extend_from_slice(&export.payload);
        }

        let put = |d: &mut Vec<u8>, pos: usize, value: i32| {
            d[pos..pos + 4].copy_from_slice(&value.to_le_bytes());
        };
        put(&mut d, name_count_pos, self.names.len() as i32);
        put(&mut d, name_offset_pos, name_offset as i32);
        put(&mut d, export_count_pos, self.exports.len() as i32);
        put(&mut d, export_offset_pos, export_offset as i32);
        put(&mut d, import_count_pos, self.imports.len() as i32);
        put(&mut d, import_offset_pos, import_offset as i32);
        let _ = header_len;
        d
    }
}

/// An import-table row with `class_package` left as name 0; `outer_index` is a
/// `FPackageIndex` (negative for another import).
pub fn test_import(
    class_name: i32,
    object_name: i32,
    outer_index: i32,
    class_package: i32,
) -> crate::object::ObjectImport {
    crate::object::ObjectImport {
        class_package: RawName {
            index: class_package,
            number: 0,
        },
        class_name: RawName {
            index: class_name,
            number: 0,
        },
        outer_index: PackageIndex(outer_index),
        object_name: RawName {
            index: object_name,
            number: 0,
        },
        package_name: None,
    }
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

// Legacy (pre-`PROPERTY_TAG_COMPLETE_TYPE_NAME`) tag tail. FPropertyTag stores
// `HasPropertyGuid` as uint8 (PropertyTag.h). The PropertyTagExtensions byte is
// appended only when `file_version_ue5 >= 1011`.
pub fn push_legacy_tag_tail(v: &mut Vec<u8>, file_version_ue5: i32) {
    v.push(0); // HasPropertyGuid = false (uint8)
    if file_version_ue5 >= crate::version::ue5::PROPERTY_TAG_EXTENSION_AND_OVERRIDABLE_SERIALIZATION
    {
        v.push(0); // PropertyTagExtensions = NoExtension
    }
}

pub fn push_legacy_tag_tail_with_guid(v: &mut Vec<u8>, file_version_ue5: i32) {
    v.push(1); // HasPropertyGuid = true (uint8)
    push_guid(v, 1, 2, 3, 4);
    if file_version_ue5 >= crate::version::ue5::PROPERTY_TAG_EXTENSION_AND_OVERRIDABLE_SERIALIZATION
    {
        v.push(0); // PropertyTagExtensions = NoExtension
    }
}

/// A legacy-layout `FEdGraphPinType` with the given category name and no
/// sub-category, object, container, or member reference.
pub fn push_minimal_pin_type(data: &mut Vec<u8>, category: i32, none: i32) {
    push_raw_name(data, category);
    push_raw_name(data, none);
    push_i32(data, 0);
    data.push(0);
    push_i32(data, 0);
    push_i32(data, 0);
    push_i32(data, 0);
    push_raw_name(data, none);
    push_guid(data, 0, 0, 0, 0);
    push_i32(data, 0);
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
