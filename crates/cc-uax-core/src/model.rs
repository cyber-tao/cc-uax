use crate::graph_models::{LogicGraph, PcgGraph, RigVmGraph, StateTreeGraph};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const ASSET_ANALYSIS_SCHEMA_VERSION: u32 = 2;

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
pub struct ParseCoverage {
    pub bytes_total: u64,
    pub exports_total: usize,
    pub exports_analyzed: usize,
    pub property_exports_total: usize,
    pub property_exports_complete: usize,
    pub properties_decoded: usize,
    pub graph_nodes_total: usize,
    pub graph_nodes_decoded: usize,
    pub pins_decoded: usize,
    pub graph_edges_decoded: usize,
    pub rigvm_graphs_total: usize,
    pub rigvm_graphs_decoded: usize,
    pub rigvm_nodes_total: usize,
    pub rigvm_nodes_decoded: usize,
    pub rigvm_pins_total: usize,
    pub rigvm_pins_decoded: usize,
    pub rigvm_links_total: usize,
    pub rigvm_links_decoded: usize,
    pub pcg_graphs_total: usize,
    pub pcg_graphs_decoded: usize,
    pub pcg_nodes_total: usize,
    pub pcg_nodes_decoded: usize,
    pub pcg_pins_total: usize,
    pub pcg_pins_decoded: usize,
    pub pcg_edges_total: usize,
    pub pcg_edges_decoded: usize,
    pub state_tree_graphs_total: usize,
    pub state_tree_graphs_decoded: usize,
    pub state_tree_states_total: usize,
    pub state_tree_states_decoded: usize,
    pub state_tree_tasks_decoded: usize,
    pub state_tree_conditions_decoded: usize,
    pub state_tree_transitions_decoded: usize,
    pub known_opaque_regions: usize,
    pub diagnostic_errors: usize,
    pub diagnostic_warnings: usize,
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
    pub object_flags: u32,
    pub serial_offset: i64,
    pub serial_size: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script_serialization_start: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script_serialization_end: Option<i64>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnownOpaqueKind {
    PropertyValue,
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
