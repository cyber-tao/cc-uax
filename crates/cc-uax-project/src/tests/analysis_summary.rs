use super::common::minimal_package;
use crate::{AssetAnalysisSummary, ProjectAnalysisSummary};
use cc_uax_core::{
    AnalysisCapability, AnalysisDiagnostic, AnalysisStatus, AssetView, CapabilityKind,
    DiagnosticSeverity, KnownOpaque, KnownOpaqueKind, PackageView,
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
