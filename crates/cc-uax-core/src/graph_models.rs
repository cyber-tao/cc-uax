use crate::model::{AssetProperty, DecodedValue, PinDirection};
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
