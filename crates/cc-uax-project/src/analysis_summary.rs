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

fn is_zero_u64(value: &u64) -> bool {
    *value == 0
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilitySummary {
    pub kind: CapabilityKind,
    pub status: AnalysisStatus,
    /// Why the capability is not complete. This is the only field that names the
    /// gap, so dropping it left a project report saying an asset was partial
    /// without saying what was missing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
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
    /// Per-state `SingleTask` nodes, counted apart from `Tasks` because a state
    /// using that layout has an empty `Tasks` array.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub single_tasks: usize,
    /// Tree-wide `UStateTreeEditorData::Evaluators`.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub evaluators: usize,
    /// Tree-wide `UStateTreeEditorData::GlobalTasks`.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub global_tasks: usize,
    pub enter_conditions: usize,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub considerations: usize,
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
    /// Total bytes across every opaque region in this asset. Equal to
    /// `coverage.opaque_bytes`, repeated here so `groups` sums back to a whole.
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub bytes: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<KnownOpaqueGroup>,
}

impl KnownOpaqueSummary {
    fn is_empty(&self) -> bool {
        self.total == 0
    }
}

/// Opaque regions of one asset grouped by what the region is and why it could not
/// be decoded, with the region count and byte total for each group.
///
/// A project report aggregates rather than listing every region: a single Niagara
/// asset can carry thousands of regions of a handful of distinct kinds, and the
/// per-region byte ranges and previews are what `cc-uax asset` and `--focus`
/// attach in full.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownOpaqueGroup {
    pub kind: KnownOpaqueKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
    pub reason: String,
    pub regions: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetAnalysisSummary {
    pub status: AnalysisStatus,
    /// `FileVersionUE5` of the package, or `None` when it was never parsed.
    ///
    /// Without this the project report said nothing about which UE versions a
    /// scan actually covered, even though the version gates are what the decoders
    /// branch on and the corpus harness treats that distribution as its key
    /// acceptance signal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_version_ue5: Option<i32>,
    /// Why this mapped package carries no decoded evidence at all: it is a real
    /// package that the parser deliberately does not target (see
    /// `cc_uax_core::PackageRejection::OutOfScope`). Absent for parsed packages,
    /// whose evidence lives in `capabilities` and `diagnostics`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unsupported_reason: Option<String>,
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
    /// Summary for a mapped package the parser deliberately does not target. The
    /// file is real evidence about the project, so it stays in the inventory as
    /// `unsupported` instead of being reduced to a scan failure.
    pub(crate) fn unsupported(reason: impl Into<String>) -> Self {
        Self {
            status: AnalysisStatus::Unsupported,
            // Nothing was parsed, so there is no version to report.
            file_version_ue5: None,
            unsupported_reason: Some(reason.into()),
            coverage: ParseCoverage::default(),
            capabilities: Vec::new(),
            graphs: Vec::new(),
            rigvm_graphs: Vec::new(),
            pcg_graphs: Vec::new(),
            state_tree_graphs: Vec::new(),
            diagnostics: AnalysisDiagnosticSummary::default(),
            known_opaque: KnownOpaqueSummary::default(),
        }
    }

    /// Whether this asset says what it is missing, through either a diagnostic
    /// code or a capability detail. A `partial` status that answers `false` here
    /// is a defect: the report names a gap it does not describe.
    pub(crate) fn explains_its_gap(&self) -> bool {
        !self.diagnostics.codes.is_empty()
            || self
                .capabilities
                .iter()
                .any(|capability| capability.detail.is_some())
    }

    pub(crate) fn from_analysis(analysis: &AssetAnalysis) -> Self {
        Self {
            status: analysis.status,
            file_version_ue5: Some(analysis.summary.file_version_ue5),
            unsupported_reason: None,
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
            detail: capability.detail.clone(),
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
            single_tasks: graph
                .states
                .iter()
                .filter(|state| state.single_task.is_some())
                .count(),
            evaluators: graph.evaluators.len(),
            global_tasks: graph.global_tasks.len(),
            enter_conditions: graph
                .states
                .iter()
                .map(|state| state.enter_conditions.len())
                .sum(),
            considerations: graph
                .states
                .iter()
                .map(|state| state.considerations.len())
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
            ..Self::default()
        };
        let mut grouped: BTreeMap<(KnownOpaqueKind, Option<String>, String), (usize, u64)> =
            BTreeMap::new();
        for region in regions {
            match region.kind {
                KnownOpaqueKind::PropertyValue => summary.property_values += 1,
                KnownOpaqueKind::PreScriptRegion => summary.pre_script_regions += 1,
                KnownOpaqueKind::PostPropertyTail => summary.post_property_tails += 1,
                KnownOpaqueKind::Metadata => summary.metadata += 1,
                KnownOpaqueKind::Capability => summary.capabilities += 1,
            }
            let bytes = region.byte_range.as_ref().map_or(0, |range| range.size);
            let entry = grouped
                .entry((region.kind, region.type_name.clone(), region.reason.clone()))
                .or_default();
            entry.0 += 1;
            entry.1 += bytes;
            summary.bytes += bytes;
        }
        summary.groups = grouped
            .into_iter()
            .map(
                |((kind, type_name, reason), (regions, bytes))| KnownOpaqueGroup {
                    kind,
                    type_name,
                    reason,
                    regions,
                    bytes,
                },
            )
            .collect();
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
    /// Partial assets carrying neither a diagnostic code nor a capability detail.
    ///
    /// Always `0` in a correct report: a partial status that names no gap tells a
    /// consumer nothing. Published here so that can be checked without walking a
    /// whole inventory.
    pub partial_assets_without_explanation: usize,
    /// Regions and bytes summed from the per-asset `known_opaque` groups.
    ///
    /// These must equal `coverage.known_opaque_regions` and
    /// `coverage.opaque_bytes`. Reporting both sides makes it verifiable that
    /// grouping did not lose a region, which is otherwise only checkable by
    /// re-summing tens of thousands of inventory entries.
    pub grouped_opaque_regions: usize,
    pub grouped_opaque_bytes: u64,
    /// How many parsed packages carried each `FileVersionUE5`, keyed by version.
    ///
    /// This is the only statement of which version gates a scan actually
    /// exercised; a report covering one version says nothing about the others,
    /// however complete it looks.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub file_versions: BTreeMap<i32, usize>,
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
            partial_assets_without_explanation: 0,
            grouped_opaque_regions: 0,
            grouped_opaque_bytes: 0,
            file_versions: BTreeMap::new(),
            coverage: ParseCoverage::default(),
        };
        for summary in summaries {
            aggregate.assets += 1;
            match summary.status {
                AnalysisStatus::Complete => aggregate.complete_assets += 1,
                AnalysisStatus::Partial => aggregate.partial_assets += 1,
                AnalysisStatus::Unsupported => aggregate.unsupported_assets += 1,
            }
            if summary.status == AnalysisStatus::Partial && !summary.explains_its_gap() {
                aggregate.partial_assets_without_explanation += 1;
            }
            for group in &summary.known_opaque.groups {
                aggregate.grouped_opaque_regions += group.regions;
                aggregate.grouped_opaque_bytes += group.bytes;
            }
            if let Some(version) = summary.file_version_ue5 {
                *aggregate.file_versions.entry(version).or_default() += 1;
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
