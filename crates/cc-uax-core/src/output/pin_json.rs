use crate::package::Package;
use crate::pin::{Pin, PinRef, PinTerminalType, container_type_label, direction_label};
use crate::reader::Guid;
use serde_json::{Value, json};
use std::collections::HashMap;

use super::property_json::name_or_null;

pub(crate) fn pins_to_json(
    package: &Package,
    pins: &[Pin],
    names: &HashMap<(i32, Guid), String>,
    export_full_names: &[String],
) -> Value {
    let arr: Vec<Value> = pins
        .iter()
        .map(|p| {
            let mut o = serde_json::Map::new();
            o.insert("name".into(), json!(p.name));
            o.insert("direction".into(), json!(direction_label(p.direction)));
            if !p.category.is_empty() {
                o.insert("category".into(), json!(p.category));
            }
            if !p.sub_category.is_empty() {
                o.insert("sub_category".into(), json!(p.sub_category));
            }
            if p.sub_category_object != 0 {
                o.insert(
                    "sub_category_object".into(),
                    package.resolve_object_ref(p.sub_category_object),
                );
            }
            o.insert(
                "container_type".into(),
                json!(container_type_label(p.container_type)),
            );
            if let Some(value_type) = &p.value_type {
                o.insert(
                    "value_type".into(),
                    terminal_type_to_json(value_type, |idx| package.resolve_object_ref(idx)),
                );
            }
            o.insert("is_reference".into(), json!(p.is_reference));
            o.insert("is_weak_pointer".into(), json!(p.is_weak_pointer));
            o.insert("is_const".into(), json!(p.is_const));
            o.insert("is_uobject_wrapper".into(), json!(p.is_uobject_wrapper));
            o.insert(
                "serialize_as_single_precision_float".into(),
                json!(p.serialize_as_single_precision_float),
            );
            if p.member_parent != 0 || !p.member_name.is_empty() || !p.member_guid.is_zero() {
                let mut member = serde_json::Map::new();
                if p.member_parent != 0 {
                    member.insert("parent".into(), package.resolve_object_ref(p.member_parent));
                }
                if !p.member_name.is_empty() {
                    member.insert("name".into(), json!(p.member_name));
                }
                if !p.member_guid.is_zero() {
                    member.insert("guid".into(), json!(p.member_guid.to_hex()));
                }
                o.insert("member_reference".into(), Value::Object(member));
            }
            if !p.default_value.is_empty() {
                o.insert("default_value".into(), json!(p.default_value));
            }
            if p.default_object != 0 {
                o.insert(
                    "default_object".into(),
                    package.resolve_object_ref(p.default_object),
                );
            }
            o.insert("pin_id".into(), json!(p.pin_id.to_hex()));
            if !p.linked_to.is_empty() {
                let links: Vec<Value> = p
                    .linked_to
                    .iter()
                    .map(|r| link_to_json(package, r, names, export_full_names))
                    .collect();
                o.insert("linked_to".into(), Value::Array(links));
            }
            if !p.sub_pins.is_empty() {
                let links: Vec<Value> = p
                    .sub_pins
                    .iter()
                    .map(|r| link_to_json(package, r, names, export_full_names))
                    .collect();
                o.insert("sub_pins".into(), Value::Array(links));
            }
            if let Some(parent) = &p.parent_pin {
                o.insert(
                    "parent_pin".into(),
                    link_to_json(package, parent, names, export_full_names),
                );
            }
            if let Some(pass_through) = &p.reference_pass_through {
                o.insert(
                    "reference_pass_through".into(),
                    link_to_json(package, pass_through, names, export_full_names),
                );
            }
            if let Some(guid) = p.persistent_guid
                && !guid.is_zero()
            {
                o.insert("persistent_guid".into(), json!(guid.to_hex()));
            }
            if let Some(flags) = &p.editor_flags {
                o.insert(
                    "editor_flags".into(),
                    json!({
                        "hidden": flags.hidden,
                        "not_connectable": flags.not_connectable,
                        "default_value_read_only": flags.default_value_read_only,
                        "default_value_ignored": flags.default_value_ignored,
                        "advanced_view": flags.advanced_view,
                        "orphaned_pin": flags.orphaned_pin,
                    }),
                );
            }
            Value::Object(o)
        })
        .collect();
    Value::Array(arr)
}

fn link_to_json(
    package: &Package,
    r: &PinRef,
    names: &HashMap<(i32, Guid), String>,
    export_full_names: &[String],
) -> Value {
    let mut o = serde_json::Map::new();
    let node = if r.node_index > 0 {
        export_full_names
            .get((r.node_index - 1) as usize)
            .cloned()
            .unwrap_or_else(|| package.resolve_full_name(r.node_index))
    } else {
        package.resolve_full_name(r.node_index)
    };
    o.insert("node".into(), name_or_null(node));
    o.insert("node_index".into(), json!(r.node_index));
    match names.get(&(r.node_index, r.pin_id)) {
        Some(name) => {
            o.insert("pin".into(), json!(name));
        }
        None => {
            o.insert("pin_id".into(), json!(r.pin_id.to_hex()));
        }
    }
    Value::Object(o)
}

fn terminal_type_to_json<F>(ty: &PinTerminalType, resolve: F) -> Value
where
    F: Fn(i32) -> Value,
{
    let mut o = serde_json::Map::new();
    o.insert("category".into(), json!(ty.category));
    if !ty.sub_category.is_empty() {
        o.insert("sub_category".into(), json!(ty.sub_category));
    }
    if ty.sub_category_object != 0 {
        o.insert(
            "sub_category_object".into(),
            resolve(ty.sub_category_object),
        );
    }
    o.insert("is_const".into(), json!(ty.is_const));
    o.insert("is_weak_pointer".into(), json!(ty.is_weak_pointer));
    o.insert("is_uobject_wrapper".into(), json!(ty.is_uobject_wrapper));
    Value::Object(o)
}
