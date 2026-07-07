mod common;

use cc_uax::name::NameMap;
use cc_uax::pin::PinSerCtx;
use cc_uax::property::{
    ParseCtx, parse_object_properties, parse_properties, parse_properties_report,
};
use cc_uax::reader::Reader;
use common::*;

#[test]
fn nested_struct_respects_declared_value_end() {
    let names = NameMap {
        names: vec![
            "Outer".to_string(),
            "StructProperty".to_string(),
            "MyStruct".to_string(),
            "Inner".to_string(),
            "IntProperty".to_string(),
            "After".to_string(),
            "None".to_string(),
        ],
    };

    let mut nested = Vec::new();
    push_raw_name(&mut nested, 3); // Inner
    push_raw_name(&mut nested, 4); // IntProperty
    push_i32(&mut nested, 0); // type name inner param count
    push_i32(&mut nested, 4); // size
    nested.push(0); // flags
    push_i32(&mut nested, 123);

    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // Outer
    push_raw_name(&mut d, 1); // StructProperty
    push_i32(&mut d, 1); // one type parameter
    push_raw_name(&mut d, 2); // MyStruct
    push_i32(&mut d, 0);
    push_i32(&mut d, nested.len() as i32);
    d.push(0); // flags
    d.extend_from_slice(&nested);

    push_raw_name(&mut d, 5); // After
    push_raw_name(&mut d, 4); // IntProperty
    push_i32(&mut d, 0);
    push_i32(&mut d, 4);
    d.push(0);
    push_i32(&mut d, 456);

    push_raw_name(&mut d, 6); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "Outer");
    let nested_props = entries[0].value["properties"].as_array().unwrap();
    assert_eq!(nested_props.len(), 1);
    assert_eq!(nested_props[0]["name"], "Inner");
    assert_eq!(entries[1].name, "After");
    assert_eq!(entries[1].value.as_i64(), Some(456));
}

#[test]
fn truncated_property_array_index_stops_parse() {
    let names = NameMap {
        names: vec![
            "Broken".to_string(),
            "IntProperty".to_string(),
            "None".to_string(),
        ],
    };
    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // Broken
    push_raw_name(&mut d, 1); // IntProperty
    push_i32(&mut d, 0); // type name inner param count
    push_i32(&mut d, 0); // declared value size
    d.push(0x01); // flags say array_index follows, but it is truncated

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);
    assert!(entries.is_empty());
}

#[test]
fn excessive_array_count_falls_back_to_hex() {
    let names = NameMap {
        names: vec![
            "Nums".to_string(),
            "ArrayProperty".to_string(),
            "IntProperty".to_string(),
            "None".to_string(),
        ],
    };
    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // Nums
    push_raw_name(&mut d, 1); // ArrayProperty
    push_i32(&mut d, 1); // one type parameter
    push_raw_name(&mut d, 2); // IntProperty
    push_i32(&mut d, 0);
    push_i32(&mut d, 4); // value is only the count
    d.push(0);
    push_i32(&mut d, 1_000_001);
    push_raw_name(&mut d, 3); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].value.get("@unparsed").and_then(|v| v.as_str()),
        Some("41420f00")
    );
}

#[test]
fn property_value_fallback_reports_diagnostic_context() {
    let names = NameMap {
        names: vec![
            "Nums".to_string(),
            "ArrayProperty".to_string(),
            "IntProperty".to_string(),
            "None".to_string(),
        ],
    };
    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // Nums
    push_raw_name(&mut d, 1); // ArrayProperty
    push_i32(&mut d, 1); // one type param
    push_raw_name(&mut d, 2); // IntProperty
    push_i32(&mut d, 0);
    push_i32(&mut d, 4); // value is only the array count
    d.push(0);
    let value_start = d.len() as u64;
    push_i32(&mut d, 1_000_001);
    push_raw_name(&mut d, 3); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let report = parse_properties_report(&mut r, &ctx, d.len() as u64, "/exports/0/properties");

    assert_eq!(report.entries.len(), 1);
    assert_eq!(report.entries[0].value["@unparsed"], "41420f00");
    let diag = report
        .diagnostics
        .iter()
        .find(|diag| diag.code == "property_value_fallback")
        .expect("fallback diagnostic should be emitted");
    assert_eq!(diag.path, "/exports/0/properties/Nums");
    assert_eq!(diag.offset, Some(value_start));
    let context = diag.context.as_ref().unwrap();
    assert_eq!(context["property"], "Nums");
    assert_eq!(context["type"], "ArrayProperty(IntProperty)");
    assert_eq!(context["size"], 4);
    assert_eq!(context["preview"], "41420f00");
}

#[test]
fn native_struct_array_falls_back_to_hex() {
    let names = NameMap {
        names: vec![
            "NativeArray".to_string(),
            "ArrayProperty".to_string(),
            "StructProperty".to_string(),
            "UnknownNative".to_string(),
            "None".to_string(),
        ],
    };

    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // NativeArray
    push_raw_name(&mut d, 1); // ArrayProperty
    push_i32(&mut d, 1);
    push_raw_name(&mut d, 2); // StructProperty
    push_i32(&mut d, 1);
    push_raw_name(&mut d, 3); // UnknownNative
    push_i32(&mut d, 0);
    push_i32(&mut d, 8); // count + one opaque 4-byte element
    d.push(0x08); // binary/native value
    push_i32(&mut d, 1); // array count
    d.extend_from_slice(&[0xaa, 0xbb, 0xcc, 0xdd]);
    push_raw_name(&mut d, 4); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let unparsed = entries[0].value.get("@unparsed").and_then(|v| v.as_str());
    assert_eq!(unparsed, Some("01000000aabbccdd"));
}

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
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, end);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "MyText");
    assert_eq!(entries[0].type_str, "TextProperty");
    let unparsed = entries[0].value.get("@unparsed").and_then(|v| v.as_str());
    assert_eq!(unparsed, Some("0000000004"));
}

#[test]
fn native_struct_box_decodes() {
    let names = NameMap {
        names: vec![
            "MyBox".to_string(),
            "StructProperty".to_string(),
            "Box".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    for x in [1.0, 2.0, 3.0, 4.0, 5.0, 6.0] {
        push_f64(&mut value, x);
    }
    value.push(1); // is_valid
    assert_eq!(value.len(), 49);
    let d = build_struct_property(2, 3, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].value["is_valid"].as_bool(), Some(true));
    assert_eq!(entries[0].value["min"]["x"].as_f64(), Some(1.0));
    assert_eq!(entries[0].value["max"]["z"].as_f64(), Some(6.0));
}

#[test]
fn native_struct_box2f_decodes() {
    let names = NameMap {
        names: vec![
            "MyBox".to_string(),
            "StructProperty".to_string(),
            "Box2f".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    for x in [1.0f32, 2.0, 3.0, 4.0] {
        push_f32(&mut value, x);
    }
    value.push(1); // bIsValid (single uint8)
    assert_eq!(value.len(), 17);
    let d = build_struct_property(2, 3, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].value["min"]["x"].as_f64(), Some(1.0));
    assert_eq!(entries[0].value["max"]["y"].as_f64(), Some(4.0));
    assert_eq!(entries[0].value["is_valid"].as_bool(), Some(true));
}

// Wrap raw FText `value` bytes as a single TextProperty, parse it, return the value.
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
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
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
fn legacy_property_tags_decode_type_metadata() {
    let names = NameMap {
        names: vec![
            "BoolVal".to_string(),          // 0
            "BoolProperty".to_string(),     // 1
            "ByteVal".to_string(),          // 2
            "ByteProperty".to_string(),     // 3
            "MyEnum".to_string(),           // 4
            "EnumValue".to_string(),        // 5
            "StructVal".to_string(),        // 6
            "StructProperty".to_string(),   // 7
            "Vector".to_string(),           // 8
            "ArrayVal".to_string(),         // 9
            "ArrayProperty".to_string(),    // 10
            "IntProperty".to_string(),      // 11
            "SetVal".to_string(),           // 12
            "SetProperty".to_string(),      // 13
            "MapVal".to_string(),           // 14
            "MapProperty".to_string(),      // 15
            "OptionalVal".to_string(),      // 16
            "OptionalProperty".to_string(), // 17
            "GuidVal".to_string(),          // 18
            "None".to_string(),             // 19
        ],
    };
    let mut d = Vec::new();
    d.push(0); // object serialization control byte

    push_legacy_tag_header(&mut d, 0, 1, 0);
    d.push(1); // BoolVal
    push_legacy_tag_tail(&mut d);

    push_legacy_tag_header(&mut d, 2, 3, 8);
    push_raw_name(&mut d, 4); // EnumName
    push_legacy_tag_tail(&mut d);
    push_raw_name(&mut d, 5); // enum value

    push_legacy_tag_header(&mut d, 6, 7, 24);
    push_raw_name(&mut d, 8); // StructName
    d.extend_from_slice(&[0u8; 16]); // StructGuid
    push_legacy_tag_tail(&mut d);
    push_f64(&mut d, 1.0);
    push_f64(&mut d, 2.0);
    push_f64(&mut d, 3.0);

    push_legacy_tag_header(&mut d, 9, 10, 12);
    push_raw_name(&mut d, 11); // InnerType
    push_legacy_tag_tail(&mut d);
    push_i32(&mut d, 2);
    push_i32(&mut d, 7);
    push_i32(&mut d, 8);

    push_legacy_tag_header(&mut d, 12, 13, 16);
    push_raw_name(&mut d, 11); // InnerType
    push_legacy_tag_tail(&mut d);
    push_i32(&mut d, 0); // NumToRemove
    push_i32(&mut d, 2);
    push_i32(&mut d, 9);
    push_i32(&mut d, 10);

    push_legacy_tag_header(&mut d, 14, 15, 16);
    push_raw_name(&mut d, 11); // Key InnerType
    push_raw_name(&mut d, 11); // ValueType
    push_legacy_tag_tail(&mut d);
    push_i32(&mut d, 0); // NumKeysToRemove
    push_i32(&mut d, 1);
    push_i32(&mut d, 3);
    push_i32(&mut d, 4);

    push_legacy_tag_header(&mut d, 16, 17, 8);
    push_raw_name(&mut d, 11); // InnerType
    push_legacy_tag_tail(&mut d);
    push_i32(&mut d, 1); // optional is set
    push_i32(&mut d, 77);

    push_legacy_tag_header(&mut d, 18, 11, 4);
    push_legacy_tag_tail_with_guid(&mut d);
    push_i32(&mut d, 99);

    push_raw_name(&mut d, 19); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5:
            cc_uax::version::ue5::PROPERTY_TAG_EXTENSION_AND_OVERRIDABLE_SERIALIZATION,
    };
    let mut r = Reader::new(&d);
    let props = parse_object_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(r.pos(), d.len() as u64);
    assert_eq!(props.len(), 8);
    assert_eq!(props[0].value.as_bool(), Some(true));
    assert_eq!(props[1].type_str, "ByteProperty(MyEnum)");
    assert_eq!(props[1].value.as_str(), Some("EnumValue"));
    assert_eq!(props[2].type_str, "StructProperty(Vector)");
    assert_eq!(props[2].value["x"].as_f64(), Some(1.0));
    assert_eq!(props[3].value.as_array().unwrap()[1].as_i64(), Some(8));
    assert_eq!(props[4].value.as_array().unwrap()[1].as_i64(), Some(10));
    assert_eq!(
        props[5].value.as_array().unwrap()[0]["key"].as_i64(),
        Some(3)
    );
    assert_eq!(
        props[5].value.as_array().unwrap()[0]["value"].as_i64(),
        Some(4)
    );
    assert_eq!(props[6].value.as_i64(), Some(77));
    assert_eq!(
        props[7].guid.as_deref(),
        Some("00000001000000020000000300000004")
    );
    assert_eq!(props[7].value.as_i64(), Some(99));
}

#[test]
fn property_tag_extensions_are_byte_aligned() {
    // A tag with HasPropertyExtensions (0x04) carries a 6-byte extension block in a
    // binary archive: uint8 flags (no presence prefix — SA_ATTRIBUTE), uint8 override
    // op, 4-byte experimental bool. If the block is mis-sized the following
    // value/property desyncs, so decoding the int value proves alignment.
    let names = NameMap {
        names: vec![
            "MyInt".to_string(),
            "IntProperty".to_string(),
            "None".to_string(),
        ],
    };
    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // MyInt
    push_raw_name(&mut d, 1); // IntProperty
    push_i32(&mut d, 0); // type name inner param count
    push_i32(&mut d, 4); // value size
    d.push(0x04); // flags = HasPropertyExtensions
    d.push(0x02); // extension flags = OverridableInformation
    d.push(0x00); // override operation
    push_i32(&mut d, 0); // bExperimentalOverridableLogic bool
    push_i32(&mut d, 12345); // IntProperty value
    push_raw_name(&mut d, 2); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "MyInt");
    assert_eq!(entries[0].value.as_i64(), Some(12345));
}

#[test]
fn skipped_serialize_property_is_marked_and_parsing_continues() {
    let names = NameMap {
        names: vec![
            "Skipped".to_string(),
            "IntProperty".to_string(),
            "None".to_string(),
            "After".to_string(),
        ],
    };
    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // Skipped
    push_raw_name(&mut d, 1); // IntProperty
    push_i32(&mut d, 0); // type name inner param count
    push_i32(&mut d, 0); // size = 0 (value skipped)
    d.push(0x20); // flags = SkippedSerialize
    push_raw_name(&mut d, 3); // After
    push_raw_name(&mut d, 1); // IntProperty
    push_i32(&mut d, 0);
    push_i32(&mut d, 4);
    d.push(0);
    push_i32(&mut d, 99);
    push_raw_name(&mut d, 2); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "Skipped");
    assert_eq!(entries[0].value["@skipped"].as_bool(), Some(true));
    assert_eq!(entries[1].name, "After");
    assert_eq!(entries[1].value.as_i64(), Some(99));
}

#[test]
fn native_struct_gameplay_tag_container_decodes() {
    let names = NameMap {
        names: vec![
            "Tags".to_string(),
            "StructProperty".to_string(),
            "GameplayTagContainer".to_string(),
            "Ability.Attack".to_string(),
            "Ability.Dash".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    push_i32(&mut value, 2); // tag count
    push_raw_name(&mut value, 3); // Ability.Attack
    push_raw_name(&mut value, 4); // Ability.Dash
    assert_eq!(value.len(), 4 + 2 * 8);
    let d = build_struct_property(2, 5, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let tags = entries[0].value["tags"].as_array().unwrap();
    assert_eq!(tags.len(), 2);
    assert_eq!(tags[0].as_str(), Some("Ability.Attack"));
    assert_eq!(tags[1].as_str(), Some("Ability.Dash"));
}

#[test]
fn native_struct_vector4f_decodes() {
    let names = NameMap {
        names: vec![
            "V".to_string(),
            "StructProperty".to_string(),
            "Vector4f".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    for x in [1.0f32, 2.0, 3.0, 4.0] {
        push_f32(&mut value, x);
    }
    assert_eq!(value.len(), 16);
    let d = build_struct_property(2, 3, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].value["x"].as_f64(), Some(1.0));
    assert_eq!(entries[0].value["w"].as_f64(), Some(4.0));
}

#[test]
fn native_struct_skeletal_mesh_sampling_lod_built_data_decodes() {
    let names = NameMap {
        names: vec![
            "Sampler".to_string(),
            "StructProperty".to_string(),
            "SkeletalMeshSamplingLODBuiltData".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    push_i32(&mut value, 1); // Prob count
    push_f32(&mut value, 0.25);
    push_i32(&mut value, 1); // Alias count
    push_i32(&mut value, 7);
    push_f32(&mut value, 2.5); // TotalWeight
    let d = build_struct_property(2, 3, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let sampler = &entries[0].value["area_weighted_triangle_sampler"];
    assert_eq!(sampler["prob"][0].as_f64(), Some(0.25));
    assert_eq!(sampler["alias"][0].as_i64(), Some(7));
    assert_eq!(sampler["total_weight"].as_f64(), Some(2.5));
    assert!(entries[0].value.get("@unparsed").is_none());
}

#[test]
fn native_struct_niagara_variable_decodes() {
    let names = NameMap {
        names: vec![
            "Var".to_string(),             // 0 property name
            "StructProperty".to_string(),  // 1
            "NiagaraVariable".to_string(), // 2 struct name
            "Particles.Color".to_string(), // 3 FName Name
            "None".to_string(),            // 4 terminator
            "Flags".to_string(),           // 5 typedef property
            "IntProperty".to_string(),     // 6
        ],
    };
    let mut value = Vec::new();
    push_raw_name(&mut value, 3); // Name = Particles.Color
    // FNiagaraTypeDefinition tagged properties: IntProperty Flags = 1, then None.
    push_raw_name(&mut value, 5); // Flags
    push_raw_name(&mut value, 6); // IntProperty
    push_i32(&mut value, 0); // type name inner param count
    push_i32(&mut value, 4); // size
    value.push(0); // flags
    push_i32(&mut value, 1); // value
    push_raw_name(&mut value, 4); // None (ends type definition)
    push_i32(&mut value, 0); // VarData count = 0
    let d = build_struct_property(2, 4, &value);

    // Niagara version below the gate must fall back to hex.
    let mut ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: 0,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);
    assert!(entries[0].value.get("@unparsed").is_some());

    // Modern Niagara version decodes Name + type definition + empty VarData.
    ctx.niagara_version = 64;
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);
    assert_eq!(entries.len(), 1);
    let v = &entries[0].value;
    assert_eq!(v["name"].as_str(), Some("Particles.Color"));
    assert_eq!(v["type"]["@struct"].as_str(), Some("NiagaraTypeDefinition"));
    let tprops = v["type"]["properties"].as_array().unwrap();
    assert_eq!(tprops.len(), 1);
    assert_eq!(tprops[0]["name"].as_str(), Some("Flags"));
    assert_eq!(tprops[0]["value"].as_i64(), Some(1));
    assert_eq!(v["data_size"].as_i64(), Some(0));
}

#[test]
fn native_struct_niagara_gpu_param_info_decodes() {
    let names = NameMap {
        names: vec![
            "GPU".to_string(),
            "StructProperty".to_string(),
            "NiagaraDataInterfaceGPUParamInfo".to_string(),
            "GetData".to_string(),
            "Key".to_string(),
            "Value".to_string(),
            "InputVar".to_string(),
            "OutputVar".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    push_fstring(&mut value, "DI_Spline");
    push_fstring(&mut value, "NiagaraDataInterfaceSpline");
    push_i32(&mut value, 1); // GeneratedFunctions count
    push_raw_name(&mut value, 3); // DefinitionName
    push_fstring(&mut value, "DI_Spline_GetData"); // InstanceName
    push_i32(&mut value, 1); // Specifiers count
    push_raw_name(&mut value, 4); // Key
    push_raw_name(&mut value, 5); // Value
    push_i32(&mut value, 1); // VariadicInputs count
    push_raw_name(&mut value, 6); // InputVar
    push_i32(&mut value, -2); // UnderlyingType import
    push_i32(&mut value, 1); // VariadicOutputs count
    push_raw_name(&mut value, 7); // OutputVar
    push_i32(&mut value, 0); // null UnderlyingType
    push_u16(&mut value, 3); // MiscUsageBitMask
    let d = build_struct_property(2, 8, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|idx: i32| serde_json::json!({ "idx": idx }),
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version:
            cc_uax::version::custom::NIAGARA_SERIALIZE_USAGE_BITMASK_TO_GPU_FUNCTION_INFO,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    assert!(entries[0].value.get("@unparsed").is_none());
    let v = &entries[0].value;
    assert_eq!(v["data_interface_hlsl_symbol"].as_str(), Some("DI_Spline"));
    assert_eq!(
        v["di_class_name"].as_str(),
        Some("NiagaraDataInterfaceSpline")
    );
    let f = &v["generated_functions"][0];
    assert_eq!(f["definition_name"].as_str(), Some("GetData"));
    assert_eq!(f["specifiers"][0]["key"].as_str(), Some("Key"));
    assert_eq!(
        f["variadic_inputs"][0]["underlying_type"]["idx"].as_i64(),
        Some(-2)
    );
    assert_eq!(f["misc_usage_bitmask"].as_u64(), Some(3));
}

#[test]
fn native_struct_spline_empty_decodes() {
    let names = NameMap {
        names: vec![
            "Spl".to_string(),
            "StructProperty".to_string(),
            "Spline".to_string(),
            "None".to_string(),
        ],
    };
    let value = vec![0u8]; // int8 implementation tag = 0 (empty spline)
    let d = build_struct_property(2, 3, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].value["implementation"].as_str(), Some("empty"));
}

#[test]
fn optional_property_decodes_set_and_unset() {
    let names = NameMap {
        names: vec![
            "OptSet".to_string(),           // 0
            "OptionalProperty".to_string(), // 1
            "BoolProperty".to_string(),     // 2
            "OptUnset".to_string(),         // 3
            "None".to_string(),             // 4
        ],
    };
    let mut d = Vec::new();
    // Set optional bool = true: presence(bool32)=1 + inner bool byte=1.
    push_raw_name(&mut d, 0); // OptSet
    push_raw_name(&mut d, 1); // OptionalProperty
    push_i32(&mut d, 1); // one inner type param
    push_raw_name(&mut d, 2); // BoolProperty
    push_i32(&mut d, 0); // inner param count
    push_i32(&mut d, 5); // size
    d.push(0); // flags
    push_i32(&mut d, 1); // presence = set
    d.push(1); // inner bool value
    // Unset optional bool: presence(bool32)=0 only.
    push_raw_name(&mut d, 3); // OptUnset
    push_raw_name(&mut d, 1); // OptionalProperty
    push_i32(&mut d, 1);
    push_raw_name(&mut d, 2); // BoolProperty
    push_i32(&mut d, 0);
    push_i32(&mut d, 4); // size
    d.push(0); // flags
    push_i32(&mut d, 0); // presence = unset
    push_raw_name(&mut d, 4); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "OptSet");
    assert_eq!(entries[0].value.as_bool(), Some(true));
    assert_eq!(entries[1].name, "OptUnset");
    assert!(entries[1].value.is_null());
}

#[test]
fn native_struct_gameplay_effect_version_decodes() {
    let names = NameMap {
        names: vec![
            "Ver".to_string(),
            "StructProperty".to_string(),
            "GameplayEffectVersion".to_string(),
            "None".to_string(),
        ],
    };
    let value = vec![2u8]; // EGameplayEffectVersion::AbilitiesComponent53
    let d = build_struct_property(2, 3, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].value["current_version"].as_u64(), Some(2));
    assert_eq!(
        entries[0].value["name"].as_str(),
        Some("AbilitiesComponent53")
    );
}

#[test]
fn float_curve_parses_as_tagged_fallback() {
    let names = NameMap {
        names: vec![
            "Curve".to_string(),
            "StructProperty".to_string(),
            "FloatCurve".to_string(),
            "CurveTypeFlags".to_string(),
            "IntProperty".to_string(),
            "None".to_string(),
        ],
    };
    // FFloatCurve defers to tagged properties: IntProperty CurveTypeFlags = 3.
    let mut value = Vec::new();
    push_raw_name(&mut value, 3); // CurveTypeFlags
    push_raw_name(&mut value, 4); // IntProperty
    push_i32(&mut value, 0); // type name inner param count
    push_i32(&mut value, 4); // size
    value.push(0); // flags
    push_i32(&mut value, 3); // value
    push_raw_name(&mut value, 5); // None
    let d = build_struct_property(2, 5, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].value["@struct"].as_str(), Some("FloatCurve"));
    let props = entries[0].value["properties"].as_array().unwrap();
    assert_eq!(props.len(), 1);
    assert_eq!(props[0]["name"].as_str(), Some("CurveTypeFlags"));
    assert_eq!(props[0]["value"].as_i64(), Some(3));
}

#[test]
fn native_struct_rich_curve_key_array_keeps_stride() {
    let names = NameMap {
        names: vec![
            "Keys".to_string(),
            "ArrayProperty".to_string(),
            "StructProperty".to_string(),
            "RichCurveKey".to_string(),
            "None".to_string(),
        ],
    };
    fn push_key(v: &mut Vec<u8>, interp: u8, time: f32, value: f32) {
        v.push(interp); // interp mode
        v.push(0); // tangent mode
        v.push(0); // tangent weight mode
        push_f32(v, time);
        push_f32(v, value);
        push_f32(v, 0.0); // arrive tangent
        push_f32(v, 0.0); // arrive tangent weight
        push_f32(v, 0.0); // leave tangent
        push_f32(v, 0.0); // leave tangent weight
    }
    let mut value = Vec::new();
    push_i32(&mut value, 2); // array count
    push_key(&mut value, 2, 0.0, 10.0);
    push_key(&mut value, 3, 1.0, 20.0);
    assert_eq!(value.len(), 4 + 2 * 27);

    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // Keys
    push_raw_name(&mut d, 1); // ArrayProperty
    push_i32(&mut d, 1); // one param
    push_raw_name(&mut d, 2); // StructProperty
    push_i32(&mut d, 1); // one param
    push_raw_name(&mut d, 3); // RichCurveKey
    push_i32(&mut d, 0);
    push_i32(&mut d, value.len() as i32);
    d.push(0x08);
    d.extend_from_slice(&value);
    push_raw_name(&mut d, 4); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let arr = entries[0].value.as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["interp_mode"].as_u64(), Some(2));
    assert_eq!(arr[0]["value"].as_f64(), Some(10.0));
    assert_eq!(arr[1]["interp_mode"].as_u64(), Some(3));
    assert_eq!(arr[1]["value"].as_f64(), Some(20.0));
    assert_eq!(arr[1]["time"].as_f64(), Some(1.0));
}

#[test]
fn material_scalar_input_resolves_expression() {
    let names = NameMap {
        names: vec![
            "Input".to_string(),
            "StructProperty".to_string(),
            "ScalarMaterialInput".to_string(),
            "R".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    push_i32(&mut value, -5); // expression object index
    push_i32(&mut value, 1); // output index
    push_raw_name(&mut value, 3); // input name "R"
    for m in [1, 1, 0, 0, 0] {
        push_i32(&mut value, m); // mask, maskR..maskA
    }
    push_i32(&mut value, 1); // use constant (bool32)
    push_f32(&mut value, 0.5); // constant
    assert_eq!(value.len(), 44);
    let d = build_struct_property(2, 4, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|idx: i32| serde_json::json!({ "index": idx }),
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let v = &entries[0].value;
    assert_eq!(v["expression"]["index"].as_i64(), Some(-5));
    assert_eq!(v["input_name"].as_str(), Some("R"));
    assert_eq!(v["output_index"].as_i64(), Some(1));
    assert_eq!(v["use_constant"].as_bool(), Some(true));
    assert_eq!(v["constant"].as_f64(), Some(0.5));
    assert_eq!(v["mask"].as_array().unwrap().len(), 5);
}

#[test]
fn native_struct_per_platform_float_decodes() {
    let names = NameMap {
        names: vec![
            "Scale".to_string(),
            "StructProperty".to_string(),
            "PerPlatformFloat".to_string(),
            "Mobile".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    push_i32(&mut value, 0); // bCooked = false
    push_f32(&mut value, 1.0); // default
    push_i32(&mut value, 1); // map count
    push_raw_name(&mut value, 3); // "Mobile"
    push_f32(&mut value, 0.5); // override value
    let d = build_struct_property(2, 4, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let v = &entries[0].value;
    assert_eq!(v["default"].as_f64(), Some(1.0));
    let pp = v["per_platform"].as_array().unwrap();
    assert_eq!(pp.len(), 1);
    assert_eq!(pp[0]["platform"].as_str(), Some("Mobile"));
    assert_eq!(pp[0]["value"].as_f64(), Some(0.5));
}

#[test]
fn native_struct_movie_scene_frame_range_decodes() {
    let names = NameMap {
        names: vec![
            "Range".to_string(),
            "StructProperty".to_string(),
            "MovieSceneFrameRange".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    value.push(1); // lower bound type (inclusive)
    push_i32(&mut value, 10);
    value.push(2); // upper bound type (exclusive)
    push_i32(&mut value, 100);
    assert_eq!(value.len(), 10);
    let d = build_struct_property(2, 3, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let v = &entries[0].value;
    assert_eq!(v["lower_bound"].as_i64(), Some(10));
    assert_eq!(v["upper_bound"].as_i64(), Some(100));
    assert_eq!(v["lower_bound_type"].as_u64(), Some(1));
    assert_eq!(v["upper_bound_type"].as_u64(), Some(2));
}

#[test]
fn native_struct_movie_scene_float_channel_decodes() {
    let names = NameMap {
        names: vec![
            "Channel".to_string(),
            "StructProperty".to_string(),
            "MovieSceneFloatChannel".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    value.push(4); // pre-infinity extrap
    value.push(4); // post-infinity extrap
    push_i32(&mut value, 4); // times element size
    push_i32(&mut value, 1); // times count
    push_i32(&mut value, 7); // frame number
    push_i32(&mut value, 28); // values element size
    push_i32(&mut value, 1); // values count
    // one 28-byte FMovieSceneFloatValue
    push_f32(&mut value, 1.5); // value (offset 0)
    push_f32(&mut value, 0.0); // arrive tangent
    push_f32(&mut value, 0.0); // leave tangent
    push_f32(&mut value, 0.0); // arrive tangent weight
    push_f32(&mut value, 0.0); // leave tangent weight
    value.push(0); // tangent weight mode (offset 20)
    value.extend_from_slice(&[0, 0, 0]); // tangent padding
    value.push(2); // interp mode (offset 24)
    value.push(1); // tangent mode (offset 25)
    value.push(0); // padding byte
    value.push(0); // unserialized padding
    push_f32(&mut value, 9.0); // default value
    push_i32(&mut value, 0); // has default value (false)
    push_i32(&mut value, 30); // tick numerator
    push_i32(&mut value, 1); // tick denominator
    push_i32(&mut value, 0); // show curve (false)
    assert_eq!(value.len(), 70);
    let d = build_struct_property(2, 3, &value);

    // bShowCurve is gated on FFortniteMainBranchObjectVersion >= 53.
    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: 53,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let v = &entries[0].value;
    assert_eq!(v["times"].as_array().unwrap()[0].as_i64(), Some(7));
    let vals = v["values"].as_array().unwrap();
    assert_eq!(vals.len(), 1);
    assert_eq!(vals[0]["value"].as_f64(), Some(1.5));
    assert_eq!(vals[0]["interp_mode"].as_u64(), Some(2));
    assert_eq!(vals[0]["tangent_mode"].as_u64(), Some(1));
    assert_eq!(v["default_value"].as_f64(), Some(9.0));
    assert_eq!(v["tick_resolution"]["numerator"].as_i64(), Some(30));
    assert_eq!(v["show_curve"].as_bool(), Some(false));
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
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
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
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let v = &entries[0].value;
    assert_eq!(v["history"].as_str(), Some("StringTableEntry"));
    assert_eq!(v["table_id"].as_str(), Some("MyTable"));
    assert_eq!(v["key"].as_str(), Some("ENTRY_KEY"));
}

#[test]
fn multicast_inline_delegate_decodes() {
    let names = NameMap {
        names: vec![
            "OnFire".to_string(),
            "MulticastInlineDelegateProperty".to_string(),
            "HandleFire".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    push_i32(&mut value, 1); // invocation count
    push_i32(&mut value, -3); // object index
    push_raw_name(&mut value, 2); // function name
    assert_eq!(value.len(), 16);

    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // OnFire
    push_raw_name(&mut d, 1); // MulticastInlineDelegateProperty
    push_i32(&mut d, 0);
    push_i32(&mut d, value.len() as i32);
    d.push(0);
    d.extend_from_slice(&value);
    push_raw_name(&mut d, 3); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|idx: i32| serde_json::json!({ "index": idx }),
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let arr = entries[0].value.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["function"].as_str(), Some("HandleFire"));
    assert_eq!(arr[0]["object"]["index"].as_i64(), Some(-3));
}

#[test]
fn native_struct_instanced_struct_decodes() {
    let names = NameMap {
        names: vec![
            "Data".to_string(),
            "StructProperty".to_string(),
            "InstancedStruct".to_string(),
            "Inner".to_string(),
            "IntProperty".to_string(),
            "None".to_string(),
        ],
    };
    // Inner struct tagged properties: one IntProperty "Inner" = 99, then None.
    let mut inner = Vec::new();
    push_raw_name(&mut inner, 3); // Inner
    push_raw_name(&mut inner, 4); // IntProperty
    push_i32(&mut inner, 0); // type name inner param count
    push_i32(&mut inner, 4); // size
    inner.push(0); // flags
    push_i32(&mut inner, 99);
    push_raw_name(&mut inner, 5); // None

    let mut value = Vec::new();
    push_i32(&mut value, -7); // script struct object index
    push_i32(&mut value, inner.len() as i32); // serial size
    value.extend_from_slice(&inner);
    let d = build_struct_property(2, 5, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|idx: i32| serde_json::json!({ "index": idx }),
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let v = &entries[0].value;
    assert_eq!(v["script_struct"]["index"].as_i64(), Some(-7));
    let props = v["properties"].as_array().unwrap();
    assert_eq!(props.len(), 1);
    assert_eq!(props[0]["name"].as_str(), Some("Inner"));
    assert_eq!(props[0]["value"].as_i64(), Some(99));
}

#[test]
fn native_struct_edgraph_pin_type_decodes() {
    let names = NameMap {
        names: vec![
            "PinType".to_string(),
            "StructProperty".to_string(),
            "EdGraphPinType".to_string(),
            "int".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    push_raw_name(&mut value, 3); // category = "int"
    push_raw_name(&mut value, 4); // sub_category = "None"
    push_i32(&mut value, -9); // sub_category_object
    value.push(0); // container_type = None
    push_i32(&mut value, 0); // bIsReference
    push_i32(&mut value, 0); // bIsWeakPointer
    push_i32(&mut value, 0); // member parent
    push_raw_name(&mut value, 4); // member name = "None"
    value.extend_from_slice(&[0u8; 16]); // member guid
    push_i32(&mut value, 0); // bIsConst
    push_i32(&mut value, 0); // bIsUObjectWrapper
    push_i32(&mut value, 0); // bSerializeAsSinglePrecisionFloat
    assert_eq!(value.len(), 69);
    let d = build_struct_property(2, 4, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|idx: i32| serde_json::json!({ "index": idx }),
        pins: PinSerCtx {
            filter_editor_only: false,
            has_source_index: false,
            has_uobject_wrapper: true,
            has_single_precision_float: true,
        },
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let v = &entries[0].value;
    assert_eq!(v["category"].as_str(), Some("int"));
    assert_eq!(v["sub_category_object"]["index"].as_i64(), Some(-9));
    assert_eq!(v["container_type"].as_str(), Some("none"));
    assert_eq!(v["is_reference"].as_bool(), Some(false));
    assert_eq!(v["is_weak_pointer"].as_bool(), Some(false));
    assert_eq!(v["is_const"].as_bool(), Some(false));
    assert_eq!(v["is_uobject_wrapper"].as_bool(), Some(false));
    assert_eq!(
        v["serialize_as_single_precision_float"].as_bool(),
        Some(false)
    );
}

#[test]
fn soft_object_property_resolves_list_index() {
    let names = NameMap {
        names: vec![
            "Ref".to_string(),
            "SoftObjectProperty".to_string(),
            "None".to_string(),
        ],
    };
    let table = vec![
        serde_json::json!({ "asset_path": "/Game/A.A" }),
        serde_json::json!({ "asset_path": "/Game/B.B" }),
    ];
    let mut value = Vec::new();
    push_i32(&mut value, 1); // index into the soft object path list

    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // Ref
    push_raw_name(&mut d, 1); // SoftObjectProperty
    push_i32(&mut d, 0); // type name inner param count
    push_i32(&mut d, value.len() as i32); // size = 4
    d.push(0); // flags
    d.extend_from_slice(&value);
    push_raw_name(&mut d, 2); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &table,
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].value["asset_path"].as_str(), Some("/Game/B.B"));
}

#[test]
fn lazy_object_property_decodes_guid() {
    // FLinkerSave writes a LazyObjectProperty value as the 16-byte FUniqueObjectGuid,
    // not a package index.
    let names = NameMap {
        names: vec![
            "Lazy".to_string(),
            "LazyObjectProperty".to_string(),
            "None".to_string(),
        ],
    };
    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // Lazy
    push_raw_name(&mut d, 1); // LazyObjectProperty
    push_i32(&mut d, 0); // type name inner param count
    push_i32(&mut d, 16); // size
    d.push(0); // flags
    for x in [0x1122_3344u32, 0x5566_7788, 0x99AA_BBCC, 0xDDEE_FF00] {
        push_u32(&mut d, x);
    }
    push_raw_name(&mut d, 2); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].value["lazy_object_guid"].as_str(),
        Some("112233445566778899AABBCCDDEEFF00")
    );
}

#[test]
fn frame_rate_struct_parses_as_tagged_properties() {
    // TStructOpsTypeTraits<FFrameRate> keeps WithSerializer disabled (UE retains the
    // generic UPROPERTY layout for existing assets), so a StructProperty(FrameRate)
    // payload is tagged Numerator/Denominator properties, not 2 raw int32s.
    let names = NameMap {
        names: vec![
            "TickResolution".to_string(), // 0
            "StructProperty".to_string(), // 1
            "FrameRate".to_string(),      // 2
            "Numerator".to_string(),      // 3
            "IntProperty".to_string(),    // 4
            "Denominator".to_string(),    // 5
            "None".to_string(),           // 6
        ],
    };
    let mut value = Vec::new();
    for (name_idx, num) in [(3, 24000), (5, 1001)] {
        push_raw_name(&mut value, name_idx);
        push_raw_name(&mut value, 4); // IntProperty
        push_i32(&mut value, 0); // type name inner param count
        push_i32(&mut value, 4); // size
        value.push(0); // flags
        push_i32(&mut value, num);
    }
    push_raw_name(&mut value, 6); // None

    // The engine does not set HasBinaryOrNativeSerialize for FrameRate, so build the
    // tag with flags = 0 (unlike build_struct_property's 0x08).
    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // TickResolution
    push_raw_name(&mut d, 1); // StructProperty
    push_i32(&mut d, 1); // one type parameter
    push_raw_name(&mut d, 2); // FrameRate
    push_i32(&mut d, 0);
    push_i32(&mut d, value.len() as i32);
    d.push(0); // flags
    d.extend_from_slice(&value);
    push_raw_name(&mut d, 6); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let v = &entries[0].value;
    assert_eq!(v["@struct"].as_str(), Some("FrameRate"));
    let props = v["properties"].as_array().unwrap();
    assert_eq!(props.len(), 2);
    assert_eq!(props[0]["name"].as_str(), Some("Numerator"));
    assert_eq!(props[0]["value"].as_i64(), Some(24000));
    assert_eq!(props[1]["name"].as_str(), Some("Denominator"));
    assert_eq!(props[1]["value"].as_i64(), Some(1001));
}

#[test]
fn map_removed_keys_are_discarded() {
    // A delta-saved TMap serializes NumKeysToRemove key payloads before the live
    // pairs; the parser must consume them to stay aligned.
    let names = NameMap {
        names: vec![
            "Weights".to_string(),
            "MapProperty".to_string(),
            "IntProperty".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    push_i32(&mut value, 1); // NumKeysToRemove
    push_i32(&mut value, 777); // removed key payload
    push_i32(&mut value, 1); // pair count
    push_i32(&mut value, 5); // key
    push_i32(&mut value, 50); // value

    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // Weights
    push_raw_name(&mut d, 1); // MapProperty
    push_i32(&mut d, 2); // two type parameters
    push_raw_name(&mut d, 2); // IntProperty (key)
    push_i32(&mut d, 0);
    push_raw_name(&mut d, 2); // IntProperty (value)
    push_i32(&mut d, 0);
    push_i32(&mut d, value.len() as i32);
    d.push(0); // flags
    d.extend_from_slice(&value);
    push_raw_name(&mut d, 3); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let pairs = entries[0].value.as_array().unwrap();
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0]["key"].as_i64(), Some(5));
    assert_eq!(pairs[0]["value"].as_i64(), Some(50));
}

#[test]
fn set_removed_elements_are_discarded() {
    let names = NameMap {
        names: vec![
            "Ids".to_string(),
            "SetProperty".to_string(),
            "IntProperty".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    push_i32(&mut value, 1); // NumElementsToRemove
    push_i32(&mut value, 999); // removed element payload
    push_i32(&mut value, 2); // element count
    push_i32(&mut value, 7);
    push_i32(&mut value, 8);

    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // Ids
    push_raw_name(&mut d, 1); // SetProperty
    push_i32(&mut d, 1); // one type parameter
    push_raw_name(&mut d, 2); // IntProperty
    push_i32(&mut d, 0);
    push_i32(&mut d, value.len() as i32);
    d.push(0); // flags
    d.extend_from_slice(&value);
    push_raw_name(&mut d, 3); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let elems = entries[0].value.as_array().unwrap();
    assert_eq!(elems.len(), 2);
    assert_eq!(elems[0].as_i64(), Some(7));
    assert_eq!(elems[1].as_i64(), Some(8));
}

#[test]
fn tagged_fallback_struct_parses_as_properties() {
    let names = NameMap {
        names: vec![
            "Constraint".to_string(),
            "StructProperty".to_string(),
            "ConstraintInstance".to_string(),
            "Inner".to_string(),
            "IntProperty".to_string(),
            "None".to_string(),
        ],
    };
    // Tagged properties: IntProperty "Inner" = 7, then None.
    let mut value = Vec::new();
    push_raw_name(&mut value, 3); // Inner
    push_raw_name(&mut value, 4); // IntProperty
    push_i32(&mut value, 0); // type name inner param count
    push_i32(&mut value, 4); // size
    value.push(0); // flags
    push_i32(&mut value, 7);
    push_raw_name(&mut value, 5); // None

    // build_struct_property sets the HasBinaryOrNativeSerialize flag (0x08), so
    // the struct would normally bail; ConstraintInstance is an allowlisted
    // tagged-fallback struct and must parse as properties instead.
    let d = build_struct_property(2, 5, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let v = &entries[0].value;
    assert_eq!(v["@struct"].as_str(), Some("ConstraintInstance"));
    let props = v["properties"].as_array().unwrap();
    assert_eq!(props.len(), 1);
    assert_eq!(props[0]["name"].as_str(), Some("Inner"));
    assert_eq!(props[0]["value"].as_i64(), Some(7));
}

#[test]
fn vm_external_function_binding_info_parses_as_tagged_fallback() {
    let names = NameMap {
        names: vec![
            "Binding".to_string(),
            "StructProperty".to_string(),
            "VMExternalFunctionBindingInfo".to_string(),
            "NumOutputs".to_string(),
            "IntProperty".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    push_raw_name(&mut value, 3); // NumOutputs
    push_raw_name(&mut value, 4); // IntProperty
    push_i32(&mut value, 0); // type name inner param count
    push_i32(&mut value, 4); // size
    value.push(0); // flags
    push_i32(&mut value, 2);
    push_raw_name(&mut value, 5); // None
    let d = build_struct_property(2, 5, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].value["@struct"].as_str(),
        Some("VMExternalFunctionBindingInfo")
    );
    assert!(entries[0].value.get("@unparsed").is_none());
    let props = entries[0].value["properties"].as_array().unwrap();
    assert_eq!(props[0]["name"].as_str(), Some("NumOutputs"));
    assert_eq!(props[0]["value"].as_i64(), Some(2));
}

#[test]
fn cloth_lod_data_common_decodes_transition_payloads() {
    let names = NameMap {
        names: vec![
            "Cloth".to_string(),
            "StructProperty".to_string(),
            "ClothLODDataCommon".to_string(),
            "LODIndex".to_string(),
            "IntProperty".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    push_raw_name(&mut value, 3); // LODIndex
    push_raw_name(&mut value, 4); // IntProperty
    push_i32(&mut value, 0); // type name inner param count
    push_i32(&mut value, 4); // size
    value.push(0); // flags
    push_i32(&mut value, 1);
    push_raw_name(&mut value, 5); // None
    push_i32(&mut value, 1); // TransitionUpSkinData count
    push_mesh_to_mesh_vert_data(&mut value, 0.75);
    push_i32(&mut value, 0); // TransitionDownSkinData count
    let d = build_struct_property(2, 5, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    assert_eq!(r.pos(), d.len() as u64);
    let v = &entries[0].value;
    assert!(v.get("@unparsed").is_none());
    assert_eq!(v["properties"][0]["name"].as_str(), Some("LODIndex"));
    assert_eq!(v["transition_up_skin_data"]["count"].as_i64(), Some(1));
    assert_eq!(
        v["transition_up_skin_data"]["sample"][0]["weight"].as_f64(),
        Some(0.75)
    );
    assert_eq!(v["transition_down_skin_data"]["count"].as_i64(), Some(0));
}

#[test]
fn groom_dataflow_settings_keeps_named_tail_payload() {
    let names = NameMap {
        names: vec![
            "Groom".to_string(),
            "StructProperty".to_string(),
            "GroomDataflowSettings".to_string(),
            "GroupIndex".to_string(),
            "IntProperty".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    push_raw_name(&mut value, 3); // GroupIndex
    push_raw_name(&mut value, 4); // IntProperty
    push_i32(&mut value, 0); // type name inner param count
    push_i32(&mut value, 4); // size
    value.push(0); // flags
    push_i32(&mut value, 3);
    push_raw_name(&mut value, 5); // None
    value.extend_from_slice(&[0xaa, 0xbb, 0xcc]); // FManagedArrayCollection tail preview
    let d = build_struct_property(2, 5, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    assert_eq!(r.pos(), d.len() as u64);
    let v = &entries[0].value;
    assert!(v.get("@unparsed").is_none());
    assert_eq!(v["rest_collection"]["size"].as_u64(), Some(3));
    assert_eq!(v["rest_collection"]["preview"].as_str(), Some("aabbcc"));
}

#[test]
fn instanced_property_bag_empty_decodes() {
    let names = NameMap {
        names: vec![
            "Bag".to_string(),
            "StructProperty".to_string(),
            "InstancedPropertyBag".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    push_i32(&mut value, 0); // bHasData = false
    let d = build_struct_property(2, 3, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].value["has_data"].as_bool(), Some(false));
    assert!(entries[0].value.get("@unparsed").is_none());
}

#[test]
fn cloth_tether_data_decodes_batches() {
    let names = NameMap {
        names: vec![
            "Tethers".to_string(),
            "StructProperty".to_string(),
            "ClothTetherData".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    push_raw_name(&mut value, 3); // None (empty tagged-property block)
    push_i32(&mut value, 2); // batch count
    push_i32(&mut value, 0); // empty first batch
    push_i32(&mut value, 1); // second batch count
    push_i32(&mut value, 238);
    push_i32(&mut value, 0);
    push_f32(&mut value, 27.473572);
    let d = build_struct_property(2, 3, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: cc_uax::version::ue4::HIGHEST,
        file_version_ue5: cc_uax::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let v = &entries[0].value;
    assert!(v.get("@unparsed").is_none());
    assert_eq!(v["batch_count"].as_i64(), Some(2));
    assert_eq!(v["tether_count"].as_i64(), Some(1));
    assert_eq!(
        v["batch_sample"][1]["sample"][0]["start"].as_i64(),
        Some(238)
    );
    assert_eq!(v["batch_sample"][1]["sample"][0]["end"].as_i64(), Some(0));
    assert_eq!(
        v["batch_sample"][1]["sample"][0]["length"].as_f64(),
        Some(27.47357177734375)
    );
}
