use crate::model::{AssetProperty, DecodedValue, MemberReference};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PcgGraph {
    pub index: i32,
    pub name: String,
    pub full_name: String,
    pub nodes_array_count: usize,
    pub default_node_count: usize,
    pub base_node_export_count: usize,
    pub nodes: Vec<PcgNode>,
    pub edges: Vec<PcgEdge>,
    pub unresolved_node_references: usize,
    pub unresolved_pin_references: usize,
    pub unresolved_edge_references: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PcgNode {
    pub index: i32,
    pub name: String,
    pub full_name: String,
    pub class: String,
    pub position_x: Option<i64>,
    pub position_y: Option<i64>,
    pub settings_index: Option<i32>,
    pub settings_path: Option<String>,
    pub pins: Vec<PcgPin>,
    pub properties: Vec<AssetProperty>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PcgPin {
    pub index: i32,
    pub name: String,
    pub full_name: String,
    pub node_index: i32,
    pub direction: PinDirection,
    pub label: Option<String>,
    pub allowed_types: Option<String>,
    pub status: Option<String>,
    pub tooltip: Option<String>,
    pub edge_indices: Vec<i32>,
    pub properties: Vec<AssetProperty>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PcgEdge {
    pub index: i32,
    pub name: String,
    pub source_node_index: i32,
    pub source_pin_index: i32,
    pub target_node_index: i32,
    pub target_pin_index: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateTreeGraph {
    pub index: i32,
    pub name: String,
    pub full_name: String,
    pub editor_data_index: Option<i32>,
    pub root_state_indices: Vec<i32>,
    pub states: Vec<StateTreeState>,
    pub unresolved_state_references: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateTreeState {
    pub index: i32,
    pub export_name: String,
    pub name: String,
    pub full_name: String,
    pub id: Option<String>,
    pub parent_index: Option<i32>,
    pub child_indices: Vec<i32>,
    pub state_type: Option<String>,
    pub selection_behavior: Option<String>,
    pub enabled: Option<bool>,
    pub tasks: Vec<StateTreeTask>,
    pub enter_conditions: Vec<StateTreeCondition>,
    pub transitions: Vec<StateTreeTransition>,
    pub properties: Vec<AssetProperty>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateTreeTask {
    pub id: Option<String>,
    pub name: Option<String>,
    pub type_name: Option<String>,
    pub instance_type_name: Option<String>,
    pub instance_object: Option<DecodedValue>,
    pub enabled: Option<bool>,
    pub node_properties: Vec<AssetProperty>,
    pub instance_properties: Vec<AssetProperty>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateTreeCondition {
    pub id: Option<String>,
    pub name: Option<String>,
    pub type_name: Option<String>,
    pub instance_type_name: Option<String>,
    pub instance_object: Option<DecodedValue>,
    pub enabled: Option<bool>,
    pub node_properties: Vec<AssetProperty>,
    pub instance_properties: Vec<AssetProperty>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateTreeTransition {
    pub id: Option<String>,
    pub trigger: Option<String>,
    pub priority: Option<String>,
    pub target_name: Option<String>,
    pub target_id: Option<String>,
    pub link_type: Option<String>,
    pub enabled: Option<bool>,
    pub delay_seconds: Option<f64>,
    pub delay_random_variance: Option<f64>,
    pub conditions: Vec<StateTreeCondition>,
    pub properties: Vec<AssetProperty>,
}

// ===== K2 / EdGraph =====

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
