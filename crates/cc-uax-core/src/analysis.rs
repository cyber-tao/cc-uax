mod capability;
mod coverage;
mod ed_graph;
mod model_map;
mod opaque;
mod pcg;
mod rigvm;
mod state_tree;
mod typed;

use crate::decode::DecodeOptions;
use crate::model::*;
use crate::package::Package;
use crate::rejection::PackageParseError;

use capability::{CapabilityInputs, build_capabilities, determine_analysis_status};
pub(crate) use coverage::ControlRigMirrors;
use coverage::{
    compute_graph_coverage, compute_pcg_coverage, compute_property_coverage,
    compute_rigvm_coverage, compute_state_tree_coverage,
};
pub(crate) use ed_graph::build_logic_graphs;
use model_map::{
    diagnostic_to_model, export_to_model, imports_to_model, references_to_model, summary_to_model,
};
use opaque::{collect_known_opaque, dedupe_known_opaque};
use pcg::build_pcg_graphs;
use rigvm::build_rigvm_graphs;
use state_tree::build_state_tree_graphs;

/// A parsed package tied to the exact byte slice from which it was created.
///
/// Decoding is intentionally available only through [`PackageView::analyze`],
/// and the view borrows its bytes for `'a`, so a view cannot outlive the buffer
/// its export offsets are decoded against:
///
/// ```compile_fail
/// let view = {
///     let bytes = Vec::<u8>::new();
///     cc_uax_core::PackageView::parse(&bytes).unwrap()
/// };
/// let _ = view.analyze(cc_uax_core::AssetView::Full);
/// ```
pub struct PackageView<'a> {
    bytes: &'a [u8],
    package: Package,
}

impl<'a> PackageView<'a> {
    pub fn parse(bytes: &'a [u8]) -> Result<Self, PackageParseError> {
        Ok(Self {
            package: Package::parse(bytes).map_err(PackageParseError::from)?,
            bytes,
        })
    }

    pub fn analyze(&self, view: AssetView) -> AssetAnalysis {
        analyze_package(&self.package, self.bytes, view)
    }

    pub fn package_name(&self) -> &str {
        &self.package.summary.package_name
    }

    pub fn references(&self) -> AssetReferences {
        references_to_model(&self.package)
    }
}

pub(crate) fn analyze_package(package: &Package, bytes: &[u8], view: AssetView) -> AssetAnalysis {
    let wants_logic = matches!(view, AssetView::Logic | AssetView::Full);
    let wants_properties = matches!(view, AssetView::Properties | AssetView::Full);
    let wants_references = matches!(view, AssetView::References | AssetView::Full);
    let options = build_decode_options(view);
    let report = package.decode(bytes, &options);
    let rigvm_adapter = if wants_logic {
        build_rigvm_graphs(&report)
    } else {
        rigvm::RigVmAdapterResult::default()
    };
    // Detected once and shared: graph assembly and graph coverage must agree on
    // which editor mirrors are dropped, or the reported graph count and the
    // node/pin totals disagree for the same Control Rig.
    let control_rig_mirrors = ControlRigMirrors::detect(&report);
    let graphs = if wants_logic {
        build_logic_graphs(&report, &control_rig_mirrors)
    } else {
        Vec::new()
    };
    let mut known_opaque = collect_known_opaque(&report, wants_properties);
    let include_serialization = matches!(view, AssetView::Full);
    let exports = report
        .exports
        .iter()
        .map(|export| export_to_model(package, export, include_serialization))
        .collect::<Vec<_>>();
    let pcg_adapter = build_pcg_graphs(if wants_logic { &exports } else { &[] });
    let state_tree_adapter = build_state_tree_graphs(if wants_logic { &exports } else { &[] });
    known_opaque.extend(pcg_adapter.known_opaque.iter().cloned());
    known_opaque.extend(state_tree_adapter.known_opaque.iter().cloned());
    dedupe_known_opaque(&mut known_opaque);
    let mut diagnostics = report
        .diagnostics
        .iter()
        .map(diagnostic_to_model)
        .collect::<Vec<_>>();
    diagnostics.extend(rigvm_adapter.diagnostics.iter().cloned());

    // Below the real-corpus-verified floor the version gates still apply, but the
    // result must not be `complete`: PackageVersion is Partial (see capabilities).
    if crate::version::is_below_verified_floor(package.summary.file_version_ue5) {
        diagnostics.push(AnalysisDiagnostic {
            severity: DiagnosticSeverity::Info,
            code: "package_below_verified_version".to_string(),
            path: "/summary/file_version_ue5".to_string(),
            message: format!(
                "FileVersionUE5={} is below the real-corpus-verified floor ({}); decoded per the version gates but not yet verified against a real asset",
                package.summary.file_version_ue5,
                crate::version::VERIFIED_FILE_VERSION_FLOOR
            ),
            offset: None,
            details: None,
        });
    }

    let graph_coverage =
        compute_graph_coverage(&report, wants_logic, &control_rig_mirrors, &graphs);
    let property_coverage = compute_property_coverage(&report, package, wants_properties);
    let pcg_coverage = compute_pcg_coverage(&pcg_adapter);
    let state_tree_coverage = compute_state_tree_coverage(&state_tree_adapter);
    let rigvm_coverage = compute_rigvm_coverage(&rigvm_adapter);

    let diagnostic_errors = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
        .count();
    let diagnostic_warnings = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity == DiagnosticSeverity::Warning)
        .count();

    let pcg_partial = pcg_coverage.is_partial(&pcg_adapter);
    let state_tree_partial = state_tree_coverage.is_partial(&state_tree_adapter);
    // Overridable serialization means the tagged-property layout was not decoded
    // faithfully, so the capability cannot claim completeness even if every
    // export otherwise parsed.
    let overridable_serialization = diagnostics
        .iter()
        .any(|diagnostic| diagnostic.code == "overridable_serialization_unsupported");
    let property_partial = property_coverage.is_partial() || overridable_serialization;
    let graph_partial = graph_coverage.is_partial(&graphs);

    let capabilities = build_capabilities(
        &report,
        package,
        CapabilityInputs {
            wants_references,
            wants_properties,
            wants_logic,
            property_coverage: &property_coverage,
            property_partial,
            graph_coverage: &graph_coverage,
            graph_partial,
            rigvm_adapter: &rigvm_adapter,
            rigvm_coverage,
            state_tree_adapter: &state_tree_adapter,
            state_tree_partial,
            state_tree_coverage: &state_tree_coverage,
            pcg_adapter: &pcg_adapter,
            pcg_partial,
            pcg_coverage: &pcg_coverage,
        },
        &mut known_opaque,
    );

    let known_opaque_regions = known_opaque.len();
    let opaque_bytes = known_opaque
        .iter()
        .filter_map(|region| region.byte_range.as_ref())
        .map(|range| range.size)
        .sum();
    // Split the export tails so a project-scale `opaque_bytes` can be read: bulk
    // class data dwarfs everything else, and lumping it with unattributed bytes
    // makes a healthy scan look like a decoder failure.
    let (class_payload_bytes, unattributed_tail_bytes) = report
        .exports
        .iter()
        .filter_map(|export| {
            export
                .post_property_tail
                .as_ref()
                .map(|tail| (export, tail))
        })
        .fold(
            (0u64, 0u64),
            |(class_bytes, unattributed), (export, tail)| {
                if export.property_block_closed {
                    (class_bytes.saturating_add(tail.size), unattributed)
                } else {
                    (class_bytes, unattributed.saturating_add(tail.size))
                }
            },
        );
    let export_bytes_total = report.exports.iter().map(|export| export.serial_size).sum();
    let unclassified_bytes = report
        .exports
        .iter()
        .map(|export| export.unclassified_bytes)
        .sum();
    let coverage = ParseCoverage {
        bytes_total: bytes.len() as u64,
        export_bytes_total,
        exports_total: package.exports.len(),
        exports_analyzed: report.exports.len(),
        property_exports_total: property_coverage.exports_total,
        property_exports_complete: property_coverage.exports_complete,
        property_exports_not_tagged: property_coverage.exports_not_tagged,
        property_exports_failed: property_coverage.exports_failed,
        properties_decoded: property_coverage.properties_decoded,
        graph_nodes_total: graph_coverage.nodes_total,
        graph_nodes_decoded: graph_coverage.nodes_decoded,
        pins_decoded: graph_coverage.pins_decoded,
        graph_edges_decoded: graph_coverage.edges_decoded,
        rigvm_graphs_total: rigvm_coverage.graphs_total,
        rigvm_graphs_decoded: rigvm_coverage.graphs_decoded,
        rigvm_nodes_total: rigvm_coverage.nodes_total,
        rigvm_nodes_decoded: rigvm_coverage.nodes_decoded,
        rigvm_pins_total: rigvm_coverage.pins_total,
        rigvm_pins_decoded: rigvm_coverage.pins_decoded,
        rigvm_links_total: rigvm_coverage.links_total,
        rigvm_links_decoded: rigvm_coverage.links_decoded,
        pcg_graphs_total: pcg_adapter.graph_exports_total,
        pcg_graphs_decoded: pcg_coverage.graphs_decoded,
        pcg_nodes_total: pcg_coverage.nodes_total,
        pcg_nodes_decoded: pcg_coverage.nodes_decoded,
        pcg_pins_total: pcg_coverage.pins_total,
        pcg_pins_decoded: pcg_coverage.pins_decoded,
        pcg_edges_total: pcg_coverage.edges_total,
        pcg_edges_decoded: pcg_coverage.edges_decoded,
        state_tree_graphs_total: state_tree_adapter.graph_exports_total,
        state_tree_graphs_decoded: state_tree_coverage.graphs_decoded,
        state_tree_states_total: state_tree_adapter.state_exports_total,
        state_tree_states_decoded: state_tree_coverage.states_decoded,
        state_tree_tasks_decoded: state_tree_coverage.tasks_decoded,
        state_tree_conditions_decoded: state_tree_coverage.conditions_decoded,
        state_tree_transitions_decoded: state_tree_coverage.transitions_decoded,
        known_opaque_regions,
        opaque_bytes,
        class_payload_bytes,
        unattributed_tail_bytes,
        unclassified_bytes,
        diagnostic_errors,
        diagnostic_warnings,
    };
    let status = determine_analysis_status(
        package,
        diagnostic_errors,
        diagnostic_warnings,
        unclassified_bytes,
        &capabilities,
    );

    AssetAnalysis {
        schema_version: ASSET_ANALYSIS_SCHEMA_VERSION,
        view,
        status,
        summary: summary_to_model(package),
        references: if wants_references {
            references_to_model(package)
        } else {
            AssetReferences {
                assets: Vec::new(),
                scripts: Vec::new(),
                soft: Vec::new(),
            }
        },
        imports: if wants_references {
            imports_to_model(package)
        } else {
            Vec::new()
        },
        exports,
        graphs,
        rigvm_graphs: rigvm_adapter.graphs,
        pcg_graphs: pcg_adapter.graphs,
        state_tree_graphs: state_tree_adapter.graphs,
        coverage,
        diagnostics,
        capabilities,
        known_opaque,
    }
}

fn build_decode_options(view: AssetView) -> DecodeOptions {
    match view {
        AssetView::Summary | AssetView::References => DecodeOptions::none(),
        AssetView::Logic => {
            let mut options = DecodeOptions::none();
            options.exports = true;
            options.pins = true;
            options
        }
        AssetView::Properties => {
            let mut options = DecodeOptions::none();
            options.exports = true;
            options.properties = true;
            options
        }
        AssetView::Full => DecodeOptions::full(),
    }
}
