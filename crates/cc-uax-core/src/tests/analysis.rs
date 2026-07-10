use super::common::*;
use crate::PackageView;
use crate::analysis::analyze_package;
use crate::model::{
    ASSET_ANALYSIS_SCHEMA_VERSION, AnalysisStatus, AssetAnalysis, AssetView, DecodedValue,
    KnownOpaqueKind,
};
use crate::name::NameMap;
use crate::object::{ObjectImport, PackageIndex};
use crate::package::Package;

#[test]
fn package_view_binds_analysis_to_its_original_bytes() {
    let bytes_a = build_minimal_package();
    let mut bytes_b = bytes_a.clone();
    bytes_b.extend_from_slice(&[1, 2, 3, 4]);

    let view_a = PackageView::parse(&bytes_a).expect("first package should parse");
    let view_b = PackageView::parse(&bytes_b).expect("second package should parse");
    let analysis_a = view_a.analyze(AssetView::Summary);
    let analysis_b = view_b.analyze(AssetView::Summary);

    assert_eq!(view_a.package_name(), "TestPkg");
    assert!(view_a.references().assets.is_empty());
    assert_eq!(analysis_a.coverage.bytes_total, bytes_a.len() as u64);
    assert_eq!(analysis_b.coverage.bytes_total, bytes_b.len() as u64);
    assert_eq!(analysis_a.schema_version, ASSET_ANALYSIS_SCHEMA_VERSION);
    assert_eq!(analysis_a.view, AssetView::Summary);
    assert_eq!(analysis_a.status, AnalysisStatus::Complete);
    assert!(analysis_a.exports.is_empty());
    assert!(analysis_a.references.assets.is_empty());
    assert_eq!(analysis_a.coverage.property_exports_total, 0);
    assert_eq!(analysis_a.coverage.graph_nodes_total, 0);

    let encoded = serde_json_crate::to_string(&analysis_a).unwrap();
    let decoded: AssetAnalysis = serde_json_crate::from_str(&encoded).unwrap();
    assert_eq!(decoded, analysis_a);
}

#[test]
fn opaque_tail_makes_analysis_partial_without_relying_on_diagnostics() {
    let base = Package::parse(&build_minimal_package()).unwrap();
    let mut data = Vec::new();
    data.push(0); // object property serialization control
    push_raw_name(&mut data, 1); // Value
    push_raw_name(&mut data, 2); // IntProperty
    push_i32(&mut data, 0); // complete type-name parameter count
    push_i32(&mut data, 4); // value size
    data.push(0); // property tag flags
    push_i32(&mut data, 42);
    push_raw_name(&mut data, 3); // None
    data.extend_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]);

    let package = Package {
        summary: base.summary,
        names: NameMap {
            names: vec![
                "Obj".into(),
                "Value".into(),
                "IntProperty".into(),
                "None".into(),
            ],
        },
        imports: Vec::new(),
        exports: vec![test_export(0, data.len() as i64, 0, 0)],
        soft_object_paths: Vec::new(),
        soft_object_path_error: None,
        soft_package_references: Vec::new(),
        soft_package_reference_error: None,
    };

    let summary_only = analyze_package(&package, &data, AssetView::Summary);
    assert_eq!(summary_only.status, AnalysisStatus::Complete);
    assert_eq!(summary_only.coverage.property_exports_total, 0);
    assert!(summary_only.known_opaque.is_empty());

    let analysis = analyze_package(&package, &data, AssetView::Full);
    assert!(analysis.diagnostics.is_empty());
    assert_eq!(analysis.status, AnalysisStatus::Partial);
    assert_eq!(analysis.coverage.property_exports_complete, 1);
    assert_eq!(analysis.coverage.known_opaque_regions, 1);
    assert_eq!(
        analysis.known_opaque[0].kind,
        KnownOpaqueKind::PostPropertyTail
    );
    assert_eq!(
        analysis.known_opaque[0].byte_range.as_ref().unwrap().size,
        4
    );
    assert!(matches!(
        analysis.exports[0].properties[0].value,
        DecodedValue::Integer(42)
    ));
}

#[test]
fn future_file_version_is_reported_as_unsupported() {
    let mut package = Package::parse(&build_minimal_package()).unwrap();
    package.summary.file_version_ue5 = crate::version::ue5::IMPORT_TYPE_HIERARCHIES + 1;
    let analysis = analyze_package(&package, &build_minimal_package(), AssetView::Summary);
    assert_eq!(analysis.status, AnalysisStatus::Unsupported);
}

#[test]
fn references_view_includes_typed_imports_without_decoding_exports() {
    let mut package = Package::parse(&build_minimal_package()).unwrap();
    package.names = NameMap {
        names: vec![
            "/Script/CoreUObject".into(),
            "Package".into(),
            "/Game/Foo".into(),
        ],
    };
    package.imports = vec![ObjectImport {
        class_package: raw_name(0),
        class_name: raw_name(1),
        outer_index: PackageIndex(0),
        object_name: raw_name(2),
        package_name: None,
    }];
    let bytes = build_minimal_package();

    let summary = analyze_package(&package, &bytes, AssetView::Summary);
    assert!(summary.imports.is_empty());
    assert!(summary.references.assets.is_empty());

    let references = analyze_package(&package, &bytes, AssetView::References);
    assert_eq!(references.coverage.exports_analyzed, 0);
    assert_eq!(references.references.assets, vec!["/Game/Foo"]);
    assert_eq!(references.imports.len(), 1);
    assert_eq!(references.imports[0].index, -1);
    assert_eq!(references.imports[0].class, "Package");
    assert_eq!(references.imports[0].name, "/Game/Foo");
}

fn raw_name(index: i32) -> crate::reader::RawName {
    crate::reader::RawName { index, number: 0 }
}
