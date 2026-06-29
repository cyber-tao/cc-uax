use cc_uax::name::NameMap;
use cc_uax::object::{ObjectExport, PackageIndex};
use cc_uax::package::{collect_package_references, package_path_from_relative};
use cc_uax::pin::PinSerCtx;
use cc_uax::property::{ParseCtx, TypeName, parse_properties};
use cc_uax::reader::{RawName, Reader};
use cc_uax::{OutputSections, Package};

#[test]
fn fstring_ansi() {
    let mut data = 6i32.to_le_bytes().to_vec();
    data.extend_from_slice(b"Hello\0");
    let mut r = Reader::new(&data);
    assert_eq!(r.read_fstring().unwrap(), "Hello");
}

#[test]
fn fstring_empty() {
    let data = 0i32.to_le_bytes();
    let mut r = Reader::new(&data);
    assert_eq!(r.read_fstring().unwrap(), "");
}

#[test]
fn fstring_utf16() {
    let mut data = (-3i32).to_le_bytes().to_vec();
    data.extend_from_slice(&[0x48, 0x00, 0x69, 0x00, 0x00, 0x00]);
    let mut r = Reader::new(&data);
    assert_eq!(r.read_fstring().unwrap(), "Hi");
}

#[test]
fn read_integers_le() {
    let data = [0x01, 0x00, 0x00, 0x00, 0xff, 0xff, 0xff, 0xff];
    let mut r = Reader::new(&data);
    assert_eq!(r.read_i32().unwrap(), 1);
    assert_eq!(r.read_i32().unwrap(), -1);
}

#[test]
fn read_raw_name() {
    let data = [0x05, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00];
    let mut r = Reader::new(&data);
    let n = r.read_raw_name().unwrap();
    assert_eq!(n.index, 5);
    assert_eq!(n.number, 2);
}

#[test]
fn read_io_hash_rejects_short_input() {
    let data = [0u8; 19];
    let mut r = Reader::new(&data);
    let err = r.read_io_hash().err().unwrap().to_string();
    assert!(err.contains("read 20 bytes out of range"));
}

#[test]
fn package_index_semantics() {
    assert!(PackageIndex(0).is_null());
    assert_eq!(PackageIndex(1).export_index(), Some(0));
    assert_eq!(PackageIndex(5).export_index(), Some(4));
    assert!(PackageIndex(5).import_index().is_none());
    assert_eq!(PackageIndex(-1).import_index(), Some(0));
    assert_eq!(PackageIndex(-3).import_index(), Some(2));
}

#[test]
fn name_resolution_with_number() {
    let names = NameMap {
        names: vec!["Foo".to_string(), "Bar".to_string()],
    };
    assert_eq!(names.resolve(0, 0), "Foo");
    assert_eq!(names.resolve(1, 3), "Bar_2");
    assert_eq!(names.resolve(99, 0), "<invalid_name#99>");
}

#[test]
fn typename_display_nested() {
    let t = TypeName {
        name: "MapProperty".to_string(),
        params: vec![
            TypeName {
                name: "NameProperty".to_string(),
                params: vec![],
            },
            TypeName {
                name: "IntProperty".to_string(),
                params: vec![],
            },
        ],
    };
    assert_eq!(t.display(), "MapProperty(NameProperty,IntProperty)");

    let simple = TypeName {
        name: "IntProperty".to_string(),
        params: vec![],
    };
    assert_eq!(simple.display(), "IntProperty");
}

#[test]
fn package_references_partition() {
    let imports = vec![
        ("Package", "/Game/Foo/BP_Bar"),
        ("Package", "/Script/Engine"),
        ("Class", "Actor"),
        ("Package", "/Game/Foo/BP_Bar"),
        ("Package", "/Script/CoreUObject"),
        ("Package", "/Game/Audio/SC_Test"),
        ("Package", ""),
    ];
    let (assets, scripts) = collect_package_references(imports);
    assert_eq!(assets, vec!["/Game/Audio/SC_Test", "/Game/Foo/BP_Bar"]);
    assert_eq!(scripts, vec!["/Script/CoreUObject", "/Script/Engine"]);
}

#[test]
fn package_path_mapping() {
    assert_eq!(
        package_path_from_relative("Foo/BP_Bar.uasset", "/Game"),
        "/Game/Foo/BP_Bar"
    );
    assert_eq!(
        package_path_from_relative("Sub\\Dir\\A.uasset", "/Game"),
        "/Game/Sub/Dir/A"
    );
    assert_eq!(
        package_path_from_relative("Maps/Main.umap", "Game/"),
        "/Game/Maps/Main"
    );
    assert_eq!(
        package_path_from_relative("/Widgets/W_HUD.uasset", "/MyPlugin"),
        "/MyPlugin/Widgets/W_HUD"
    );
    assert_eq!(
        package_path_from_relative("Foo/BP_Upper.UASSET", "/Game"),
        "/Game/Foo/BP_Upper"
    );
    assert_eq!(
        package_path_from_relative("Maps/Main.UMAP", "/Game"),
        "/Game/Maps/Main"
    );
}

#[test]
fn output_sections_parse_presets_and_aliases() {
    let logic = OutputSections::parse("logic,refs").unwrap();
    assert!(logic.summary);
    assert!(logic.exports);
    assert!(logic.pins);
    assert!(logic.references);
    assert!(!logic.properties);

    let debug = OutputSections::parse("debug").unwrap();
    assert!(debug.summary);
    assert!(debug.imports);
    assert!(debug.exports);
    assert!(debug.properties);
    assert!(debug.layout);
    assert!(!debug.pins);

    assert!(OutputSections::parse(" ").is_err());
    assert!(OutputSections::parse("summary,unknown").is_err());
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
fn put_i32(v: &mut [u8], offset: usize, x: i32) {
    v[offset..offset + 4].copy_from_slice(&x.to_le_bytes());
}
fn push_raw_name(v: &mut Vec<u8>, index: i32) {
    push_i32(v, index);
    push_i32(v, 0);
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

// Minimal versioned UE5 package header (legacy=-8, ue4=522, ue5=1018,
// FilterEditorOnly set to skip editor-only fields, all tables empty).
fn build_minimal_package() -> Vec<u8> {
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

fn test_export(
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
        forced_export: false,
        not_for_client: false,
        not_for_server: false,
        is_inherited_instance: false,
        package_flags: 0,
        not_always_loaded_for_editor_game: false,
        is_asset: false,
        generate_public_hash: false,
        first_export_dependency: -1,
        serialization_before_serialization_deps: -1,
        create_before_serialization_deps: -1,
        serialization_before_create_deps: -1,
        create_before_create_deps: -1,
        script_serialization_start_offset: script_start,
        script_serialization_end_offset: script_end,
    }
}

#[test]
fn package_rejects_pre_ue5_version() {
    let mut d = Vec::new();
    push_u32(&mut d, 0x9E2A_83C1); // PACKAGE_FILE_TAG
    push_i32(&mut d, -8); // legacy_file_version
    push_i32(&mut d, 0); // legacy ue3 version
    push_i32(&mut d, 522); // file_version_ue4
    push_i32(&mut d, 999); // below UE5 initial version
    push_i32(&mut d, 0); // file_version_licensee

    let err = Package::parse(&d).err().unwrap().to_string();
    assert!(err.contains("FileVersionUE5=999"));
}

#[test]
fn name_map_rejects_negative_count() {
    let data = [];
    let mut r = Reader::new(&data);

    let err = NameMap::parse(&mut r, 0, -1, 522)
        .err()
        .unwrap()
        .to_string();
    assert!(err.contains("name count out of range"));
}

#[test]
fn package_parse_minimal_header() {
    let data = build_minimal_package();
    let pkg = Package::parse(&data).expect("minimal package should parse");

    assert_eq!(pkg.summary.file_version_ue4, 522);
    assert_eq!(pkg.summary.file_version_ue5, 1018);
    assert_eq!(pkg.summary.package_name, "TestPkg");
    assert_eq!(pkg.summary.export_count, 0);
    assert!(pkg.imports.is_empty());
    assert!(pkg.exports.is_empty());

    let json = pkg.to_json(&data, &OutputSections::full());
    assert_eq!(json["summary"]["package_name"], "TestPkg");
    assert_eq!(json["summary"]["file_version_ue5"], 1018);
    assert!(json["imports"].as_array().unwrap().is_empty());
    assert!(json["exports"].as_array().unwrap().is_empty());
}

#[test]
fn soft_object_path_table_error_is_reported() {
    let mut data = build_minimal_package();
    put_i32(&mut data, 76, 1); // soft_object_paths_count
    put_i32(&mut data, 80, 999_999); // soft_object_paths_offset

    let pkg = Package::parse(&data).unwrap();
    let err = pkg.soft_object_path_error.as_deref().unwrap();
    assert!(err.contains("soft object path table seek failed"));

    let json = pkg.to_json(&data, &OutputSections::full());
    assert!(
        json["summary"]["soft_object_paths_error"]
            .as_str()
            .unwrap()
            .contains("soft object path table seek failed")
    );
}

#[test]
fn nested_struct_respects_declared_value_end() {
    let names = NameMap {
        names: vec![
            "Outer".to_string(),
            "StructProperty".to_string(),
            "MyStruct".to_string(),
            "Inner".to_string(),
            "IntProperty".to_string(),
            "After".to_string(),
            "None".to_string(),
        ],
    };

    let mut nested = Vec::new();
    push_raw_name(&mut nested, 3); // Inner
    push_raw_name(&mut nested, 4); // IntProperty
    push_i32(&mut nested, 0); // type name inner param count
    push_i32(&mut nested, 4); // size
    nested.push(0); // flags
    push_i32(&mut nested, 123);

    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // Outer
    push_raw_name(&mut d, 1); // StructProperty
    push_i32(&mut d, 1); // one type parameter
    push_raw_name(&mut d, 2); // MyStruct
    push_i32(&mut d, 0);
    push_i32(&mut d, nested.len() as i32);
    d.push(0); // flags
    d.extend_from_slice(&nested);

    push_raw_name(&mut d, 5); // After
    push_raw_name(&mut d, 4); // IntProperty
    push_i32(&mut d, 0);
    push_i32(&mut d, 4);
    d.push(0);
    push_i32(&mut d, 456);

    push_raw_name(&mut d, 6); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "Outer");
    let nested_props = entries[0].value["properties"].as_array().unwrap();
    assert_eq!(nested_props.len(), 1);
    assert_eq!(nested_props[0]["name"], "Inner");
    assert_eq!(entries[1].name, "After");
    assert_eq!(entries[1].value.as_i64(), Some(456));
}

#[test]
fn truncated_property_array_index_stops_parse() {
    let names = NameMap {
        names: vec![
            "Broken".to_string(),
            "IntProperty".to_string(),
            "None".to_string(),
        ],
    };
    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // Broken
    push_raw_name(&mut d, 1); // IntProperty
    push_i32(&mut d, 0); // type name inner param count
    push_i32(&mut d, 0); // declared value size
    d.push(0x01); // flags say array_index follows, but it is truncated

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);
    assert!(entries.is_empty());
}

#[test]
fn excessive_array_count_falls_back_to_hex() {
    let names = NameMap {
        names: vec![
            "Nums".to_string(),
            "ArrayProperty".to_string(),
            "IntProperty".to_string(),
            "None".to_string(),
        ],
    };
    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // Nums
    push_raw_name(&mut d, 1); // ArrayProperty
    push_i32(&mut d, 1); // one type parameter
    push_raw_name(&mut d, 2); // IntProperty
    push_i32(&mut d, 0);
    push_i32(&mut d, 4); // value is only the count
    d.push(0);
    push_i32(&mut d, 1_000_001);
    push_raw_name(&mut d, 3); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].value.get("@unparsed").and_then(|v| v.as_str()),
        Some("41420f00")
    );
}

#[test]
fn native_struct_array_falls_back_to_hex() {
    let names = NameMap {
        names: vec![
            "NativeArray".to_string(),
            "ArrayProperty".to_string(),
            "StructProperty".to_string(),
            "UnknownNative".to_string(),
            "None".to_string(),
        ],
    };

    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // NativeArray
    push_raw_name(&mut d, 1); // ArrayProperty
    push_i32(&mut d, 1);
    push_raw_name(&mut d, 2); // StructProperty
    push_i32(&mut d, 1);
    push_raw_name(&mut d, 3); // UnknownNative
    push_i32(&mut d, 0);
    push_i32(&mut d, 8); // count + one opaque 4-byte element
    d.push(0x08); // binary/native value
    push_i32(&mut d, 1); // array count
    d.extend_from_slice(&[0xaa, 0xbb, 0xcc, 0xdd]);
    push_raw_name(&mut d, 4); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let unparsed = entries[0].value.get("@unparsed").and_then(|v| v.as_str());
    assert_eq!(unparsed, Some("01000000aabbccdd"));
}

#[test]
fn invalid_script_window_is_reported_in_layout() {
    let base = Package::parse(&build_minimal_package()).unwrap();
    let pkg = Package {
        summary: base.summary,
        names: NameMap {
            names: vec!["Obj".to_string()],
        },
        imports: Vec::new(),
        exports: vec![test_export(0, 4, 0, 8)],
        soft_object_paths: Vec::new(),
        soft_object_path_error: None,
    };
    let mut sections = OutputSections::none();
    sections.exports = true;
    sections.layout = true;
    sections.properties = true;

    let json = pkg.to_json(&[0; 4], &sections);
    let err = json["exports"][0]["serial_window_error"].as_str().unwrap();
    assert!(err.contains("outside serial size"));
    assert!(json["exports"][0].get("properties").is_none());
}

#[test]
fn zero_script_window_uses_serial_range() {
    let base = Package::parse(&build_minimal_package()).unwrap();
    let mut data = Vec::new();
    data.push(0); // property tag extension control byte
    push_raw_name(&mut data, 1); // Value
    push_raw_name(&mut data, 2); // IntProperty
    push_i32(&mut data, 0);
    push_i32(&mut data, 4);
    data.push(0);
    push_i32(&mut data, 42);
    push_raw_name(&mut data, 3); // None

    let pkg = Package {
        summary: base.summary,
        names: NameMap {
            names: vec![
                "Obj".to_string(),
                "Value".to_string(),
                "IntProperty".to_string(),
                "None".to_string(),
            ],
        },
        imports: Vec::new(),
        exports: vec![test_export(0, data.len() as i64, 0, 0)],
        soft_object_paths: Vec::new(),
        soft_object_path_error: None,
    };
    let mut sections = OutputSections::none();
    sections.exports = true;
    sections.layout = true;
    sections.properties = true;

    let json = pkg.to_json(&data, &sections);
    let props = json["exports"][0]["properties"].as_array().unwrap();
    assert_eq!(props.len(), 1);
    assert_eq!(props[0]["name"].as_str(), Some("Value"));
    assert_eq!(props[0]["value"].as_i64(), Some(42));
}

#[test]
fn text_property_unknown_history_falls_back_to_hex() {
    let names = NameMap {
        names: vec![
            "MyText".to_string(),
            "TextProperty".to_string(),
            "None".to_string(),
        ],
    };

    let mut d = Vec::new();
    push_i32(&mut d, 0); // property name FName index ("MyText")
    push_i32(&mut d, 0); // .number
    push_i32(&mut d, 1); // type name FName index ("TextProperty")
    push_i32(&mut d, 0); // .number
    push_i32(&mut d, 0); // type name inner param count
    push_i32(&mut d, 5); // size
    d.push(0); // flags
    push_u32(&mut d, 0); // FText flags
    d.push(4u8); // FText history_type = 4 (unhandled)
    push_i32(&mut d, 2); // terminator FName index ("None")
    push_i32(&mut d, 0); // .number

    let end = d.len() as u64;
    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, end);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "MyText");
    assert_eq!(entries[0].type_str, "TextProperty");
    let unparsed = entries[0].value.get("@unparsed").and_then(|v| v.as_str());
    assert_eq!(unparsed, Some("0000000004"));
}

fn push_f32(v: &mut Vec<u8>, x: f32) {
    v.extend_from_slice(&x.to_le_bytes());
}
fn push_f64(v: &mut Vec<u8>, x: f64) {
    v.extend_from_slice(&x.to_le_bytes());
}

// Wrap pre-built `value` bytes as a single StructProperty named index 0 with a
// struct type name at `struct_idx`, then a trailing None (index `none_idx`).
fn build_struct_property(struct_idx: i32, none_idx: i32, value: &[u8]) -> Vec<u8> {
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

#[test]
fn native_struct_box_decodes() {
    let names = NameMap {
        names: vec![
            "MyBox".to_string(),
            "StructProperty".to_string(),
            "Box".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    for x in [1.0, 2.0, 3.0, 4.0, 5.0, 6.0] {
        push_f64(&mut value, x);
    }
    value.push(1); // is_valid
    assert_eq!(value.len(), 49);
    let d = build_struct_property(2, 3, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].value["is_valid"].as_bool(), Some(true));
    assert_eq!(entries[0].value["min"]["x"].as_f64(), Some(1.0));
    assert_eq!(entries[0].value["max"]["z"].as_f64(), Some(6.0));
}

#[test]
fn native_struct_box2f_decodes() {
    let names = NameMap {
        names: vec![
            "MyBox".to_string(),
            "StructProperty".to_string(),
            "Box2f".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    for x in [1.0f32, 2.0, 3.0, 4.0] {
        push_f32(&mut value, x);
    }
    value.push(1); // bIsValid (single uint8)
    assert_eq!(value.len(), 17);
    let d = build_struct_property(2, 3, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].value["min"]["x"].as_f64(), Some(1.0));
    assert_eq!(entries[0].value["max"]["y"].as_f64(), Some(4.0));
    assert_eq!(entries[0].value["is_valid"].as_bool(), Some(true));
}

#[test]
fn native_struct_gameplay_tag_container_decodes() {
    let names = NameMap {
        names: vec![
            "Tags".to_string(),
            "StructProperty".to_string(),
            "GameplayTagContainer".to_string(),
            "Ability.Attack".to_string(),
            "Ability.Dash".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    push_i32(&mut value, 2); // tag count
    push_raw_name(&mut value, 3); // Ability.Attack
    push_raw_name(&mut value, 4); // Ability.Dash
    assert_eq!(value.len(), 4 + 2 * 8);
    let d = build_struct_property(2, 5, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let tags = entries[0].value["tags"].as_array().unwrap();
    assert_eq!(tags.len(), 2);
    assert_eq!(tags[0].as_str(), Some("Ability.Attack"));
    assert_eq!(tags[1].as_str(), Some("Ability.Dash"));
}

#[test]
fn native_struct_vector4f_decodes() {
    let names = NameMap {
        names: vec![
            "V".to_string(),
            "StructProperty".to_string(),
            "Vector4f".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    for x in [1.0f32, 2.0, 3.0, 4.0] {
        push_f32(&mut value, x);
    }
    assert_eq!(value.len(), 16);
    let d = build_struct_property(2, 3, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].value["x"].as_f64(), Some(1.0));
    assert_eq!(entries[0].value["w"].as_f64(), Some(4.0));
}

#[test]
fn native_struct_rich_curve_key_array_keeps_stride() {
    let names = NameMap {
        names: vec![
            "Keys".to_string(),
            "ArrayProperty".to_string(),
            "StructProperty".to_string(),
            "RichCurveKey".to_string(),
            "None".to_string(),
        ],
    };
    fn push_key(v: &mut Vec<u8>, interp: u8, time: f32, value: f32) {
        v.push(interp); // interp mode
        v.push(0); // tangent mode
        v.push(0); // tangent weight mode
        push_f32(v, time);
        push_f32(v, value);
        push_f32(v, 0.0); // arrive tangent
        push_f32(v, 0.0); // arrive tangent weight
        push_f32(v, 0.0); // leave tangent
        push_f32(v, 0.0); // leave tangent weight
    }
    let mut value = Vec::new();
    push_i32(&mut value, 2); // array count
    push_key(&mut value, 2, 0.0, 10.0);
    push_key(&mut value, 3, 1.0, 20.0);
    assert_eq!(value.len(), 4 + 2 * 27);

    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // Keys
    push_raw_name(&mut d, 1); // ArrayProperty
    push_i32(&mut d, 1); // one param
    push_raw_name(&mut d, 2); // StructProperty
    push_i32(&mut d, 1); // one param
    push_raw_name(&mut d, 3); // RichCurveKey
    push_i32(&mut d, 0);
    push_i32(&mut d, value.len() as i32);
    d.push(0x08);
    d.extend_from_slice(&value);
    push_raw_name(&mut d, 4); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let arr = entries[0].value.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["interp_mode"].as_u64(), Some(2));
    assert_eq!(arr[0]["value"].as_f64(), Some(10.0));
    assert_eq!(arr[1]["interp_mode"].as_u64(), Some(3));
    assert_eq!(arr[1]["value"].as_f64(), Some(20.0));
    assert_eq!(arr[1]["time"].as_f64(), Some(1.0));
}

#[test]
fn material_scalar_input_resolves_expression() {
    let names = NameMap {
        names: vec![
            "Input".to_string(),
            "StructProperty".to_string(),
            "ScalarMaterialInput".to_string(),
            "R".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    push_i32(&mut value, -5); // expression object index
    push_i32(&mut value, 1); // output index
    push_raw_name(&mut value, 3); // input name "R"
    for m in [1, 1, 0, 0, 0] {
        push_i32(&mut value, m); // mask, maskR..maskA
    }
    push_i32(&mut value, 1); // use constant (bool32)
    push_f32(&mut value, 0.5); // constant
    assert_eq!(value.len(), 44);
    let d = build_struct_property(2, 4, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|idx: i32| serde_json::json!({ "index": idx }),
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let v = &entries[0].value;
    assert_eq!(v["expression"]["index"].as_i64(), Some(-5));
    assert_eq!(v["input_name"].as_str(), Some("R"));
    assert_eq!(v["output_index"].as_i64(), Some(1));
    assert_eq!(v["use_constant"].as_bool(), Some(true));
    assert_eq!(v["constant"].as_f64(), Some(0.5));
    assert_eq!(v["mask"].as_array().unwrap().len(), 5);
}

#[test]
fn native_struct_per_platform_float_decodes() {
    let names = NameMap {
        names: vec![
            "Scale".to_string(),
            "StructProperty".to_string(),
            "PerPlatformFloat".to_string(),
            "Mobile".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    push_i32(&mut value, 0); // bCooked = false
    push_f32(&mut value, 1.0); // default
    push_i32(&mut value, 1); // map count
    push_raw_name(&mut value, 3); // "Mobile"
    push_f32(&mut value, 0.5); // override value
    let d = build_struct_property(2, 4, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let v = &entries[0].value;
    assert_eq!(v["default"].as_f64(), Some(1.0));
    let pp = v["per_platform"].as_array().unwrap();
    assert_eq!(pp.len(), 1);
    assert_eq!(pp[0]["platform"].as_str(), Some("Mobile"));
    assert_eq!(pp[0]["value"].as_f64(), Some(0.5));
}

#[test]
fn native_struct_movie_scene_frame_range_decodes() {
    let names = NameMap {
        names: vec![
            "Range".to_string(),
            "StructProperty".to_string(),
            "MovieSceneFrameRange".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    value.push(1); // lower bound type (inclusive)
    push_i32(&mut value, 10);
    value.push(2); // upper bound type (exclusive)
    push_i32(&mut value, 100);
    assert_eq!(value.len(), 10);
    let d = build_struct_property(2, 3, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let v = &entries[0].value;
    assert_eq!(v["lower_bound"].as_i64(), Some(10));
    assert_eq!(v["upper_bound"].as_i64(), Some(100));
    assert_eq!(v["lower_bound_type"].as_u64(), Some(1));
    assert_eq!(v["upper_bound_type"].as_u64(), Some(2));
}

#[test]
fn native_struct_movie_scene_float_channel_decodes() {
    let names = NameMap {
        names: vec![
            "Channel".to_string(),
            "StructProperty".to_string(),
            "MovieSceneFloatChannel".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    value.push(4); // pre-infinity extrap
    value.push(4); // post-infinity extrap
    push_i32(&mut value, 4); // times element size
    push_i32(&mut value, 1); // times count
    push_i32(&mut value, 7); // frame number
    push_i32(&mut value, 28); // values element size
    push_i32(&mut value, 1); // values count
    // one 28-byte FMovieSceneFloatValue
    push_f32(&mut value, 1.5); // value (offset 0)
    push_f32(&mut value, 0.0); // arrive tangent
    push_f32(&mut value, 0.0); // leave tangent
    push_f32(&mut value, 0.0); // arrive tangent weight
    push_f32(&mut value, 0.0); // leave tangent weight
    value.push(0); // tangent weight mode (offset 20)
    value.extend_from_slice(&[0, 0, 0]); // tangent padding
    value.push(2); // interp mode (offset 24)
    value.push(1); // tangent mode (offset 25)
    value.push(0); // padding byte
    value.push(0); // unserialized padding
    push_f32(&mut value, 9.0); // default value
    push_i32(&mut value, 0); // has default value (false)
    push_i32(&mut value, 30); // tick numerator
    push_i32(&mut value, 1); // tick denominator
    push_i32(&mut value, 0); // show curve (false)
    assert_eq!(value.len(), 70);
    let d = build_struct_property(2, 3, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let v = &entries[0].value;
    assert_eq!(v["times"].as_array().unwrap()[0].as_i64(), Some(7));
    let vals = v["values"].as_array().unwrap();
    assert_eq!(vals.len(), 1);
    assert_eq!(vals[0]["value"].as_f64(), Some(1.5));
    assert_eq!(vals[0]["interp_mode"].as_u64(), Some(2));
    assert_eq!(vals[0]["tangent_mode"].as_u64(), Some(1));
    assert_eq!(v["default_value"].as_f64(), Some(9.0));
    assert_eq!(v["tick_resolution"]["numerator"].as_i64(), Some(30));
    assert_eq!(v["show_curve"].as_bool(), Some(false));
}

#[test]
fn text_ordered_format_decodes() {
    let names = NameMap {
        names: vec![
            "Label".to_string(),
            "TextProperty".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    push_u32(&mut value, 0); // outer FText flags
    value.push(2u8); // OrderedFormat
    push_u32(&mut value, 0); // nested format text flags
    value.push(0u8); // nested history = Base
    push_fstring(&mut value, ""); // namespace
    push_fstring(&mut value, "KEY"); // key
    push_fstring(&mut value, "{0} apples"); // source
    push_i32(&mut value, 1); // argument count
    value.push(0u8); // arg type 0 = Int
    push_i64(&mut value, 42);

    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // Label
    push_raw_name(&mut d, 1); // TextProperty
    push_i32(&mut d, 0); // type name inner param count
    push_i32(&mut d, value.len() as i32);
    d.push(0); // flags
    d.extend_from_slice(&value);
    push_raw_name(&mut d, 2); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let v = &entries[0].value;
    assert_eq!(v["history"].as_str(), Some("OrderedFormat"));
    assert_eq!(v["format"]["text"].as_str(), Some("{0} apples"));
    let args = v["arguments"].as_array().unwrap();
    assert_eq!(args.len(), 1);
    assert_eq!(args[0].as_i64(), Some(42));
}

#[test]
fn text_string_table_entry_decodes() {
    let names = NameMap {
        names: vec![
            "Label".to_string(),
            "TextProperty".to_string(),
            "MyTable".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    push_u32(&mut value, 0); // flags
    value.push(11u8); // StringTableEntry
    push_raw_name(&mut value, 2); // table id "MyTable"
    push_fstring(&mut value, "ENTRY_KEY");

    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // Label
    push_raw_name(&mut d, 1); // TextProperty
    push_i32(&mut d, 0);
    push_i32(&mut d, value.len() as i32);
    d.push(0);
    d.extend_from_slice(&value);
    push_raw_name(&mut d, 3); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let v = &entries[0].value;
    assert_eq!(v["history"].as_str(), Some("StringTableEntry"));
    assert_eq!(v["table_id"].as_str(), Some("MyTable"));
    assert_eq!(v["key"].as_str(), Some("ENTRY_KEY"));
}

#[test]
fn multicast_inline_delegate_decodes() {
    let names = NameMap {
        names: vec![
            "OnFire".to_string(),
            "MulticastInlineDelegateProperty".to_string(),
            "HandleFire".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    push_i32(&mut value, 1); // invocation count
    push_i32(&mut value, -3); // object index
    push_raw_name(&mut value, 2); // function name
    assert_eq!(value.len(), 16);

    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // OnFire
    push_raw_name(&mut d, 1); // MulticastInlineDelegateProperty
    push_i32(&mut d, 0);
    push_i32(&mut d, value.len() as i32);
    d.push(0);
    d.extend_from_slice(&value);
    push_raw_name(&mut d, 3); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|idx: i32| serde_json::json!({ "index": idx }),
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let arr = entries[0].value.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["function"].as_str(), Some("HandleFire"));
    assert_eq!(arr[0]["object"]["index"].as_i64(), Some(-3));
}

#[test]
fn native_struct_instanced_struct_decodes() {
    let names = NameMap {
        names: vec![
            "Data".to_string(),
            "StructProperty".to_string(),
            "InstancedStruct".to_string(),
            "Inner".to_string(),
            "IntProperty".to_string(),
            "None".to_string(),
        ],
    };
    // Inner struct tagged properties: one IntProperty "Inner" = 99, then None.
    let mut inner = Vec::new();
    push_raw_name(&mut inner, 3); // Inner
    push_raw_name(&mut inner, 4); // IntProperty
    push_i32(&mut inner, 0); // type name inner param count
    push_i32(&mut inner, 4); // size
    inner.push(0); // flags
    push_i32(&mut inner, 99);
    push_raw_name(&mut inner, 5); // None

    let mut value = Vec::new();
    push_i32(&mut value, -7); // script struct object index
    push_i32(&mut value, inner.len() as i32); // serial size
    value.extend_from_slice(&inner);
    let d = build_struct_property(2, 5, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|idx: i32| serde_json::json!({ "index": idx }),
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let v = &entries[0].value;
    assert_eq!(v["script_struct"]["index"].as_i64(), Some(-7));
    let props = v["properties"].as_array().unwrap();
    assert_eq!(props.len(), 1);
    assert_eq!(props[0]["name"].as_str(), Some("Inner"));
    assert_eq!(props[0]["value"].as_i64(), Some(99));
}

#[test]
fn native_struct_edgraph_pin_type_decodes() {
    let names = NameMap {
        names: vec![
            "PinType".to_string(),
            "StructProperty".to_string(),
            "EdGraphPinType".to_string(),
            "int".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    push_raw_name(&mut value, 3); // category = "int"
    push_raw_name(&mut value, 4); // sub_category = "None"
    push_i32(&mut value, -9); // sub_category_object
    value.push(0); // container_type = None
    push_i32(&mut value, 0); // bIsReference
    push_i32(&mut value, 0); // bIsWeakPointer
    push_i32(&mut value, 0); // member parent
    push_raw_name(&mut value, 4); // member name = "None"
    value.extend_from_slice(&[0u8; 16]); // member guid
    push_i32(&mut value, 0); // bIsConst
    push_i32(&mut value, 0); // bIsUObjectWrapper
    push_i32(&mut value, 0); // bSerializeAsSinglePrecisionFloat
    assert_eq!(value.len(), 69);
    let d = build_struct_property(2, 4, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|idx: i32| serde_json::json!({ "index": idx }),
        pins: PinSerCtx {
            filter_editor_only: false,
            has_source_index: false,
            has_uobject_wrapper: true,
            has_single_precision_float: true,
        },
        soft_object_paths: &[],
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let v = &entries[0].value;
    assert_eq!(v["category"].as_str(), Some("int"));
    assert_eq!(v["sub_category_object"]["index"].as_i64(), Some(-9));
}

#[test]
fn soft_object_property_resolves_list_index() {
    let names = NameMap {
        names: vec![
            "Ref".to_string(),
            "SoftObjectProperty".to_string(),
            "None".to_string(),
        ],
    };
    let table = vec![
        serde_json::json!({ "asset_path": "/Game/A.A" }),
        serde_json::json!({ "asset_path": "/Game/B.B" }),
    ];
    let mut value = Vec::new();
    push_i32(&mut value, 1); // index into the soft object path list

    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // Ref
    push_raw_name(&mut d, 1); // SoftObjectProperty
    push_i32(&mut d, 0); // type name inner param count
    push_i32(&mut d, value.len() as i32); // size = 4
    d.push(0); // flags
    d.extend_from_slice(&value);
    push_raw_name(&mut d, 2); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &table,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].value["asset_path"].as_str(), Some("/Game/B.B"));
}

#[test]
fn tagged_fallback_struct_parses_as_properties() {
    let names = NameMap {
        names: vec![
            "Constraint".to_string(),
            "StructProperty".to_string(),
            "ConstraintInstance".to_string(),
            "Inner".to_string(),
            "IntProperty".to_string(),
            "None".to_string(),
        ],
    };
    // Tagged properties: IntProperty "Inner" = 7, then None.
    let mut value = Vec::new();
    push_raw_name(&mut value, 3); // Inner
    push_raw_name(&mut value, 4); // IntProperty
    push_i32(&mut value, 0); // type name inner param count
    push_i32(&mut value, 4); // size
    value.push(0); // flags
    push_i32(&mut value, 7);
    push_raw_name(&mut value, 5); // None

    // build_struct_property sets the HasBinaryOrNativeSerialize flag (0x08), so
    // the struct would normally bail; ConstraintInstance is an allowlisted
    // tagged-fallback struct and must parse as properties instead.
    let d = build_struct_property(2, 5, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let v = &entries[0].value;
    assert_eq!(v["@struct"].as_str(), Some("ConstraintInstance"));
    let props = v["properties"].as_array().unwrap();
    assert_eq!(props.len(), 1);
    assert_eq!(props[0]["name"].as_str(), Some("Inner"));
    assert_eq!(props[0]["value"].as_i64(), Some(7));
}
