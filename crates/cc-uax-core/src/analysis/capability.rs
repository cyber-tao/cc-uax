//! Capability aggregation and the final status decision.
//!
//! A capability is the unit a consumer reads to decide whether it may trust a
//! kind of evidence, so each one states what it needed and, when incomplete, why.
//! determine_analysis_status is deliberately the only place a report's status is
//! decided.

use super::coverage::{
    GraphCoverage, PcgCoverage, PropertyCoverage, RigVmCoverage, StateTreeCoverage,
};
use super::{pcg, rigvm, state_tree};
use crate::decode::{DecodeReport, is_niagara_compiled_class, is_script_bytecode_class};
use crate::model::{
    AnalysisCapability, AnalysisStatus, CapabilityKind, KnownOpaque, KnownOpaqueKind,
};
use crate::package::Package;
use crate::version::ue5;

/// Grouped coverage/adapter inputs for capability aggregation, so the single
/// call site does not need a long positional argument list.
pub(super) struct CapabilityInputs<'a> {
    pub(super) wants_references: bool,
    pub(super) wants_properties: bool,
    pub(super) wants_logic: bool,
    pub(super) property_coverage: &'a PropertyCoverage,
    pub(super) property_partial: bool,
    pub(super) graph_coverage: &'a GraphCoverage,
    pub(super) graph_partial: bool,
    pub(super) rigvm_adapter: &'a rigvm::RigVmAdapterResult,
    pub(super) rigvm_coverage: RigVmCoverage,
    pub(super) state_tree_adapter: &'a state_tree::StateTreeAdapterResult,
    pub(super) state_tree_partial: bool,
    pub(super) state_tree_coverage: &'a StateTreeCoverage,
    pub(super) pcg_adapter: &'a pcg::PcgAdapterResult,
    pub(super) pcg_partial: bool,
    pub(super) pcg_coverage: &'a PcgCoverage,
}

pub(super) fn build_capabilities(
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
    // Compiled Blueprint and Niagara payloads are the same kind of gap as compiled
    // RigVM bytecode: the source-level graph decodes but the compiled form does
    // not. Without a named capability they were an anonymous export tail, so a
    // Niagara system could report `complete` while its compiled VM was missing.
    if wants_logic {
        let compiled = |predicate: fn(&str) -> bool| {
            report
                .exports
                .iter()
                .filter(|export| {
                    predicate(&export.identity.class)
                        && export
                            .post_property_tail
                            .as_ref()
                            .is_some_and(|tail| tail.size > 0)
                })
                .count()
        };
        let bytecode_exports = compiled(is_script_bytecode_class);
        if bytecode_exports > 0 {
            capabilities.push(AnalysisCapability {
                kind: CapabilityKind::BlueprintBytecode,
                status: AnalysisStatus::Unsupported,
                detail: Some(format!(
                    "compiled script bytecode on {bytecode_exports} export(s) is retained as known opaque data"
                )),
            });
            known_opaque.push(KnownOpaque {
                path: "/capabilities/blueprint_bytecode".into(),
                kind: KnownOpaqueKind::Capability,
                type_name: Some("ScriptBytecode".into()),
                reason: "compiled Blueprint bytecode semantics are not decoded".into(),
                byte_range: None,
            });
        }
        let niagara_exports = compiled(is_niagara_compiled_class);
        if niagara_exports > 0 {
            capabilities.push(AnalysisCapability {
                kind: CapabilityKind::NiagaraCompiled,
                status: AnalysisStatus::Unsupported,
                detail: Some(format!(
                    "compiled Niagara VM/GPU data on {niagara_exports} export(s) is retained as known opaque data"
                )),
            });
            known_opaque.push(KnownOpaque {
                path: "/capabilities/niagara_compiled".into(),
                kind: KnownOpaqueKind::Capability,
                type_name: Some("NiagaraCompiled".into()),
                reason: "compiled Niagara VM/GPU semantics are not decoded".into(),
                byte_range: None,
            });
        }
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
                    "{}/{} graphs and {}/{} editor states decoded, {} opaque PropertyBag region(s)",
                    state_tree_coverage.graphs_decoded,
                    state_tree_adapter.graph_exports_total,
                    state_tree_coverage.states_decoded,
                    state_tree_adapter.state_exports_total,
                    state_tree_adapter.known_opaque.len()
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

pub(super) fn determine_analysis_status(
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
