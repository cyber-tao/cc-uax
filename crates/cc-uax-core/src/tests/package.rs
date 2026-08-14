use super::common::*;
use crate::analysis::analyze_package;
use crate::name::NameMap;
use crate::reader::Reader;
use crate::{
    AnalysisDiagnostic, AssetView, DiagnosticSeverity, KnownOpaqueKind, Package, PackageRejection,
    PackageView, PropertyDecodeStatus,
};

fn diagnostic_with_code<'a>(
    diagnostics: &'a [AnalysisDiagnostic],
    code: &str,
) -> &'a AnalysisDiagnostic {
    diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == code)
        .unwrap_or_else(|| panic!("missing diagnostic code {code}: {diagnostics:#?}"))
}

#[test]
fn package_rejects_pre_ue5_version() {
    let mut data = Vec::new();
    push_u32(&mut data, 0x9E2A_83C1);
    push_i32(&mut data, -8);
    push_i32(&mut data, 0);
    push_i32(&mut data, 522);
    push_i32(&mut data, 999);
    push_i32(&mut data, 0);

    let error = Package::parse(&data).err().unwrap().to_string();
    assert!(error.contains("FileVersionUE5=999"));
}

/// A header that stops right after the version fields, so `PackageView::parse`
/// rejects it on the version check rather than on a later table read.
fn version_only_header(legacy: i32, ue4: i32, ue5: i32, licensee: i32) -> Vec<u8> {
    let mut data = Vec::new();
    push_u32(&mut data, 0x9E2A_83C1);
    push_i32(&mut data, legacy);
    push_i32(&mut data, 0);
    push_i32(&mut data, ue4);
    if legacy <= -8 {
        push_i32(&mut data, ue5);
    }
    push_i32(&mut data, licensee);
    data
}

// A package this tool deliberately does not target is `OutOfScope`: callers that
// scan whole projects must be able to record it as `unsupported` evidence instead
// of a scan failure. Bytes that are not a readable package stay `Malformed`.
#[test]
fn out_of_scope_packages_are_classified_apart_from_malformed_ones() {
    let out_of_scope: [(&str, Vec<u8>); 4] = [
        // UE4 package: FileVersionUE5 is 0 but the UE4 version is set.
        ("ue4 package", version_only_header(-7, 522, 0, 0)),
        // Cooked/unversioned package: every version field is 0.
        ("unversioned", version_only_header(-8, 0, 0, 0)),
        // UE3 package: LegacyFileVersion is not negative.
        ("ue3 package", version_only_header(1, 0, 0, 0)),
        // FileVersionUE5 below the supported floor but non-zero.
        ("below floor", version_only_header(-8, 522, 999, 0)),
    ];
    for (label, data) in out_of_scope {
        let error = PackageView::parse(&data)
            .err()
            .unwrap_or_else(|| panic!("{label} should be rejected"));
        assert_eq!(
            error.rejection(),
            PackageRejection::OutOfScope,
            "{label} should be out of scope, got: {error}"
        );
        assert!(error.is_out_of_scope(), "{label}");
    }

    // Wrong package magic is not a package at all.
    let mut wrong_magic = Vec::new();
    push_u32(&mut wrong_magic, 0x1234_5678);
    push_i32(&mut wrong_magic, -8);
    let error = PackageView::parse(&wrong_magic)
        .err()
        .expect("wrong magic should be rejected");
    assert_eq!(error.rejection(), PackageRejection::Malformed);
    assert!(!error.is_out_of_scope());

    // A truncated but otherwise in-scope header is malformed, not out of scope:
    // the version fields pass and the name table read runs off the end.
    let mut truncated = build_minimal_package();
    truncated.truncate(40);
    let error = PackageView::parse(&truncated)
        .err()
        .expect("truncated package should be rejected");
    assert_eq!(
        error.rejection(),
        PackageRejection::Malformed,
        "truncated package should be malformed, got: {error}"
    );
}

// Big-endian console packages and package-level compression are readable formats
// this tool does not target, so they must not look like corruption either.
#[test]
fn swapped_byte_order_is_out_of_scope() {
    let mut data = Vec::new();
    push_u32(&mut data, 0xC183_2A9E);
    let error = PackageView::parse(&data)
        .err()
        .expect("swapped tag should be rejected");
    assert_eq!(error.rejection(), PackageRejection::OutOfScope);
}

#[test]
fn package_accepts_ue50_file_versions() {
    // FileVersionUE5 1000 is INITIAL_VERSION; 1007 is the last UE5.0 AUTOMATIC value.
    for (fv, major, minor) in [(1000, 5, 0), (1004, 5, 0), (1007, 5, 0)] {
        let data = build_minimal_package_with_version(fv, major, minor);
        let package = Package::parse(&data)
            .unwrap_or_else(|err| panic!("UE5.0 FileVersionUE5 {fv} failed to parse: {err:#}"));
        assert_eq!(package.summary.file_version_ue5, fv);
    }
}

#[test]
fn unfiltered_editor_package_parses_localization_and_persistent_guid() {
    for (fv, major, minor, legacy) in [(1008, 5, 1, -8), (1017, 5, 6, -9), (1018, 5, 8, -9)] {
        let data = build_minimal_editor_package_with_version(fv, major, minor);
        let package = Package::parse(&data).unwrap_or_else(|err| {
            panic!("unfiltered FileVersionUE5 {fv} failed to parse: {err:#}")
        });
        assert_eq!(package.summary.file_version_ue5, fv);
        assert_eq!(package.summary.legacy_file_version, legacy);
        assert!(!package.summary.filter_editor_only());
    }
}

#[test]
fn name_map_rejects_negative_count() {
    let data = [];
    let mut reader = Reader::new(&data);

    let error = NameMap::parse(&mut reader, 0, -1, 522)
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("name count out of range"));
}

#[test]
fn package_view_analyzes_the_bound_minimal_package() {
    let data = build_minimal_package();
    let view = PackageView::parse(&data).expect("minimal package should parse");
    let analysis = view.analyze(AssetView::Full);

    assert_eq!(analysis.summary.file_version_ue4, 522);
    assert_eq!(analysis.summary.file_version_ue5, 1018);
    assert_eq!(analysis.summary.package_name, "TestPkg");
    assert_eq!(analysis.summary.export_count, 0);
    assert!(analysis.imports.is_empty());
    assert!(analysis.exports.is_empty());
}

#[test]
fn package_view_parses_ue56_file_version_1017() {
    let data = build_minimal_package_with_version(1017, 5, 6);
    let view = PackageView::parse(&data).expect("UE5.6 package should parse");
    let analysis = view.analyze(AssetView::Full);

    assert_eq!(analysis.summary.file_version_ue5, 1017);
    assert_eq!(analysis.summary.package_name, "TestPkg");
    assert_eq!(analysis.summary.export_count, 0);
    assert!(analysis.imports.is_empty());
    assert!(analysis.exports.is_empty());
}

#[test]
fn minimal_package_parses_across_supported_ue5_versions() {
    // The version-aware summary builder must emit a header the parser accepts for
    // every supported FileVersionUE5, from UE5.0 (1000) through UE5.8 (1018). Each
    // version gates a different set of summary fields, so a layout bug for any one
    // of them surfaces here as a parse failure.
    for (fv, major, minor) in [
        (1000, 5, 0),
        (1004, 5, 0),
        (1007, 5, 0),
        (1008, 5, 1),
        (1009, 5, 2),
        (1009, 5, 3),
        (1012, 5, 4),
        (1013, 5, 5),
        (1017, 5, 6),
        (1018, 5, 7),
        (1018, 5, 8),
    ] {
        let data = build_minimal_package_with_version(fv, major, minor);
        let package = Package::parse(&data).unwrap_or_else(|err| {
            panic!("FileVersionUE5 {fv} (UE{major}.{minor}) failed to parse: {err:#}")
        });
        assert_eq!(package.summary.file_version_ue5, fv);
    }
}

#[test]
fn import_package_name_gate_follows_engine_version_for_filtered_packages() {
    // UE5.6/5.7 omit FObjectImport::PackageName for FilterEditorOnly packages, but UE5.8
    // always writes it, and both share FileVersionUE5 = 1018. The engine version is the
    // only signal separating the layouts, so a 5.8 filtered package must read PackageName
    // while a 5.7 filtered package must not.
    use crate::object::ObjectImport;
    use crate::summary::EngineVersion;

    let ue4 = crate::version::ue4::HIGHEST;
    let ue5 = crate::version::ue5::IMPORT_TYPE_HIERARCHIES; // 1018

    let entry = |with_package_name: bool| {
        // parse_table requires a positive table offset, so pad the front and point at it.
        let mut table = vec![0u8; 4];
        let start = table.len();
        push_raw_name(&mut table, 0); // class_package
        push_raw_name(&mut table, 1); // class_name
        push_i32(&mut table, 0); // outer_index
        push_raw_name(&mut table, 2); // object_name
        if with_package_name {
            push_raw_name(&mut table, 3); // package_name
        }
        push_i32(&mut table, 0); // is_optional (bool32, OPTIONAL_RESOURCES)
        (table, start as i32)
    };

    // UE5.8 filtered package: bytes carry PackageName, and it must be read.
    let (table, offset) = entry(true);
    let mut r = Reader::new(&table);
    let engine_58 = EngineVersion {
        major: 5,
        minor: 8,
        ..Default::default()
    };
    let imports = ObjectImport::parse_table(&mut r, offset, 1, ue4, ue5, true, &engine_58)
        .expect("5.8 filtered import table parses");
    assert_eq!(imports[0].package_name.as_ref().map(|n| n.index), Some(3));
    assert_eq!(
        r.pos(),
        table.len() as u64,
        "5.8 entry must consume PackageName"
    );

    // UE5.7 filtered package: bytes omit PackageName, and none is read.
    let (table, offset) = entry(false);
    let mut r = Reader::new(&table);
    let engine_57 = EngineVersion {
        major: 5,
        minor: 7,
        ..Default::default()
    };
    let imports = ObjectImport::parse_table(&mut r, offset, 1, ue4, ue5, true, &engine_57)
        .expect("5.7 filtered import table parses");
    assert!(imports[0].package_name.is_none());
    assert_eq!(
        r.pos(),
        table.len() as u64,
        "5.7 entry must not read PackageName"
    );
}

#[test]
fn soft_object_path_table_error_is_structured() {
    let mut data = build_minimal_package();
    put_i32(&mut data, 76, 1);
    put_i32(&mut data, 80, 999_999);

    let package = Package::parse(&data).unwrap();
    assert!(
        package
            .soft_object_path_error
            .as_deref()
            .unwrap()
            .contains("soft object path table seek failed")
    );

    let analysis = analyze_package(&package, &data, AssetView::References);
    let diagnostic = diagnostic_with_code(&analysis.diagnostics, "soft_object_path_table_error");
    assert_eq!(diagnostic.severity, DiagnosticSeverity::Warning);
    assert_eq!(diagnostic.path, "/summary/soft_object_paths");
    assert!(
        diagnostic
            .message
            .contains("soft object path table seek failed")
    );
}

// Every header table rejects a declared count that cannot fit in the remaining
// file before allocating for it. The soft-object-path table is no exception: a
// package must not be able to make the parser reserve for a count it cannot back.
#[test]
fn soft_object_path_count_beyond_the_file_is_rejected_before_reading() {
    // A FileVersionUE5 = 1018 entry is two FNames plus an FString length: 20 bytes.
    const ENTRY_BYTES: usize = 20;
    let base = build_minimal_package();
    // Point the table at the very end of the file so `remaining` is a known value.
    let table_offset = base.len();

    for (label, count, extra_bytes, should_fail) in [
        (
            "count needs one byte more than the file has",
            2,
            ENTRY_BYTES * 2 - 1,
            true,
        ),
        ("count fits exactly", 2, ENTRY_BYTES * 2, false),
    ] {
        let mut data = base.clone();
        data.resize(table_offset + extra_bytes, 0);
        put_i32(&mut data, 76, count);
        put_i32(&mut data, 80, table_offset as i32);

        let package = Package::parse(&data).unwrap();
        let error = package.soft_object_path_error.as_deref();
        if should_fail {
            let error = error.unwrap_or_else(|| panic!("{label}: expected a structured error"));
            assert!(
                error.contains("soft object path count out of range"),
                "{label}: {error}"
            );
            assert!(
                package.soft_object_paths.is_empty(),
                "{label}: nothing should be decoded"
            );
        } else {
            assert!(error.is_none(), "{label}: {error:?}");
            assert_eq!(package.soft_object_paths.len(), count as usize, "{label}");
        }
    }
}

#[test]
fn invalid_script_window_is_structured() {
    let base = Package::parse(&build_minimal_package()).unwrap();
    let package = Package {
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

    let analysis = analyze_package(&package, &[0; 4], AssetView::Properties);
    let diagnostic = diagnostic_with_code(&analysis.diagnostics, "serial_window_invalid");
    assert_eq!(diagnostic.severity, DiagnosticSeverity::Error);
    assert_eq!(diagnostic.path, "/exports/0");
    assert!(diagnostic.message.contains("outside serial size"));
    assert!(analysis.exports[0].properties.is_empty());
}

#[test]
fn zero_script_window_uses_serial_range() {
    let base = Package::parse(&build_minimal_package()).unwrap();
    let mut data = Vec::new();
    data.push(0);
    push_raw_name(&mut data, 1);
    push_raw_name(&mut data, 2);
    push_i32(&mut data, 0);
    push_i32(&mut data, 4);
    data.push(0);
    push_i32(&mut data, 42);
    push_raw_name(&mut data, 3);

    let package = Package {
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

    let analysis = analyze_package(&package, &data, AssetView::Properties);
    let properties = &analysis.exports[0].properties;
    assert_eq!(properties.len(), 1);
    assert_eq!(properties[0].name, "Value");
    assert_eq!(properties[0].value.as_i64(), Some(42));
}

#[test]
fn pre_complete_typename_version_decodes_legacy_properties() {
    let mut base = Package::parse(&build_minimal_package()).unwrap();
    base.summary.file_version_ue5 = 1011;
    let mut data = Vec::new();
    data.push(0);
    push_legacy_tag_header(&mut data, 1, 2, 4);
    push_legacy_tag_tail(&mut data, 1011);
    push_i32(&mut data, 123);
    push_raw_name(&mut data, 3);

    let package = Package {
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

    let analysis = analyze_package(&package, &data, AssetView::Properties);
    let properties = &analysis.exports[0].properties;
    assert_eq!(properties.len(), 1);
    assert_eq!(properties[0].name, "Value");
    assert_eq!(properties[0].type_name, "IntProperty");
    assert_eq!(properties[0].value.as_i64(), Some(123));
    assert!(
        analysis
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != "properties_unsupported_version")
    );
}

#[test]
fn post_property_tail_is_classified_with_its_byte_range() {
    let base = Package::parse(&build_minimal_package()).unwrap();
    let mut data = Vec::new();
    data.push(0);
    push_raw_name(&mut data, 1);
    push_raw_name(&mut data, 2);
    push_i32(&mut data, 0);
    push_i32(&mut data, 4);
    data.push(0);
    push_i32(&mut data, 123);
    push_raw_name(&mut data, 3);
    data.extend_from_slice(&[1, 2, 3, 4]);

    let package = Package {
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

    let analysis = analyze_package(&package, &data, AssetView::Properties);
    let tail = analysis
        .known_opaque
        .iter()
        .find(|opaque| opaque.kind == KnownOpaqueKind::PostPropertyTail)
        .unwrap();
    let range = tail.byte_range.as_ref().unwrap();
    assert_eq!(range.size, 4);
    assert_eq!(range.preview, "01020304");
    assert!(
        analysis
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != "post_property_tail")
    );
}

#[test]
fn non_tagged_property_payload_is_reported_as_status() {
    let base = Package::parse(&build_minimal_package()).unwrap();
    let mut data = vec![0];
    data.extend_from_slice(&[1, 2, 3, 4]);
    let package = Package {
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

    let analysis = analyze_package(&package, &data, AssetView::Properties);
    assert_eq!(
        analysis.exports[0].property_status,
        Some(PropertyDecodeStatus::NonTaggedPayload)
    );
    assert!(analysis.exports[0].properties.is_empty());
    assert!(analysis.diagnostics.is_empty());
}

#[test]
fn soft_package_references_are_parsed_and_filtered() {
    let mut data = build_minimal_package();
    let name_offset = data.len() as i32;
    push_fstring(&mut data, "/Game/Foo/SoftDep");
    push_u32(&mut data, 0);
    push_fstring(&mut data, "None");
    push_u32(&mut data, 0);
    let soft_offset = data.len() as i32;
    push_raw_name(&mut data, 0);
    push_raw_name(&mut data, 1);
    put_i32(&mut data, 68, 2);
    put_i32(&mut data, 72, name_offset);
    put_i32(&mut data, 132, 2);
    put_i32(&mut data, 136, soft_offset);

    let package = Package::parse(&data).expect("package with soft refs should parse");
    assert!(package.soft_package_reference_error.is_none());
    assert_eq!(
        package.soft_package_references,
        vec!["/Game/Foo/SoftDep", "None"]
    );

    let analysis = analyze_package(&package, &data, AssetView::References);
    assert_eq!(analysis.references.soft, vec!["/Game/Foo/SoftDep"]);
}

#[test]
fn soft_package_table_failure_is_not_silenced() {
    let mut data = build_minimal_package();
    put_i32(&mut data, 132, 1);
    put_i32(&mut data, 136, 999_999);

    let package = Package::parse(&data).expect("broken optional table keeps package inspectable");
    assert!(package.soft_package_reference_error.is_some());
    let analysis = analyze_package(&package, &data, AssetView::References);
    let diagnostic =
        diagnostic_with_code(&analysis.diagnostics, "soft_package_reference_table_error");
    assert_eq!(diagnostic.severity, DiagnosticSeverity::Warning);
    assert!(
        diagnostic
            .message
            .contains("soft package reference table seek failed")
    );
}
