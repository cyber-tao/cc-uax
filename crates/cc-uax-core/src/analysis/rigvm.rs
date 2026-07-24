use super::property_to_model;
use crate::decode::rigvm::{is_rigvm_graph_class, is_rigvm_node_class};
use crate::decode::{DecodeReport, DecodedExport};
use crate::graph_models::{
    RigVmGraph, RigVmInjection, RigVmLinearColor, RigVmLink, RigVmNode, RigVmPin,
    RigVmPinDirection, RigVmVector2,
};
use crate::model::{AnalysisDiagnostic, AssetProperty, DiagnosticSeverity};
use crate::property::{PropertyEntry, PropertyParseStatus};
use crate::structured_value::{Map, Value};
use std::collections::{HashMap, HashSet};

const MAX_PIN_DEPTH: usize = 128;

#[derive(Default)]
pub(super) struct RigVmAdapterResult {
    pub(super) graphs: Vec<RigVmGraph>,
    pub(super) graphs_total: usize,
    pub(super) graphs_decoded: usize,
    pub(super) nodes_total: usize,
    pub(super) nodes_decoded: usize,
    pub(super) pins_total: usize,
    pub(super) pins_decoded: usize,
    pub(super) links_total: usize,
    pub(super) links_decoded: usize,
    pub(super) diagnostics: Vec<AnalysisDiagnostic>,
}

impl RigVmAdapterResult {
    pub(super) fn is_complete(&self) -> bool {
        self.graphs_decoded == self.graphs_total
            && self.nodes_decoded == self.nodes_total
            && self.pins_decoded == self.pins_total
            && self.links_decoded == self.links_total
            && self.diagnostics.is_empty()
    }
}

pub(super) fn build_rigvm_graphs(report: &DecodeReport<'_>) -> RigVmAdapterResult {
    let export_by_index = report
        .exports
        .iter()
        .map(|export| (export.identity.index, export))
        .collect::<HashMap<_, _>>();
    let mut result = RigVmAdapterResult::default();

    for graph_export in report
        .exports
        .iter()
        .filter(|export| is_rigvm_graph_class(&export.identity.class))
    {
        result.graphs_total += 1;
        if properties_are_complete(graph_export) {
            result.graphs_decoded += 1;
        } else {
            result.diagnostics.push(warning(
                "rigvm_graph_properties_partial",
                format!("/rigvm_graphs/{}", graph_export.identity.index),
                format!(
                    "RigVM graph '{}' does not have a completely decoded property block",
                    graph_export.identity.name
                ),
            ));
        }

        let mut unresolved_node_references = 0;
        let mut unresolved_pin_references = 0;
        let mut unresolved_link_references = 0;
        let node_indices =
            property_object_indices(graph_export, "Nodes", &mut unresolved_node_references);
        let link_indices =
            property_object_indices(graph_export, "Links", &mut unresolved_link_references);
        let graph_index = graph_export.identity.index;
        let graph_path = format!("/rigvm_graphs/{graph_index}");

        let mut nodes = Vec::with_capacity(node_indices.len());
        for node_index in node_indices {
            result.nodes_total += 1;
            let Some(node_export) = export_by_index.get(&node_index).copied() else {
                unresolved_node_references += 1;
                result.diagnostics.push(warning(
                    "rigvm_node_reference_unresolved",
                    format!("{graph_path}/nodes/{node_index}"),
                    format!("RigVM graph references missing node export {node_index}"),
                ));
                continue;
            };
            if properties_are_complete(node_export) {
                result.nodes_decoded += 1;
            } else {
                result.diagnostics.push(warning(
                    "rigvm_node_properties_partial",
                    format!("{graph_path}/nodes/{node_index}"),
                    format!(
                        "RigVM node '{}' does not have a completely decoded property block",
                        node_export.identity.name
                    ),
                ));
            }
            let mut active_pins = HashSet::new();
            let mut active_nodes = HashSet::from([node_index]);
            nodes.push(node_from_export(
                report,
                &export_by_index,
                graph_index,
                node_export,
                &graph_path,
                &mut active_pins,
                &mut active_nodes,
                &mut unresolved_pin_references,
                &mut result,
            ));
        }

        let mut links = Vec::with_capacity(link_indices.len());
        for link_index in link_indices {
            result.links_total += 1;
            let Some(link_export) = export_by_index.get(&link_index).copied() else {
                unresolved_link_references += 1;
                result.diagnostics.push(warning(
                    "rigvm_link_reference_unresolved",
                    format!("{graph_path}/links/{link_index}"),
                    format!("RigVM graph references missing link export {link_index}"),
                ));
                continue;
            };
            let Some(link) = &link_export.rigvm_link else {
                unresolved_link_references += 1;
                result.diagnostics.push(warning(
                    "rigvm_link_payload_unresolved",
                    format!("{graph_path}/links/{link_index}"),
                    format!(
                        "RigVM link '{}' did not decode as two bounded FString paths",
                        link_export.identity.name
                    ),
                ));
                continue;
            };
            result.links_decoded += 1;
            links.push(RigVmLink {
                index: link_index,
                name: link_export.identity.name.clone(),
                source_pin_path: link.source_pin_path.clone(),
                target_pin_path: link.target_pin_path.clone(),
            });
        }

        result.graphs.push(RigVmGraph {
            index: graph_index,
            name: graph_export.identity.name.clone(),
            full_name: report.package.resolve_full_name(graph_index),
            nodes,
            links,
            unresolved_node_references,
            unresolved_pin_references,
            unresolved_link_references,
        });
    }

    result
}

#[allow(clippy::too_many_arguments)]
fn node_from_export(
    report: &DecodeReport<'_>,
    export_by_index: &HashMap<i32, &DecodedExport>,
    graph_index: i32,
    export: &DecodedExport,
    graph_path: &str,
    active_pins: &mut HashSet<i32>,
    active_nodes: &mut HashSet<i32>,
    unresolved_pin_references: &mut usize,
    result: &mut RigVmAdapterResult,
) -> RigVmNode {
    let pin_indices = property_object_indices(export, "Pins", unresolved_pin_references);
    let orphaned_pin_indices =
        property_object_indices(export, "OrphanedPins", unresolved_pin_references);
    let pins = pin_indices
        .into_iter()
        .filter_map(|pin_index| {
            pin_from_index(
                report,
                export_by_index,
                graph_index,
                pin_index,
                graph_path,
                active_pins,
                active_nodes,
                unresolved_pin_references,
                result,
                0,
            )
        })
        .collect();
    let orphaned_pins = orphaned_pin_indices
        .into_iter()
        .filter_map(|pin_index| {
            pin_from_index(
                report,
                export_by_index,
                graph_index,
                pin_index,
                graph_path,
                active_pins,
                active_nodes,
                unresolved_pin_references,
                result,
                0,
            )
        })
        .collect();

    RigVmNode {
        index: export.identity.index,
        name: export.identity.name.clone(),
        path: export.identity.name.clone(),
        class: export.identity.class.clone(),
        title: string_property(export, "NodeTitle"),
        position: vector2_property(export, "Position"),
        size: vector2_property(export, "Size"),
        color: color_property(export, "NodeColor"),
        pins,
        orphaned_pins,
        properties: properties_to_model(export),
    }
}

#[allow(clippy::too_many_arguments)]
fn pin_from_index(
    report: &DecodeReport<'_>,
    export_by_index: &HashMap<i32, &DecodedExport>,
    graph_index: i32,
    pin_index: i32,
    graph_path: &str,
    active_pins: &mut HashSet<i32>,
    active_nodes: &mut HashSet<i32>,
    unresolved_pin_references: &mut usize,
    result: &mut RigVmAdapterResult,
    depth: usize,
) -> Option<RigVmPin> {
    result.pins_total += 1;
    if depth >= MAX_PIN_DEPTH || !active_pins.insert(pin_index) {
        *unresolved_pin_references += 1;
        result.diagnostics.push(warning(
            "rigvm_pin_hierarchy_cycle",
            format!("{graph_path}/pins/{pin_index}"),
            format!(
                "RigVM pin hierarchy reached a cycle or exceeded depth {MAX_PIN_DEPTH} at export {pin_index}"
            ),
        ));
        return None;
    }
    let Some(export) = export_by_index.get(&pin_index).copied() else {
        active_pins.remove(&pin_index);
        *unresolved_pin_references += 1;
        result.diagnostics.push(warning(
            "rigvm_pin_reference_unresolved",
            format!("{graph_path}/pins/{pin_index}"),
            format!("RigVM node or pin references missing pin export {pin_index}"),
        ));
        return None;
    };
    if properties_are_complete(export) {
        result.pins_decoded += 1;
    } else {
        result.diagnostics.push(warning(
            "rigvm_pin_properties_partial",
            format!("{graph_path}/pins/{pin_index}"),
            format!(
                "RigVM pin '{}' does not have a completely decoded property block",
                export.identity.name
            ),
        ));
    }

    let sub_pin_indices = property_object_indices(export, "SubPins", unresolved_pin_references);
    let sub_pins = sub_pin_indices
        .into_iter()
        .filter_map(|sub_pin_index| {
            pin_from_index(
                report,
                export_by_index,
                graph_index,
                sub_pin_index,
                graph_path,
                active_pins,
                active_nodes,
                unresolved_pin_references,
                result,
                depth + 1,
            )
        })
        .collect();
    let injection_indices =
        property_object_indices(export, "InjectionInfos", unresolved_pin_references);
    let injections = injection_indices
        .into_iter()
        .filter_map(|injection_index| {
            injection_from_index(
                report,
                export_by_index,
                graph_index,
                injection_index,
                graph_path,
                active_pins,
                active_nodes,
                unresolved_pin_references,
                result,
            )
        })
        .collect();
    active_pins.remove(&pin_index);

    Some(RigVmPin {
        index: pin_index,
        name: export.identity.name.clone(),
        path: rigvm_pin_path(report, pin_index),
        display_name: string_property(export, "DisplayName"),
        direction: rigvm_pin_direction(string_property(export, "Direction")),
        is_expanded: bool_property(export, "bIsExpanded"),
        is_constant: bool_property(export, "bIsConstant"),
        is_dynamic_array: bool_property(export, "bIsDynamicArray"),
        is_lazy: bool_property(export, "bIsLazy"),
        cpp_type: string_property(export, "CPPType"),
        cpp_type_object: value_property(export, "CPPTypeObject").cloned(),
        cpp_type_object_path: string_property(export, "CPPTypeObjectPath"),
        default_value: string_property(export, "DefaultValue"),
        default_value_type: string_property(export, "DefaultValueType"),
        custom_widget_name: string_property(export, "CustomWidgetName"),
        user_defined_category: string_property(export, "UserDefinedCategory"),
        index_in_category: integer_property(export, "IndexInCategory"),
        sub_pins,
        injections,
        properties: properties_to_model(export),
    })
}

#[allow(clippy::too_many_arguments)]
fn injection_from_index(
    report: &DecodeReport<'_>,
    export_by_index: &HashMap<i32, &DecodedExport>,
    graph_index: i32,
    injection_index: i32,
    graph_path: &str,
    active_pins: &mut HashSet<i32>,
    active_nodes: &mut HashSet<i32>,
    unresolved_references: &mut usize,
    result: &mut RigVmAdapterResult,
) -> Option<RigVmInjection> {
    let Some(injection_export) = export_by_index.get(&injection_index).copied() else {
        *unresolved_references += 1;
        result.diagnostics.push(warning(
            "rigvm_injection_reference_unresolved",
            format!("{graph_path}/injections/{injection_index}"),
            format!("RigVM pin references missing injection export {injection_index}"),
        ));
        return None;
    };
    if !properties_are_complete(injection_export) {
        result.diagnostics.push(warning(
            "rigvm_injection_properties_partial",
            format!("{graph_path}/injections/{injection_index}"),
            format!(
                "RigVM injection '{}' does not have a completely decoded property block",
                injection_export.identity.name
            ),
        ));
    }

    let injected_node = object_index_property(injection_export, "Node").and_then(|node_index| {
        result.nodes_total += 1;
        if !active_nodes.insert(node_index) {
            *unresolved_references += 1;
            result.diagnostics.push(warning(
                "rigvm_injected_node_cycle",
                format!("{graph_path}/injections/{injection_index}/node/{node_index}"),
                format!("RigVM injected-node hierarchy cycles at export {node_index}"),
            ));
            return None;
        }
        let Some(node_export) = export_by_index.get(&node_index).copied() else {
            active_nodes.remove(&node_index);
            *unresolved_references += 1;
            result.diagnostics.push(warning(
                "rigvm_injected_node_unresolved",
                format!("{graph_path}/injections/{injection_index}/node/{node_index}"),
                format!("RigVM injection references missing node export {node_index}"),
            ));
            return None;
        };
        if properties_are_complete(node_export) {
            result.nodes_decoded += 1;
        } else {
            result.diagnostics.push(warning(
                "rigvm_injected_node_properties_partial",
                format!("{graph_path}/injections/{injection_index}/node/{node_index}"),
                format!(
                    "RigVM injected node '{}' does not have a completely decoded property block",
                    node_export.identity.name
                ),
            ));
        }
        let node = node_from_export(
            report,
            export_by_index,
            graph_index,
            node_export,
            graph_path,
            active_pins,
            active_nodes,
            unresolved_references,
            result,
        );
        active_nodes.remove(&node_index);
        Some(Box::new(node))
    });

    Some(RigVmInjection {
        index: injection_index,
        name: injection_export.identity.name.clone(),
        injected_as_input: bool_property(injection_export, "bInjectedAsInput"),
        input_pin_index: object_index_property(injection_export, "InputPin"),
        output_pin_index: object_index_property(injection_export, "OutputPin"),
        node: injected_node,
        properties: properties_to_model(injection_export),
    })
}

fn properties_are_complete(export: &DecodedExport) -> bool {
    matches!(
        export.property_status,
        Some(PropertyParseStatus::Complete | PropertyParseStatus::Empty)
    )
}

fn properties_to_model(export: &DecodedExport) -> Vec<AssetProperty> {
    export
        .properties
        .as_deref()
        .unwrap_or_default()
        .iter()
        .map(property_to_model)
        .collect()
}

fn property<'a>(export: &'a DecodedExport, name: &str) -> Option<&'a PropertyEntry> {
    export
        .properties
        .as_deref()?
        .iter()
        .find(|property| property.name == name)
}

fn value_property<'a>(export: &'a DecodedExport, name: &str) -> Option<&'a Value> {
    property(export, name).map(|property| &property.value)
}

fn string_property(export: &DecodedExport, name: &str) -> Option<String> {
    value_property(export, name)
        .and_then(Value::as_str)
        .map(str::to_owned)
}

fn bool_property(export: &DecodedExport, name: &str) -> bool {
    value_property(export, name)
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn integer_property(export: &DecodedExport, name: &str) -> Option<i32> {
    value_property(export, name)
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
}

fn object_index_property(export: &DecodedExport, name: &str) -> Option<i32> {
    value_property(export, name)?
        .as_object()?
        .get("index")?
        .as_i64()
        .and_then(|index| i32::try_from(index).ok())
        .filter(|index| *index > 0)
}

fn vector2_property(export: &DecodedExport, name: &str) -> Option<RigVmVector2> {
    let object = value_property(export, name)?.as_object()?;
    Some(RigVmVector2 {
        x: object_number(object, "x")?,
        y: object_number(object, "y")?,
    })
}

fn color_property(export: &DecodedExport, name: &str) -> Option<RigVmLinearColor> {
    let object = value_property(export, name)?.as_object()?;
    Some(RigVmLinearColor {
        r: object_number(object, "r")?,
        g: object_number(object, "g")?,
        b: object_number(object, "b")?,
        a: object_number(object, "a")?,
    })
}

fn object_number(object: &Map, key: &str) -> Option<f64> {
    object
        .get(key)
        .or_else(|| object.get(&key.to_ascii_uppercase()))
        .and_then(Value::as_f64)
}

fn property_object_indices(
    export: &DecodedExport,
    name: &str,
    invalid_references: &mut usize,
) -> Vec<i32> {
    let Some(value) = value_property(export, name) else {
        return Vec::new();
    };
    let values: Vec<&Value> = match value {
        Value::Array(values) => values.iter().collect(),
        Value::Object(_) => vec![value],
        Value::Null => Vec::new(),
        _ => {
            *invalid_references += 1;
            return Vec::new();
        }
    };
    values
        .into_iter()
        .filter_map(|value| {
            let index = value
                .as_object()
                .and_then(|object| object.get("index"))
                .and_then(Value::as_i64)
                .and_then(|index| i32::try_from(index).ok());
            match index {
                Some(index) if index > 0 => Some(index),
                _ => {
                    *invalid_references += 1;
                    None
                }
            }
        })
        .collect()
}

fn rigvm_pin_path(report: &DecodeReport<'_>, pin_index: i32) -> String {
    let mut segments = Vec::new();
    let mut current = pin_index;
    let mut visited = HashSet::new();
    while current > 0 && visited.insert(current) && segments.len() < MAX_PIN_DEPTH {
        let Some(export) = report.exports.get((current - 1) as usize) else {
            break;
        };
        if is_rigvm_node_class(&export.identity.class) {
            segments.push(export.identity.name.clone());
            break;
        }
        if export.identity.class == crate::decode::rigvm::RIGVM_PIN_CLASS {
            segments.push(export.identity.name.clone());
        }
        let Some(raw) = report.package.exports.get((current - 1) as usize) else {
            break;
        };
        current = raw.outer_index.0;
    }
    segments.reverse();
    if segments.is_empty() {
        report.package.resolve_full_name(pin_index)
    } else {
        segments.join(".")
    }
}

fn rigvm_pin_direction(value: Option<String>) -> RigVmPinDirection {
    let value = value.unwrap_or_else(|| "<missing>".into());
    match value.rsplit("::").next().unwrap_or(&value) {
        "Input" => RigVmPinDirection::Input,
        "Output" => RigVmPinDirection::Output,
        "IO" => RigVmPinDirection::Io,
        "Visible" => RigVmPinDirection::Visible,
        "Hidden" => RigVmPinDirection::Hidden,
        "Invalid" => RigVmPinDirection::Invalid,
        _ => RigVmPinDirection::Unknown(value),
    }
}

fn warning(
    code: impl Into<String>,
    path: impl Into<String>,
    message: impl Into<String>,
) -> AnalysisDiagnostic {
    AnalysisDiagnostic {
        severity: DiagnosticSeverity::Warning,
        code: code.into(),
        path: path.into(),
        message: message.into(),
        offset: None,
        details: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structured_value::json;

    #[test]
    fn direction_keeps_unknown_values() {
        assert_eq!(
            rigvm_pin_direction(Some("ERigVMPinDirection::IO".into())),
            RigVmPinDirection::Io
        );
        assert_eq!(
            rigvm_pin_direction(Some("FutureDirection".into())),
            RigVmPinDirection::Unknown("FutureDirection".into())
        );
    }

    #[test]
    fn reference_arrays_reject_non_export_indices() {
        let export = DecodedExport {
            identity: crate::decode::DecodedExportIdentity {
                index: 1,
                name: "Graph".into(),
                class: "/Script/RigVMDeveloper.RigVMGraph".into(),
                is_asset: false,
            },
            properties: Some(vec![PropertyEntry {
                name: "Nodes".into(),
                type_str: "ArrayProperty(ObjectProperty)".into(),
                array_index: 0,
                value: json!([
                    { "index": 3, "ref": "Graph.Node" },
                    { "index": -1, "ref": "/Script/Foo" },
                    { "invalid": true }
                ]),
                guid: None,
            }]),
            property_status: Some(PropertyParseStatus::Complete),
            post_property_tail: None,
            object_guid: None,
            metadata: None,
            pins: None,
            user_defined_pins: None,
            member: None,
            rigvm_link: None,
        };
        let mut invalid = 0;
        assert_eq!(property_object_indices(&export, "Nodes", &mut invalid), [3]);
        assert_eq!(invalid, 2);
    }
}
