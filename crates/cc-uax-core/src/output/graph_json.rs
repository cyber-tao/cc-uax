use crate::decode::{DecodeReport, DecodedExport};
use crate::pin::{Pin, direction_label};
use crate::reader::Guid;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Clone, Copy)]
struct PinEndpoint<'a> {
    graph_index: i32,
    node_index: i32,
    pin: &'a Pin,
}

pub(crate) fn graphs_to_json(report: &DecodeReport<'_>) -> Value {
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

    Value::Array(
        graphs
            .into_iter()
            .map(|(graph_index, nodes)| graph_to_json(report, graph_index, &nodes, &pin_by_id))
            .collect(),
    )
}

fn graph_to_json(
    report: &DecodeReport<'_>,
    graph_index: i32,
    nodes: &[&DecodedExport],
    pin_by_id: &HashMap<(i32, Guid), PinEndpoint<'_>>,
) -> Value {
    let mut object = serde_json::Map::new();
    object.insert("index".into(), json!(graph_index));
    object.insert(
        "name".into(),
        json!(
            report
                .exports
                .get((graph_index - 1).max(0) as usize)
                .map(|export| export.identity.name.as_str())
                .unwrap_or("<unresolved_graph>")
        ),
    );
    object.insert(
        "full_name".into(),
        json!(report.package.resolve_full_name(graph_index)),
    );
    object.insert(
        "nodes".into(),
        Value::Array(
            nodes
                .iter()
                .map(|node| {
                    json!({
                        "index": node.identity.index,
                        "name": node.identity.name,
                        "class": node.identity.class,
                        "pin_count": node.pins.as_ref().map_or(0, Vec::len),
                    })
                })
                .collect(),
        ),
    );

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
                edges.push(json!({
                    "kind": if source.pin.category == "exec" || target.pin.category == "exec" {
                        "exec"
                    } else {
                        "data"
                    },
                    "from": endpoint_to_json(source),
                    "to": endpoint_to_json(target),
                }));
            }
        }
    }
    object.insert("edges".into(), Value::Array(edges));
    if !cross_graph_links.is_empty() {
        object.insert(
            "excluded_cross_graph_links".into(),
            json!(cross_graph_links.len()),
        );
    }
    if !unresolved_links.is_empty() {
        object.insert("unresolved_links".into(), json!(unresolved_links.len()));
    }
    Value::Object(object)
}

fn orient_edge<'a>(
    left: PinEndpoint<'a>,
    right: PinEndpoint<'a>,
) -> (PinEndpoint<'a>, PinEndpoint<'a>) {
    match (
        direction_label(left.pin.direction),
        direction_label(right.pin.direction),
    ) {
        ("output", "input") => (left, right),
        ("input", "output") => (right, left),
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

fn endpoint_to_json(endpoint: PinEndpoint<'_>) -> Value {
    json!({
        "node_index": endpoint.node_index,
        "pin_id": endpoint.pin.pin_id.to_hex(),
    })
}
