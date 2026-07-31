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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<PcgNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<PcgEdge>,
    #[serde(default, skip_serializing_if = "crate::model::is_zero_usize")]
    pub unresolved_node_references: usize,
    #[serde(default, skip_serializing_if = "crate::model::is_zero_usize")]
    pub unresolved_pin_references: usize,
    #[serde(default, skip_serializing_if = "crate::model::is_zero_usize")]
    pub unresolved_edge_references: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PcgNode {
    pub index: i32,
    pub name: String,
    pub full_name: String,
    pub class: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position_x: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position_y: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings_index: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings_path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pins: Vec<PcgPin>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PcgPin {
    pub index: i32,
    pub name: String,
    pub full_name: String,
    pub node_index: i32,
    pub direction: PinDirection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_types: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edge_indices: Vec<i32>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor_data_index: Option<i32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub root_state_indices: Vec<i32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub states: Vec<StateTreeState>,
    #[serde(default, skip_serializing_if = "crate::model::is_zero_usize")]
    pub unresolved_state_references: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateTreeState {
    pub index: i32,
    pub export_name: String,
    pub name: String,
    pub full_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_index: Option<i32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub child_indices: Vec<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_behavior: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tasks: Vec<StateTreeTask>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub enter_conditions: Vec<StateTreeCondition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transitions: Vec<StateTreeTransition>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateTreeTask {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_type_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_object: Option<DecodedValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub node_properties: Vec<AssetProperty>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instance_properties: Vec<AssetProperty>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateTreeCondition {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_type_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance_object: Option<DecodedValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub node_properties: Vec<AssetProperty>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instance_properties: Vec<AssetProperty>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateTreeTransition {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay_seconds: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay_random_variance: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conditions: Vec<StateTreeCondition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<AssetProperty>,
}

// ===== K2 / EdGraph =====

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogicGraph {
    pub index: i32,
    pub name: String,
    pub full_name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<GraphNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<GraphEdge>,
    #[serde(default, skip_serializing_if = "crate::model::is_zero_usize")]
    pub excluded_cross_graph_links: usize,
    #[serde(default, skip_serializing_if = "crate::model::is_zero_usize")]
    pub unresolved_links: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RigVmGraph {
    pub index: i32,
    pub name: String,
    pub full_name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<RigVmNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<RigVmLink>,
    #[serde(default, skip_serializing_if = "crate::model::is_zero_usize")]
    pub unresolved_node_references: usize,
    #[serde(default, skip_serializing_if = "crate::model::is_zero_usize")]
    pub unresolved_pin_references: usize,
    #[serde(default, skip_serializing_if = "crate::model::is_zero_usize")]
    pub unresolved_link_references: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RigVmNode {
    pub index: i32,
    pub name: String,
    pub path: String,
    pub class: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<RigVmVector2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<RigVmVector2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<RigVmLinearColor>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pins: Vec<RigVmPin>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub orphaned_pins: Vec<RigVmPin>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RigVmPin {
    pub index: i32,
    pub name: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub direction: RigVmPinDirection,
    #[serde(default, skip_serializing_if = "crate::model::is_false")]
    pub is_expanded: bool,
    #[serde(default, skip_serializing_if = "crate::model::is_false")]
    pub is_constant: bool,
    #[serde(default, skip_serializing_if = "crate::model::is_false")]
    pub is_dynamic_array: bool,
    #[serde(default, skip_serializing_if = "crate::model::is_false")]
    pub is_lazy: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpp_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpp_type_object: Option<DecodedValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpp_type_object_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_widget_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_defined_category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index_in_category: Option<i32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sub_pins: Vec<RigVmPin>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub injections: Vec<RigVmInjection>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RigVmInjection {
    pub index: i32,
    pub name: String,
    #[serde(default, skip_serializing_if = "crate::model::is_false")]
    pub injected_as_input: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_pin_index: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_pin_index: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<Box<RigVmNode>>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member: Option<MemberReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pins: Vec<GraphPin>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub user_defined_pins: Vec<UserDefinedGraphPin>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphPin {
    pub pin_id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub friendly_name: Option<DecodedValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_index: Option<i32>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tooltip: String,
    pub direction: PinDirection,
    pub pin_type: GraphPinType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autogenerated_default_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_object: Option<DecodedValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_text: Option<DecodedValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub linked_to: Vec<GraphPinReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sub_pins: Vec<GraphPinReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_pin: Option<GraphPinReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_pass_through: Option<GraphPinReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persistent_guid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editor_flags: Option<GraphPinEditorFlags>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphPinReference {
    pub node_index: i32,
    pub pin_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphPinEditorFlags {
    #[serde(default, skip_serializing_if = "crate::model::is_false")]
    pub hidden: bool,
    #[serde(default, skip_serializing_if = "crate::model::is_false")]
    pub not_connectable: bool,
    #[serde(default, skip_serializing_if = "crate::model::is_false")]
    pub default_value_read_only: bool,
    #[serde(default, skip_serializing_if = "crate::model::is_false")]
    pub default_value_ignored: bool,
    #[serde(default, skip_serializing_if = "crate::model::is_false")]
    pub advanced_view: bool,
    #[serde(default, skip_serializing_if = "crate::model::is_false")]
    pub orphaned_pin: bool,
}

impl GraphPinEditorFlags {
    pub(crate) fn is_empty(&self) -> bool {
        !(self.hidden
            || self.not_connectable
            || self.default_value_read_only
            || self.default_value_ignored
            || self.advanced_view
            || self.orphaned_pin)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserDefinedGraphPin {
    pub name: String,
    pub direction: PinDirection,
    pub pin_type: GraphPinType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphPinType {
    pub category: String,
    #[serde(default, skip_serializing_if = "crate::model::is_absent_name")]
    pub sub_category: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_category_object: Option<DecodedValue>,
    #[serde(default, skip_serializing_if = "is_container_none")]
    pub container: PinContainer,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_type: Option<GraphTerminalType>,
    #[serde(default, skip_serializing_if = "crate::model::is_false")]
    pub is_reference: bool,
    #[serde(default, skip_serializing_if = "crate::model::is_false")]
    pub is_weak_pointer: bool,
    #[serde(default, skip_serializing_if = "crate::model::is_false")]
    pub is_const: bool,
    #[serde(default, skip_serializing_if = "crate::model::is_false")]
    pub is_uobject_wrapper: bool,
    #[serde(default, skip_serializing_if = "crate::model::is_false")]
    pub serialize_as_single_precision_float: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member_reference: Option<MemberReference>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraphTerminalType {
    pub category: String,
    #[serde(default, skip_serializing_if = "crate::model::is_absent_name")]
    pub sub_category: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_category_object: Option<DecodedValue>,
    #[serde(default, skip_serializing_if = "crate::model::is_false")]
    pub is_const: bool,
    #[serde(default, skip_serializing_if = "crate::model::is_false")]
    pub is_weak_pointer: bool,
    #[serde(default, skip_serializing_if = "crate::model::is_false")]
    pub is_uobject_wrapper: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinDirection {
    Input,
    Output,
    Unknown(u8),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PinContainer {
    #[default]
    None,
    Array,
    Set,
    Map,
    Unknown(u8),
}

fn is_container_none(container: &PinContainer) -> bool {
    matches!(container, PinContainer::None)
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
