use super::typed::{
    array, boolean, float, nested_properties, nested_property, object, object_ref_index,
    object_ref_indices, object_ref_path, property, string,
};
use crate::graph_models::{
    StateTreeCondition, StateTreeGraph, StateTreeState, StateTreeTask, StateTreeTransition,
};
use crate::model::{AssetExport, AssetProperty, DecodedValue};
use std::collections::{BTreeMap, BTreeSet};

const STATE_TREE_CLASS: &str = "/Script/StateTreeModule.StateTree";
const STATE_TREE_EDITOR_DATA_CLASS: &str = "/Script/StateTreeEditorModule.StateTreeEditorData";
const STATE_TREE_STATE_CLASS: &str = "/Script/StateTreeEditorModule.StateTreeState";

pub(crate) struct StateTreeAdapterResult {
    pub(crate) graph_exports_total: usize,
    pub(crate) state_exports_total: usize,
    pub(crate) graphs: Vec<StateTreeGraph>,
}

pub(crate) fn build_state_tree_graphs(exports: &[AssetExport]) -> StateTreeAdapterResult {
    let by_index = exports
        .iter()
        .map(|export| (export.index, export))
        .collect::<BTreeMap<_, _>>();
    let graph_exports = exports
        .iter()
        .filter(|export| export.class == STATE_TREE_CLASS)
        .collect::<Vec<_>>();
    let state_exports_total = exports
        .iter()
        .filter(|export| export.class == STATE_TREE_STATE_CLASS)
        .count();
    let mut graphs = graph_exports
        .iter()
        .map(|export| build_graph(export, exports, &by_index))
        .collect::<Vec<_>>();
    graphs.sort_by_key(|graph| graph.index);

    StateTreeAdapterResult {
        graph_exports_total: graph_exports.len(),
        state_exports_total,
        graphs,
    }
}

fn build_graph(
    graph: &AssetExport,
    exports: &[AssetExport],
    by_index: &BTreeMap<i32, &AssetExport>,
) -> StateTreeGraph {
    let editor_data_index = property(graph, "EditorData").and_then(object_ref_index);
    let mut unresolved_state_references = usize::from(editor_data_index.is_none());
    let editor_data = editor_data_index
        .and_then(|index| by_index.get(&index).copied())
        .filter(|export| export.class == STATE_TREE_EDITOR_DATA_CLASS);
    if editor_data_index.is_some() && editor_data.is_none() {
        unresolved_state_references += 1;
    }

    let roots_value = editor_data.and_then(|export| property(export, "SubTrees"));
    let expected_roots = roots_value.and_then(array).map_or(0, <[DecodedValue]>::len);
    let root_state_indices = roots_value.map(object_ref_indices).unwrap_or_default();
    unresolved_state_references += expected_roots.saturating_sub(root_state_indices.len());

    let mut states = editor_data_index.map_or_else(Vec::new, |editor_index| {
        exports
            .iter()
            .filter(|export| {
                export.class == STATE_TREE_STATE_CLASS
                    && is_descendant_of(export, editor_index, by_index)
            })
            .map(build_state)
            .collect::<Vec<_>>()
    });
    states.sort_by_key(|state| state.index);
    let state_indices = states
        .iter()
        .map(|state| state.index)
        .collect::<BTreeSet<_>>();
    for root in &root_state_indices {
        if !state_indices.contains(root) {
            unresolved_state_references += 1;
        }
    }
    for state in &states {
        if state
            .parent_index
            .is_some_and(|index| !state_indices.contains(&index))
        {
            unresolved_state_references += 1;
        }
        unresolved_state_references += state
            .child_indices
            .iter()
            .filter(|index| !state_indices.contains(index))
            .count();
    }

    StateTreeGraph {
        index: graph.index,
        name: graph.name.clone(),
        full_name: graph.full_name.clone(),
        editor_data_index,
        root_state_indices,
        states,
        unresolved_state_references,
    }
}

fn is_descendant_of(
    export: &AssetExport,
    ancestor_index: i32,
    by_index: &BTreeMap<i32, &AssetExport>,
) -> bool {
    let mut current = export.outer_index;
    let mut visited = BTreeSet::new();
    while current > 0 && visited.insert(current) {
        if current == ancestor_index {
            return true;
        }
        current = by_index
            .get(&current)
            .map_or(0, |export| export.outer_index);
    }
    false
}

fn build_state(state: &AssetExport) -> StateTreeState {
    StateTreeState {
        index: state.index,
        export_name: state.name.clone(),
        name: property(state, "Name")
            .and_then(string)
            .unwrap_or(&state.name)
            .to_owned(),
        full_name: state.full_name.clone(),
        id: property(state, "ID").and_then(string).map(str::to_owned),
        parent_index: property(state, "Parent").and_then(object_ref_index),
        child_indices: property(state, "Children")
            .map(object_ref_indices)
            .unwrap_or_default(),
        state_type: property(state, "Type").and_then(string).map(str::to_owned),
        selection_behavior: property(state, "SelectionBehavior")
            .and_then(string)
            .map(str::to_owned),
        enabled: property(state, "bEnabled").and_then(boolean),
        tasks: property(state, "Tasks")
            .and_then(array)
            .into_iter()
            .flatten()
            .map(build_task)
            .collect(),
        enter_conditions: property(state, "EnterConditions")
            .and_then(array)
            .into_iter()
            .flatten()
            .map(build_condition)
            .collect(),
        transitions: property(state, "Transitions")
            .and_then(array)
            .into_iter()
            .flatten()
            .map(build_transition)
            .collect(),
        properties: state.properties.clone(),
    }
}

fn build_task(value: &DecodedValue) -> StateTreeTask {
    let inst = build_node_instance(value, "bTaskEnabled");
    StateTreeTask {
        id: inst.id,
        name: inst.name,
        type_name: inst.type_name,
        instance_type_name: inst.instance_type_name,
        instance_object: inst.instance_object,
        enabled: inst.enabled,
        node_properties: inst.node_properties,
        instance_properties: inst.instance_properties,
    }
}

fn build_condition(value: &DecodedValue) -> StateTreeCondition {
    let inst = build_node_instance(value, "bConditionEnabled");
    StateTreeCondition {
        id: inst.id,
        name: inst.name,
        type_name: inst.type_name,
        instance_type_name: inst.instance_type_name,
        instance_object: inst.instance_object,
        enabled: inst.enabled,
        node_properties: inst.node_properties,
        instance_properties: inst.instance_properties,
    }
}

struct NodeInstance {
    id: Option<String>,
    name: Option<String>,
    type_name: Option<String>,
    instance_type_name: Option<String>,
    instance_object: Option<DecodedValue>,
    enabled: Option<bool>,
    node_properties: Vec<AssetProperty>,
    instance_properties: Vec<AssetProperty>,
}

fn build_node_instance(value: &DecodedValue, primary_enabled_field: &str) -> NodeInstance {
    let node = nested_property(value, "Node");
    let instance = nested_property(value, "Instance");
    let node_properties = node.map(nested_properties).unwrap_or_default();
    let instance_properties = instance.map(nested_properties).unwrap_or_default();
    let enabled = asset_property(&node_properties, primary_enabled_field)
        .or_else(|| asset_property(&node_properties, "bEnabled"))
        .and_then(boolean);
    NodeInstance {
        id: nested_property(value, "ID")
            .and_then(string)
            .map(str::to_owned),
        name: asset_property(&node_properties, "Name")
            .and_then(string)
            .map(str::to_owned),
        type_name: node.and_then(instanced_struct_type),
        instance_type_name: instance.and_then(instanced_struct_type),
        instance_object: non_null(nested_property(value, "InstanceObject")),
        enabled,
        node_properties,
        instance_properties,
    }
}

fn build_transition(value: &DecodedValue) -> StateTreeTransition {
    let properties = nested_properties(value);
    let state_link = asset_property(&properties, "State");
    StateTreeTransition {
        id: asset_property(&properties, "ID")
            .and_then(string)
            .map(str::to_owned),
        trigger: asset_property(&properties, "Trigger")
            .and_then(string)
            .map(str::to_owned),
        priority: asset_property(&properties, "Priority")
            .and_then(string)
            .map(str::to_owned),
        target_name: state_link
            .and_then(|value| nested_property(value, "Name"))
            .and_then(string)
            .map(str::to_owned),
        target_id: state_link
            .and_then(|value| nested_property(value, "ID"))
            .and_then(string)
            .map(str::to_owned),
        link_type: state_link
            .and_then(|value| nested_property(value, "LinkType"))
            .and_then(string)
            .map(str::to_owned),
        enabled: asset_property(&properties, "bTransitionEnabled").and_then(boolean),
        delay_seconds: asset_property(&properties, "DelayDuration").and_then(float),
        delay_random_variance: asset_property(&properties, "DelayRandomVariance").and_then(float),
        conditions: asset_property(&properties, "Conditions")
            .and_then(array)
            .into_iter()
            .flatten()
            .map(build_condition)
            .collect(),
        properties,
    }
}

fn asset_property<'a>(properties: &'a [AssetProperty], name: &str) -> Option<&'a DecodedValue> {
    properties
        .iter()
        .find(|property| property.name == name)
        .map(|property| &property.value)
}

fn instanced_struct_type(value: &DecodedValue) -> Option<String> {
    object(value)
        .and_then(|value| value.get("script_struct"))
        .and_then(object_ref_path)
        .map(str::to_owned)
}

fn non_null(value: Option<&DecodedValue>) -> Option<DecodedValue> {
    value
        .filter(|value| !matches!(value, DecodedValue::Null))
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn editor_hierarchy_builds_tasks_conditions_and_transitions() {
        let task = editor_node(
            "/Script/StateTreeModule.StateTreeDelayTask",
            "Task",
            "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "bTaskEnabled",
        );
        let enter_condition = editor_node(
            "/Script/StateTreeModule.StateTreeCompareBoolCondition",
            "Enter",
            "BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBB",
            "bConditionEnabled",
        );
        let transition_condition = editor_node(
            "/Script/StateTreeModule.StateTreeCompareBoolCondition",
            "Transition",
            "CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC",
            "bConditionEnabled",
        );
        let transition = struct_value(
            "StateTreeTransition",
            vec![
                prop("ID", text_value("DDDDDDDDDDDDDDDDDDDDDDDDDDDDDDDD")),
                prop("Trigger", text_value("EStateTreeTransitionTrigger::OnTick")),
                prop(
                    "Priority",
                    text_value("EStateTreeTransitionPriority::Normal"),
                ),
                prop(
                    "State",
                    struct_value(
                        "StateTreeStateLink",
                        vec![
                            prop("Name", text_value("Root")),
                            prop("ID", text_value("EEEEEEEEEEEEEEEEEEEEEEEEEEEEEEEE")),
                            prop(
                                "LinkType",
                                text_value("EStateTreeTransitionType::GotoState"),
                            ),
                        ],
                    ),
                ),
                prop("bTransitionEnabled", DecodedValue::Bool(true)),
                prop("DelayDuration", DecodedValue::Float(0.25)),
                prop(
                    "Conditions",
                    DecodedValue::Array(vec![transition_condition]),
                ),
            ],
        );
        let exports = vec![
            export(
                1,
                "Tree",
                STATE_TREE_CLASS,
                0,
                vec![prop("EditorData", object_ref(2, "Tree.EditorData"))],
            ),
            export(
                2,
                "EditorData",
                STATE_TREE_EDITOR_DATA_CLASS,
                1,
                vec![prop("SubTrees", refs(&[(3, "Tree.EditorData.Root")]))],
            ),
            export(
                3,
                "State_0",
                STATE_TREE_STATE_CLASS,
                2,
                vec![
                    prop("Name", text_value("Root")),
                    prop("Children", refs(&[(4, "Tree.EditorData.Root.Child")])),
                    prop("Tasks", DecodedValue::Array(vec![task])),
                ],
            ),
            export(
                4,
                "State_1",
                STATE_TREE_STATE_CLASS,
                3,
                vec![
                    prop("Name", text_value("Child")),
                    prop("Parent", object_ref(3, "Tree.EditorData.Root")),
                    prop(
                        "EnterConditions",
                        DecodedValue::Array(vec![enter_condition]),
                    ),
                    prop("Transitions", DecodedValue::Array(vec![transition])),
                ],
            ),
        ];

        let result = build_state_tree_graphs(&exports);
        let graph = &result.graphs[0];
        assert_eq!(graph.root_state_indices, vec![3]);
        assert_eq!(graph.states.len(), 2);
        assert_eq!(graph.unresolved_state_references, 0);
        assert_eq!(
            graph
                .states
                .iter()
                .map(|state| state.tasks.len())
                .sum::<usize>(),
            1
        );
        assert_eq!(
            graph
                .states
                .iter()
                .map(|state| state.enter_conditions.len())
                .sum::<usize>(),
            1
        );
        let transition = &graph.states[1].transitions[0];
        assert_eq!(transition.target_name.as_deref(), Some("Root"));
        assert_eq!(transition.conditions.len(), 1);
        assert_eq!(transition.delay_seconds, Some(0.25));
    }

    fn editor_node(type_name: &str, name: &str, id: &str, enabled_name: &str) -> DecodedValue {
        struct_value(
            "StateTreeEditorNode",
            vec![
                prop(
                    "Node",
                    DecodedValue::Object(BTreeMap::from([
                        ("script_struct".into(), object_ref(-1, type_name)),
                        (
                            "properties".into(),
                            DecodedValue::Array(vec![
                                property_entry(prop("Name", text_value(name))),
                                property_entry(prop(enabled_name, DecodedValue::Bool(true))),
                            ]),
                        ),
                        ("serial_size".into(), DecodedValue::Integer(1)),
                    ])),
                ),
                prop(
                    "Instance",
                    DecodedValue::Object(BTreeMap::from([
                        (
                            "script_struct".into(),
                            object_ref(-2, &format!("{type_name}InstanceData")),
                        ),
                        ("properties".into(), DecodedValue::Array(Vec::new())),
                        ("serial_size".into(), DecodedValue::Integer(1)),
                    ])),
                ),
                prop("InstanceObject", DecodedValue::Null),
                prop("ID", text_value(id)),
            ],
        )
    }

    fn export(
        index: i32,
        name: &str,
        class: &str,
        outer_index: i32,
        properties: Vec<AssetProperty>,
    ) -> AssetExport {
        AssetExport {
            index,
            name: name.into(),
            class: class.into(),
            super_name: String::new(),
            template_name: String::new(),
            outer_index,
            outer_name: String::new(),
            full_name: format!("Tree.{name}"),
            is_asset: index == 1,
            object_flags: 0,
            serial_offset: 0,
            serial_size: 0,
            script_serialization_start: None,
            script_serialization_end: None,
            object_guid: None,
            property_status: None,
            properties,
            metadata: None,
            member: None,
        }
    }

    fn prop(name: &str, value: DecodedValue) -> AssetProperty {
        AssetProperty {
            name: name.into(),
            type_name: String::new(),
            array_index: 0,
            value,
            guid: None,
        }
    }

    fn object_ref(index: i32, path: &str) -> DecodedValue {
        DecodedValue::Object(BTreeMap::from([
            ("index".into(), DecodedValue::Integer(index.into())),
            ("ref".into(), text_value(path)),
        ]))
    }

    fn refs(values: &[(i32, &str)]) -> DecodedValue {
        DecodedValue::Array(
            values
                .iter()
                .map(|(index, path)| object_ref(*index, path))
                .collect(),
        )
    }

    fn struct_value(name: &str, properties: Vec<AssetProperty>) -> DecodedValue {
        DecodedValue::Object(BTreeMap::from([
            ("@struct".into(), text_value(name)),
            (
                "properties".into(),
                DecodedValue::Array(properties.into_iter().map(property_entry).collect()),
            ),
        ]))
    }

    fn property_entry(property: AssetProperty) -> DecodedValue {
        DecodedValue::Object(BTreeMap::from([
            ("name".into(), text_value(&property.name)),
            ("type".into(), text_value(&property.type_name)),
            ("value".into(), property.value),
        ]))
    }

    fn text_value(value: &str) -> DecodedValue {
        DecodedValue::String(value.into())
    }
}
