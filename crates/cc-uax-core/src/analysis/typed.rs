use crate::model::{
    AssetExport, AssetProperty, DecodedValue, KnownOpaque, KnownOpaqueKind, OpaqueByteRange,
};
use std::collections::{BTreeMap, BTreeSet};

const PROPERTY_BAG_TYPE: &str = "InstancedPropertyBag";

pub(super) fn property<'a>(export: &'a AssetExport, name: &str) -> Option<&'a DecodedValue> {
    export
        .properties
        .iter()
        .find(|property| property.name == name)
        .map(|property| &property.value)
}

pub(super) fn object(value: &DecodedValue) -> Option<&BTreeMap<String, DecodedValue>> {
    value.as_object()
}

pub(super) fn array(value: &DecodedValue) -> Option<&[DecodedValue]> {
    value.as_array()
}

pub(super) fn string(value: &DecodedValue) -> Option<&str> {
    value.as_str()
}

pub(super) fn boolean(value: &DecodedValue) -> Option<bool> {
    value.as_bool()
}

pub(super) fn integer(value: &DecodedValue) -> Option<i64> {
    value.as_i64()
}

pub(super) fn float(value: &DecodedValue) -> Option<f64> {
    value.as_f64()
}

pub(super) fn object_ref_index(value: &DecodedValue) -> Option<i32> {
    object(value)
        .and_then(|value| value.get("index"))
        .and_then(integer)
        .and_then(|value| i32::try_from(value).ok())
        .filter(|value| *value > 0)
}

pub(super) fn object_ref_path(value: &DecodedValue) -> Option<&str> {
    object(value)
        .and_then(|value| value.get("ref"))
        .and_then(string)
}

pub(super) fn object_ref_indices(value: &DecodedValue) -> Vec<i32> {
    resolved_object_refs(value).0
}

/// Export indices from an array of object references, plus how many entries did
/// not yield one.
///
/// The count matters: a truncated or null-bearing reference array otherwise looks
/// identical to a short one, so an adapter that only takes the resolved indices
/// reports zero unresolved references for evidence it actually lost.
pub(super) fn resolved_object_refs(value: &DecodedValue) -> (Vec<i32>, usize) {
    let entries = array(value).unwrap_or_default();
    let resolved: Vec<i32> = entries.iter().filter_map(object_ref_index).collect();
    let dropped = entries.len().saturating_sub(resolved.len());
    (resolved, dropped)
}

pub(super) fn nested_property<'a>(value: &'a DecodedValue, name: &str) -> Option<&'a DecodedValue> {
    nested_property_entry(value, name).and_then(|entry| entry.get("value"))
}

pub(super) fn nested_property_entry<'a>(
    value: &'a DecodedValue,
    name: &str,
) -> Option<&'a BTreeMap<String, DecodedValue>> {
    object(value)
        .and_then(|value| value.get("properties"))
        .and_then(array)
        .into_iter()
        .flatten()
        .filter_map(object)
        .find(|entry| entry.get("name").and_then(string) == Some(name))
}

pub(super) fn nested_properties(value: &DecodedValue) -> Vec<AssetProperty> {
    object(value)
        .and_then(|value| value.get("properties"))
        .and_then(array)
        .into_iter()
        .flatten()
        .filter_map(asset_property_from_entry)
        .collect()
}

fn asset_property_from_entry(value: &DecodedValue) -> Option<AssetProperty> {
    let entry = object(value)?;
    Some(AssetProperty {
        name: entry.get("name").and_then(string)?.to_owned(),
        type_name: entry
            .get("type")
            .and_then(string)
            .unwrap_or_default()
            .to_owned(),
        array_index: entry
            .get("array_index")
            .and_then(integer)
            .and_then(|value| i32::try_from(value).ok())
            .unwrap_or_default(),
        value: entry.get("value").cloned().unwrap_or(DecodedValue::Null),
        guid: entry.get("guid").and_then(string).map(str::to_owned),
    })
}

pub(super) fn text(value: &DecodedValue) -> Option<String> {
    string(value).map(str::to_owned).or_else(|| {
        object(value)
            .and_then(|value| value.get("text"))
            .and_then(string)
            .map(str::to_owned)
    })
}

/// Every `FInstancedPropertyBag` in `exports` whose payload stayed opaque.
///
/// A bag falls back to a raw `serialized_data` blob when its property descriptors
/// depend on a reflection registry. Any adapter whose semantics are carried in
/// bags has that much less evidence, so the gap has to block its capability
/// rather than sit in the report unremarked.
pub(super) fn collect_property_bag_gaps(exports: &[AssetExport]) -> Vec<KnownOpaque> {
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

/// Escapes a property name for use as one RFC 6901 JSON-pointer segment.
fn json_pointer_segment(name: &str) -> String {
    name.replace('~', "~0").replace('/', "~1")
}
