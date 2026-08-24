mod ed_graph;
mod pcg;
mod rigvm;
mod state_tree;
mod typed;

use crate::decode::pins::is_graph_node_class;
use crate::decode::rigvm::{is_rigvm_graph_class, is_rigvm_link_class};
use crate::decode::{DecodeOptions, DecodeReport, DecodedExport};
use crate::diagnostic::{Diagnostic, Severity};
use crate::graph_models::*;
use crate::model::*;
use crate::package::Package;
use crate::property::{PropertyEntry, PropertyParseStatus};
use crate::references::collect_package_references;
use crate::rejection::PackageParseError;
use crate::structured_value::{Map, Value};
use crate::version::ue5;
use std::collections::{BTreeSet, HashSet};

pub(crate) use ed_graph::build_logic_graphs;
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

struct GraphCoverage {
    nodes_total: usize,
    nodes_decoded: usize,
    pins_decoded: usize,
    edges_decoded: usize,
}

impl GraphCoverage {
    fn is_partial(&self, graphs: &[LogicGraph]) -> bool {
        self.nodes_decoded < self.nodes_total
            || graphs
                .iter()
                .any(|graph| graph.excluded_cross_graph_links > 0 || graph.unresolved_links > 0)
    }
}

fn compute_graph_coverage(
    report: &DecodeReport<'_>,
    wants_logic: bool,
    mirrors: &ControlRigMirrors,
    graphs: &[LogicGraph],
) -> GraphCoverage {
    if !wants_logic {
        return GraphCoverage {
            nodes_total: 0,
            nodes_decoded: 0,
            pins_decoded: 0,
            edges_decoded: 0,
        };
    }
    let counted = report
        .exports
        .iter()
        .filter(|export| !mirrors.excludes_export(report, export));
    let nodes_total = counted
        .clone()
        .filter(|export| is_graph_node_class(&export.identity.class))
        .count();
    let nodes_decoded = counted
        .clone()
        .filter(|export| is_graph_node_class(&export.identity.class) && export.pins.is_some())
        .count();
    let pins_decoded = counted
        .map(|export| export.pins.as_ref().map_or(0, Vec::len))
        .sum();
    let edges_decoded = graphs.iter().map(|graph| graph.edges.len()).sum();
    GraphCoverage {
        nodes_total,
        nodes_decoded,
        pins_decoded,
        edges_decoded,
    }
}

/// Which EdGraph exports mirror an authoritative RigVM model.
///
/// A Control Rig stores the same logic twice: the RigVM model is authoritative
/// and the `ControlRigGraph`/`RigVMEdGraph` editor graphs mirror it. When the
/// package carries a RigVM model those mirrors are dropped so a rig is not
/// counted twice; when it does not, the editor graphs are the only graphs there
/// are and must be kept.
///
/// Graph assembly and graph coverage share one instance so the graph list and the
/// node/pin totals cannot disagree about what was excluded.
pub(crate) struct ControlRigMirrors {
    /// Export indices of the editor graphs. Empty when suppression is off.
    editor_graphs: HashSet<i32>,
    suppress: bool,
}

impl ControlRigMirrors {
    pub(crate) fn detect(report: &DecodeReport<'_>) -> Self {
        let suppress = report
            .exports
            .iter()
            .any(|export| is_rigvm_graph_class(&export.identity.class));
        let editor_graphs = if suppress {
            report
                .exports
                .iter()
                .filter(|export| is_control_rig_editor_graph(&export.identity.class))
                .map(|export| export.identity.index)
                .collect()
        } else {
            HashSet::new()
        };
        Self {
            editor_graphs,
            suppress,
        }
    }

    /// For a pin-bearing export whose owning graph index is already known.
    pub(crate) fn excludes_class_or_graph(&self, class_full: &str, graph_index: i32) -> bool {
        self.suppress
            && (self.editor_graphs.contains(&graph_index)
                || is_control_rig_editor_mirror_node(class_full))
    }

    /// For an arbitrary export, resolving its owner from the export table.
    fn excludes_export(&self, report: &DecodeReport<'_>, export: &DecodedExport) -> bool {
        if !self.suppress {
            return false;
        }
        if is_control_rig_editor_mirror_node(&export.identity.class) {
            return true;
        }
        report
            .package
            .exports
            .get((export.identity.index - 1).max(0) as usize)
            .is_some_and(|raw| self.editor_graphs.contains(&raw.outer_index.0))
    }
}

fn simple_class_name(class_full: &str) -> Option<&str> {
    class_full.rsplit(['.', '/']).next()
}

fn is_control_rig_editor_mirror_node(class_full: &str) -> bool {
    simple_class_name(class_full)
        .is_some_and(|simple| simple == "ControlRigGraphNode" || simple == "RigVMEdGraphNode")
}

fn is_control_rig_editor_graph(class_full: &str) -> bool {
    simple_class_name(class_full)
        .is_some_and(|simple| simple == "ControlRigGraph" || simple == "RigVMEdGraph")
}

struct PropertyCoverage {
    exports_total: usize,
    exports_complete: usize,
    exports_not_tagged: usize,
    exports_failed: usize,
    properties_decoded: usize,
}

impl PropertyCoverage {
    fn is_partial(&self) -> bool {
        self.exports_complete < self.exports_total
    }
}

fn compute_property_coverage(
    report: &DecodeReport<'_>,
    package: &Package,
    wants_properties: bool,
) -> PropertyCoverage {
    let mut coverage = PropertyCoverage {
        exports_total: 0,
        exports_complete: 0,
        exports_not_tagged: 0,
        exports_failed: 0,
        properties_decoded: 0,
    };
    if !wants_properties {
        return coverage;
    }
    for (export, raw) in report.exports.iter().zip(&package.exports) {
        coverage.properties_decoded += export.properties.as_ref().map_or(0, Vec::len);
        if raw.serial_size <= 0 || is_rigvm_link_class(&export.identity.class) {
            continue;
        }
        coverage.exports_total += 1;
        // The two incomplete outcomes are different evidence: a payload the
        // decoder does not model at all versus a tagged block that broke
        // partway. Collapsing them hides which one a report is reporting.
        match export.property_status {
            Some(PropertyParseStatus::Complete | PropertyParseStatus::Empty) => {
                coverage.exports_complete += 1;
            }
            Some(PropertyParseStatus::NonTaggedPayload) => coverage.exports_not_tagged += 1,
            Some(PropertyParseStatus::FailedAfterEntries) => coverage.exports_failed += 1,
            None => {}
        }
    }
    coverage
}

struct PcgCoverage {
    graphs_decoded: usize,
    nodes_decoded: usize,
    nodes_total: usize,
    pins_decoded: usize,
    pins_total: usize,
    edges_decoded: usize,
    edges_total: usize,
}

impl PcgCoverage {
    fn is_partial(&self, adapter: &pcg::PcgAdapterResult) -> bool {
        self.graphs_decoded < adapter.graph_exports_total
            || self.nodes_decoded < self.nodes_total
            || self.pins_decoded < self.pins_total
            || self.edges_decoded < self.edges_total
            || !adapter.known_opaque.is_empty()
    }
}

fn compute_pcg_coverage(adapter: &pcg::PcgAdapterResult) -> PcgCoverage {
    let graphs_decoded = adapter
        .graphs
        .iter()
        .filter(|graph| graph.nodes_array_count > 0 || graph.default_node_count > 0)
        .count();
    let nodes_decoded = adapter.graphs.iter().map(|graph| graph.nodes.len()).sum();
    let nodes_total = nodes_decoded
        + adapter
            .graphs
            .iter()
            .map(|graph| graph.unresolved_node_references)
            .sum::<usize>();
    let pins_decoded = adapter
        .graphs
        .iter()
        .flat_map(|graph| &graph.nodes)
        .map(|node| node.pins.len())
        .sum::<usize>();
    let pins_total = pins_decoded
        + adapter
            .graphs
            .iter()
            .map(|graph| graph.unresolved_pin_references)
            .sum::<usize>();
    let edges_decoded = adapter.graphs.iter().map(|graph| graph.edges.len()).sum();
    let edges_total = edges_decoded
        + adapter
            .graphs
            .iter()
            .map(|graph| graph.unresolved_edge_references)
            .sum::<usize>();
    PcgCoverage {
        graphs_decoded,
        nodes_decoded,
        nodes_total,
        pins_decoded,
        pins_total,
        edges_decoded,
        edges_total,
    }
}

struct StateTreeCoverage {
    graphs_decoded: usize,
    states_decoded: usize,
    tasks_decoded: usize,
    conditions_decoded: usize,
    transitions_decoded: usize,
}

impl StateTreeCoverage {
    fn is_partial(&self, adapter: &state_tree::StateTreeAdapterResult) -> bool {
        self.graphs_decoded < adapter.graph_exports_total
            || self.states_decoded < adapter.state_exports_total
            || adapter.states_incomplete > 0
            || adapter
                .graphs
                .iter()
                .any(|graph| graph.unresolved_state_references > 0)
    }
}

fn compute_state_tree_coverage(adapter: &state_tree::StateTreeAdapterResult) -> StateTreeCoverage {
    let graphs_decoded = adapter
        .graphs
        .iter()
        .filter(|graph| graph.editor_data_index.is_some())
        .count();
    let states_decoded = adapter.graphs.iter().map(|graph| graph.states.len()).sum();
    let tasks_decoded = adapter
        .graphs
        .iter()
        .flat_map(|graph| &graph.states)
        .map(|state| state.tasks.len())
        .sum::<usize>();
    let conditions_decoded = adapter
        .graphs
        .iter()
        .flat_map(|graph| &graph.states)
        .map(|state| state.enter_conditions.len())
        .sum::<usize>();
    let transitions_decoded = adapter
        .graphs
        .iter()
        .flat_map(|graph| &graph.states)
        .map(|state| state.transitions.len())
        .sum::<usize>();
    StateTreeCoverage {
        graphs_decoded,
        states_decoded,
        tasks_decoded,
        conditions_decoded,
        transitions_decoded,
    }
}

#[derive(Clone, Copy)]
struct RigVmCoverage {
    graphs_total: usize,
    graphs_decoded: usize,
    nodes_total: usize,
    nodes_decoded: usize,
    pins_total: usize,
    pins_decoded: usize,
    links_total: usize,
    links_decoded: usize,
}

fn compute_rigvm_coverage(adapter: &rigvm::RigVmAdapterResult) -> RigVmCoverage {
    RigVmCoverage {
        graphs_total: adapter.graphs_total,
        graphs_decoded: adapter.graphs_decoded,
        nodes_total: adapter.nodes_total,
        nodes_decoded: adapter.nodes_decoded,
        pins_total: adapter.pins_total,
        pins_decoded: adapter.pins_decoded,
        links_total: adapter.links_total,
        links_decoded: adapter.links_decoded,
    }
}

/// Grouped coverage/adapter inputs for capability aggregation, so the single
/// call site does not need a long positional argument list.
struct CapabilityInputs<'a> {
    wants_references: bool,
    wants_properties: bool,
    wants_logic: bool,
    property_coverage: &'a PropertyCoverage,
    property_partial: bool,
    graph_coverage: &'a GraphCoverage,
    graph_partial: bool,
    rigvm_adapter: &'a rigvm::RigVmAdapterResult,
    rigvm_coverage: RigVmCoverage,
    state_tree_adapter: &'a state_tree::StateTreeAdapterResult,
    state_tree_partial: bool,
    state_tree_coverage: &'a StateTreeCoverage,
    pcg_adapter: &'a pcg::PcgAdapterResult,
    pcg_partial: bool,
    pcg_coverage: &'a PcgCoverage,
}

fn build_capabilities(
    report: &DecodeReport<'_>,
    package: &Package,
    input: CapabilityInputs<'_>,
    known_opaque: &mut Vec<KnownOpaque>,
) -> Vec<AnalysisCapability> {
    let CapabilityInputs {
        wants_references,
        wants_properties,
        wants_logic,
        property_coverage,
        property_partial,
        graph_coverage,
        graph_partial,
        rigvm_adapter,
        rigvm_coverage,
        state_tree_adapter,
        state_tree_partial,
        state_tree_coverage,
        pcg_adapter,
        pcg_partial,
        pcg_coverage,
    } = input;
    let mut capabilities = vec![AnalysisCapability {
        kind: CapabilityKind::PackageTables,
        status: if report.diagnostics.iter().any(|diagnostic| {
            diagnostic.code.contains("table") || diagnostic.code == "serial_window_invalid"
        }) {
            AnalysisStatus::Partial
        } else {
            AnalysisStatus::Complete
        },
        detail: None,
    }];
    if crate::version::is_below_verified_floor(package.summary.file_version_ue5) {
        capabilities.push(AnalysisCapability {
            kind: CapabilityKind::PackageVersion,
            status: AnalysisStatus::Partial,
            detail: Some(format!(
                "FileVersionUE5={} is below the real-corpus-verified floor ({})",
                package.summary.file_version_ue5,
                crate::version::VERIFIED_FILE_VERSION_FLOOR
            )),
        });
    }
    if wants_references {
        capabilities.push(AnalysisCapability {
            kind: CapabilityKind::ReferenceTables,
            status: if package.soft_object_path_error.is_some()
                || package.soft_package_reference_error.is_some()
            {
                AnalysisStatus::Partial
            } else {
                AnalysisStatus::Complete
            },
            detail: None,
        });
    }
    if wants_properties {
        capabilities.push(AnalysisCapability {
            kind: CapabilityKind::TaggedProperties,
            status: if property_partial {
                AnalysisStatus::Partial
            } else {
                AnalysisStatus::Complete
            },
            detail: property_partial.then(|| {
                format!(
                    "{}/{} non-empty exports have complete tagged-property coverage ({} not a tagged payload, {} failed partway)",
                    property_coverage.exports_complete,
                    property_coverage.exports_total,
                    property_coverage.exports_not_tagged,
                    property_coverage.exports_failed
                )
            }),
        });
    }
    if wants_logic && graph_coverage.nodes_total > 0 {
        capabilities.push(AnalysisCapability {
            kind: CapabilityKind::EdGraphLogic,
            status: if graph_partial {
                AnalysisStatus::Partial
            } else {
                AnalysisStatus::Complete
            },
            detail: graph_partial.then(|| {
                format!(
                    "{}/{} graph nodes decoded; unresolved or cross-graph links are excluded",
                    graph_coverage.nodes_decoded, graph_coverage.nodes_total
                )
            }),
        });
    }
    if wants_logic && rigvm_coverage.graphs_total > 0 {
        let rigvm_complete = rigvm_adapter.is_complete();
        capabilities.push(AnalysisCapability {
            kind: CapabilityKind::RigVmModel,
            status: if rigvm_complete {
                AnalysisStatus::Complete
            } else {
                AnalysisStatus::Partial
            },
            detail: Some(format!(
                "{}/{} graphs, {}/{} nodes, {}/{} pins and {}/{} links decoded from the authoritative RigVM model",
                rigvm_coverage.graphs_decoded,
                rigvm_coverage.graphs_total,
                rigvm_coverage.nodes_decoded,
                rigvm_coverage.nodes_total,
                rigvm_coverage.pins_decoded,
                rigvm_coverage.pins_total,
                rigvm_coverage.links_decoded,
                rigvm_coverage.links_total,
            )),
        });
    }

    let has_rigvm = rigvm_coverage.graphs_total > 0;
    if wants_logic && has_rigvm {
        capabilities.push(AnalysisCapability {
            kind: CapabilityKind::RigVmBytecode,
            status: AnalysisStatus::Unsupported,
            detail: Some("compiled RigVM bytecode is retained as known opaque data".into()),
        });
        known_opaque.push(KnownOpaque {
            path: "/capabilities/rigvm_bytecode".into(),
            kind: KnownOpaqueKind::Capability,
            type_name: Some("RigVMBytecode".into()),
            reason: "compiled RigVM bytecode semantics are not decoded".into(),
            byte_range: None,
        });
        capabilities.push(AnalysisCapability {
            kind: CapabilityKind::RigHierarchy,
            status: AnalysisStatus::Unsupported,
            detail: Some("compressed RigHierarchy data is retained as known opaque data".into()),
        });
        known_opaque.push(KnownOpaque {
            path: "/capabilities/rig_hierarchy".into(),
            kind: KnownOpaqueKind::Capability,
            type_name: Some("RigHierarchy".into()),
            reason: "compressed RigHierarchy semantics are not decoded".into(),
            byte_range: None,
        });
    }
    if wants_logic && state_tree_adapter.graph_exports_total > 0 {
        capabilities.push(AnalysisCapability {
            kind: CapabilityKind::StateTreeSemantics,
            status: if state_tree_partial {
                AnalysisStatus::Partial
            } else {
                AnalysisStatus::Complete
            },
            detail: state_tree_partial.then(|| {
                format!(
                    "{}/{} graphs and {}/{} editor states decoded",
                    state_tree_coverage.graphs_decoded,
                    state_tree_adapter.graph_exports_total,
                    state_tree_coverage.states_decoded,
                    state_tree_adapter.state_exports_total
                )
            }),
        });
    }
    if wants_logic && pcg_adapter.graph_exports_total > 0 {
        capabilities.push(AnalysisCapability {
            kind: CapabilityKind::PcgSemantics,
            status: if pcg_partial {
                AnalysisStatus::Partial
            } else {
                AnalysisStatus::Complete
            },
            detail: pcg_partial.then(|| {
                format!(
                    "{}/{} graphs, {}/{} nodes, {}/{} pins, and {}/{} edges decoded; {} PropertyBag payloads remain known opaque",
                    pcg_coverage.graphs_decoded,
                    pcg_adapter.graph_exports_total,
                    pcg_coverage.nodes_decoded,
                    pcg_coverage.nodes_total,
                    pcg_coverage.pins_decoded,
                    pcg_coverage.pins_total,
                    pcg_coverage.edges_decoded,
                    pcg_coverage.edges_total,
                    pcg_adapter.known_opaque.len()
                )
            }),
        });
    }
    capabilities
}

fn determine_analysis_status(
    package: &Package,
    diagnostic_errors: usize,
    diagnostic_warnings: usize,
    unclassified_bytes: u64,
    capabilities: &[AnalysisCapability],
) -> AnalysisStatus {
    // `PackageFileSummary::parse` already rejects a file version above
    // `ue5::HIGHEST` as out of scope, matching UE's own refusal to read a package
    // whose version it does not know. This backstop only fires for a `Package`
    // assembled in-crate without going through that parse.
    let unsupported_version = package.summary.file_version_ue5 > ue5::HIGHEST;
    // A classified opaque region is honest evidence, not a defect, so it only
    // downgrades status through the capability it blocks (already surfaced as a
    // non-complete capability). Unclassified bytes are always a defect.
    let has_incomplete_capability = capabilities
        .iter()
        .any(|capability| capability.status != AnalysisStatus::Complete);
    if unsupported_version {
        AnalysisStatus::Unsupported
    } else if diagnostic_errors > 0
        || diagnostic_warnings > 0
        || unclassified_bytes > 0
        || has_incomplete_capability
    {
        AnalysisStatus::Partial
    } else {
        AnalysisStatus::Complete
    }
}

fn summary_to_model(package: &Package) -> AssetSummary {
    let summary = &package.summary;
    AssetSummary {
        package_name: summary.package_name.clone(),
        tag: summary.tag,
        legacy_file_version: summary.legacy_file_version,
        file_version_ue4: summary.file_version_ue4,
        file_version_ue5: summary.file_version_ue5,
        file_version_licensee: summary.file_version_licensee_ue,
        package_flags: summary.package_flags,
        filter_editor_only: summary.filter_editor_only(),
        total_header_size: summary.total_header_size,
        bulk_data_start_offset: summary.bulk_data_start_offset,
        name_count: summary.name_count,
        import_count: summary.import_count,
        export_count: summary.export_count,
        saved_by_engine_version: summary.engine_version.display(),
        compatible_engine_version: summary.compatible_engine_version.display(),
        custom_versions: summary
            .custom_versions
            .iter()
            .map(|version| CustomVersionInfo {
                guid: version.key.to_hex(),
                version: version.version,
            })
            .collect(),
    }
}

fn references_to_model(package: &Package) -> AssetReferences {
    let (assets, scripts) = collect_package_references(package.import_class_object_names());
    let soft = package
        .soft_package_references
        .iter()
        .filter(|reference| !reference.is_empty() && reference.as_str() != "None")
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    AssetReferences {
        assets,
        scripts,
        soft,
    }
}

fn imports_to_model(package: &Package) -> Vec<AssetImport> {
    package
        .imports
        .iter()
        .enumerate()
        .map(|(index, import)| {
            let package_index = -((index as i32) + 1);
            AssetImport {
                index: package_index,
                class_package: package.names.resolve_raw(import.class_package),
                class: package.names.resolve_raw(import.class_name),
                name: package.names.resolve_raw(import.object_name),
                outer_index: import.outer_index.0,
                outer_name: package.resolve_full_name(import.outer_index.0),
                package_name: import
                    .package_name
                    .map(|name| package.names.resolve_raw(name)),
                full_name: package.resolve_full_name(package_index),
            }
        })
        .collect()
}

fn export_to_model(
    package: &Package,
    export: &DecodedExport,
    include_serialization: bool,
) -> AssetExport {
    let raw = package
        .exports
        .get((export.identity.index - 1).max(0) as usize);
    let outer_index = raw.map_or(0, |raw| raw.outer_index.0);
    AssetExport {
        index: export.identity.index,
        name: export.identity.name.clone(),
        class: export.identity.class.clone(),
        super_name: raw.map_or_else(String::new, |raw| {
            package.resolve_full_name(raw.super_index.0)
        }),
        template_name: raw.map_or_else(String::new, |raw| {
            package.resolve_full_name(raw.template_index.0)
        }),
        outer_index,
        outer_name: package.resolve_full_name(outer_index),
        full_name: package.resolve_full_name(export.identity.index),
        is_asset: export.identity.is_asset,
        serialization: include_serialization.then(|| ExportSerialization {
            object_flags: raw.map_or(0, |raw| raw.object_flags),
            serial_offset: raw.map_or(0, |raw| raw.serial_offset),
            serial_size: raw.map_or(0, |raw| raw.serial_size),
            script_serialization_start: raw
                .filter(|_| package.summary.file_version_ue5 >= ue5::SCRIPT_SERIALIZATION_OFFSET)
                .map(|raw| raw.script_serialization_start_offset),
            script_serialization_end: raw
                .filter(|_| package.summary.file_version_ue5 >= ue5::SCRIPT_SERIALIZATION_OFFSET)
                .map(|raw| raw.script_serialization_end_offset),
        }),
        object_guid: export.object_guid.clone(),
        property_status: export.property_status.map(property_status_to_model),
        properties: export
            .properties
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(property_to_model)
            .collect(),
        metadata: export.metadata.clone(),
        member: export.member.as_ref().map(|member| MemberReference {
            name: member.name.clone(),
            parent: member.parent.clone(),
        }),
    }
}

fn property_status_to_model(status: PropertyParseStatus) -> PropertyDecodeStatus {
    match status {
        PropertyParseStatus::Complete => PropertyDecodeStatus::Complete,
        PropertyParseStatus::Empty => PropertyDecodeStatus::Empty,
        PropertyParseStatus::NonTaggedPayload => PropertyDecodeStatus::NonTaggedPayload,
        PropertyParseStatus::FailedAfterEntries => PropertyDecodeStatus::FailedAfterEntries,
    }
}

fn property_to_model(property: &PropertyEntry) -> AssetProperty {
    AssetProperty {
        name: property.name.clone(),
        type_name: property.type_str.clone(),
        array_index: property.array_index,
        value: property.value.clone(),
        guid: property.guid.clone(),
    }
}

fn diagnostic_to_model(diagnostic: &Diagnostic) -> AnalysisDiagnostic {
    AnalysisDiagnostic {
        severity: match diagnostic.severity {
            Severity::Error => DiagnosticSeverity::Error,
            Severity::Warning => DiagnosticSeverity::Warning,
            Severity::Info => DiagnosticSeverity::Info,
        },
        code: diagnostic.code.clone(),
        path: diagnostic.path.clone(),
        message: diagnostic.message.clone(),
        offset: diagnostic.offset,
        details: diagnostic.context.as_deref().cloned(),
    }
}

fn collect_known_opaque(
    report: &DecodeReport<'_>,
    include_property_values: bool,
) -> Vec<KnownOpaque> {
    let mut opaque = Vec::new();
    for export in &report.exports {
        let export_path = format!("/exports/{}", export.identity.index);
        if let Some(pre) = &export.pre_script_region
            && pre.size > 0
        {
            opaque.push(KnownOpaque {
                path: format!("{export_path}/pre_script_region"),
                kind: KnownOpaqueKind::PreScriptRegion,
                type_name: Some(export.identity.class.clone()),
                reason: "bytes precede the tagged-property block and are not decoded".into(),
                byte_range: Some(OpaqueByteRange {
                    start: pre.start,
                    end: pre.end,
                    size: pre.size,
                    preview: pre.preview.clone(),
                }),
            });
        }
        if let Some(tail) = &export.post_property_tail
            && tail.size > 0
        {
            opaque.push(KnownOpaque {
                path: format!("{export_path}/post_property_tail"),
                kind: KnownOpaqueKind::PostPropertyTail,
                type_name: Some(export.identity.class.clone()),
                reason: "bytes remain after all known export serializers".into(),
                byte_range: Some(OpaqueByteRange {
                    start: tail.start,
                    end: tail.end,
                    size: tail.size,
                    preview: tail.preview.clone(),
                }),
            });
        }
        if include_property_values {
            if let Some(properties) = &export.properties {
                for property in properties {
                    collect_opaque_value(
                        &property.value,
                        &format!("{export_path}/properties/{}", property.name),
                        Some(&property.type_str),
                        KnownOpaqueKind::PropertyValue,
                        &mut opaque,
                    );
                }
            }
            if let Some(metadata) = &export.metadata {
                collect_opaque_value(
                    metadata,
                    &format!("{export_path}/metadata"),
                    Some("PackageMetaData"),
                    KnownOpaqueKind::Metadata,
                    &mut opaque,
                );
            }
        }
    }
    opaque
}

fn collect_opaque_value(
    value: &Value,
    path: &str,
    type_name: Option<&str>,
    kind: KnownOpaqueKind,
    output: &mut Vec<KnownOpaque>,
) {
    match value {
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                collect_opaque_value(value, &format!("{path}/{index}"), type_name, kind, output);
            }
        }
        Value::Object(object) => {
            if let Some(properties) = object.get("properties").and_then(Value::as_array) {
                for property in properties {
                    let Some(entry) = property.as_object() else {
                        continue;
                    };
                    let (Some(name), Some(value)) = (
                        entry.get("name").and_then(Value::as_str),
                        entry.get("value"),
                    ) else {
                        continue;
                    };
                    collect_opaque_value(
                        value,
                        &format!("{path}/{name}"),
                        entry.get("type").and_then(Value::as_str).or(type_name),
                        kind,
                        output,
                    );
                }
            }
            let reason = if object.contains_key("@unparsed") {
                Some("property decoder emitted an unparsed byte preview".to_string())
            } else if object.get("status").and_then(Value::as_str) == Some("opaque") {
                Some(
                    object
                        .get("reason")
                        .and_then(Value::as_str)
                        .unwrap_or("decoder marked the value opaque")
                        .to_string(),
                )
            } else if object.contains_key("@struct") && object.contains_key("payload") {
                Some("custom struct payload is retained without semantic decoding".to_string())
            } else if object.get("size").is_some_and(Value::is_number)
                && object.get("preview").is_some_and(Value::is_string)
            {
                Some("byte payload is represented only by a bounded preview".to_string())
            } else {
                None
            };
            if let Some(reason) = reason {
                let path = path.strip_suffix("/serialized_data").unwrap_or(path);
                output.push(KnownOpaque {
                    path: path.to_string(),
                    kind,
                    type_name: type_name.map(normalize_opaque_type_name),
                    reason,
                    byte_range: opaque_byte_range(object).or_else(|| {
                        object
                            .get("payload")
                            .and_then(Value::as_object)
                            .and_then(opaque_byte_range)
                    }),
                });
                return;
            }
            for (key, value) in object {
                if key == "properties" {
                    continue;
                }
                collect_opaque_value(value, &format!("{path}/{key}"), type_name, kind, output);
            }
        }
        _ => {}
    }
}

fn normalize_opaque_type_name(type_name: &str) -> String {
    let Some(offset) = type_name.find("StructProperty(") else {
        return type_name.to_string();
    };
    let rest = &type_name[offset + "StructProperty(".len()..];
    rest.split(['(', ')']).next().unwrap_or(rest).to_string()
}

fn dedupe_known_opaque(values: &mut Vec<KnownOpaque>) {
    let mut seen = BTreeSet::new();
    values.retain(|value| {
        seen.insert((
            opaque_kind_rank(value.kind),
            value.path.clone(),
            value.type_name.clone(),
        ))
    });
}

fn opaque_kind_rank(kind: KnownOpaqueKind) -> u8 {
    match kind {
        KnownOpaqueKind::PropertyValue => 0,
        KnownOpaqueKind::PreScriptRegion => 1,
        KnownOpaqueKind::PostPropertyTail => 2,
        KnownOpaqueKind::Metadata => 3,
        KnownOpaqueKind::Capability => 4,
    }
}

fn opaque_byte_range(object: &Map) -> Option<OpaqueByteRange> {
    let start = object.get("start")?.as_u64()?;
    let end = object.get("end")?.as_u64()?;
    let size = object.get("size")?.as_u64()?;
    if end.checked_sub(start)? != size {
        return None;
    }
    Some(OpaqueByteRange {
        start,
        end,
        size,
        preview: object
            .get("preview")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    })
}
