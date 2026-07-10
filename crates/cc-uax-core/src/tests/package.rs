use super::common::*;
use crate::analysis::analyze_package;
use crate::name::NameMap;
use crate::reader::Reader;
use crate::{
    AnalysisDiagnostic, AssetView, DiagnosticSeverity, KnownOpaqueKind, Package, PackageView,
    PropertyDecodeStatus,
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
    push_legacy_tag_tail(&mut data);
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
