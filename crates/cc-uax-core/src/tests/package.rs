use super::common::*;
use crate::name::NameMap;
use crate::reader::Reader;
use crate::{OutputSections, Package, referenced_packages_from_bytes};

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

    let json = pkg.to_json(&data, &OutputSections::dump());
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

    let json = pkg.to_json(&data, &OutputSections::dump());
    let diag = diagnostic_with_code(&json, "soft_object_path_table_error");
    assert_eq!(diag["severity"].as_str(), Some("warning"));
    assert_eq!(diag["path"].as_str(), Some("/summary/soft_object_paths"));
    assert!(
        diag["message"]
            .as_str()
            .unwrap()
            .contains("soft object path table seek failed")
    );
    assert!(json["summary"].get("soft_object_paths_error").is_none());
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
        soft_package_references: Vec::new(),
        soft_package_reference_error: None,
    };
    let mut sections = OutputSections::none();
    sections.exports = true;
    sections.layout = true;
    sections.properties = true;

    let json = pkg.to_json(&[0; 4], &sections);
    let diag = diagnostic_with_code(&json, "serial_window_invalid");
    assert_eq!(diag["severity"].as_str(), Some("error"));
    assert_eq!(diag["path"].as_str(), Some("/exports/0"));
    assert!(
        diag["message"]
            .as_str()
            .unwrap()
            .contains("outside serial size")
    );
    assert!(json["exports"][0].get("serial_window_error").is_none());
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
        soft_package_references: Vec::new(),
        soft_package_reference_error: None,
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
fn pre_complete_typename_version_decodes_legacy_properties() {
    let mut base = Package::parse(&build_minimal_package()).unwrap();
    base.summary.file_version_ue5 = 1011;
    let mut data = Vec::new();
    data.push(0); // object serialization control byte
    push_legacy_tag_header(&mut data, 1, 2, 4);
    push_legacy_tag_tail(&mut data);
    push_i32(&mut data, 123);
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
        soft_package_references: Vec::new(),
        soft_package_reference_error: None,
    };
    let mut sections = OutputSections::none();
    sections.exports = true;
    sections.properties = true;

    let json = pkg.to_json(&data, &sections);
    let props = json["exports"][0]["properties"].as_array().unwrap();
    assert_eq!(props.len(), 1);
    assert_eq!(props[0]["name"].as_str(), Some("Value"));
    assert_eq!(props[0]["type"].as_str(), Some("IntProperty"));
    assert_eq!(props[0]["value"].as_i64(), Some(123));
    assert!(
        !json["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|diag| diag["code"].as_str() == Some("properties_unsupported_version"))
    );
}

#[test]
fn post_property_tail_is_reported_on_export() {
    let base = Package::parse(&build_minimal_package()).unwrap();
    let mut data = Vec::new();
    data.push(0); // object serialization control byte
    push_raw_name(&mut data, 1); // Value
    push_raw_name(&mut data, 2); // IntProperty
    push_i32(&mut data, 0); // type name inner param count
    push_i32(&mut data, 4); // size
    data.push(0); // flags
    push_i32(&mut data, 123);
    push_raw_name(&mut data, 3); // None
    data.extend_from_slice(&[1, 2, 3, 4]);

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
        soft_package_references: Vec::new(),
        soft_package_reference_error: None,
    };
    let mut sections = OutputSections::none();
    sections.exports = true;
    sections.properties = true;

    let json = pkg.to_json(&data, &sections);
    assert_eq!(
        json["exports"][0]["post_property_tail"]["size"].as_u64(),
        Some(4)
    );
    assert_eq!(
        json["exports"][0]["post_property_tail"]["preview"].as_str(),
        Some("01020304")
    );
    let diag = diagnostic_with_code(&json, "post_property_tail");
    assert_eq!(diag["severity"].as_str(), Some("warning"));
    assert_eq!(
        diag["offset"].as_u64(),
        json["exports"][0]["post_property_tail"]["start"].as_u64()
    );
    assert_eq!(
        diag["context"]["tail"]["preview"].as_str(),
        Some("01020304")
    );
}

#[test]
fn non_tagged_property_payload_is_reported_as_status() {
    let base = Package::parse(&build_minimal_package()).unwrap();
    let mut data = Vec::new();
    data.push(0); // object serialization control byte
    data.extend_from_slice(&[1, 2, 3, 4]); // not enough bytes for a tagged property name

    let pkg = Package {
        summary: base.summary,
        names: NameMap {
            names: vec!["Obj".to_string()],
        },
        imports: Vec::new(),
        exports: vec![test_export(0, data.len() as i64, 0, 0)],
        soft_object_paths: Vec::new(),
        soft_object_path_error: None,
        soft_package_references: Vec::new(),
        soft_package_reference_error: None,
    };
    let mut sections = OutputSections::none();
    sections.exports = true;
    sections.properties = true;

    let json = pkg.to_json(&data, &sections);

    assert_eq!(
        json["exports"][0]["property_parse_status"].as_str(),
        Some("non_tagged_payload")
    );
    assert!(
        json["exports"][0]["properties"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(json["diagnostics"].as_array().unwrap().is_empty());
}

#[test]
fn soft_package_references_parse_and_merge() {
    // Append a name table and a SoftPackageReferences table to the minimal package,
    // then patch the summary offsets (name_count@68/name_offset@72,
    // soft_package_references count@132/offset@136).
    let mut data = build_minimal_package();
    let name_offset = data.len() as i32;
    push_fstring(&mut data, "/Game/Foo/SoftDep");
    push_u32(&mut data, 0); // name hashes (ue4 >= 504)
    push_fstring(&mut data, "None");
    push_u32(&mut data, 0);
    let soft_offset = data.len() as i32;
    push_raw_name(&mut data, 0); // /Game/Foo/SoftDep
    push_raw_name(&mut data, 1); // None (filtered out of references)
    put_i32(&mut data, 68, 2); // name_count
    put_i32(&mut data, 72, name_offset);
    put_i32(&mut data, 132, 2); // soft_package_references_count
    put_i32(&mut data, 136, soft_offset);

    let pkg = Package::parse(&data).expect("package with soft refs should parse");
    assert!(pkg.soft_package_reference_error.is_none());
    assert_eq!(
        pkg.soft_package_references,
        vec!["/Game/Foo/SoftDep", "None"]
    );
    assert_eq!(pkg.referenced_packages(), vec!["/Game/Foo/SoftDep"]);
    assert!(pkg.references_package("/game/foo/softdep"));

    let json = pkg.to_json(&data, &OutputSections::parse("refs").unwrap());
    let soft = json["references"]["soft"].as_array().unwrap();
    assert_eq!(soft.len(), 1);
    assert_eq!(soft[0].as_str(), Some("/Game/Foo/SoftDep"));

    let refs = referenced_packages_from_bytes(&data).unwrap();
    assert_eq!(refs, vec!["/Game/Foo/SoftDep"]);
}

#[test]
fn fast_reference_extraction_rejects_soft_package_table_errors() {
    let mut data = build_minimal_package();
    put_i32(&mut data, 132, 1); // soft_package_references_count
    put_i32(&mut data, 136, 999_999); // soft_package_references_offset

    let pkg = Package::parse(&data).expect("package with broken soft ref table should parse");
    assert!(pkg.soft_package_reference_error.is_some());

    let json = pkg.to_json(&data, &OutputSections::parse("refs").unwrap());
    let diag = diagnostic_with_code(&json, "soft_package_reference_table_error");
    assert_eq!(diag["severity"].as_str(), Some("warning"));

    let err = referenced_packages_from_bytes(&data)
        .unwrap_err()
        .to_string();
    assert!(err.contains("soft package reference table failed"));
    assert!(err.contains("soft package reference table seek failed"));
}
