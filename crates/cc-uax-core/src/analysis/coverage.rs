//! Evidence accounting: how much of what each adapter was asked for actually
//! decoded, and the mirror policy that keeps a Control Rig from being counted
//! twice.
//!
//! Each *Coverage owns one completeness predicate. Keeping them here rather
//! than beside the orchestration means adding an adapter has one obvious place to
//! declare what "complete" means for it.

use super::{pcg, rigvm, state_tree};
use crate::decode::pins::is_graph_node_class;
use crate::decode::rigvm::{is_rigvm_graph_class, is_rigvm_link_class};
use crate::decode::{DecodeReport, DecodedExport};
use crate::graph_models::LogicGraph;
use crate::package::Package;
use crate::property::PropertyParseStatus;
use std::collections::HashSet;

pub(super) struct GraphCoverage {
    pub(super) nodes_total: usize,
    pub(super) nodes_decoded: usize,
    pub(super) pins_decoded: usize,
    pub(super) edges_decoded: usize,
}

impl GraphCoverage {
    pub(super) fn is_partial(&self, graphs: &[LogicGraph]) -> bool {
        self.nodes_decoded < self.nodes_total
            || graphs
                .iter()
                .any(|graph| graph.excluded_cross_graph_links > 0 || graph.unresolved_links > 0)
    }
}

pub(super) fn compute_graph_coverage(
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
    pub(super) suppress: bool,
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

    /// The single exclusion decision, for a class and its owning graph index.
    ///
    /// Assembly already knows the owning graph; coverage has to resolve it. That
    /// was the only difference between the two callers, and having them make the
    /// decision separately let the graph list and the node/pin totals disagree
    /// about what had been excluded.
    pub(crate) fn excludes(&self, class_full: &str, graph_index: i32) -> bool {
        self.suppress
            && (self.editor_graphs.contains(&graph_index)
                || is_control_rig_editor_mirror_node(class_full))
    }

    /// For an arbitrary export, resolving its owner from the export table first.
    fn excludes_export(&self, report: &DecodeReport<'_>, export: &DecodedExport) -> bool {
        let owner = report
            .package
            .exports
            .get((export.identity.index - 1).max(0) as usize)
            .map_or(0, |raw| raw.outer_index.0);
        self.excludes(&export.identity.class, owner)
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

pub(super) struct PropertyCoverage {
    pub(super) exports_total: usize,
    pub(super) exports_complete: usize,
    pub(super) exports_not_tagged: usize,
    pub(super) exports_failed: usize,
    pub(super) properties_decoded: usize,
}

impl PropertyCoverage {
    pub(super) fn is_partial(&self) -> bool {
        self.exports_complete < self.exports_total
    }
}

pub(super) fn compute_property_coverage(
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

pub(super) struct PcgCoverage {
    pub(super) graphs_decoded: usize,
    pub(super) nodes_decoded: usize,
    pub(super) nodes_total: usize,
    pub(super) pins_decoded: usize,
    pub(super) pins_total: usize,
    pub(super) edges_decoded: usize,
    pub(super) edges_total: usize,
}

impl PcgCoverage {
    pub(super) fn is_partial(&self, adapter: &pcg::PcgAdapterResult) -> bool {
        self.graphs_decoded < adapter.graph_exports_total
            || self.nodes_decoded < self.nodes_total
            || self.pins_decoded < self.pins_total
            || self.edges_decoded < self.edges_total
            || !adapter.known_opaque.is_empty()
    }
}

pub(super) fn compute_pcg_coverage(adapter: &pcg::PcgAdapterResult) -> PcgCoverage {
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

pub(super) struct StateTreeCoverage {
    pub(super) graphs_decoded: usize,
    pub(super) states_decoded: usize,
    pub(super) tasks_decoded: usize,
    pub(super) conditions_decoded: usize,
    pub(super) transitions_decoded: usize,
}

impl StateTreeCoverage {
    pub(super) fn is_partial(&self, adapter: &state_tree::StateTreeAdapterResult) -> bool {
        self.graphs_decoded < adapter.graph_exports_total
            || self.states_decoded < adapter.state_exports_total
            || adapter.states_incomplete > 0
            // StateTree parameters and node bindings live in PropertyBags, so an
            // opaque bag is missing semantics, exactly as it is for PCG.
            || !adapter.known_opaque.is_empty()
            || adapter
                .graphs
                .iter()
                .any(|graph| graph.unresolved_state_references > 0)
    }
}

pub(super) fn compute_state_tree_coverage(
    adapter: &state_tree::StateTreeAdapterResult,
) -> StateTreeCoverage {
    let graphs_decoded = adapter
        .graphs
        .iter()
        .filter(|graph| graph.editor_data_index.is_some())
        .count();
    let states_decoded = adapter.graphs.iter().map(|graph| graph.states.len()).sum();
    let states = || adapter.graphs.iter().flat_map(|graph| &graph.states);
    // Tree-wide evaluators/global tasks and per-state single tasks are logic too;
    // counting only `Tasks` reported a tree as having fewer nodes than it has.
    let tasks_decoded = states()
        .map(|state| state.tasks.len() + usize::from(state.single_task.is_some()))
        .sum::<usize>()
        + adapter
            .graphs
            .iter()
            .map(|graph| graph.evaluators.len() + graph.global_tasks.len())
            .sum::<usize>();
    // Transition conditions were decoded but never counted, so a tree with only
    // transition-level conditions reported zero.
    let conditions_decoded = states()
        .map(|state| {
            state.enter_conditions.len()
                + state.considerations.len()
                + state
                    .transitions
                    .iter()
                    .map(|transition| transition.conditions.len())
                    .sum::<usize>()
        })
        .sum::<usize>();
    let transitions_decoded = states().map(|state| state.transitions.len()).sum::<usize>();
    StateTreeCoverage {
        graphs_decoded,
        states_decoded,
        tasks_decoded,
        conditions_decoded,
        transitions_decoded,
    }
}

#[derive(Clone, Copy)]
pub(super) struct RigVmCoverage {
    pub(super) graphs_total: usize,
    pub(super) graphs_decoded: usize,
    pub(super) nodes_total: usize,
    pub(super) nodes_decoded: usize,
    pub(super) pins_total: usize,
    pub(super) pins_decoded: usize,
    pub(super) links_total: usize,
    pub(super) links_decoded: usize,
}

pub(super) fn compute_rigvm_coverage(adapter: &rigvm::RigVmAdapterResult) -> RigVmCoverage {
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
