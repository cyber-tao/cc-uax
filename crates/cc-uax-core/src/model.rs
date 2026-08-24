use crate::graph_models::{LogicGraph, PcgGraph, RigVmGraph, StateTreeGraph};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ops::AddAssign;

pub const ASSET_ANALYSIS_SCHEMA_VERSION: u32 = 5;

/// serde `skip_serializing_if` helper: drop `false` booleans from the rendered
/// report so only set flags are emitted.
pub(crate) fn is_false(value: &bool) -> bool {
    !*value
}

/// serde `skip_serializing_if` helper: drop an FName-derived string when it is
/// empty or the canonical UE null name `None`.
pub(crate) fn is_absent_name(value: &str) -> bool {
    value.is_empty() || value == "None"
}

/// serde `skip_serializing_if` helper: drop a zero `i32` (default array index).
pub(crate) fn is_zero_i32(value: &i32) -> bool {
    *value == 0
}

/// serde `skip_serializing_if` helper: drop a zero `usize` (default unresolved count).
pub(crate) fn is_zero_usize(value: &usize) -> bool {
    *value == 0
}

/// serde `skip_serializing_if` helper: drop a zero `u64` byte total.
pub(crate) fn is_zero_u64(value: &u64) -> bool {
    *value == 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisStatus {
    Complete,
    Partial,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetView {
    Summary,
    Logic,
    Properties,
    References,
    Full,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetAnalysis {
    pub schema_version: u32,
    pub view: AssetView,
    pub status: AnalysisStatus,
    pub summary: AssetSummary,
    #[serde(default, skip_serializing_if = "AssetReferences::is_empty")]
    pub references: AssetReferences,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imports: Vec<AssetImport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exports: Vec<AssetExport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graphs: Vec<LogicGraph>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rigvm_graphs: Vec<RigVmGraph>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pcg_graphs: Vec<PcgGraph>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub state_tree_graphs: Vec<StateTreeGraph>,
    pub coverage: ParseCoverage,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<AnalysisDiagnostic>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<AnalysisCapability>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub known_opaque: Vec<KnownOpaque>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ParseCoverage {
    pub bytes_total: u64,
    /// Sum of every analyzed export's `serial_size`; the denominator for byte
    /// conservation over export payloads.
    #[serde(skip_serializing_if = "is_zero_u64")]
    pub export_bytes_total: u64,
    pub exports_total: usize,
    pub exports_analyzed: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub property_exports_total: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub property_exports_complete: usize,
    /// Exports whose payload is not a tagged-property block at all, so nothing was
    /// decoded and the whole declared range stays opaque. Distinct from
    /// [`Self::property_exports_failed`]: this is a payload shape the decoder does
    /// not model, not a tagged block that broke.
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub property_exports_not_tagged: usize,
    /// Exports whose tagged-property block started decoding and then failed, so
    /// the properties before the failure are evidence and the rest is opaque.
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub property_exports_failed: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub properties_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub graph_nodes_total: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub graph_nodes_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub pins_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub graph_edges_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub rigvm_graphs_total: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub rigvm_graphs_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub rigvm_nodes_total: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub rigvm_nodes_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub rigvm_pins_total: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub rigvm_pins_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub rigvm_links_total: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub rigvm_links_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub pcg_graphs_total: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub pcg_graphs_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub pcg_nodes_total: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub pcg_nodes_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub pcg_pins_total: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub pcg_pins_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub pcg_edges_total: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub pcg_edges_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub state_tree_graphs_total: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub state_tree_graphs_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub state_tree_states_total: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub state_tree_states_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub state_tree_tasks_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub state_tree_conditions_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub state_tree_transitions_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub known_opaque_regions: usize,
    /// Total bytes covered by `known_opaque` regions that carry a byte range.
    #[serde(skip_serializing_if = "is_zero_u64")]
    pub opaque_bytes: u64,
    /// Export payload bytes that are neither decoded nor classified as opaque.
    /// Always a defect: a non-zero value means an export window was not fully
    /// accounted for.
    #[serde(skip_serializing_if = "is_zero_u64")]
    pub unclassified_bytes: u64,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub diagnostic_errors: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub diagnostic_warnings: usize,
}

impl AddAssign<&ParseCoverage> for ParseCoverage {
    /// Field-complete accumulation used to aggregate per-asset coverage into a
    /// project total. Every field must be summed here; the drift test in
    /// `cc-uax-project` walks the serialized map to catch any omission.
    fn add_assign(&mut self, other: &ParseCoverage) {
        let ParseCoverage {
            bytes_total,
            export_bytes_total,
            exports_total,
            exports_analyzed,
            property_exports_total,
            property_exports_complete,
            property_exports_not_tagged,
            property_exports_failed,
            properties_decoded,
            graph_nodes_total,
            graph_nodes_decoded,
            pins_decoded,
            graph_edges_decoded,
            rigvm_graphs_total,
            rigvm_graphs_decoded,
            rigvm_nodes_total,
            rigvm_nodes_decoded,
            rigvm_pins_total,
            rigvm_pins_decoded,
            rigvm_links_total,
            rigvm_links_decoded,
            pcg_graphs_total,
            pcg_graphs_decoded,
            pcg_nodes_total,
            pcg_nodes_decoded,
            pcg_pins_total,
            pcg_pins_decoded,
            pcg_edges_total,
            pcg_edges_decoded,
            state_tree_graphs_total,
            state_tree_graphs_decoded,
            state_tree_states_total,
            state_tree_states_decoded,
            state_tree_tasks_decoded,
            state_tree_conditions_decoded,
            state_tree_transitions_decoded,
            known_opaque_regions,
            opaque_bytes,
            unclassified_bytes,
            diagnostic_errors,
            diagnostic_warnings,
        } = other;
        self.bytes_total = self.bytes_total.saturating_add(*bytes_total);
        self.export_bytes_total = self.export_bytes_total.saturating_add(*export_bytes_total);
        self.exports_total = self.exports_total.saturating_add(*exports_total);
        self.exports_analyzed = self.exports_analyzed.saturating_add(*exports_analyzed);
        self.property_exports_total = self
            .property_exports_total
            .saturating_add(*property_exports_total);
        self.property_exports_complete = self
            .property_exports_complete
            .saturating_add(*property_exports_complete);
        self.property_exports_not_tagged = self
            .property_exports_not_tagged
            .saturating_add(*property_exports_not_tagged);
        self.property_exports_failed = self
            .property_exports_failed
            .saturating_add(*property_exports_failed);
        self.properties_decoded = self.properties_decoded.saturating_add(*properties_decoded);
        self.graph_nodes_total = self.graph_nodes_total.saturating_add(*graph_nodes_total);
        self.graph_nodes_decoded = self
            .graph_nodes_decoded
            .saturating_add(*graph_nodes_decoded);
        self.pins_decoded = self.pins_decoded.saturating_add(*pins_decoded);
        self.graph_edges_decoded = self
            .graph_edges_decoded
            .saturating_add(*graph_edges_decoded);
        self.rigvm_graphs_total = self.rigvm_graphs_total.saturating_add(*rigvm_graphs_total);
        self.rigvm_graphs_decoded = self
            .rigvm_graphs_decoded
            .saturating_add(*rigvm_graphs_decoded);
        self.rigvm_nodes_total = self.rigvm_nodes_total.saturating_add(*rigvm_nodes_total);
        self.rigvm_nodes_decoded = self
            .rigvm_nodes_decoded
            .saturating_add(*rigvm_nodes_decoded);
        self.rigvm_pins_total = self.rigvm_pins_total.saturating_add(*rigvm_pins_total);
        self.rigvm_pins_decoded = self.rigvm_pins_decoded.saturating_add(*rigvm_pins_decoded);
        self.rigvm_links_total = self.rigvm_links_total.saturating_add(*rigvm_links_total);
        self.rigvm_links_decoded = self
            .rigvm_links_decoded
            .saturating_add(*rigvm_links_decoded);
        self.pcg_graphs_total = self.pcg_graphs_total.saturating_add(*pcg_graphs_total);
        self.pcg_graphs_decoded = self.pcg_graphs_decoded.saturating_add(*pcg_graphs_decoded);
        self.pcg_nodes_total = self.pcg_nodes_total.saturating_add(*pcg_nodes_total);
        self.pcg_nodes_decoded = self.pcg_nodes_decoded.saturating_add(*pcg_nodes_decoded);
        self.pcg_pins_total = self.pcg_pins_total.saturating_add(*pcg_pins_total);
        self.pcg_pins_decoded = self.pcg_pins_decoded.saturating_add(*pcg_pins_decoded);
        self.pcg_edges_total = self.pcg_edges_total.saturating_add(*pcg_edges_total);
        self.pcg_edges_decoded = self.pcg_edges_decoded.saturating_add(*pcg_edges_decoded);
        self.state_tree_graphs_total = self
            .state_tree_graphs_total
            .saturating_add(*state_tree_graphs_total);
        self.state_tree_graphs_decoded = self
            .state_tree_graphs_decoded
            .saturating_add(*state_tree_graphs_decoded);
        self.state_tree_states_total = self
            .state_tree_states_total
            .saturating_add(*state_tree_states_total);
        self.state_tree_states_decoded = self
            .state_tree_states_decoded
            .saturating_add(*state_tree_states_decoded);
        self.state_tree_tasks_decoded = self
            .state_tree_tasks_decoded
            .saturating_add(*state_tree_tasks_decoded);
        self.state_tree_conditions_decoded = self
            .state_tree_conditions_decoded
            .saturating_add(*state_tree_conditions_decoded);
        self.state_tree_transitions_decoded = self
            .state_tree_transitions_decoded
            .saturating_add(*state_tree_transitions_decoded);
        self.known_opaque_regions = self
            .known_opaque_regions
            .saturating_add(*known_opaque_regions);
        self.opaque_bytes = self.opaque_bytes.saturating_add(*opaque_bytes);
        self.unclassified_bytes = self.unclassified_bytes.saturating_add(*unclassified_bytes);
        self.diagnostic_errors = self.diagnostic_errors.saturating_add(*diagnostic_errors);
        self.diagnostic_warnings = self
            .diagnostic_warnings
            .saturating_add(*diagnostic_warnings);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DecodedValue {
    Null,
    Bool(bool),
    Integer(i64),
    Unsigned(u64),
    Float(f64),
    String(String),
    Array(Vec<DecodedValue>),
    Object(BTreeMap<String, DecodedValue>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetSummary {
    pub package_name: String,
    pub tag: u32,
    pub legacy_file_version: i32,
    pub file_version_ue4: i32,
    pub file_version_ue5: i32,
    pub file_version_licensee: i32,
    pub package_flags: u32,
    pub filter_editor_only: bool,
    pub total_header_size: i32,
    pub bulk_data_start_offset: i64,
    pub name_count: i32,
    pub import_count: i32,
    pub export_count: i32,
    pub saved_by_engine_version: String,
    pub compatible_engine_version: String,
    pub custom_versions: Vec<CustomVersionInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomVersionInfo {
    pub guid: String,
    pub version: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetReferences {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assets: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub soft: Vec<String>,
}

impl AssetReferences {
    fn is_empty(&self) -> bool {
        self.assets.is_empty() && self.scripts.is_empty() && self.soft.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetImport {
    pub index: i32,
    pub class_package: String,
    pub class: String,
    pub name: String,
    pub outer_index: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub outer_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_name: Option<String>,
    pub full_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetExport {
    pub index: i32,
    pub name: String,
    pub class: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub super_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub template_name: String,
    pub outer_index: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub outer_name: String,
    pub full_name: String,
    #[serde(default, skip_serializing_if = "crate::model::is_false")]
    pub is_asset: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serialization: Option<ExportSerialization>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_guid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub property_status: Option<PropertyDecodeStatus>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<AssetProperty>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<DecodedValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member: Option<MemberReference>,
}

/// Byte-level export placement, emitted only for the `full` view. Focused views
/// (summary/logic/properties/references) omit this bookkeeping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportSerialization {
    pub object_flags: u32,
    pub serial_offset: i64,
    pub serial_size: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script_serialization_start: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script_serialization_end: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropertyDecodeStatus {
    Complete,
    Empty,
    NonTaggedPayload,
    FailedAfterEntries,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetProperty {
    pub name: String,
    pub type_name: String,
    #[serde(default, skip_serializing_if = "crate::model::is_zero_i32")]
    pub array_index: i32,
    pub value: DecodedValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemberReference {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<DecodedValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisCapability {
    pub kind: CapabilityKind,
    pub status: AnalysisStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityKind {
    PackageTables,
    ReferenceTables,
    TaggedProperties,
    EdGraphLogic,
    RigVmModel,
    RigVmBytecode,
    RigHierarchy,
    StateTreeSemantics,
    PcgSemantics,
    PackageVersion,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownOpaque {
    pub path: String,
    pub kind: KnownOpaqueKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_range: Option<OpaqueByteRange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnownOpaqueKind {
    PropertyValue,
    PreScriptRegion,
    PostPropertyTail,
    Metadata,
    Capability,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpaqueByteRange {
    pub start: u64,
    pub end: u64,
    pub size: u64,
    pub preview: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalysisDiagnostic {
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub path: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<DecodedValue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}
