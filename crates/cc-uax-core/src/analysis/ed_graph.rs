//! EdGraph (K2/Blueprint) graph assembly: groups decoded pin-bearing exports by
//! their owning graph, resolves pin links into typed exec/data edges, and maps
//! pins, pin types and user-defined pins into the public graph model.

use super::ControlRigMirrors;
use crate::decode::{DecodeReport, DecodedExport};
use crate::graph_models::*;
use crate::model::MemberReference;
use crate::package::Package;
use crate::pin::{
    CONTAINER_TYPE_ARRAY, CONTAINER_TYPE_MAP, CONTAINER_TYPE_NONE, CONTAINER_TYPE_SET, Pin,
    PinTerminalType, PinType, UserDefinedPin,
};
use crate::reader::Guid;
use crate::structured_value::Value;
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Clone, Copy)]
struct PinEndpoint<'a> {
    graph_index: i32,
    node_index: i32,
    pin: &'a Pin,
}

pub(crate) fn build_logic_graphs(
    report: &DecodeReport<'_>,
    mirrors: &ControlRigMirrors,
) -> Vec<LogicGraph> {
    let mut graphs: BTreeMap<i32, Vec<&DecodedExport>> = BTreeMap::new();
    let mut node_graph = HashMap::new();
    for export in &report.exports {
        if export.pins.is_none() {
            continue;
        }
        let Some(object_export) = report
            .package
            .exports
            .get((export.identity.index - 1).max(0) as usize)
        else {
            continue;
        };
        let graph_index = object_export.outer_index.0;
        if mirrors.excludes_class_or_graph(&export.identity.class, graph_index) {
            continue;
        }
        graphs.entry(graph_index).or_default().push(export);
        node_graph.insert(export.identity.index, graph_index);
    }

    let mut pin_by_id: HashMap<(i32, Guid), PinEndpoint<'_>> = HashMap::new();
    for export in &report.exports {
        let Some(&graph_index) = node_graph.get(&export.identity.index) else {
            continue;
        };
        let Some(pins) = &export.pins else {
            continue;
        };
        for pin in pins {
            pin_by_id.insert(
                (export.identity.index, pin.pin_id),
                PinEndpoint {
                    graph_index,
                    node_index: export.identity.index,
                    pin,
                },
            );
        }
    }

    graphs
        .into_iter()
        .map(|(graph_index, nodes)| graph_from_exports(report, graph_index, &nodes, &pin_by_id))
        .collect()
}

fn graph_from_exports(
    report: &DecodeReport<'_>,
    graph_index: i32,
    nodes: &[&DecodedExport],
    pin_by_id: &HashMap<(i32, Guid), PinEndpoint<'_>>,
) -> LogicGraph {
    let graph_name = positive_export(report, graph_index)
        .map(|export| export.identity.name.clone())
        .unwrap_or_else(|| "<unresolved_graph>".into());
    let mut edges = Vec::new();
    let mut seen_edges = HashSet::new();
    let mut cross_graph_links = HashSet::new();
    let mut unresolved_links = HashSet::new();
    for node in nodes {
        let Some(pins) = &node.pins else {
            continue;
        };
        for pin in pins {
            let current = PinEndpoint {
                graph_index,
                node_index: node.identity.index,
                pin,
            };
            for linked in &pin.linked_to {
                let Some(target) = pin_by_id.get(&(linked.node_index, linked.pin_id)).copied()
                else {
                    unresolved_links.insert((
                        current.node_index,
                        current.pin.pin_id,
                        linked.node_index,
                        linked.pin_id,
                    ));
                    continue;
                };
                if target.graph_index != graph_index {
                    cross_graph_links.insert(canonical_pair(current, target));
                    continue;
                }
                let (source, target) = orient_edge(current, target);
                let key = (
                    source.node_index,
                    source.pin.pin_id,
                    target.node_index,
                    target.pin.pin_id,
                );
                if !seen_edges.insert(key) {
                    continue;
                }
                edges.push(GraphEdge {
                    kind: if source.pin.category == "exec" || target.pin.category == "exec" {
                        EdgeKind::Exec
                    } else {
                        EdgeKind::Data
                    },
                    from: graph_endpoint(source),
                    to: graph_endpoint(target),
                });
            }
        }
    }

    // Intra-graph connectivity is fully carried by `edges`; keep only cross-graph
    // and unresolved targets on each pin's `linked_to` so it is not duplicated.
    let intra_graph_pins: HashSet<(i32, Guid)> = nodes
        .iter()
        .filter_map(|export| {
            export
                .pins
                .as_ref()
                .map(|pins| (export.identity.index, pins))
        })
        .flat_map(|(index, pins)| pins.iter().map(move |pin| (index, pin.pin_id)))
        .collect();

    LogicGraph {
        index: graph_index,
        name: graph_name,
        full_name: report.package.resolve_full_name(graph_index),
        nodes: nodes
            .iter()
            .map(|export| graph_node_from_export(report.package, export, &intra_graph_pins))
            .collect(),
        edges,
        excluded_cross_graph_links: cross_graph_links.len(),
        unresolved_links: unresolved_links.len(),
    }
}

fn graph_node_from_export(
    package: &Package,
    export: &DecodedExport,
    intra_graph_pins: &HashSet<(i32, Guid)>,
) -> GraphNode {
    GraphNode {
        index: export.identity.index,
        name: export.identity.name.clone(),
        class: export.identity.class.clone(),
        member: export.member.as_ref().map(|member| MemberReference {
            name: member.name.clone(),
            parent: member.parent.clone(),
        }),
        pins: export
            .pins
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|pin| graph_pin_from_pin(package, pin, intra_graph_pins))
            .collect(),
        user_defined_pins: export
            .user_defined_pins
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|pin| user_defined_pin_to_model(package, pin))
            .collect(),
    }
}

/// An FText decodes to a `{text, flags, ...}` object. Treat the null value and
/// the "no source string" form (`text` null with no history/namespace) as empty
/// so a pin's default/friendly text is dropped when it carries no real value.
fn ftext_is_empty(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Object(map) => {
            map.get("text").is_none_or(Value::is_null)
                && !map.contains_key("history")
                && !map.contains_key("namespace")
                && !map.contains_key("format")
        }
        _ => false,
    }
}

fn graph_pin_from_pin(
    package: &Package,
    pin: &Pin,
    intra_graph_pins: &HashSet<(i32, Guid)>,
) -> GraphPin {
    let pin_type = PinType {
        category: pin.category.clone(),
        sub_category: pin.sub_category.clone(),
        sub_category_object: pin.sub_category_object,
        container_type: pin.container_type,
        value_type: pin.value_type.clone(),
        is_reference: pin.is_reference,
        is_weak_pointer: pin.is_weak_pointer,
        member_parent: pin.member_parent,
        member_name: pin.member_name.clone(),
        member_guid: pin.member_guid,
        is_const: pin.is_const,
        is_uobject_wrapper: pin.is_uobject_wrapper,
        serialize_as_single_precision_float: pin.serialize_as_single_precision_float,
    };
    GraphPin {
        pin_id: pin.pin_id.to_hex(),
        name: pin.name.clone(),
        friendly_name: pin
            .friendly_name
            .clone()
            .filter(|value| !ftext_is_empty(value)),
        source_index: pin.source_index.filter(|&index| index >= 0),
        tooltip: pin.tooltip.clone(),
        direction: pin_direction(pin.direction),
        pin_type: pin_type_to_model(package, &pin_type),
        default_value: (!pin.default_value.is_empty()).then(|| pin.default_value.clone()),
        autogenerated_default_value: (!pin.autogenerated_default_value.is_empty())
            .then(|| pin.autogenerated_default_value.clone()),
        default_object: (pin.default_object != 0)
            .then(|| package.resolve_object_ref(pin.default_object)),
        default_text: Some(pin.default_text.clone()).filter(|value| !ftext_is_empty(value)),
        // Only cross-graph and unresolved links are kept; intra-graph links are in `edges`.
        linked_to: pin
            .linked_to
            .iter()
            .filter(|reference| {
                !intra_graph_pins.contains(&(reference.node_index, reference.pin_id))
            })
            .map(pin_reference_to_model)
            .collect(),
        sub_pins: pin.sub_pins.iter().map(pin_reference_to_model).collect(),
        parent_pin: pin.parent_pin.as_ref().map(pin_reference_to_model),
        reference_pass_through: pin
            .reference_pass_through
            .as_ref()
            .map(pin_reference_to_model),
        persistent_guid: pin
            .persistent_guid
            .filter(|guid| !guid.is_zero())
            .map(|guid| guid.to_hex()),
        editor_flags: pin.editor_flags.as_ref().and_then(|flags| {
            let mapped = GraphPinEditorFlags {
                hidden: flags.hidden,
                not_connectable: flags.not_connectable,
                default_value_read_only: flags.default_value_read_only,
                default_value_ignored: flags.default_value_ignored,
                advanced_view: flags.advanced_view,
                orphaned_pin: flags.orphaned_pin,
            };
            (!mapped.is_empty()).then_some(mapped)
        }),
    }
}

fn pin_reference_to_model(reference: &crate::pin::PinRef) -> GraphPinReference {
    GraphPinReference {
        node_index: reference.node_index,
        pin_id: reference.pin_id.to_hex(),
    }
}

fn user_defined_pin_to_model(package: &Package, pin: &UserDefinedPin) -> UserDefinedGraphPin {
    UserDefinedGraphPin {
        name: pin.name.clone(),
        direction: pin_direction(pin.direction),
        pin_type: pin_type_to_model(package, &pin.pin_type),
        default_value: (!pin.default_value.is_empty()).then(|| pin.default_value.clone()),
    }
}

fn pin_type_to_model(package: &Package, pin_type: &PinType) -> GraphPinType {
    let member_reference = (pin_type.member_parent != 0
        || !crate::model::is_absent_name(&pin_type.member_name)
        || !pin_type.member_guid.is_zero())
    .then(|| MemberReference {
        name: pin_type.member_name.clone(),
        parent: (pin_type.member_parent != 0)
            .then(|| package.resolve_object_ref(pin_type.member_parent)),
    });
    GraphPinType {
        category: pin_type.category.clone(),
        sub_category: pin_type.sub_category.clone(),
        sub_category_object: (pin_type.sub_category_object != 0)
            .then(|| package.resolve_object_ref(pin_type.sub_category_object)),
        container: match pin_type.container_type {
            CONTAINER_TYPE_NONE => PinContainer::None,
            CONTAINER_TYPE_ARRAY => PinContainer::Array,
            CONTAINER_TYPE_SET => PinContainer::Set,
            CONTAINER_TYPE_MAP => PinContainer::Map,
            value => PinContainer::Unknown(value),
        },
        value_type: pin_type
            .value_type
            .as_ref()
            .map(|terminal| terminal_type_to_model(package, terminal)),
        is_reference: pin_type.is_reference,
        is_weak_pointer: pin_type.is_weak_pointer,
        is_const: pin_type.is_const,
        is_uobject_wrapper: pin_type.is_uobject_wrapper,
        serialize_as_single_precision_float: pin_type.serialize_as_single_precision_float,
        member_reference,
    }
}

fn terminal_type_to_model(package: &Package, terminal: &PinTerminalType) -> GraphTerminalType {
    GraphTerminalType {
        category: terminal.category.clone(),
        sub_category: terminal.sub_category.clone(),
        sub_category_object: (terminal.sub_category_object != 0)
            .then(|| package.resolve_object_ref(terminal.sub_category_object)),
        is_const: terminal.is_const,
        is_weak_pointer: terminal.is_weak_pointer,
        is_uobject_wrapper: terminal.is_uobject_wrapper,
    }
}

fn pin_direction(direction: u8) -> PinDirection {
    match direction {
        0 => PinDirection::Input,
        1 => PinDirection::Output,
        value => PinDirection::Unknown(value),
    }
}

fn orient_edge<'a>(
    left: PinEndpoint<'a>,
    right: PinEndpoint<'a>,
) -> (PinEndpoint<'a>, PinEndpoint<'a>) {
    match (
        pin_direction(left.pin.direction),
        pin_direction(right.pin.direction),
    ) {
        (PinDirection::Output, PinDirection::Input) => (left, right),
        (PinDirection::Input, PinDirection::Output) => (right, left),
        _ if endpoint_key(left) <= endpoint_key(right) => (left, right),
        _ => (right, left),
    }
}

fn canonical_pair(left: PinEndpoint<'_>, right: PinEndpoint<'_>) -> (i32, Guid, i32, Guid) {
    if endpoint_key(left) <= endpoint_key(right) {
        (
            left.node_index,
            left.pin.pin_id,
            right.node_index,
            right.pin.pin_id,
        )
    } else {
        (
            right.node_index,
            right.pin.pin_id,
            left.node_index,
            left.pin.pin_id,
        )
    }
}

fn endpoint_key(endpoint: PinEndpoint<'_>) -> (i32, [u32; 4]) {
    (endpoint.node_index, endpoint.pin.pin_id.0)
}

fn graph_endpoint(endpoint: PinEndpoint<'_>) -> GraphEndpoint {
    GraphEndpoint {
        node_index: endpoint.node_index,
        pin_id: endpoint.pin.pin_id.to_hex(),
    }
}

fn positive_export<'a>(report: &'a DecodeReport<'_>, index: i32) -> Option<&'a DecodedExport> {
    usize::try_from(index.checked_sub(1)?)
        .ok()
        .and_then(|index| report.exports.get(index))
}
