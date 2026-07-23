use crate::graph_models::{PcgGraph, StateTreeGraph};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const ASSET_ANALYSIS_SCHEMA_VERSION: u32 = 1;

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
    pub references: AssetReferences,
    pub imports: Vec<AssetImport>,
    pub exports: Vec<AssetExport>,
    pub graphs: Vec<LogicGraph>,
    pub rigvm_graphs: Vec<RigVmGraph>,
    pub pcg_graphs: Vec<PcgGraph>,
    pub state_tree_graphs: Vec<StateTreeGraph>,
    pub coverage: ParseCoverage,
    pub diagnostics: Vec<AnalysisDiagnostic>,
    pub capabilities: Vec<AnalysisCapability>,
    pub known_opaque: Vec<KnownOpaque>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    pub state_tree_tasks_total: usize,
    pub state_tree_tasks_decoded: usize,
    pub state_tree_conditions_total: usize,
    pub state_tree_conditions_decoded: usize,
    pub state_tree_transitions_total: usize,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetReferences {
    pub assets: Vec<String>,
    pub scripts: Vec<String>,
    pub soft: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetImport {
    pub index: i32,
    pub class_package: String,
    pub class: String,
    pub name: String,
    pub outer_index: i32,
    pub outer_name: String,
    pub package_name: Option<String>,
    pub full_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetExport {
    pub index: i32,
    pub name: String,
    pub class: String,
    pub super_name: String,
    pub template_name: String,
    pub outer_index: i32,
    pub outer_name: String,
    pub full_name: String,
    pub is_asset: bool,
    pub object_flags: u32,
    pub serial_offset: i64,
    pub serial_size: i64,
    pub script_serialization_start: Option<i64>,
    pub script_serialization_end: Option<i64>,
    pub object_guid: Option<String>,
    pub property_status: Option<PropertyDecodeStatus>,
    pub properties: Vec<AssetProperty>,
    pub metadata: Option<DecodedValue>,
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
    pub array_index: i32,
    pub value: DecodedValue,
    pub guid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemberReference {
    pub name: String,
    pub parent: Option<DecodedValue>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogicGraph {
    pub index: i32,
    pub name: String,
    pub full_name: String,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub excluded_cross_graph_links: usize,
    pub unresolved_links: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RigVmGraph {
    pub index: i32,
    pub name: String,
    pub full_name: String,
    pub nodes: Vec<RigVmNode>,
    pub links: Vec<RigVmLink>,
    pub unresolved_node_references: usize,
    pub unresolved_pin_references: usize,
    pub unresolved_link_references: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RigVmNode {
    pub index: i32,
    pub name: String,
    pub path: String,
    pub class: String,
    pub title: Option<String>,
    pub position: Option<RigVmVector2>,
    pub size: Option<RigVmVector2>,
    pub color: Option<RigVmLinearColor>,
    pub pins: Vec<RigVmPin>,
    pub orphaned_pins: Vec<RigVmPin>,
    pub properties: Vec<AssetProperty>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RigVmPin {
    pub index: i32,
    pub name: String,
    pub path: String,
    pub display_name: Option<String>,
    pub direction: RigVmPinDirection,
    pub is_expanded: bool,
    pub is_constant: bool,
    pub is_dynamic_array: bool,
    pub is_lazy: bool,
    pub cpp_type: Option<String>,
    pub cpp_type_object: Option<DecodedValue>,
    pub cpp_type_object_path: Option<String>,
    pub default_value: Option<String>,
    pub default_value_type: Option<String>,
    pub custom_widget_name: Option<String>,
    pub user_defined_category: Option<String>,
    pub index_in_category: Option<i32>,
    pub sub_pins: Vec<RigVmPin>,
    pub injections: Vec<RigVmInjection>,
    pub properties: Vec<AssetProperty>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RigVmInjection {
    pub index: i32,
    pub name: String,
    pub injected_as_input: bool,
    pub input_pin_index: Option<i32>,
    pub output_pin_index: Option<i32>,
    pub node: Option<Box<RigVmNode>>,
    pub properties: Vec<AssetProperty>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RigVmLink {
    pub index: i32,
    pub name: String,
    pub source_pin_path: String,
    pub target_pin_path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RigVmVector2 {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RigVmLinearColor {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RigVmPinDirection {
    Input,
    Output,
    Io,
    Visible,
    Hidden,
    Invalid,
    Unknown(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphNode {
    pub index: i32,
    pub name: String,
    pub class: String,
    pub member: Option<MemberReference>,
    pub pins: Vec<GraphPin>,
    pub user_defined_pins: Vec<UserDefinedGraphPin>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphPin {
    pub pin_id: String,
    pub name: String,
    pub friendly_name: Option<DecodedValue>,
    pub source_index: Option<i32>,
    pub tooltip: String,
    pub direction: PinDirection,
    pub pin_type: GraphPinType,
    pub default_value: Option<String>,
    pub autogenerated_default_value: Option<String>,
    pub default_object: Option<DecodedValue>,
    pub default_text: DecodedValue,
    pub linked_to: Vec<GraphPinReference>,
    pub sub_pins: Vec<GraphPinReference>,
    pub parent_pin: Option<GraphPinReference>,
    pub reference_pass_through: Option<GraphPinReference>,
    pub persistent_guid: Option<String>,
    pub editor_flags: Option<GraphPinEditorFlags>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphPinReference {
    pub node_index: i32,
    pub pin_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphPinEditorFlags {
    pub hidden: bool,
    pub not_connectable: bool,
    pub default_value_read_only: bool,
    pub default_value_ignored: bool,
    pub advanced_view: bool,
    pub orphaned_pin: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserDefinedGraphPin {
    pub name: String,
    pub direction: PinDirection,
    pub pin_type: GraphPinType,
    pub default_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphPinType {
    pub category: String,
    pub sub_category: String,
    pub sub_category_object: Option<DecodedValue>,
    pub container: PinContainer,
    pub value_type: Option<GraphTerminalType>,
    pub is_reference: bool,
    pub is_weak_pointer: bool,
    pub is_const: bool,
    pub is_uobject_wrapper: bool,
    pub serialize_as_single_precision_float: bool,
    pub member_reference: Option<MemberReference>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphTerminalType {
    pub category: String,
    pub sub_category: String,
    pub sub_category_object: Option<DecodedValue>,
    pub is_const: bool,
    pub is_weak_pointer: bool,
    pub is_uobject_wrapper: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum PinDirection {
    Input,
    Output,
    Unknown(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum PinContainer {
    None,
    Array,
    Set,
    Map,
    Unknown(u8),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct GraphEndpoint {
    pub node_index: i32,
    pub pin_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub kind: EdgeKind,
    pub from: GraphEndpoint,
    pub to: GraphEndpoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    Exec,
    Data,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisCapability {
    pub kind: CapabilityKind,
    pub status: AnalysisStatus,
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
    pub type_name: Option<String>,
    pub reason: String,
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
    pub offset: Option<u64>,
    pub details: Option<DecodedValue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}
