use super::typed::{
    array, integer, nested_properties, nested_property, object, object_ref_index,
    object_ref_indices, object_ref_path, property, string, text,
};
use crate::graph_models::{PcgEdge, PcgGraph, PcgNode, PcgPin};
use crate::model::{
    AssetExport, DecodedValue, KnownOpaque, KnownOpaqueKind, OpaqueByteRange, PinDirection,
};
use std::collections::{BTreeMap, BTreeSet};

const PCG_GRAPH_CLASS: &str = "/Script/PCG.PCGGraph";
const PCG_NODE_CLASS: &str = "/Script/PCG.PCGNode";
const PCG_PIN_CLASS: &str = "/Script/PCG.PCGPin";
const PCG_EDGE_CLASS: &str = "/Script/PCG.PCGEdge";
const PROPERTY_BAG_TYPE: &str = "InstancedPropertyBag";

pub(crate) struct PcgAdapterResult {
    pub(crate) graph_exports_total: usize,
    pub(crate) graphs: Vec<PcgGraph>,
    pub(crate) known_opaque: Vec<KnownOpaque>,
}

pub(crate) fn build_pcg_graphs(exports: &[AssetExport]) -> PcgAdapterResult {
    let by_index = exports
        .iter()
        .map(|export| (export.index, export))
        .collect::<BTreeMap<_, _>>();
    let graph_exports = exports
        .iter()
        .filter(|export| export.class == PCG_GRAPH_CLASS)
        .collect::<Vec<_>>();
    let mut graphs = graph_exports
        .iter()
        .map(|export| build_graph(export, &by_index))
        .collect::<Vec<_>>();
    graphs.sort_by_key(|graph| graph.index);

    let known_opaque = if graph_exports.is_empty() {
        Vec::new()
    } else {
        collect_property_bag_gaps(exports)
    };

    PcgAdapterResult {
        graph_exports_total: graph_exports.len(),
        graphs,
        known_opaque,
    }
}

fn build_graph(graph: &AssetExport, by_index: &BTreeMap<i32, &AssetExport>) -> PcgGraph {
    let nodes_value = property(graph, "Nodes");
    let nodes_array_count = nodes_value.and_then(array).map_or(0, <[DecodedValue]>::len);
    let node_array_refs = nodes_value.map(object_ref_indices).unwrap_or_default();
    let input_node = property(graph, "InputNode").and_then(object_ref_index);
    let output_node = property(graph, "OutputNode").and_then(object_ref_index);
    let default_node_count = usize::from(input_node.is_some()) + usize::from(output_node.is_some());

    let mut node_refs = Vec::new();
    let mut seen_nodes = BTreeSet::new();
    for index in input_node
        .into_iter()
        .chain(node_array_refs.iter().copied())
        .chain(output_node)
    {
        if seen_nodes.insert(index) {
            node_refs.push(index);
        }
    }

    let expected_node_references = nodes_array_count + default_node_count;
    let mut unresolved_node_references = expected_node_references.saturating_sub(node_refs.len());
    let mut unresolved_pin_references = 0;
    let mut unresolved_edge_references = 0;
    let mut edge_refs = BTreeSet::new();
    let mut nodes = Vec::new();

    for node_index in node_refs {
        let Some(node_export) = by_index.get(&node_index).copied() else {
            unresolved_node_references += 1;
            continue;
        };
        let (node, missing_pins, node_edge_refs, missing_edges) = build_node(node_export, by_index);
        unresolved_pin_references += missing_pins;
        unresolved_edge_references += missing_edges;
        edge_refs.extend(node_edge_refs);
        nodes.push(node);
    }

    let pin_to_node = nodes
        .iter()
        .flat_map(|node| node.pins.iter().map(move |pin| (pin.index, node.index)))
        .collect::<BTreeMap<_, _>>();
    let mut edges = Vec::new();
    for edge_index in edge_refs {
        let Some(edge_export) = by_index.get(&edge_index).copied() else {
            unresolved_edge_references += 1;
            continue;
        };
        if edge_export.class != PCG_EDGE_CLASS {
            unresolved_edge_references += 1;
            continue;
        }
        let Some(source_pin_index) = property(edge_export, "InputPin").and_then(object_ref_index)
        else {
            unresolved_edge_references += 1;
            continue;
        };
        let Some(target_pin_index) = property(edge_export, "OutputPin").and_then(object_ref_index)
        else {
            unresolved_edge_references += 1;
            continue;
        };
        let (Some(source_node_index), Some(target_node_index)) = (
            pin_to_node.get(&source_pin_index).copied(),
            pin_to_node.get(&target_pin_index).copied(),
        ) else {
            unresolved_edge_references += 1;
            continue;
        };

        // UPCGEdge serializes the producer as InputPin and the consumer as OutputPin.
        // Preserve that engine-defined direction instead of inferring it from export order.
        edges.push(PcgEdge {
            index: edge_export.index,
            name: edge_export.name.clone(),
            source_node_index,
            source_pin_index,
            target_node_index,
            target_pin_index,
        });
    }
    edges.sort_by_key(|edge| edge.index);

    PcgGraph {
        index: graph.index,
        name: graph.name.clone(),
        full_name: graph.full_name.clone(),
        nodes_array_count,
        default_node_count,
        base_node_export_count: nodes
            .iter()
            .filter(|node| node.class == PCG_NODE_CLASS)
            .count(),
        nodes,
        edges,
        unresolved_node_references,
        unresolved_pin_references,
        unresolved_edge_references,
    }
}

fn build_node(
    node: &AssetExport,
    by_index: &BTreeMap<i32, &AssetExport>,
) -> (PcgNode, usize, BTreeSet<i32>, usize) {
    let input_value = property(node, "InputPins");
    let output_value = property(node, "OutputPins");
    let expected_pins = input_value.and_then(array).map_or(0, <[DecodedValue]>::len)
        + output_value
            .and_then(array)
            .map_or(0, <[DecodedValue]>::len);
    let input_refs = input_value.map(object_ref_indices).unwrap_or_default();
    let output_refs = output_value.map(object_ref_indices).unwrap_or_default();
    let resolved_ref_count = input_refs.len() + output_refs.len();
    let mut unresolved_pin_references = expected_pins.saturating_sub(resolved_ref_count);
    let mut unresolved_edge_references = 0;
    let mut edge_refs = BTreeSet::new();
    let mut pins = Vec::new();

    for (pin_index, direction) in input_refs
        .into_iter()
        .map(|index| (index, PinDirection::Input))
        .chain(
            output_refs
                .into_iter()
                .map(|index| (index, PinDirection::Output)),
        )
    {
        let Some(pin_export) = by_index.get(&pin_index).copied() else {
            unresolved_pin_references += 1;
            continue;
        };
        if pin_export.class != PCG_PIN_CLASS {
            unresolved_pin_references += 1;
            continue;
        }
        let (pin, pin_edges, missing_edges) = build_pin(pin_export, node.index, direction);
        edge_refs.extend(pin_edges);
        unresolved_edge_references += missing_edges;
        pins.push(pin);
    }

    let settings = property(node, "SettingsInterface");
    (
        PcgNode {
            index: node.index,
            name: node.name.clone(),
            full_name: node.full_name.clone(),
            class: node.class.clone(),
            position_x: property(node, "PositionX").and_then(integer),
            position_y: property(node, "PositionY").and_then(integer),
            settings_index: settings.and_then(object_ref_index),
            settings_path: settings.and_then(object_ref_path).map(str::to_owned),
            pins,
            properties: node.properties.clone(),
        },
        unresolved_pin_references,
        edge_refs,
        unresolved_edge_references,
    )
}

fn build_pin(
    pin: &AssetExport,
    owning_node_index: i32,
    direction: PinDirection,
) -> (PcgPin, Vec<i32>, usize) {
    let node_index = property(pin, "Node")
        .and_then(object_ref_index)
        .unwrap_or(owning_node_index);
    let edges_value = property(pin, "Edges");
    let expected_edges = edges_value.and_then(array).map_or(0, <[DecodedValue]>::len);
    let edge_indices = edges_value.map(object_ref_indices).unwrap_or_default();
    let missing_edges = expected_edges.saturating_sub(edge_indices.len());
    let pin_properties = property(pin, "Properties");
    let label = pin_properties
        .and_then(|value| nested_property(value, "Label"))
        .and_then(string)
        .map(str::to_owned);
    let allowed_types = pin_properties
        .and_then(|value| nested_property(value, "AllowedTypes"))
        .and_then(string)
        .map(str::to_owned);
    let status = pin_properties
        .and_then(|value| nested_property(value, "PinStatus"))
        .and_then(string)
        .map(str::to_owned);
    let tooltip = pin_properties
        .and_then(|value| nested_property(value, "Tooltip"))
        .and_then(text);

    (
        PcgPin {
            index: pin.index,
            name: pin.name.clone(),
            full_name: pin.full_name.clone(),
            node_index,
            direction,
            label,
            allowed_types,
            status,
            tooltip,
            edge_indices: edge_indices.clone(),
            properties: pin_properties.map(nested_properties).unwrap_or_default(),
        },
        edge_indices,
        missing_edges,
    )
}

fn collect_property_bag_gaps(exports: &[AssetExport]) -> Vec<KnownOpaque> {
    let mut opaque = Vec::new();
    let mut seen_paths = BTreeSet::new();
    for export in exports {
        for property in &export.properties {
            let path = format!(
                "/exports/{}/properties/{}",
                export.index,
                json_pointer_segment(&property.name)
            );
            if property.type_name.contains(PROPERTY_BAG_TYPE)
                && has_serialized_payload(&property.value)
            {
                push_property_bag_gap(&path, &property.value, &mut seen_paths, &mut opaque);
            }
            collect_nested_property_bags(&property.value, &path, &mut seen_paths, &mut opaque);
        }
    }
    opaque
}

fn collect_nested_property_bags(
    value: &DecodedValue,
    parent_path: &str,
    seen_paths: &mut BTreeSet<String>,
    opaque: &mut Vec<KnownOpaque>,
) {
    match value {
        DecodedValue::Array(values) => {
            for value in values {
                collect_nested_property_bags(value, parent_path, seen_paths, opaque);
            }
        }
        DecodedValue::Object(values) => {
            let nested_name = values.get("name").and_then(string);
            let nested_type = values.get("type").and_then(string);
            let nested_value = values.get("value");
            if let (Some(name), Some(type_name), Some(value)) =
                (nested_name, nested_type, nested_value)
                && type_name.contains(PROPERTY_BAG_TYPE)
                && has_serialized_payload(value)
            {
                let path = format!("{parent_path}/{}", json_pointer_segment(name));
                push_property_bag_gap(&path, value, seen_paths, opaque);
                return;
            }
            for value in values.values() {
                collect_nested_property_bags(value, parent_path, seen_paths, opaque);
            }
        }
        _ => {}
    }
}

fn has_serialized_payload(value: &DecodedValue) -> bool {
    object(value)
        .and_then(|value| value.get("serialized_data"))
        .and_then(object)
        .and_then(|value| value.get("size"))
        .and_then(integer)
        .is_some_and(|size| size > 0)
}

fn push_property_bag_gap(
    path: &str,
    value: &DecodedValue,
    seen_paths: &mut BTreeSet<String>,
    opaque: &mut Vec<KnownOpaque>,
) {
    if !seen_paths.insert(path.to_owned()) {
        return;
    }
    opaque.push(KnownOpaque {
        path: path.to_owned(),
        kind: KnownOpaqueKind::PropertyValue,
        type_name: Some(PROPERTY_BAG_TYPE.to_owned()),
        reason: "registry-dependent PropertyBag serialized_data is retained as opaque".to_owned(),
        byte_range: serialized_payload_range(value),
    });
}

fn serialized_payload_range(value: &DecodedValue) -> Option<OpaqueByteRange> {
    let payload = object(value)
        .and_then(|value| value.get("serialized_data"))
        .and_then(object)?;
    let start = payload.get("start").and_then(integer)?;
    let end = payload.get("end").and_then(integer)?;
    let size = payload.get("size").and_then(integer)?;
    if start < 0 || end < start || size < 0 || end - start != size {
        return None;
    }
    Some(OpaqueByteRange {
        start: start as u64,
        end: end as u64,
        size: size as u64,
        preview: payload
            .get("preview")
            .and_then(string)
            .unwrap_or_default()
            .to_owned(),
    })
}

fn json_pointer_segment(value: &str) -> String {
    value.replace('~', "~0").replace('/', "~1")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::AssetProperty;

    #[test]
    fn graph_closure_keeps_subclass_nodes_and_engine_edge_direction() {
        let exports = vec![
            export(
                1,
                "Graph",
                PCG_GRAPH_CLASS,
                0,
                vec![
                    prop("Nodes", "ArrayProperty", refs(&[(4, "Graph.Custom")])),
                    prop("InputNode", "ObjectProperty", object_ref(2, "Graph.Input")),
                    prop(
                        "OutputNode",
                        "ObjectProperty",
                        object_ref(3, "Graph.Output"),
                    ),
                    prop(
                        "UserParameters",
                        "StructProperty(InstancedPropertyBag(/Script/CoreUObject))",
                        property_bag(8),
                    ),
                ],
            ),
            export(2, "Input", PCG_NODE_CLASS, 1, vec![]),
            export(
                3,
                "Output",
                PCG_NODE_CLASS,
                1,
                vec![prop(
                    "InputPins",
                    "ArrayProperty",
                    refs(&[(11, "Graph.Output.In")]),
                )],
            ),
            export(
                4,
                "Custom",
                "/Script/PCG.PCGSpawnActorNode",
                1,
                vec![prop(
                    "OutputPins",
                    "ArrayProperty",
                    refs(&[(10, "Graph.Custom.Out")]),
                )],
            ),
            export(
                10,
                "Out",
                PCG_PIN_CLASS,
                4,
                vec![
                    prop("Node", "ObjectProperty", object_ref(4, "Graph.Custom")),
                    prop("Edges", "ArrayProperty", refs(&[(12, "Graph.Edge")])),
                    prop(
                        "Properties",
                        "StructProperty(PCGPinProperties(/Script/PCG))",
                        struct_value(
                            "PCGPinProperties",
                            vec![prop("Label", "NameProperty", text_value("Out"))],
                        ),
                    ),
                ],
            ),
            export(
                11,
                "In",
                PCG_PIN_CLASS,
                3,
                vec![
                    prop("Node", "ObjectProperty", object_ref(3, "Graph.Output")),
                    prop("Edges", "ArrayProperty", refs(&[(12, "Graph.Edge")])),
                    prop(
                        "Properties",
                        "StructProperty(PCGPinProperties(/Script/PCG))",
                        struct_value(
                            "PCGPinProperties",
                            vec![prop("Label", "NameProperty", text_value("In"))],
                        ),
                    ),
                ],
            ),
            export(
                12,
                "Edge",
                PCG_EDGE_CLASS,
                10,
                vec![
                    prop(
                        "InputPin",
                        "ObjectProperty",
                        object_ref(10, "Graph.Custom.Out"),
                    ),
                    prop(
                        "OutputPin",
                        "ObjectProperty",
                        object_ref(11, "Graph.Output.In"),
                    ),
                ],
            ),
        ];

        let result = build_pcg_graphs(&exports);
        let graph = &result.graphs[0];
        assert_eq!(graph.nodes_array_count, 1);
        assert_eq!(graph.default_node_count, 2);
        assert_eq!(graph.nodes.len(), 3);
        assert_eq!(graph.base_node_export_count, 2);
        assert_eq!(graph.nodes.iter().flat_map(|node| &node.pins).count(), 2);
        assert_eq!(
            graph.edges,
            vec![PcgEdge {
                index: 12,
                name: "Edge".into(),
                source_node_index: 4,
                source_pin_index: 10,
                target_node_index: 3,
                target_pin_index: 11,
            }]
        );
        assert_eq!(result.known_opaque.len(), 1);
        assert_eq!(
            result.known_opaque[0].path,
            "/exports/1/properties/UserParameters"
        );
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
            full_name: format!("Graph.{name}"),
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

    fn prop(name: &str, type_name: &str, value: DecodedValue) -> AssetProperty {
        AssetProperty {
            name: name.into(),
            type_name: type_name.into(),
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

    fn property_bag(size: i64) -> DecodedValue {
        DecodedValue::Object(BTreeMap::from([
            ("has_data".into(), DecodedValue::Bool(true)),
            (
                "serialized_data".into(),
                DecodedValue::Object(BTreeMap::from([(
                    "size".into(),
                    DecodedValue::Integer(size),
                )])),
            ),
        ]))
    }

    fn text_value(value: &str) -> DecodedValue {
        DecodedValue::String(value.into())
    }
}
