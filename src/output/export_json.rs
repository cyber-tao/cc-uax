use crate::decode::{DecodeReport, DecodedExport};
use crate::property::entries_to_json;
use crate::reader::Guid;
use serde_json::{Value, json};
use std::collections::HashMap;

use super::pin_json::pins_to_json;
use super::property_json::{name_or_null, tail_to_json};

pub(crate) fn exports_to_json(report: &DecodeReport<'_>) -> Value {
    let package = report.package;
    let export_full_names: Vec<String> = (0..package.exports.len())
        .map(|i| package.resolve_full_name((i as i32) + 1))
        .collect();

    let mut pin_name_by_id: HashMap<(i32, Guid), String> = HashMap::new();
    for export in &report.exports {
        if let Some(pins) = &export.pins {
            for p in pins {
                pin_name_by_id.insert((export.identity.index, p.pin_id), p.name.clone());
            }
        }
    }

    let arr: Vec<Value> = report
        .exports
        .iter()
        .map(|export| export_to_json(report, export, &pin_name_by_id, &export_full_names))
        .collect();
    Value::Array(arr)
}

fn export_to_json(
    report: &DecodeReport<'_>,
    export: &DecodedExport,
    pin_name_by_id: &HashMap<(i32, Guid), String>,
    export_full_names: &[String],
) -> Value {
    let package = report.package;
    let mut obj = serde_json::Map::new();
    obj.insert("index".into(), json!(export.identity.index));
    obj.insert("name".into(), json!(export.identity.name));
    obj.insert("class".into(), name_or_null(export.identity.class.clone()));
    if export.identity.is_asset {
        obj.insert("is_asset".into(), json!(true));
    }
    if let Some(layout) = &export.layout {
        obj.insert("super".into(), name_or_null(layout.super_name.clone()));
        obj.insert(
            "template".into(),
            name_or_null(layout.template_name.clone()),
        );
        obj.insert("outer".into(), name_or_null(layout.outer_name.clone()));
        obj.insert("full_name".into(), json!(layout.full_name));
        obj.insert(
            "object_flags".into(),
            json!(format!("0x{:08X}", layout.object_flags)),
        );
        obj.insert("serial_offset".into(), json!(layout.serial_offset));
        obj.insert("serial_size".into(), json!(layout.serial_size));
        if let Some(start) = layout.script_serialization_start {
            obj.insert("script_serialization_start".into(), json!(start));
        }
        if let Some(end) = layout.script_serialization_end {
            obj.insert("script_serialization_end".into(), json!(end));
        }
        if let Some(guid) = &export.object_guid {
            obj.insert("object_guid".into(), json!(guid));
        }
    }
    if let Some(member) = &export.member {
        obj.insert("member".into(), json!(member.name));
        if let Some(parent) = &member.parent {
            obj.insert("member_from".into(), parent.clone());
        }
    }
    if let Some(props) = &export.properties {
        obj.insert("properties".into(), entries_to_json(props));
    }
    if let Some(metadata) = &export.metadata {
        obj.insert("metadata".into(), metadata.clone());
    }
    if let Some(tail) = &export.post_property_tail {
        obj.insert("post_property_tail".into(), tail_to_json(tail));
    }
    if report.sections.pins
        && let Some(pins) = &export.pins
    {
        obj.insert(
            "pins".into(),
            pins_to_json(package, pins, pin_name_by_id, export_full_names),
        );
    }
    Value::Object(obj)
}
