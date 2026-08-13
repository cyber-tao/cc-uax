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
        resolve_object: &|_idx: i32| crate::DecodedValue::Null,
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

fn parse_text_property_value(value: &[u8]) -> crate::DecodedValue {
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
        resolve_object: &|_idx: i32| crate::DecodedValue::Null,
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
        resolve_object: &|_idx: i32| crate::DecodedValue::Null,
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
        resolve_object: &|_idx: i32| crate::DecodedValue::Null,
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

fn parse_text_bytes(
    value: &[u8],
    serialization: crate::version::SerializationPolicy,
    filter_editor_only: bool,
) -> (crate::DecodedValue, u64, bool) {
    let names = NameMap {
        names: vec!["None".to_string()],
    };
    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| crate::DecodedValue::Null,
        pins: PinSerCtx {
            filter_editor_only,
            ..PinSerCtx::default()
        },
        soft_object_paths: &[],
        serialization,
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(value);
    match crate::property::parse_text(&mut r, &ctx, 0) {
        Ok(decoded) => (decoded, r.pos(), true),
        Err(_) => (crate::DecodedValue::Null, r.pos(), false),
    }
}

fn push_base_ftext(v: &mut Vec<u8>, source: &str) {
    push_u32(v, 0);
    v.push(0); // history = Base
    push_fstring(v, "NS");
    push_fstring(v, "KEY");
    push_fstring(v, source);
}

#[test]
fn text_base_history_reads_dev_notes_at_fortnite_main_260() {
    let mut v = Vec::new();
    push_base_ftext(&mut v, "Hello");
    push_fstring(&mut v, "editor note");
    let serialization = crate::version::SerializationPolicy {
        fortnite_main_version: crate::version::custom::ADD_DEV_NOTES_TO_FTEXT,
        ..Default::default()
    };
    let (value, pos, ok) = parse_text_bytes(&v, serialization, false);
    assert!(ok);
    assert_eq!(pos, v.len() as u64);
    assert_eq!(value["text"].as_str(), Some("Hello"));
    assert_eq!(value["namespace"].as_str(), Some("NS"));
}

#[test]
fn text_base_history_skips_dev_notes_below_260_and_when_filtered() {
    let mut v = Vec::new();
    push_base_ftext(&mut v, "Hello");
    push_fstring(&mut v, "editor note");
    let at_threshold_minus_one = crate::version::SerializationPolicy {
        fortnite_main_version: crate::version::custom::ADD_DEV_NOTES_TO_FTEXT - 1,
        ..Default::default()
    };
    let (value, pos, ok) = parse_text_bytes(&v, at_threshold_minus_one, false);
    assert!(ok);
    assert_eq!(value["text"].as_str(), Some("Hello"));
    assert!(
        pos < v.len() as u64,
        "DevNotes must remain unconsumed below 260"
    );

    let missing = crate::version::SerializationPolicy::default();
    let (_, missing_pos, missing_ok) = parse_text_bytes(&v, missing, false);
    assert!(missing_ok);
    assert!(missing_pos < v.len() as u64);

    let at_260 = crate::version::SerializationPolicy {
        fortnite_main_version: crate::version::custom::ADD_DEV_NOTES_TO_FTEXT,
        ..Default::default()
    };
    let (_, filtered_pos, filtered_ok) = parse_text_bytes(&v, at_260, true);
    assert!(filtered_ok);
    assert!(
        filtered_pos < v.len() as u64,
        "FilterEditorOnly packages do not write DevNotes"
    );
}

#[test]
fn text_base_history_dev_notes_truncation_fails() {
    let mut v = Vec::new();
    push_base_ftext(&mut v, "Hello");
    push_i32(&mut v, 8); // DevNotes length, but no payload
    let serialization = crate::version::SerializationPolicy {
        fortnite_main_version: crate::version::custom::ADD_DEV_NOTES_TO_FTEXT,
        ..Default::default()
    };
    let (_, _, ok) = parse_text_bytes(&v, serialization, false);
    assert!(!ok, "truncated DevNotes must fail rather than guess");
}

#[test]
fn format_argument_data_int_width_follows_release_stream_12() {
    let make = |int_bytes: &[u8]| {
        let mut v = Vec::new();
        push_u32(&mut v, 0);
        v.push(3u8); // ArgumentFormat
        push_base_ftext(&mut v, "{0}");
        push_i32(&mut v, 1);
        push_fstring(&mut v, "Count");
        v.push(0u8); // Int
        v.extend_from_slice(int_bytes);
        v
    };

    let i32_payload = make(&42i32.to_le_bytes());
    let below = crate::version::SerializationPolicy {
        ue5_release_stream_version: crate::version::custom::TEXT_FORMAT_ARGUMENT_DATA_64BIT_SUPPORT
            - 1,
        ..Default::default()
    };
    let (value, pos, ok) = parse_text_bytes(&i32_payload, below, true);
    assert!(ok);
    assert_eq!(pos, i32_payload.len() as u64);
    assert_eq!(value["history"].as_str(), Some("ArgumentFormat"));
    assert_eq!(value["arguments"][0]["value"].as_i64(), Some(42));

    let missing = crate::version::SerializationPolicy::default();
    let (missing_value, missing_pos, missing_ok) = parse_text_bytes(&i32_payload, missing, true);
    assert!(missing_ok);
    assert_eq!(missing_pos, i32_payload.len() as u64);
    assert_eq!(missing_value["arguments"][0]["value"].as_i64(), Some(42));

    let i64_payload = make(&42i64.to_le_bytes());
    let at = crate::version::SerializationPolicy {
        ue5_release_stream_version: crate::version::custom::TEXT_FORMAT_ARGUMENT_DATA_64BIT_SUPPORT,
        ..Default::default()
    };
    let (wide, wide_pos, wide_ok) = parse_text_bytes(&i64_payload, at, true);
    assert!(wide_ok);
    assert_eq!(wide_pos, i64_payload.len() as u64);
    assert_eq!(wide["arguments"][0]["value"].as_i64(), Some(42));
}

#[test]
fn format_argument_data_unknown_type_fails() {
    let mut v = Vec::new();
    push_u32(&mut v, 0);
    v.push(3u8);
    push_base_ftext(&mut v, "{0}");
    push_i32(&mut v, 1);
    push_fstring(&mut v, "X");
    v.push(1u8); // UInt is not valid on FFormatArgumentData
    let (_, _, ok) = parse_text_bytes(&v, crate::version::SerializationPolicy::default(), true);
    assert!(!ok);
}

#[test]
fn format_argument_value_gender_stays_u64() {
    // FFormatArgumentValue Gender is stored as UInt (u64). Do not confuse with
    // FFormatArgumentData Gender, which is uint8.
    let mut v = Vec::new();
    push_u32(&mut v, 0);
    v.push(2u8); // OrderedFormat
    push_base_ftext(&mut v, "{0}");
    push_i32(&mut v, 1);
    v.push(5u8); // Gender
    push_u64(&mut v, 2);
    let (value, pos, ok) =
        parse_text_bytes(&v, crate::version::SerializationPolicy::default(), true);
    assert!(ok);
    assert_eq!(pos, v.len() as u64);
    assert_eq!(value["arguments"][0].as_u64(), Some(2));
}
