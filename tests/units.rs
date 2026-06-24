use cc_uax::name::NameMap;
use cc_uax::object::PackageIndex;
use cc_uax::package::{collect_package_references, package_path_from_relative};
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
        exports: vec![cc_uax::object::ObjectExport {
            class_index: PackageIndex(0),
            super_index: PackageIndex(0),
            template_index: PackageIndex(0),
            outer_index: PackageIndex(0),
            object_name: RawName {
                index: 0,
                number: 0,
            },
            object_flags: 0,
            serial_size: 4,
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
            script_serialization_start_offset: 0,
            script_serialization_end_offset: 8,
        }],
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
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, end);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "MyText");
    assert_eq!(entries[0].type_str, "TextProperty");
    let unparsed = entries[0].value.get("@unparsed").and_then(|v| v.as_str());
    assert_eq!(unparsed, Some("0000000004"));
}
