use super::super::common::*;
use crate::name::NameMap;
use crate::pin::PinSerCtx;
use crate::property::{ParseCtx, parse_properties};
use crate::reader::Reader;

#[test]
fn text_property_unknown_history_falls_back_to_hex() {
    let names = NameMap {
        names: vec![
            "MyText".to_string(),
            "TextProperty".to_string(),
            "None".to_string(),
        ],
    };

    let mut d = Vec::new();
    push_i32(&mut d, 0); // property name FName index ("MyText")
    push_i32(&mut d, 0); // .number
    push_i32(&mut d, 1); // type name FName index ("TextProperty")
    push_i32(&mut d, 0); // .number
    push_i32(&mut d, 0); // type name inner param count
    push_i32(&mut d, 5); // size
    d.push(0); // flags
    push_u32(&mut d, 0); // FText flags
    d.push(4u8); // FText history_type = 4 (unhandled)
    push_i32(&mut d, 2); // terminator FName index ("None")
    push_i32(&mut d, 0); // .number

    let end = d.len() as u64;
    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, end);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "MyText");
    assert_eq!(entries[0].type_str, "TextProperty");
    let unparsed = entries[0].value.get("@unparsed").and_then(|v| v.as_str());
    assert_eq!(unparsed, Some("0000000004"));
}

fn parse_text_property_value(value: &[u8]) -> serde_json::Value {
    let names = NameMap {
        names: vec![
            "MyText".to_string(),
            "TextProperty".to_string(),
            "None".to_string(),
        ],
    };
    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // property name
    push_raw_name(&mut d, 1); // TextProperty
    push_i32(&mut d, 0); // type name inner param count
    push_i32(&mut d, value.len() as i32); // size
    d.push(0); // flags
    d.extend_from_slice(value);
    push_raw_name(&mut d, 2); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);
    assert_eq!(entries.len(), 1, "expected one property: {entries:?}");
    assert_eq!(entries[0].type_str, "TextProperty");
    entries[0].value.clone()
}

#[test]
fn text_history_as_number_decodes() {
    // FTextHistory_AsNumber: SourceValue(Double) + bHasFormatOptions + options + culture.
    let mut v = Vec::new();
    push_u32(&mut v, 0); // FText flags
    v.push(4u8); // history = AsNumber
    v.push(3u8); // FFormatArgumentValue type = Double
    push_f64(&mut v, 555.0); // SourceValue
    push_i32(&mut v, 1); // bHasFormatOptions = true
    push_i32(&mut v, 0); // always_sign
    push_i32(&mut v, 1); // use_grouping
    v.push(0u8); // rounding_mode
    push_i32(&mut v, 1); // minimum_integral_digits
    push_i32(&mut v, 324); // maximum_integral_digits
    push_i32(&mut v, 0); // minimum_fractional_digits
    push_i32(&mut v, 3); // maximum_fractional_digits
    push_fstring(&mut v, ""); // culture name
    assert_eq!(v.len(), 47);

    let value = parse_text_property_value(&v);
    assert_eq!(value["history"], "AsNumber");
    assert_eq!(value["source_value"].as_f64(), Some(555.0));
    assert_eq!(value["format_options"]["use_grouping"], true);
    assert_eq!(value["format_options"]["maximum_integral_digits"], 324);
    assert_eq!(value["culture"], "");
    assert!(value.get("@unparsed").is_none());
}

#[test]
fn text_history_as_number_without_options() {
    let mut v = Vec::new();
    push_u32(&mut v, 0); // FText flags
    v.push(5u8); // history = AsPercent
    v.push(0u8); // FFormatArgumentValue type = Int
    push_i64(&mut v, 42); // SourceValue
    push_i32(&mut v, 0); // bHasFormatOptions = false
    push_fstring(&mut v, "en"); // culture name

    let value = parse_text_property_value(&v);
    assert_eq!(value["history"], "AsPercent");
    assert_eq!(value["source_value"].as_i64(), Some(42));
    assert!(value.get("format_options").is_none());
    assert_eq!(value["culture"], "en");
}

#[test]
fn text_history_as_date_decodes() {
    let mut v = Vec::new();
    push_u32(&mut v, 0); // FText flags
    v.push(7u8); // history = AsDate
    push_i64(&mut v, 123_456_789); // SourceDateTime
    v.push(2u8); // DateStyle (int8)
    push_fstring(&mut v, "UTC"); // TimeZone
    push_fstring(&mut v, "en-US"); // Culture

    let value = parse_text_property_value(&v);
    assert_eq!(value["history"], "AsDate");
    assert_eq!(value["datetime"].as_i64(), Some(123_456_789));
    assert_eq!(value["date_style"], 2);
    assert_eq!(value["time_zone"], "UTC");
    assert_eq!(value["culture"], "en-US");
}

#[test]
fn text_history_transform_decodes_nested_text() {
    let mut v = Vec::new();
    push_u32(&mut v, 0); // FText flags
    v.push(10u8); // history = Transform
    // Nested source text: history -1, no culture-invariant string.
    push_u32(&mut v, 0); // nested flags
    v.push(0xFFu8); // nested history = -1 (None)
    push_i32(&mut v, 0); // has_culture_invariant = false
    v.push(1u8); // TransformType = ToUpper

    let value = parse_text_property_value(&v);
    assert_eq!(value["history"], "Transform");
    assert_eq!(value["transform_type"], 1);
    assert!(value["source"]["text"].is_null());
}

#[test]
fn text_ordered_format_decodes() {
    let names = NameMap {
        names: vec![
            "Label".to_string(),
            "TextProperty".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    push_u32(&mut value, 0); // outer FText flags
    value.push(2u8); // OrderedFormat
    push_u32(&mut value, 0); // nested format text flags
    value.push(0u8); // nested history = Base
    push_fstring(&mut value, ""); // namespace
    push_fstring(&mut value, "KEY"); // key
    push_fstring(&mut value, "{0} apples"); // source
    push_i32(&mut value, 1); // argument count
    value.push(0u8); // arg type 0 = Int
    push_i64(&mut value, 42);

    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // Label
    push_raw_name(&mut d, 1); // TextProperty
    push_i32(&mut d, 0); // type name inner param count
    push_i32(&mut d, value.len() as i32);
    d.push(0); // flags
    d.extend_from_slice(&value);
    push_raw_name(&mut d, 2); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let v = &entries[0].value;
    assert_eq!(v["history"].as_str(), Some("OrderedFormat"));
    assert_eq!(v["format"]["text"].as_str(), Some("{0} apples"));
    let args = v["arguments"].as_array().unwrap();
    assert_eq!(args.len(), 1);
    assert_eq!(args[0].as_i64(), Some(42));
}

#[test]
fn text_string_table_entry_decodes() {
    let names = NameMap {
        names: vec![
            "Label".to_string(),
            "TextProperty".to_string(),
            "MyTable".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    push_u32(&mut value, 0); // flags
    value.push(11u8); // StringTableEntry
    push_raw_name(&mut value, 2); // table id "MyTable"
    push_fstring(&mut value, "ENTRY_KEY");

    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // Label
    push_raw_name(&mut d, 1); // TextProperty
    push_i32(&mut d, 0);
    push_i32(&mut d, value.len() as i32);
    d.push(0);
    d.extend_from_slice(&value);
    push_raw_name(&mut d, 3); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let v = &entries[0].value;
    assert_eq!(v["history"].as_str(), Some("StringTableEntry"));
    assert_eq!(v["table_id"].as_str(), Some("MyTable"));
    assert_eq!(v["key"].as_str(), Some("ENTRY_KEY"));
}
