use cc_uax_core::{
    AnalysisCapability, AnalysisDiagnostic, AnalysisStatus, AssetAnalysis, CapabilityKind,
    DiagnosticSeverity, KnownOpaque, KnownOpaqueKind, LogicGraph, ParseCoverage, PcgGraph,
    RigVmGraph, StateTreeGraph,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// serde `skip_serializing_if` helper: drop zero counts from per-asset summaries
/// so a project report only carries the non-zero accounting for each asset.
fn is_zero(value: &usize) -> bool {
    *value == 0
}

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
    #[serde(default, skip_serializing_if = "is_zero")]
    pub excluded_cross_graph_links: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
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
    #[serde(default, skip_serializing_if = "is_zero")]
    pub unresolved_node_references: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub unresolved_pin_references: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
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
    #[serde(default, skip_serializing_if = "is_zero")]
    pub unresolved_node_references: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub unresolved_pin_references: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
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
    #[serde(default, skip_serializing_if = "is_zero")]
    pub unresolved_state_references: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisDiagnosticSummary {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub errors: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub warnings: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub info: usize,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub codes: BTreeMap<String, usize>,
}

impl AnalysisDiagnosticSummary {
    fn is_empty(&self) -> bool {
        self.errors == 0 && self.warnings == 0 && self.info == 0 && self.codes.is_empty()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownOpaqueSummary {
    #[serde(default, skip_serializing_if = "is_zero")]
    pub total: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub property_values: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub pre_script_regions: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub post_property_tails: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub metadata: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub capabilities: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub identities: Vec<KnownOpaqueIdentity>,
}

impl KnownOpaqueSummary {
    fn is_empty(&self) -> bool {
        self.total == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownOpaqueIdentity {
    pub path: String,
    pub kind: KnownOpaqueKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetAnalysisSummary {
    pub status: AnalysisStatus,
    pub coverage: ParseCoverage,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<CapabilitySummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graphs: Vec<GraphSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rigvm_graphs: Vec<RigVmGraphSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pcg_graphs: Vec<PcgGraphSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub state_tree_graphs: Vec<StateTreeGraphSummary>,
    #[serde(default, skip_serializing_if = "AnalysisDiagnosticSummary::is_empty")]
    pub diagnostics: AnalysisDiagnosticSummary,
    #[serde(default, skip_serializing_if = "KnownOpaqueSummary::is_empty")]
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
                KnownOpaqueKind::PreScriptRegion => summary.pre_script_regions += 1,
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
            coverage: ParseCoverage::default(),
        };
        for summary in summaries {
            aggregate.assets += 1;
            match summary.status {
                AnalysisStatus::Complete => aggregate.complete_assets += 1,
                AnalysisStatus::Partial => aggregate.partial_assets += 1,
                AnalysisStatus::Unsupported => aggregate.unsupported_assets += 1,
            }
            aggregate.coverage += &summary.coverage;
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
