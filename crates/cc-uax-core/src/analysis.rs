mod pcg;
mod rigvm;
mod state_tree;
mod typed;

use crate::decode::pins::is_graph_node_class;
use crate::decode::rigvm::{is_rigvm_graph_class, is_rigvm_link_class};
use crate::decode::{DecodeOptions, DecodeReport, DecodedExport};
use crate::diagnostic::{Diagnostic, Severity};
use crate::model::*;
use crate::package::Package;
use crate::pin::{
    CONTAINER_TYPE_ARRAY, CONTAINER_TYPE_MAP, CONTAINER_TYPE_NONE, CONTAINER_TYPE_SET, Pin,
    PinTerminalType, PinType, UserDefinedPin,
};
use crate::property::{PropertyEntry, PropertyParseStatus};
use crate::reader::Guid;
use crate::references::collect_package_references;
use crate::structured_value::{Map, Value};
use crate::version::ue5;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use pcg::build_pcg_graphs;
use rigvm::build_rigvm_graphs;
use state_tree::build_state_tree_graphs;

/// A parsed package tied to the exact byte slice from which it was created.
///
/// Decoding is intentionally available only through [`PackageView::analyze`],
/// so callers cannot accidentally parse package tables from one file and then
/// decode export offsets against a different byte buffer.
///
/// ```compile_fail
/// let bytes_a = Vec::<u8>::new();
/// let bytes_b = Vec::<u8>::new();
/// let view = cc_uax_core::PackageView::parse(&bytes_a).unwrap();
/// let _ = view.analyze(cc_uax_core::AssetView::Full, &bytes_b);
/// ```
pub struct PackageView<'a> {
    bytes: &'a [u8],
    package: Package,
}

impl<'a> PackageView<'a> {
    pub fn parse(bytes: &'a [u8]) -> anyhow::Result<Self> {
        Ok(Self {
            package: Package::parse(bytes)?,
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
    let graphs = if wants_logic {
        build_logic_graphs(&report)
    } else {
        Vec::new()
    };
    let mut known_opaque = collect_known_opaque(&report, wants_properties);
    let exports = report
        .exports
        .iter()
        .map(|export| export_to_model(package, export))
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

    let has_authoritative_rigvm_graph = report
        .exports
        .iter()
        .any(|export| is_rigvm_graph_class(&export.identity.class));
    let control_rig_editor_graphs = if has_authoritative_rigvm_graph {
        control_rig_editor_graph_indices(&report)
    } else {
        HashSet::new()
    };

    let graph_coverage = compute_graph_coverage(
        &report,
        wants_logic,
        has_authoritative_rigvm_graph,
        &control_rig_editor_graphs,
        &graphs,
    );
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
    let property_partial = property_coverage.is_partial();
    let graph_partial = graph_coverage.is_partial(&graphs);

    let capabilities = build_capabilities(
        &report,
        package,
        wants_references,
        wants_properties,
        wants_logic,
        &property_coverage,
        property_partial,
        &graph_coverage,
        graph_partial,
        &rigvm_adapter,
        rigvm_coverage,
        &state_tree_adapter,
        state_tree_partial,
        &state_tree_coverage,
        &pcg_adapter,
        pcg_partial,
        &pcg_coverage,
        &mut known_opaque,
    );

    let known_opaque_regions = known_opaque.len();
    let coverage = ParseCoverage {
        bytes_total: bytes.len() as u64,
        exports_total: package.exports.len(),
        exports_analyzed: report.exports.len(),
        property_exports_total: property_coverage.exports_total,
        property_exports_complete: property_coverage.exports_complete,
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
        state_tree_tasks_total: state_tree_coverage.tasks_decoded,
        state_tree_tasks_decoded: state_tree_coverage.tasks_decoded,
        state_tree_conditions_total: state_tree_coverage.conditions_decoded,
        state_tree_conditions_decoded: state_tree_coverage.conditions_decoded,
        state_tree_transitions_total: state_tree_coverage.transitions_decoded,
        state_tree_transitions_decoded: state_tree_coverage.transitions_decoded,
        known_opaque_regions,
        diagnostic_errors,
        diagnostic_warnings,
    };
    let status = determine_analysis_status(
        package,
        diagnostic_errors,
        diagnostic_warnings,
        property_partial,
        graph_partial,
        known_opaque_regions,
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
    has_authoritative_rigvm_graph: bool,
    control_rig_editor_graphs: &HashSet<i32>,
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
    let nodes_total = report
        .exports
        .iter()
        .filter(|export| {
            is_graph_node_class(&export.identity.class)
                && !(has_authoritative_rigvm_graph
                    && is_control_rig_editor_mirror_export(
                        report,
                        export,
                        control_rig_editor_graphs,
                    ))
        })
        .count();
    let nodes_decoded = report
        .exports
        .iter()
        .filter(|export| {
            is_graph_node_class(&export.identity.class)
                && export.pins.is_some()
                && !(has_authoritative_rigvm_graph
                    && is_control_rig_editor_mirror_export(
                        report,
                        export,
                        control_rig_editor_graphs,
                    ))
        })
        .count();
    let pins_decoded = report
        .exports
        .iter()
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

struct PropertyCoverage {
    exports_total: usize,
    exports_complete: usize,
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
    if !wants_properties {
        return PropertyCoverage {
            exports_total: 0,
            exports_complete: 0,
            properties_decoded: 0,
        };
    }
    let exports_total = report
        .exports
        .iter()
        .zip(&package.exports)
        .filter(|(export, raw)| raw.serial_size > 0 && !is_rigvm_link_class(&export.identity.class))
        .count();
    let exports_complete = report
        .exports
        .iter()
        .zip(&package.exports)
        .filter(|(export, raw)| {
            raw.serial_size > 0
                && !is_rigvm_link_class(&export.identity.class)
                && matches!(
                    export.property_status,
                    Some(PropertyParseStatus::Complete | PropertyParseStatus::Empty)
                )
        })
        .count();
    let properties_decoded = report
        .exports
        .iter()
        .map(|export| export.properties.as_ref().map_or(0, Vec::len))
        .sum();
    PropertyCoverage {
        exports_total,
        exports_complete,
        properties_decoded,
    }
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

#[allow(clippy::too_many_arguments)]
fn build_capabilities(
    report: &DecodeReport<'_>,
    package: &Package,
    wants_references: bool,
    wants_properties: bool,
    wants_logic: bool,
    property_coverage: &PropertyCoverage,
    property_partial: bool,
    graph_coverage: &GraphCoverage,
    graph_partial: bool,
    rigvm_adapter: &rigvm::RigVmAdapterResult,
    rigvm_coverage: RigVmCoverage,
    state_tree_adapter: &state_tree::StateTreeAdapterResult,
    state_tree_partial: bool,
    state_tree_coverage: &StateTreeCoverage,
    pcg_adapter: &pcg::PcgAdapterResult,
    pcg_partial: bool,
    pcg_coverage: &PcgCoverage,
    known_opaque: &mut Vec<KnownOpaque>,
) -> Vec<AnalysisCapability> {
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
                    "{}/{} non-empty exports have complete tagged-property coverage",
                    property_coverage.exports_complete, property_coverage.exports_total
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
    property_partial: bool,
    graph_partial: bool,
    known_opaque_regions: usize,
    capabilities: &[AnalysisCapability],
) -> AnalysisStatus {
    let unsupported_version = package.summary.file_version_ue5 > ue5::IMPORT_TYPE_HIERARCHIES;
    let has_incomplete_capability = capabilities
        .iter()
        .any(|capability| capability.status != AnalysisStatus::Complete);
    if unsupported_version {
        AnalysisStatus::Unsupported
    } else if diagnostic_errors > 0
        || diagnostic_warnings > 0
        || property_partial
        || graph_partial
        || known_opaque_regions > 0
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

fn export_to_model(package: &Package, export: &DecodedExport) -> AssetExport {
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
        object_flags: raw.map_or(0, |raw| raw.object_flags),
        serial_offset: raw.map_or(0, |raw| raw.serial_offset),
        serial_size: raw.map_or(0, |raw| raw.serial_size),
        script_serialization_start: raw
            .filter(|_| package.summary.file_version_ue5 >= ue5::SCRIPT_SERIALIZATION_OFFSET)
            .map(|raw| raw.script_serialization_start_offset),
        script_serialization_end: raw
            .filter(|_| package.summary.file_version_ue5 >= ue5::SCRIPT_SERIALIZATION_OFFSET)
            .map(|raw| raw.script_serialization_end_offset),
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
        KnownOpaqueKind::PostPropertyTail => 1,
        KnownOpaqueKind::Metadata => 2,
        KnownOpaqueKind::Capability => 3,
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

#[derive(Clone, Copy)]
struct PinEndpoint<'a> {
    graph_index: i32,
    node_index: i32,
    pin: &'a Pin,
}

pub(crate) fn build_logic_graphs(report: &DecodeReport<'_>) -> Vec<LogicGraph> {
    let suppress_control_rig_mirrors = report
        .exports
        .iter()
        .any(|export| is_rigvm_graph_class(&export.identity.class));
    let control_rig_editor_graphs = if suppress_control_rig_mirrors {
        control_rig_editor_graph_indices(report)
    } else {
        HashSet::new()
    };
    let mut graphs: BTreeMap<i32, Vec<&DecodedExport>> = BTreeMap::new();
    let mut node_graph = HashMap::new();
    for export in &report.exports {
        if export.pins.is_none() {
            continue;
        }
        let Some(object_export) = report
            .package
            .exports
            .get((export.identity.index - 1).max(0) as usize)
        else {
            continue;
        };
        let graph_index = object_export.outer_index.0;
        if suppress_control_rig_mirrors
            && (control_rig_editor_graphs.contains(&graph_index)
                || is_control_rig_editor_mirror(&export.identity.class))
        {
            continue;
        }
        graphs.entry(graph_index).or_default().push(export);
        node_graph.insert(export.identity.index, graph_index);
    }

    let mut pin_by_id: HashMap<(i32, Guid), PinEndpoint<'_>> = HashMap::new();
    for export in &report.exports {
        let Some(&graph_index) = node_graph.get(&export.identity.index) else {
            continue;
        };
        let Some(pins) = &export.pins else {
            continue;
        };
        for pin in pins {
            pin_by_id.insert(
                (export.identity.index, pin.pin_id),
                PinEndpoint {
                    graph_index,
                    node_index: export.identity.index,
                    pin,
                },
            );
        }
    }

    graphs
        .into_iter()
        .map(|(graph_index, nodes)| graph_from_exports(report, graph_index, &nodes, &pin_by_id))
        .collect()
}

fn is_control_rig_editor_mirror(class_full: &str) -> bool {
    class_full
        .rsplit(['.', '/'])
        .next()
        .is_some_and(|simple| simple == "ControlRigGraphNode")
}

fn control_rig_editor_graph_indices(report: &DecodeReport<'_>) -> HashSet<i32> {
    report
        .exports
        .iter()
        .filter(|export| {
            export
                .identity
                .class
                .rsplit(['.', '/'])
                .next()
                .is_some_and(|simple| simple == "ControlRigGraph")
        })
        .map(|export| export.identity.index)
        .collect()
}

fn is_control_rig_editor_mirror_export(
    report: &DecodeReport<'_>,
    export: &DecodedExport,
    control_rig_editor_graphs: &HashSet<i32>,
) -> bool {
    if is_control_rig_editor_mirror(&export.identity.class) {
        return true;
    }
    report
        .package
        .exports
        .get((export.identity.index - 1).max(0) as usize)
        .is_some_and(|raw| control_rig_editor_graphs.contains(&raw.outer_index.0))
}

fn graph_from_exports(
    report: &DecodeReport<'_>,
    graph_index: i32,
    nodes: &[&DecodedExport],
    pin_by_id: &HashMap<(i32, Guid), PinEndpoint<'_>>,
) -> LogicGraph {
    let graph_name = positive_export(report, graph_index)
        .map(|export| export.identity.name.clone())
        .unwrap_or_else(|| "<unresolved_graph>".into());
    let mut edges = Vec::new();
    let mut seen_edges = HashSet::new();
    let mut cross_graph_links = HashSet::new();
    let mut unresolved_links = HashSet::new();
    for node in nodes {
        let Some(pins) = &node.pins else {
            continue;
        };
        for pin in pins {
            let current = PinEndpoint {
                graph_index,
                node_index: node.identity.index,
                pin,
            };
            for linked in &pin.linked_to {
                let Some(target) = pin_by_id.get(&(linked.node_index, linked.pin_id)).copied()
                else {
                    unresolved_links.insert((
                        current.node_index,
                        current.pin.pin_id,
                        linked.node_index,
                        linked.pin_id,
                    ));
                    continue;
                };
                if target.graph_index != graph_index {
                    cross_graph_links.insert(canonical_pair(current, target));
                    continue;
                }
                let (source, target) = orient_edge(current, target);
                let key = (
                    source.node_index,
                    source.pin.pin_id,
                    target.node_index,
                    target.pin.pin_id,
                );
                if !seen_edges.insert(key) {
                    continue;
                }
                edges.push(GraphEdge {
                    kind: if source.pin.category == "exec" || target.pin.category == "exec" {
                        EdgeKind::Exec
                    } else {
                        EdgeKind::Data
                    },
                    from: graph_endpoint(source),
                    to: graph_endpoint(target),
                });
            }
        }
    }

    LogicGraph {
        index: graph_index,
        name: graph_name,
        full_name: report.package.resolve_full_name(graph_index),
        nodes: nodes
            .iter()
            .map(|export| graph_node_from_export(report.package, export))
            .collect(),
        edges,
        excluded_cross_graph_links: cross_graph_links.len(),
        unresolved_links: unresolved_links.len(),
    }
}

fn graph_node_from_export(package: &Package, export: &DecodedExport) -> GraphNode {
    GraphNode {
        index: export.identity.index,
        name: export.identity.name.clone(),
        class: export.identity.class.clone(),
        member: export.member.as_ref().map(|member| MemberReference {
            name: member.name.clone(),
            parent: member.parent.clone(),
        }),
        pins: export
            .pins
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|pin| graph_pin_from_pin(package, pin))
            .collect(),
        user_defined_pins: export
            .user_defined_pins
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|pin| user_defined_pin_to_model(package, pin))
            .collect(),
    }
}

fn graph_pin_from_pin(package: &Package, pin: &Pin) -> GraphPin {
    let pin_type = PinType {
        category: pin.category.clone(),
        sub_category: pin.sub_category.clone(),
        sub_category_object: pin.sub_category_object,
        container_type: pin.container_type,
        value_type: pin.value_type.clone(),
        is_reference: pin.is_reference,
        is_weak_pointer: pin.is_weak_pointer,
        member_parent: pin.member_parent,
        member_name: pin.member_name.clone(),
        member_guid: pin.member_guid,
        is_const: pin.is_const,
        is_uobject_wrapper: pin.is_uobject_wrapper,
        serialize_as_single_precision_float: pin.serialize_as_single_precision_float,
    };
    GraphPin {
        pin_id: pin.pin_id.to_hex(),
        name: pin.name.clone(),
        friendly_name: pin.friendly_name.clone(),
        source_index: pin.source_index,
        tooltip: pin.tooltip.clone(),
        direction: pin_direction(pin.direction),
        pin_type: pin_type_to_model(package, &pin_type),
        default_value: (!pin.default_value.is_empty()).then(|| pin.default_value.clone()),
        autogenerated_default_value: (!pin.autogenerated_default_value.is_empty())
            .then(|| pin.autogenerated_default_value.clone()),
        default_object: (pin.default_object != 0)
            .then(|| package.resolve_object_ref(pin.default_object)),
        default_text: pin.default_text.clone(),
        linked_to: pin.linked_to.iter().map(pin_reference_to_model).collect(),
        sub_pins: pin.sub_pins.iter().map(pin_reference_to_model).collect(),
        parent_pin: pin.parent_pin.as_ref().map(pin_reference_to_model),
        reference_pass_through: pin
            .reference_pass_through
            .as_ref()
            .map(pin_reference_to_model),
        persistent_guid: pin
            .persistent_guid
            .filter(|guid| !guid.is_zero())
            .map(|guid| guid.to_hex()),
        editor_flags: pin.editor_flags.as_ref().map(|flags| GraphPinEditorFlags {
            hidden: flags.hidden,
            not_connectable: flags.not_connectable,
            default_value_read_only: flags.default_value_read_only,
            default_value_ignored: flags.default_value_ignored,
            advanced_view: flags.advanced_view,
            orphaned_pin: flags.orphaned_pin,
        }),
    }
}

fn pin_reference_to_model(reference: &crate::pin::PinRef) -> GraphPinReference {
    GraphPinReference {
        node_index: reference.node_index,
        pin_id: reference.pin_id.to_hex(),
    }
}

fn user_defined_pin_to_model(package: &Package, pin: &UserDefinedPin) -> UserDefinedGraphPin {
    UserDefinedGraphPin {
        name: pin.name.clone(),
        direction: pin_direction(pin.direction),
        pin_type: pin_type_to_model(package, &pin.pin_type),
        default_value: (!pin.default_value.is_empty()).then(|| pin.default_value.clone()),
    }
}

fn pin_type_to_model(package: &Package, pin_type: &PinType) -> GraphPinType {
    let member_reference = (pin_type.member_parent != 0
        || !pin_type.member_name.is_empty()
        || !pin_type.member_guid.is_zero())
    .then(|| MemberReference {
        name: pin_type.member_name.clone(),
        parent: (pin_type.member_parent != 0)
            .then(|| package.resolve_object_ref(pin_type.member_parent)),
    });
    GraphPinType {
        category: pin_type.category.clone(),
        sub_category: pin_type.sub_category.clone(),
        sub_category_object: (pin_type.sub_category_object != 0)
            .then(|| package.resolve_object_ref(pin_type.sub_category_object)),
        container: match pin_type.container_type {
            CONTAINER_TYPE_NONE => PinContainer::None,
            CONTAINER_TYPE_ARRAY => PinContainer::Array,
            CONTAINER_TYPE_SET => PinContainer::Set,
            CONTAINER_TYPE_MAP => PinContainer::Map,
            value => PinContainer::Unknown(value),
        },
        value_type: pin_type
            .value_type
            .as_ref()
            .map(|terminal| terminal_type_to_model(package, terminal)),
        is_reference: pin_type.is_reference,
        is_weak_pointer: pin_type.is_weak_pointer,
        is_const: pin_type.is_const,
        is_uobject_wrapper: pin_type.is_uobject_wrapper,
        serialize_as_single_precision_float: pin_type.serialize_as_single_precision_float,
        member_reference,
    }
}

fn terminal_type_to_model(package: &Package, terminal: &PinTerminalType) -> GraphTerminalType {
    GraphTerminalType {
        category: terminal.category.clone(),
        sub_category: terminal.sub_category.clone(),
        sub_category_object: (terminal.sub_category_object != 0)
            .then(|| package.resolve_object_ref(terminal.sub_category_object)),
        is_const: terminal.is_const,
        is_weak_pointer: terminal.is_weak_pointer,
        is_uobject_wrapper: terminal.is_uobject_wrapper,
    }
}

fn pin_direction(direction: u8) -> PinDirection {
    match direction {
        0 => PinDirection::Input,
        1 => PinDirection::Output,
        value => PinDirection::Unknown(value),
    }
}

fn orient_edge<'a>(
    left: PinEndpoint<'a>,
    right: PinEndpoint<'a>,
) -> (PinEndpoint<'a>, PinEndpoint<'a>) {
    match (
        pin_direction(left.pin.direction),
        pin_direction(right.pin.direction),
    ) {
        (PinDirection::Output, PinDirection::Input) => (left, right),
        (PinDirection::Input, PinDirection::Output) => (right, left),
        _ if endpoint_key(left) <= endpoint_key(right) => (left, right),
        _ => (right, left),
    }
}

fn canonical_pair(left: PinEndpoint<'_>, right: PinEndpoint<'_>) -> (i32, Guid, i32, Guid) {
    if endpoint_key(left) <= endpoint_key(right) {
        (
            left.node_index,
            left.pin.pin_id,
            right.node_index,
            right.pin.pin_id,
        )
    } else {
        (
            right.node_index,
            right.pin.pin_id,
            left.node_index,
            left.pin.pin_id,
        )
    }
}

fn endpoint_key(endpoint: PinEndpoint<'_>) -> (i32, [u32; 4]) {
    (endpoint.node_index, endpoint.pin.pin_id.0)
}

fn graph_endpoint(endpoint: PinEndpoint<'_>) -> GraphEndpoint {
    GraphEndpoint {
        node_index: endpoint.node_index,
        pin_id: endpoint.pin.pin_id.to_hex(),
    }
}

fn positive_export<'a>(report: &'a DecodeReport<'_>, index: i32) -> Option<&'a DecodedExport> {
    usize::try_from(index.checked_sub(1)?)
        .ok()
        .and_then(|index| report.exports.get(index))
}
