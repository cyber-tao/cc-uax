use cc_uax_core::{
    AnalysisCapability, AnalysisDiagnostic, AnalysisStatus, AssetAnalysis, CapabilityKind,
    DiagnosticSeverity, KnownOpaque, KnownOpaqueKind, LogicGraph, ParseCoverage, PcgGraph,
    RigVmGraph, StateTreeGraph,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilitySummary {
    pub kind: CapabilityKind,
    pub status: AnalysisStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphSummary {
    pub index: i32,
    pub name: String,
    pub full_name: String,
    pub nodes: usize,
    pub pins: usize,
    pub edges: usize,
    pub excluded_cross_graph_links: usize,
    pub unresolved_links: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RigVmGraphSummary {
    pub index: i32,
    pub name: String,
    pub full_name: String,
    pub nodes: usize,
    pub pins: usize,
    pub links: usize,
    pub unresolved_node_references: usize,
    pub unresolved_pin_references: usize,
    pub unresolved_link_references: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PcgGraphSummary {
    pub index: i32,
    pub name: String,
    pub full_name: String,
    pub nodes_array: usize,
    pub default_nodes: usize,
    pub base_node_exports: usize,
    pub nodes: usize,
    pub pins: usize,
    pub edges: usize,
    pub unresolved_node_references: usize,
    pub unresolved_pin_references: usize,
    pub unresolved_edge_references: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateTreeGraphSummary {
    pub index: i32,
    pub name: String,
    pub full_name: String,
    pub states: usize,
    pub tasks: usize,
    pub enter_conditions: usize,
    pub transitions: usize,
    pub transition_conditions: usize,
    pub child_links: usize,
    pub unresolved_state_references: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisDiagnosticSummary {
    pub errors: usize,
    pub warnings: usize,
    pub info: usize,
    pub codes: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownOpaqueSummary {
    pub total: usize,
    pub property_values: usize,
    pub post_property_tails: usize,
    pub metadata: usize,
    pub capabilities: usize,
    pub identities: Vec<KnownOpaqueIdentity>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownOpaqueIdentity {
    pub path: String,
    pub kind: KnownOpaqueKind,
    pub type_name: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetAnalysisSummary {
    pub status: AnalysisStatus,
    pub coverage: ParseCoverage,
    pub capabilities: Vec<CapabilitySummary>,
    pub graphs: Vec<GraphSummary>,
    pub rigvm_graphs: Vec<RigVmGraphSummary>,
    pub pcg_graphs: Vec<PcgGraphSummary>,
    pub state_tree_graphs: Vec<StateTreeGraphSummary>,
    pub diagnostics: AnalysisDiagnosticSummary,
    pub known_opaque: KnownOpaqueSummary,
}

impl AssetAnalysisSummary {
    pub(crate) fn from_analysis(analysis: &AssetAnalysis) -> Self {
        Self {
            status: analysis.status,
            coverage: analysis.coverage.clone(),
            capabilities: analysis
                .capabilities
                .iter()
                .map(CapabilitySummary::from_capability)
                .collect(),
            graphs: analysis
                .graphs
                .iter()
                .map(GraphSummary::from_graph)
                .collect(),
            rigvm_graphs: analysis
                .rigvm_graphs
                .iter()
                .map(RigVmGraphSummary::from_graph)
                .collect(),
            pcg_graphs: analysis
                .pcg_graphs
                .iter()
                .map(PcgGraphSummary::from_graph)
                .collect(),
            state_tree_graphs: analysis
                .state_tree_graphs
                .iter()
                .map(StateTreeGraphSummary::from_graph)
                .collect(),
            diagnostics: AnalysisDiagnosticSummary::from_diagnostics(&analysis.diagnostics),
            known_opaque: KnownOpaqueSummary::from_regions(&analysis.known_opaque),
        }
    }
}

impl CapabilitySummary {
    fn from_capability(capability: &AnalysisCapability) -> Self {
        Self {
            kind: capability.kind,
            status: capability.status,
        }
    }
}

impl GraphSummary {
    fn from_graph(graph: &LogicGraph) -> Self {
        Self {
            index: graph.index,
            name: graph.name.clone(),
            full_name: graph.full_name.clone(),
            nodes: graph.nodes.len(),
            pins: graph
                .nodes
                .iter()
                .map(|node| node.pins.len() + node.user_defined_pins.len())
                .sum(),
            edges: graph.edges.len(),
            excluded_cross_graph_links: graph.excluded_cross_graph_links,
            unresolved_links: graph.unresolved_links,
        }
    }
}

impl RigVmGraphSummary {
    fn from_graph(graph: &RigVmGraph) -> Self {
        Self {
            index: graph.index,
            name: graph.name.clone(),
            full_name: graph.full_name.clone(),
            nodes: graph.nodes.len(),
            pins: graph
                .nodes
                .iter()
                .map(|node| {
                    node.pins.iter().map(count_rigvm_pin_tree).sum::<usize>()
                        + node
                            .orphaned_pins
                            .iter()
                            .map(count_rigvm_pin_tree)
                            .sum::<usize>()
                })
                .sum(),
            links: graph.links.len(),
            unresolved_node_references: graph.unresolved_node_references,
            unresolved_pin_references: graph.unresolved_pin_references,
            unresolved_link_references: graph.unresolved_link_references,
        }
    }
}

impl PcgGraphSummary {
    fn from_graph(graph: &PcgGraph) -> Self {
        Self {
            index: graph.index,
            name: graph.name.clone(),
            full_name: graph.full_name.clone(),
            nodes_array: graph.nodes_array_count,
            default_nodes: graph.default_node_count,
            base_node_exports: graph.base_node_export_count,
            nodes: graph.nodes.len(),
            pins: graph.nodes.iter().map(|node| node.pins.len()).sum(),
            edges: graph.edges.len(),
            unresolved_node_references: graph.unresolved_node_references,
            unresolved_pin_references: graph.unresolved_pin_references,
            unresolved_edge_references: graph.unresolved_edge_references,
        }
    }
}

impl StateTreeGraphSummary {
    fn from_graph(graph: &StateTreeGraph) -> Self {
        Self {
            index: graph.index,
            name: graph.name.clone(),
            full_name: graph.full_name.clone(),
            states: graph.states.len(),
            tasks: graph.states.iter().map(|state| state.tasks.len()).sum(),
            enter_conditions: graph
                .states
                .iter()
                .map(|state| state.enter_conditions.len())
                .sum(),
            transitions: graph
                .states
                .iter()
                .map(|state| state.transitions.len())
                .sum(),
            transition_conditions: graph
                .states
                .iter()
                .flat_map(|state| &state.transitions)
                .map(|transition| transition.conditions.len())
                .sum(),
            child_links: graph
                .states
                .iter()
                .map(|state| state.child_indices.len())
                .sum(),
            unresolved_state_references: graph.unresolved_state_references,
        }
    }
}

fn count_rigvm_pin_tree(pin: &cc_uax_core::RigVmPin) -> usize {
    1 + pin.sub_pins.iter().map(count_rigvm_pin_tree).sum::<usize>()
}

impl AnalysisDiagnosticSummary {
    fn from_diagnostics(diagnostics: &[AnalysisDiagnostic]) -> Self {
        let mut summary = Self::default();
        for diagnostic in diagnostics {
            match diagnostic.severity {
                DiagnosticSeverity::Error => summary.errors += 1,
                DiagnosticSeverity::Warning => summary.warnings += 1,
                DiagnosticSeverity::Info => summary.info += 1,
            }
            *summary.codes.entry(diagnostic.code.clone()).or_default() += 1;
        }
        summary
    }
}

impl KnownOpaqueSummary {
    fn from_regions(regions: &[KnownOpaque]) -> Self {
        let mut summary = Self {
            total: regions.len(),
            identities: regions
                .iter()
                .map(|region| KnownOpaqueIdentity {
                    path: region.path.clone(),
                    kind: region.kind,
                    type_name: region.type_name.clone(),
                    reason: region.reason.clone(),
                })
                .collect(),
            ..Self::default()
        };
        for region in regions {
            match region.kind {
                KnownOpaqueKind::PropertyValue => summary.property_values += 1,
                KnownOpaqueKind::PostPropertyTail => summary.post_property_tails += 1,
                KnownOpaqueKind::Metadata => summary.metadata += 1,
                KnownOpaqueKind::Capability => summary.capabilities += 1,
            }
        }
        summary
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectAnalysisSummary {
    pub status: AnalysisStatus,
    pub assets: usize,
    pub complete_assets: usize,
    pub partial_assets: usize,
    pub unsupported_assets: usize,
    pub scan_failures: usize,
    pub coverage: ParseCoverage,
}

impl ProjectAnalysisSummary {
    pub(crate) fn aggregate<'a>(
        summaries: impl Iterator<Item = &'a AssetAnalysisSummary>,
        scan_failures: usize,
    ) -> Self {
        let mut aggregate = Self {
            status: AnalysisStatus::Complete,
            assets: 0,
            complete_assets: 0,
            partial_assets: 0,
            unsupported_assets: 0,
            scan_failures,
            coverage: empty_coverage(),
        };
        for summary in summaries {
            aggregate.assets += 1;
            match summary.status {
                AnalysisStatus::Complete => aggregate.complete_assets += 1,
                AnalysisStatus::Partial => aggregate.partial_assets += 1,
                AnalysisStatus::Unsupported => aggregate.unsupported_assets += 1,
            }
            add_coverage(&mut aggregate.coverage, &summary.coverage);
        }
        aggregate.status = if aggregate.scan_failures > 0 {
            AnalysisStatus::Partial
        } else if aggregate.assets == 0 || aggregate.complete_assets == aggregate.assets {
            AnalysisStatus::Complete
        } else if aggregate.unsupported_assets == aggregate.assets {
            AnalysisStatus::Unsupported
        } else {
            AnalysisStatus::Partial
        };
        aggregate
    }
}

fn empty_coverage() -> ParseCoverage {
    ParseCoverage {
        bytes_total: 0,
        exports_total: 0,
        exports_analyzed: 0,
        property_exports_total: 0,
        property_exports_complete: 0,
        properties_decoded: 0,
        graph_nodes_total: 0,
        graph_nodes_decoded: 0,
        pins_decoded: 0,
        graph_edges_decoded: 0,
        rigvm_graphs_total: 0,
        rigvm_graphs_decoded: 0,
        rigvm_nodes_total: 0,
        rigvm_nodes_decoded: 0,
        rigvm_pins_total: 0,
        rigvm_pins_decoded: 0,
        rigvm_links_total: 0,
        rigvm_links_decoded: 0,
        pcg_graphs_total: 0,
        pcg_graphs_decoded: 0,
        pcg_nodes_total: 0,
        pcg_nodes_decoded: 0,
        pcg_pins_total: 0,
        pcg_pins_decoded: 0,
        pcg_edges_total: 0,
        pcg_edges_decoded: 0,
        state_tree_graphs_total: 0,
        state_tree_graphs_decoded: 0,
        state_tree_states_total: 0,
        state_tree_states_decoded: 0,
        state_tree_tasks_total: 0,
        state_tree_tasks_decoded: 0,
        state_tree_conditions_total: 0,
        state_tree_conditions_decoded: 0,
        state_tree_transitions_total: 0,
        state_tree_transitions_decoded: 0,
        known_opaque_regions: 0,
        diagnostic_errors: 0,
        diagnostic_warnings: 0,
    }
}

fn add_coverage(total: &mut ParseCoverage, value: &ParseCoverage) {
    total.bytes_total = total.bytes_total.saturating_add(value.bytes_total);
    total.exports_total = total.exports_total.saturating_add(value.exports_total);
    total.exports_analyzed = total
        .exports_analyzed
        .saturating_add(value.exports_analyzed);
    total.property_exports_total = total
        .property_exports_total
        .saturating_add(value.property_exports_total);
    total.property_exports_complete = total
        .property_exports_complete
        .saturating_add(value.property_exports_complete);
    total.properties_decoded = total
        .properties_decoded
        .saturating_add(value.properties_decoded);
    total.graph_nodes_total = total
        .graph_nodes_total
        .saturating_add(value.graph_nodes_total);
    total.graph_nodes_decoded = total
        .graph_nodes_decoded
        .saturating_add(value.graph_nodes_decoded);
    total.pins_decoded = total.pins_decoded.saturating_add(value.pins_decoded);
    total.graph_edges_decoded = total
        .graph_edges_decoded
        .saturating_add(value.graph_edges_decoded);
    total.rigvm_graphs_total = total
        .rigvm_graphs_total
        .saturating_add(value.rigvm_graphs_total);
    total.rigvm_graphs_decoded = total
        .rigvm_graphs_decoded
        .saturating_add(value.rigvm_graphs_decoded);
    total.rigvm_nodes_total = total
        .rigvm_nodes_total
        .saturating_add(value.rigvm_nodes_total);
    total.rigvm_nodes_decoded = total
        .rigvm_nodes_decoded
        .saturating_add(value.rigvm_nodes_decoded);
    total.rigvm_pins_total = total
        .rigvm_pins_total
        .saturating_add(value.rigvm_pins_total);
    total.rigvm_pins_decoded = total
        .rigvm_pins_decoded
        .saturating_add(value.rigvm_pins_decoded);
    total.rigvm_links_total = total
        .rigvm_links_total
        .saturating_add(value.rigvm_links_total);
    total.rigvm_links_decoded = total
        .rigvm_links_decoded
        .saturating_add(value.rigvm_links_decoded);
    total.pcg_graphs_total = total
        .pcg_graphs_total
        .saturating_add(value.pcg_graphs_total);
    total.pcg_graphs_decoded = total
        .pcg_graphs_decoded
        .saturating_add(value.pcg_graphs_decoded);
    total.pcg_nodes_total = total.pcg_nodes_total.saturating_add(value.pcg_nodes_total);
    total.pcg_nodes_decoded = total
        .pcg_nodes_decoded
        .saturating_add(value.pcg_nodes_decoded);
    total.pcg_pins_total = total.pcg_pins_total.saturating_add(value.pcg_pins_total);
    total.pcg_pins_decoded = total
        .pcg_pins_decoded
        .saturating_add(value.pcg_pins_decoded);
    total.pcg_edges_total = total.pcg_edges_total.saturating_add(value.pcg_edges_total);
    total.pcg_edges_decoded = total
        .pcg_edges_decoded
        .saturating_add(value.pcg_edges_decoded);
    total.state_tree_graphs_total = total
        .state_tree_graphs_total
        .saturating_add(value.state_tree_graphs_total);
    total.state_tree_graphs_decoded = total
        .state_tree_graphs_decoded
        .saturating_add(value.state_tree_graphs_decoded);
    total.state_tree_states_total = total
        .state_tree_states_total
        .saturating_add(value.state_tree_states_total);
    total.state_tree_states_decoded = total
        .state_tree_states_decoded
        .saturating_add(value.state_tree_states_decoded);
    total.state_tree_tasks_total = total
        .state_tree_tasks_total
        .saturating_add(value.state_tree_tasks_total);
    total.state_tree_tasks_decoded = total
        .state_tree_tasks_decoded
        .saturating_add(value.state_tree_tasks_decoded);
    total.state_tree_conditions_total = total
        .state_tree_conditions_total
        .saturating_add(value.state_tree_conditions_total);
    total.state_tree_conditions_decoded = total
        .state_tree_conditions_decoded
        .saturating_add(value.state_tree_conditions_decoded);
    total.state_tree_transitions_total = total
        .state_tree_transitions_total
        .saturating_add(value.state_tree_transitions_total);
    total.state_tree_transitions_decoded = total
        .state_tree_transitions_decoded
        .saturating_add(value.state_tree_transitions_decoded);
    total.known_opaque_regions = total
        .known_opaque_regions
        .saturating_add(value.known_opaque_regions);
    total.diagnostic_errors = total
        .diagnostic_errors
        .saturating_add(value.diagnostic_errors);
    total.diagnostic_warnings = total
        .diagnostic_warnings
        .saturating_add(value.diagnostic_warnings);
}
