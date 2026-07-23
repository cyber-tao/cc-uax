use crate::model::{AssetExport, AssetProperty, DecodedValue};
use std::collections::BTreeMap;

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
    array(value)
        .into_iter()
        .flatten()
        .filter_map(object_ref_index)
        .collect()
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
