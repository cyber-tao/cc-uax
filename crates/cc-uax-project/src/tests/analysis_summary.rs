use super::common::minimal_package;
use crate::{AssetAnalysisSummary, CapabilitySummary, ProjectAnalysisSummary};
use cc_uax_core::{
    AnalysisCapability, AnalysisDiagnostic, AnalysisStatus, AssetView, CapabilityKind,
    DiagnosticSeverity, KnownOpaque, KnownOpaqueKind, PackageView, ReferenceEvidence,
    ReferenceEvidenceSources,
};

fn complete_summary() -> AssetAnalysisSummary {
    let bytes = minimal_package();
    let analysis = PackageView::parse(&bytes).unwrap().analyze(AssetView::Full);
    AssetAnalysisSummary::from_analysis(&analysis)
}

#[test]
fn preserves_partial_coverage_capabilities_and_compact_limitations() {
    let bytes = minimal_package();
    let mut analysis = PackageView::parse(&bytes).unwrap().analyze(AssetView::Full);
    analysis.status = AnalysisStatus::Partial;
    analysis.coverage.known_opaque_regions = 1;
    analysis.coverage.diagnostic_warnings = 1;
    analysis.capabilities.push(AnalysisCapability {
        kind: CapabilityKind::EdGraphLogic,
        status: AnalysisStatus::Partial,
        detail: Some("full diagnostic detail is intentionally omitted".to_string()),
    });
    analysis.diagnostics.push(AnalysisDiagnostic {
        severity: DiagnosticSeverity::Warning,
        code: "test_partial".to_string(),
        path: "/exports/0".to_string(),
        message: "full diagnostic message is intentionally omitted".to_string(),
        offset: None,
        details: None,
    });
    analysis.known_opaque.push(KnownOpaque {
        path: "/exports/0/tail".to_string(),
        kind: KnownOpaqueKind::PostPropertyTail,
        type_name: None,
        reason: "full opaque reason is intentionally omitted".to_string(),
        byte_range: None,
    });

    let summary = AssetAnalysisSummary::from_analysis(&analysis);

    assert_eq!(summary.status, AnalysisStatus::Partial);
    assert_eq!(summary.coverage.known_opaque_regions, 1);
    assert_eq!(summary.diagnostics.warnings, 1);
    assert_eq!(summary.diagnostics.codes.get("test_partial"), Some(&1));
    assert_eq!(summary.known_opaque.total, 1);
    assert_eq!(summary.known_opaque.post_property_tails, 1);
    assert_eq!(summary.known_opaque.groups.len(), 1);
    assert_eq!(
        summary.known_opaque.groups[0].kind,
        KnownOpaqueKind::PostPropertyTail
    );
    assert!(summary.known_opaque.groups[0].type_name.is_none());
    assert_eq!(
        summary.known_opaque.groups[0].reason,
        "full opaque reason is intentionally omitted"
    );
    assert_eq!(summary.known_opaque.groups[0].regions, 1);
    assert!(summary.capabilities.iter().any(|capability| {
        capability.kind == CapabilityKind::EdGraphLogic
            && capability.status == AnalysisStatus::Partial
    }));
}

// A project report aggregates opaque regions instead of listing every one: an
// asset can carry thousands. Grouping must keep the region count and, unlike the
// old per-region identity list, the byte total so `opaque_bytes` is attributable.
#[test]
fn opaque_regions_are_grouped_by_kind_type_and_reason_with_byte_totals() {
    let bytes = minimal_package();
    let mut analysis = PackageView::parse(&bytes).unwrap().analyze(AssetView::Full);
    let tail = |index: usize, type_name: &str, size: u64| KnownOpaque {
        path: format!("/exports/{index}/post_property_tail"),
        kind: KnownOpaqueKind::PostPropertyTail,
        type_name: Some(type_name.to_string()),
        reason: "bytes remain after all known export serializers".to_string(),
        byte_range: Some(cc_uax_core::OpaqueByteRange {
            start: 0,
            end: size,
            size,
            preview: String::new(),
        }),
    };
    analysis.known_opaque = vec![
        tail(1, "/Script/Engine.Texture2D", 68),
        tail(2, "/Script/Engine.Texture2D", 68),
        tail(3, "/Script/Engine.Material", 12),
        KnownOpaque {
            path: "/exports/4/properties/Map".to_string(),
            kind: KnownOpaqueKind::PropertyValue,
            type_name: Some("MapProperty(StructProperty,NameProperty)".to_string()),
            reason: "property decoder emitted an unparsed byte preview".to_string(),
            byte_range: None,
        },
    ];

    let summary = AssetAnalysisSummary::from_analysis(&analysis);

    assert_eq!(summary.known_opaque.total, 4);
    assert_eq!(summary.known_opaque.post_property_tails, 3);
    assert_eq!(summary.known_opaque.property_values, 1);
    assert_eq!(summary.known_opaque.bytes, 68 + 68 + 12);
    // Four regions collapse to three groups; the two Texture2D tails merge.
    assert_eq!(summary.known_opaque.groups.len(), 3);
    let texture = summary
        .known_opaque
        .groups
        .iter()
        .find(|group| group.type_name.as_deref() == Some("/Script/Engine.Texture2D"))
        .expect("the Texture2D tails are one group");
    assert_eq!(texture.regions, 2);
    assert_eq!(texture.bytes, 136);
    assert_eq!(
        summary
            .known_opaque
            .groups
            .iter()
            .map(|group| group.regions)
            .sum::<usize>(),
        summary.known_opaque.total,
        "every region belongs to exactly one group"
    );
    assert_eq!(
        summary
            .known_opaque
            .groups
            .iter()
            .map(|group| group.bytes)
            .sum::<u64>(),
        summary.known_opaque.bytes,
        "group bytes sum back to the asset total"
    );
}

#[test]
fn aggregates_status_and_coverage_across_assets() {
    let complete = complete_summary();
    let mut partial = complete.clone();
    partial.status = AnalysisStatus::Partial;
    partial.coverage.diagnostic_warnings = 2;

    let aggregate = ProjectAnalysisSummary::aggregate([&complete, &partial].into_iter(), 0);

    assert_eq!(aggregate.status, AnalysisStatus::Partial);
    assert_eq!(aggregate.assets, 2);
    assert_eq!(aggregate.complete_assets, 1);
    assert_eq!(aggregate.partial_assets, 1);
    assert_eq!(aggregate.unsupported_assets, 0);
    assert_eq!(aggregate.scan_failures, 0);
    assert_eq!(
        aggregate.coverage.bytes_total,
        complete.coverage.bytes_total + partial.coverage.bytes_total
    );
    assert_eq!(aggregate.coverage.diagnostic_warnings, 2);

    let failed = ProjectAnalysisSummary::aggregate([&complete].into_iter(), 1);
    assert_eq!(failed.status, AnalysisStatus::Partial);
    assert_eq!(failed.scan_failures, 1);
}

fn partial_with_capability(kind: CapabilityKind) -> AssetAnalysisSummary {
    let mut summary = complete_summary();
    summary.status = AnalysisStatus::Partial;
    summary.capabilities.push(CapabilitySummary {
        kind,
        status: AnalysisStatus::Unsupported,
        detail: Some("compiled payload is retained as known opaque data".to_string()),
    });
    summary
}

// Every compiled Blueprint makes its package partial, so `partial_assets` alone
// reads as missing evidence on a project that has none. The histogram and the
// compiled-payload-only count are what separate the two.
#[test]
fn capability_histogram_and_compiled_payload_only_count_separate_named_gaps() {
    let bytecode = partial_with_capability(CapabilityKind::BlueprintBytecode);
    let niagara = partial_with_capability(CapabilityKind::NiagaraCompiled);
    // Same named gap, but this asset also failed to decode a property value, so
    // its evidence is not whole and it must not be counted as bytecode-only.
    let mut mixed = partial_with_capability(CapabilityKind::BlueprintBytecode);
    let properties_entry = mixed
        .capabilities
        .iter_mut()
        .find(|entry| entry.kind == CapabilityKind::TaggedProperties)
        .expect("the full view reports tagged properties");
    properties_entry.status = AnalysisStatus::Partial;
    properties_entry.detail = Some("one export failed partway".to_string());
    // Partial from a warning alone: no capability names a gap, so it is not
    // compiled-payload-only either.
    let mut warned = complete_summary();
    warned.status = AnalysisStatus::Partial;
    warned.diagnostics.warnings = 1;
    warned
        .diagnostics
        .codes
        .insert("property_value_fallback".to_string(), 1);

    let aggregate =
        ProjectAnalysisSummary::aggregate([&bytecode, &niagara, &mixed, &warned].into_iter(), 0);

    assert_eq!(aggregate.partial_assets, 4);
    assert_eq!(aggregate.partial_assets_compiled_payload_only, 2);
    assert_eq!(aggregate.partial_assets_without_explanation, 0);

    let count = |kind: CapabilityKind| {
        aggregate
            .capabilities
            .iter()
            .find(|entry| entry.kind == kind)
            .cloned()
    };
    let bytecode_count = count(CapabilityKind::BlueprintBytecode).expect("histogram names the gap");
    assert_eq!(bytecode_count.unsupported, 2);
    assert_eq!(bytecode_count.partial, 0);
    let properties = count(CapabilityKind::TaggedProperties).expect("properties are reported");
    assert_eq!(properties.partial, 1);
    assert_eq!(properties.complete, 3);
    for entry in &aggregate.capabilities {
        assert!(
            entry.complete + entry.partial + entry.unsupported <= aggregate.assets,
            "no capability can be reported by more assets than the scan has"
        );
    }
}

#[test]
fn reference_evidence_totals_count_distinct_value_only_packages_across_assets() {
    let mut first = complete_summary();
    first.reference_evidence = Some(ReferenceEvidence {
        value_packages: 3,
        confirmed_by_tables: 2,
        value_only_packages: vec!["/Game/Runtime/A".into()],
        sources: ReferenceEvidenceSources::default(),
    });
    let mut second = complete_summary();
    second.reference_evidence = Some(ReferenceEvidence {
        value_packages: 2,
        confirmed_by_tables: 1,
        // Names the same package as `first`, plus one of its own.
        value_only_packages: vec!["/Game/Runtime/A".into(), "/Game/Runtime/B".into()],
        sources: ReferenceEvidenceSources::default(),
    });
    let clean = complete_summary();

    let aggregate = ProjectAnalysisSummary::aggregate([&first, &second, &clean].into_iter(), 0);
    let evidence = &aggregate.reference_evidence;

    assert_eq!(evidence.checked_assets, 3);
    assert_eq!(evidence.value_packages, 5);
    assert_eq!(evidence.confirmed_by_tables, 3);
    assert_eq!(evidence.assets_with_value_only_packages, 2);
    assert_eq!(
        evidence.value_only_packages, 2,
        "the shared package is one project-wide gap, not two"
    );
}
