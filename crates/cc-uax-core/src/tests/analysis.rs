use super::common::*;
use crate::PackageView;
use crate::analysis::analyze_package;
use crate::model::{
    ASSET_ANALYSIS_SCHEMA_VERSION, AnalysisStatus, AssetAnalysis, AssetView, CapabilityKind,
    DecodedValue, DiagnosticSeverity, KnownOpaqueKind, ParseCoverage,
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
fn classified_opaque_tail_is_recorded_without_forcing_partial() {
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
    // A classified opaque tail is honest evidence, not a defect: the asset stays
    // complete while the bytes are recorded, and nothing is left unclassified.
    assert_eq!(analysis.status, AnalysisStatus::Complete);
    assert_eq!(analysis.coverage.property_exports_complete, 1);
    assert_eq!(analysis.coverage.known_opaque_regions, 1);
    assert_eq!(analysis.coverage.opaque_bytes, 4);
    assert_eq!(analysis.coverage.unclassified_bytes, 0);
    assert_eq!(analysis.coverage.export_bytes_total, data.len() as u64);
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
fn pre_and_post_script_regions_are_classified_with_zero_unclassified() {
    let base = Package::parse(&build_minimal_package()).unwrap();
    let mut data = Vec::new();
    data.extend_from_slice(&[0x11u8; 8]); // pre-script region (before the tagged block)
    let tagged_start = data.len();
    data.push(0); // object property serialization control
    push_raw_name(&mut data, 1); // Value
    push_raw_name(&mut data, 2); // IntProperty
    push_i32(&mut data, 0); // complete type-name parameter count
    push_i32(&mut data, 4); // value size
    data.push(0); // property tag flags
    push_i32(&mut data, 42);
    push_raw_name(&mut data, 3); // None
    let tagged_end = data.len();
    data.extend_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]); // post-property tail

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
        exports: vec![test_export(
            0,
            data.len() as i64,
            tagged_start as i64,
            tagged_end as i64,
        )],
        soft_object_paths: Vec::new(),
        soft_object_path_error: None,
        soft_package_references: Vec::new(),
        soft_package_reference_error: None,
    };

    let analysis = analyze_package(&package, &data, AssetView::Full);
    assert!(analysis.diagnostics.is_empty());
    assert_eq!(analysis.status, AnalysisStatus::Complete);
    assert_eq!(analysis.coverage.unclassified_bytes, 0);
    assert_eq!(analysis.coverage.export_bytes_total, data.len() as u64);
    assert_eq!(analysis.coverage.opaque_bytes, 12);
    assert_eq!(analysis.coverage.known_opaque_regions, 2);

    let pre = analysis
        .known_opaque
        .iter()
        .find(|region| region.kind == KnownOpaqueKind::PreScriptRegion)
        .expect("pre-script region is classified");
    let pre_range = pre.byte_range.as_ref().unwrap();
    assert_eq!(pre_range.start, 0);
    assert_eq!(pre_range.size, tagged_start as u64);

    let post = analysis
        .known_opaque
        .iter()
        .find(|region| region.kind == KnownOpaqueKind::PostPropertyTail)
        .expect("post-property tail is classified");
    let post_range = post.byte_range.as_ref().unwrap();
    assert_eq!(post_range.end, data.len() as u64);
    assert_eq!(post_range.size, 4);

    assert!(matches!(
        analysis.exports[0].properties[0].value,
        DecodedValue::Integer(42)
    ));
}

#[test]
fn non_tagged_payload_is_classified_as_one_opaque_region() {
    let base = Package::parse(&build_minimal_package()).unwrap();
    let mut data = vec![0]; // object property serialization control
    data.extend_from_slice(&[1, 2, 3, 4]); // not a decodable tagged-property layout

    let package = Package {
        summary: base.summary,
        names: NameMap {
            names: vec!["Obj".into()],
        },
        imports: Vec::new(),
        exports: vec![test_export(0, data.len() as i64, 0, 0)],
        soft_object_paths: Vec::new(),
        soft_object_path_error: None,
        soft_package_references: Vec::new(),
        soft_package_reference_error: None,
    };

    let analysis = analyze_package(&package, &data, AssetView::Full);
    // A non-tagged payload decodes nothing; the whole window must surface as one
    // classified opaque region and the tagged-property capability is partial.
    assert_eq!(analysis.status, AnalysisStatus::Partial);
    assert_eq!(analysis.coverage.unclassified_bytes, 0);
    assert_eq!(analysis.coverage.export_bytes_total, data.len() as u64);
    assert_eq!(analysis.coverage.known_opaque_regions, 1);
    assert_eq!(
        analysis.known_opaque[0].kind,
        KnownOpaqueKind::PostPropertyTail
    );
    let range = analysis.known_opaque[0].byte_range.as_ref().unwrap();
    assert_eq!(range.start, 0);
    assert_eq!(range.size, data.len() as u64);
}

#[test]
fn parse_coverage_add_assign_doubles_every_serialized_field() {
    // A full struct literal forces every field to be named here, and the
    // AddAssign impl destructures every field, so a new coverage field cannot be
    // added without updating both -- drift is a compile error, not a silent gap.
    let base = ParseCoverage {
        bytes_total: 1,
        export_bytes_total: 2,
        exports_total: 3,
        exports_analyzed: 4,
        property_exports_total: 5,
        property_exports_complete: 6,
        properties_decoded: 7,
        graph_nodes_total: 8,
        graph_nodes_decoded: 9,
        pins_decoded: 10,
        graph_edges_decoded: 11,
        rigvm_graphs_total: 12,
        rigvm_graphs_decoded: 13,
        rigvm_nodes_total: 14,
        rigvm_nodes_decoded: 15,
        rigvm_pins_total: 16,
        rigvm_pins_decoded: 17,
        rigvm_links_total: 18,
        rigvm_links_decoded: 19,
        pcg_graphs_total: 20,
        pcg_graphs_decoded: 21,
        pcg_nodes_total: 22,
        pcg_nodes_decoded: 23,
        pcg_pins_total: 24,
        pcg_pins_decoded: 25,
        pcg_edges_total: 26,
        pcg_edges_decoded: 27,
        state_tree_graphs_total: 28,
        state_tree_graphs_decoded: 29,
        state_tree_states_total: 30,
        state_tree_states_decoded: 31,
        state_tree_tasks_decoded: 32,
        state_tree_conditions_decoded: 33,
        state_tree_transitions_decoded: 34,
        known_opaque_regions: 35,
        opaque_bytes: 36,
        unclassified_bytes: 37,
        diagnostic_errors: 38,
        diagnostic_warnings: 39,
    };

    let mut doubled = base.clone();
    doubled += &base;

    let base_map = serde_json_crate::to_value(&base).unwrap();
    let doubled_map = serde_json_crate::to_value(&doubled).unwrap();
    let base_obj = base_map.as_object().unwrap();
    let doubled_obj = doubled_map.as_object().unwrap();
    assert_eq!(
        base_obj.len(),
        39,
        "every coverage field must be non-zero here"
    );
    for (key, value) in base_obj {
        let single = value.as_u64().unwrap();
        let summed = doubled_obj
            .get(key)
            .and_then(serde_json_crate::Value::as_u64)
            .unwrap_or_else(|| panic!("field {key} was dropped by AddAssign"));
        assert_eq!(summed, single * 2, "field {key} was not summed");
    }
}

#[test]
fn object_guid_after_tagged_properties_is_decoded() {
    let base = Package::parse(&build_minimal_package()).unwrap();

    // Case 1: PossiblySerializeObjectGuid flag set, followed by a real FGuid.
    let mut data = Vec::new();
    data.push(0); // object property serialization control
    push_raw_name(&mut data, 1); // Value
    push_raw_name(&mut data, 2); // IntProperty
    push_i32(&mut data, 0);
    push_i32(&mut data, 4);
    data.push(0);
    push_i32(&mut data, 42);
    push_raw_name(&mut data, 3); // None
    let tagged_end = data.len();
    push_i32(&mut data, 1); // bSerializeGuid = true
    data.extend_from_slice(&[0x01, 0, 0, 0, 0x02, 0, 0, 0, 0x03, 0, 0, 0, 0x04, 0, 0, 0]); // 16-byte FGuid

    let names = || NameMap {
        names: vec![
            "Obj".into(),
            "Value".into(),
            "IntProperty".into(),
            "None".into(),
        ],
    };
    let package = Package {
        summary: base.summary.clone(),
        names: names(),
        imports: Vec::new(),
        exports: vec![test_export(0, data.len() as i64, 0, tagged_end as i64)],
        soft_object_paths: Vec::new(),
        soft_object_path_error: None,
        soft_package_references: Vec::new(),
        soft_package_reference_error: None,
    };
    let analysis = analyze_package(&package, &data, AssetView::Full);
    assert_eq!(analysis.status, AnalysisStatus::Complete);
    assert!(analysis.exports[0].object_guid.is_some());
    assert_eq!(analysis.coverage.unclassified_bytes, 0);
    assert!(analysis.known_opaque.is_empty());

    // Case 2: the flag is clear, so no GUID is recorded and only the flag byte is
    // consumed; the trailing bytes remain a classified opaque tail.
    let mut data = Vec::new();
    data.push(0);
    push_raw_name(&mut data, 1);
    push_raw_name(&mut data, 2);
    push_i32(&mut data, 0);
    push_i32(&mut data, 4);
    data.push(0);
    push_i32(&mut data, 42);
    push_raw_name(&mut data, 3);
    let tagged_end = data.len();
    push_i32(&mut data, 0); // bSerializeGuid = false
    data.extend_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]); // bulk tail

    let package = Package {
        summary: base.summary,
        names: names(),
        imports: Vec::new(),
        exports: vec![test_export(0, data.len() as i64, 0, tagged_end as i64)],
        soft_object_paths: Vec::new(),
        soft_object_path_error: None,
        soft_package_references: Vec::new(),
        soft_package_reference_error: None,
    };
    let analysis = analyze_package(&package, &data, AssetView::Full);
    assert!(analysis.exports[0].object_guid.is_none());
    assert_eq!(analysis.coverage.unclassified_bytes, 0);
    assert_eq!(analysis.coverage.known_opaque_regions, 1);
    assert_eq!(
        analysis.known_opaque[0].byte_range.as_ref().unwrap().size,
        4
    );
}

/// Builds an export payload of one IntProperty plus a `None` terminator, then
/// whatever `tail` the caller wants after the tagged-property block. Returns the
/// bytes and the offset where the property block ends.
fn export_with_property_tail(tail: &[u8]) -> (Vec<u8>, usize) {
    let mut data = Vec::new();
    data.push(0); // object property serialization control
    push_raw_name(&mut data, 1); // Value
    push_raw_name(&mut data, 2); // IntProperty
    push_i32(&mut data, 0);
    push_i32(&mut data, 4);
    data.push(0);
    push_i32(&mut data, 42);
    push_raw_name(&mut data, 3); // None
    let tagged_end = data.len();
    data.extend_from_slice(tail);
    (data, tagged_end)
}

fn guid_tail_names() -> NameMap {
    NameMap {
        names: vec![
            "Obj".into(),
            "Value".into(),
            "IntProperty".into(),
            "None".into(),
        ],
    }
}

/// `UObject::Serialize` writes `PossiblySerializeObjectGuid` right after
/// `SerializeScriptProperties` returns (Obj.cpp), *before* any subclass override
/// appends its own data. So a real GUID need not land on the export end, and the
/// flag must be read whether or not UE declared an explicit property range —
/// without a declared range `property_end` is just `serial_end` and carries no
/// information about where the property block closed.
#[test]
fn object_guid_is_read_after_the_property_block_in_both_window_shapes() {
    let base = Package::parse(&build_minimal_package()).unwrap();
    let mut tail = Vec::new();
    push_i32(&mut tail, 1); // bSerializeObjectGuid = true
    tail.extend_from_slice(&[0x01, 0, 0, 0, 0x02, 0, 0, 0, 0x03, 0, 0, 0, 0x04, 0, 0, 0]);
    tail.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]); // subclass Serialize payload
    let (data, tagged_end) = export_with_property_tail(&tail);

    // `script_end = 0` is the shape UE writes for every export that does not need
    // an explicit property range, which is the overwhelming majority.
    for (label, script_end) in [
        ("declared property range", tagged_end as i64),
        ("no declared property range", 0),
    ] {
        let package = Package {
            summary: base.summary.clone(),
            names: guid_tail_names(),
            imports: Vec::new(),
            exports: vec![test_export(0, data.len() as i64, 0, script_end)],
            soft_object_paths: Vec::new(),
            soft_object_path_error: None,
            soft_package_references: Vec::new(),
            soft_package_reference_error: None,
        };
        let analysis = analyze_package(&package, &data, AssetView::Full);
        assert_eq!(
            analysis.exports[0].object_guid.as_deref(),
            Some("00000001000000020000000300000004"),
            "{label}"
        );
        assert_eq!(analysis.coverage.unclassified_bytes, 0, "{label}");
        // Only the 4 bytes after the GUID stay opaque.
        assert_eq!(analysis.coverage.known_opaque_regions, 1, "{label}");
        assert_eq!(
            analysis.known_opaque[0].byte_range.as_ref().unwrap().size,
            4,
            "{label}"
        );
    }
}

/// `TryEnterField` emits `FArchive::SerializeBool`, which writes exactly 0 or 1.
/// Any other leading value means the property block did not end where the decoder
/// thinks it did, so the tail must stay classified opaque instead of being
/// consumed as a presence flag.
#[test]
fn a_tail_whose_flag_is_not_a_serialized_bool_stays_opaque() {
    let base = Package::parse(&build_minimal_package()).unwrap();
    let mut tail = Vec::new();
    push_i32(&mut tail, 0x4433_2211); // not 0 or 1
    tail.extend_from_slice(&[0x55, 0x66, 0x77, 0x88]);
    let (data, _) = export_with_property_tail(&tail);

    let package = Package {
        summary: base.summary,
        names: guid_tail_names(),
        imports: Vec::new(),
        exports: vec![test_export(0, data.len() as i64, 0, 0)],
        soft_object_paths: Vec::new(),
        soft_object_path_error: None,
        soft_package_references: Vec::new(),
        soft_package_reference_error: None,
    };
    let analysis = analyze_package(&package, &data, AssetView::Full);
    assert!(analysis.exports[0].object_guid.is_none());
    assert_eq!(analysis.coverage.unclassified_bytes, 0);
    // The whole 8-byte tail stays one classified opaque region.
    assert_eq!(analysis.coverage.known_opaque_regions, 1);
    assert_eq!(
        analysis.known_opaque[0].byte_range.as_ref().unwrap().size,
        8
    );
}

/// A cleared flag consumes only its four bytes; whatever a subclass `Serialize`
/// wrote after it stays classified opaque.
#[test]
fn a_cleared_guid_flag_is_consumed_without_a_declared_property_range() {
    let base = Package::parse(&build_minimal_package()).unwrap();
    let mut tail = Vec::new();
    push_i32(&mut tail, 0); // bSerializeObjectGuid = false
    tail.extend_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]);
    let (data, _) = export_with_property_tail(&tail);

    let package = Package {
        summary: base.summary,
        names: guid_tail_names(),
        imports: Vec::new(),
        exports: vec![test_export(0, data.len() as i64, 0, 0)],
        soft_object_paths: Vec::new(),
        soft_object_path_error: None,
        soft_package_references: Vec::new(),
        soft_package_reference_error: None,
    };
    let analysis = analyze_package(&package, &data, AssetView::Full);
    assert!(analysis.exports[0].object_guid.is_none());
    assert_eq!(analysis.coverage.unclassified_bytes, 0);
    assert_eq!(analysis.coverage.known_opaque_regions, 1);
    assert_eq!(
        analysis.known_opaque[0].byte_range.as_ref().unwrap().size,
        4
    );
}

/// A four-byte cleared flag that fills the tail exactly leaves no opaque region:
/// this is the common case that used to be reported as a false 4-byte opaque tail
/// on every plain UObject export.
#[test]
fn a_cleared_guid_flag_that_ends_the_export_leaves_no_opaque_tail() {
    let base = Package::parse(&build_minimal_package()).unwrap();
    let mut tail = Vec::new();
    push_i32(&mut tail, 0);
    let (data, _) = export_with_property_tail(&tail);

    let package = Package {
        summary: base.summary,
        names: guid_tail_names(),
        imports: Vec::new(),
        exports: vec![test_export(0, data.len() as i64, 0, 0)],
        soft_object_paths: Vec::new(),
        soft_object_path_error: None,
        soft_package_references: Vec::new(),
        soft_package_reference_error: None,
    };
    let analysis = analyze_package(&package, &data, AssetView::Full);
    assert_eq!(analysis.status, AnalysisStatus::Complete);
    assert!(analysis.exports[0].object_guid.is_none());
    assert_eq!(analysis.coverage.unclassified_bytes, 0);
    assert_eq!(analysis.coverage.known_opaque_regions, 0);
    assert!(analysis.known_opaque.is_empty());
}

#[test]
fn overridable_serialization_is_declared_unsupported() {
    let base = Package::parse(&build_minimal_package()).unwrap();
    let mut data = Vec::new();
    data.push(0x02); // EClassSerializationControlExtension: OverridableSerialization
    data.push(0x00); // EOverriddenPropertyOperation
    push_raw_name(&mut data, 1); // Value
    push_raw_name(&mut data, 2); // IntProperty
    push_i32(&mut data, 0);
    push_i32(&mut data, 4);
    data.push(0);
    push_i32(&mut data, 42);
    push_raw_name(&mut data, 3); // None

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

    let analysis = analyze_package(&package, &data, AssetView::Full);
    // The control byte's overridable bit downgrades tagged properties instead of
    // decoding the unsupported container layout as if it were normal.
    assert_eq!(analysis.status, AnalysisStatus::Partial);
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|d| d.code == "overridable_serialization_unsupported")
    );
    let tagged = analysis
        .capabilities
        .iter()
        .find(|c| c.kind == CapabilityKind::TaggedProperties)
        .expect("tagged-property capability is present");
    assert_eq!(tagged.status, AnalysisStatus::Partial);
    assert_eq!(analysis.coverage.unclassified_bytes, 0);
}

#[test]
fn future_file_version_is_reported_as_unsupported() {
    let mut package = Package::parse(&build_minimal_package()).unwrap();
    package.summary.file_version_ue5 = crate::version::ue5::IMPORT_TYPE_HIERARCHIES + 1;
    let analysis = analyze_package(&package, &build_minimal_package(), AssetView::Summary);
    assert_eq!(analysis.status, AnalysisStatus::Unsupported);
}

#[test]
fn below_verified_floor_is_partial_with_package_version_capability() {
    // Parse rejects FileVersionUE5 below SUPPORTED_FILE_VERSION_FLOOR, which currently
    // equals the verified floor. Mutate a parsed package to lock the diagnostic and
    // PackageVersion capability path.
    let bytes = build_minimal_package_with_version(1000, 5, 0);
    let mut package = Package::parse(&bytes).unwrap();
    package.summary.file_version_ue5 = crate::version::VERIFIED_FILE_VERSION_FLOOR - 1;
    let analysis = analyze_package(&package, &bytes, AssetView::Summary);
    assert!(
        analysis.diagnostics.iter().any(|diagnostic| diagnostic.code
            == "package_below_verified_version"
            && diagnostic.severity == DiagnosticSeverity::Info),
        "expected a package_below_verified_version info note"
    );
    assert_eq!(analysis.status, AnalysisStatus::Partial);
    let version = analysis
        .capabilities
        .iter()
        .find(|capability| capability.kind == CapabilityKind::PackageVersion)
        .expect("PackageVersion capability missing");
    assert_eq!(version.status, AnalysisStatus::Partial);
}

#[test]
fn verified_floor_version_adds_no_version_note() {
    for (fv, major, minor) in [
        (1000, 5, 0),
        (1004, 5, 0),
        (1008, 5, 1),
        (1009, 5, 3),
        (1012, 5, 4),
        (1017, 5, 6),
        (1018, 5, 8),
    ] {
        let bytes = build_minimal_package_with_version(fv, major, minor);
        let package = Package::parse(&bytes).unwrap();
        let analysis = analyze_package(&package, &bytes, AssetView::Summary);
        assert!(
            !analysis
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "package_below_verified_version"),
            "FileVersionUE5={fv} is at/above the verified floor"
        );
        assert!(
            !analysis
                .capabilities
                .iter()
                .any(|capability| capability.kind == CapabilityKind::PackageVersion),
            "FileVersionUE5={fv} must not emit PackageVersion"
        );
        assert_eq!(analysis.status, AnalysisStatus::Complete);
    }
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

#[test]
fn rigvm_edgraph_mirrors_are_excluded_from_edgraph_coverage_when_model_exists() {
    use crate::object::{ObjectImport, PackageIndex};

    let header = build_minimal_package();
    let mut package = Package::parse(&header).unwrap();
    package.names = NameMap {
        names: vec![
            "MyPin".into(),
            "exec".into(),
            "None".into(),
            "RigVMGraph".into(),
            "RigVMEdGraphNode".into(),
            "/Script/RigVMDeveloper".into(),
            "/Script/RigVMEditor".into(),
            "Model".into(),
            "Mirror".into(),
        ],
    };
    package.imports = vec![
        ObjectImport {
            class_package: raw_name(5),
            class_name: raw_name(5),
            outer_index: PackageIndex(0),
            object_name: raw_name(5),
            package_name: None,
        },
        ObjectImport {
            class_package: raw_name(5),
            class_name: raw_name(3),
            outer_index: PackageIndex(-1),
            object_name: raw_name(3),
            package_name: None,
        },
        ObjectImport {
            class_package: raw_name(6),
            class_name: raw_name(6),
            outer_index: PackageIndex(0),
            object_name: raw_name(6),
            package_name: None,
        },
        ObjectImport {
            class_package: raw_name(6),
            class_name: raw_name(4),
            outer_index: PackageIndex(-3),
            object_name: raw_name(4),
            package_name: None,
        },
    ];

    let mut data = Vec::new();
    data.push(0); // object control
    push_raw_name(&mut data, 2); // None
    let property_end = data.len();
    push_i32(&mut data, 0);
    push_i32(&mut data, 1);
    push_i32(&mut data, 0);
    push_i32(&mut data, 3);
    push_guid(&mut data, 1, 2, 3, 4);
    push_i32(&mut data, 3);
    push_guid(&mut data, 1, 2, 3, 4);
    push_raw_name(&mut data, 0);
    push_fstring(&mut data, "");
    data.push(1);
    push_raw_name(&mut data, 1);
    push_raw_name(&mut data, 2);
    push_i32(&mut data, 0);
    data.push(0);
    push_i32(&mut data, 0);
    push_i32(&mut data, 0);
    push_i32(&mut data, 0);
    push_raw_name(&mut data, 2);
    push_guid(&mut data, 0, 0, 0, 0);
    push_i32(&mut data, 0);
    push_fstring(&mut data, "");
    push_fstring(&mut data, "");
    push_i32(&mut data, 0);
    push_empty_ftext(&mut data);
    push_i32(&mut data, 0);
    push_i32(&mut data, 0);
    push_i32(&mut data, 1);
    push_i32(&mut data, 1);

    let mut model = test_export(7, 0, 0, 0);
    model.class_index = PackageIndex(-2);
    let mut mirror = test_export(8, data.len() as i64, 0, property_end as i64);
    mirror.class_index = PackageIndex(-4);
    package.exports = vec![model, mirror];

    let analysis = analyze_package(&package, &data, AssetView::Logic);
    assert_eq!(
        analysis.coverage.graph_nodes_total, 0,
        "RigVMEdGraphNode must not count toward EdGraph coverage when a RigVM model exists"
    );
    assert_eq!(analysis.coverage.pins_decoded, 0);
    assert!(analysis.graphs.is_empty());
}
