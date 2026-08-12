use super::super::common::*;
use crate::name::NameMap;
use crate::pin::PinSerCtx;
use crate::property::{
    ParseCtx, PropertyParseStatus, parse_object_properties, parse_properties,
    parse_properties_report,
};
use crate::reader::Reader;

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
        resolve_object: &|_idx: i32| crate::DecodedValue::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
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
fn under_consumed_property_value_is_recorded_not_silently_dropped() {
    let names = NameMap {
        names: vec![
            "Padded".to_string(),      // 0
            "IntProperty".to_string(), // 1
            "None".to_string(),        // 2
        ],
    };
    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // Padded
    push_raw_name(&mut d, 1); // IntProperty
    push_i32(&mut d, 0); // type name inner param count
    push_i32(&mut d, 8); // declared size is 8, but IntProperty only reads 4
    d.push(0); // flags
    let value_start = d.len() as u64;
    push_i32(&mut d, 123); // the decoded int (4 bytes)
    push_i32(&mut d, 0); // 4 trailing bytes the decoder does not consume
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
    let parsed = parse_properties_report(&mut r, &ctx, d.len() as u64, "/exports/0/properties");

    assert_eq!(parsed.entries.len(), 1);
    assert_eq!(parsed.entries[0].name, "Padded");
    assert_eq!(parsed.entries[0].value.as_i64(), Some(123));
    let diag = parsed
        .diagnostics
        .iter()
        .find(|diag| diag.code == "property_value_incomplete")
        .expect("under-consumed value should be recorded, not silently dropped");
    assert_eq!(diag.path, "/exports/0/properties/Padded");
    assert_eq!(diag.offset, Some(value_start + 4));
    let context = diag.context.as_ref().unwrap();
    assert_eq!(context["unconsumed_bytes"].as_u64(), Some(4));
    assert_eq!(context["consumed_to"].as_u64(), Some(value_start + 4));
    assert_eq!(context["declared_end"].as_u64(), Some(value_start + 8));
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
        resolve_object: &|_idx: i32| crate::DecodedValue::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);
    assert!(entries.is_empty());

    let mut r = Reader::new(&d);
    let parsed = parse_properties_report(&mut r, &ctx, d.len() as u64, "/properties");
    assert!(parsed.entries.is_empty());
    assert!(parsed.diagnostics.is_empty());
    assert_eq!(parsed.status, PropertyParseStatus::NonTaggedPayload);
}

#[test]
fn failed_property_after_entries_reports_status_and_diagnostic() {
    let names = NameMap {
        names: vec![
            "Broken".to_string(),
            "IntProperty".to_string(),
            "Value".to_string(),
        ],
    };
    let mut d = Vec::new();
    push_raw_name(&mut d, 2); // Value
    push_raw_name(&mut d, 1); // IntProperty
    push_i32(&mut d, 0);
    push_i32(&mut d, 4);
    d.push(0);
    push_i32(&mut d, 7);
    push_raw_name(&mut d, 0); // Broken tag with no type payload after it

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
    let parsed = parse_properties_report(&mut r, &ctx, d.len() as u64, "/properties");

    assert_eq!(parsed.entries.len(), 1);
    assert_eq!(parsed.status, PropertyParseStatus::FailedAfterEntries);
    assert_eq!(parsed.diagnostics.len(), 1);
    assert_eq!(parsed.diagnostics[0].code, "property_tag_parse_failed");
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
    let fv = crate::version::ue5::PROPERTY_TAG_EXTENSION_AND_OVERRIDABLE_SERIALIZATION;
    let mut d = Vec::new();
    d.push(0); // object serialization control byte

    push_legacy_tag_header(&mut d, 0, 1, 0);
    push_u32(&mut d, 1); // BoolVal (4-byte legacy UBOOL)
    push_legacy_tag_tail(&mut d, fv);

    push_legacy_tag_header(&mut d, 2, 3, 8);
    push_raw_name(&mut d, 4); // EnumName
    push_legacy_tag_tail(&mut d, fv);
    push_raw_name(&mut d, 5); // enum value

    push_legacy_tag_header(&mut d, 6, 7, 24);
    push_raw_name(&mut d, 8); // StructName
    d.extend_from_slice(&[0u8; 16]); // StructGuid
    push_legacy_tag_tail(&mut d, fv);
    push_f64(&mut d, 1.0);
    push_f64(&mut d, 2.0);
    push_f64(&mut d, 3.0);

    push_legacy_tag_header(&mut d, 9, 10, 12);
    push_raw_name(&mut d, 11); // InnerType
    push_legacy_tag_tail(&mut d, fv);
    push_i32(&mut d, 2);
    push_i32(&mut d, 7);
    push_i32(&mut d, 8);

    push_legacy_tag_header(&mut d, 12, 13, 16);
    push_raw_name(&mut d, 11); // InnerType
    push_legacy_tag_tail(&mut d, fv);
    push_i32(&mut d, 0); // NumToRemove
    push_i32(&mut d, 2);
    push_i32(&mut d, 9);
    push_i32(&mut d, 10);

    push_legacy_tag_header(&mut d, 14, 15, 16);
    push_raw_name(&mut d, 11); // Key InnerType
    push_raw_name(&mut d, 11); // ValueType
    push_legacy_tag_tail(&mut d, fv);
    push_i32(&mut d, 0); // NumKeysToRemove
    push_i32(&mut d, 1);
    push_i32(&mut d, 3);
    push_i32(&mut d, 4);

    push_legacy_tag_header(&mut d, 16, 17, 8);
    push_raw_name(&mut d, 11); // InnerType
    push_legacy_tag_tail(&mut d, fv);
    push_i32(&mut d, 1); // optional is set
    push_i32(&mut d, 77);

    push_legacy_tag_header(&mut d, 18, 11, 4);
    push_legacy_tag_tail_with_guid(&mut d, fv);
    push_i32(&mut d, 99);

    push_raw_name(&mut d, 19); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| crate::DecodedValue::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: fv,
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
fn legacy_property_tags_match_ue5_1_on_disk_layout() {
    // UE5.1 (FileVersionUE5 = 1008) legacy tags have no complete type name and no
    // extension byte (< 1011), and both `BoolVal` and `HasPropertyGuid` are serialized
    // as 4-byte legacy UBOOLs (FArchive::SerializeBool). Reading either as a single
    // byte desyncs every following tag, so exact byte conservation plus the decoded
    // values prove the widths are right.
    let names = NameMap {
        names: vec![
            "Flag".to_string(),         // 0
            "BoolProperty".to_string(), // 1
            "Count".to_string(),        // 2
            "IntProperty".to_string(),  // 3
            "None".to_string(),         // 4
        ],
    };
    let fv = 1008; // UE5.1
    let mut d = Vec::new();
    // No object serialization-control byte: that is only written for a top-level
    // UClass when FileVersionUE5 >= 1011 (UE5.4+).

    // BoolProperty: 4-byte BoolVal, then 4-byte HasPropertyGuid = false, no ext byte.
    push_legacy_tag_header(&mut d, 0, 1, 0);
    push_u32(&mut d, 1); // BoolVal = true (4-byte UBOOL)
    push_legacy_tag_tail(&mut d, fv);

    // IntProperty carrying a property GUID: 4-byte HasPropertyGuid = true + 16-byte GUID.
    push_legacy_tag_header(&mut d, 2, 3, 4);
    push_legacy_tag_tail_with_guid(&mut d, fv);
    push_i32(&mut d, 42);

    push_raw_name(&mut d, 4); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| crate::DecodedValue::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: fv,
    };
    let mut r = Reader::new(&d);
    let props = parse_object_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(r.pos(), d.len() as u64);
    assert_eq!(props.len(), 2);
    assert_eq!(props[0].name, "Flag");
    assert_eq!(props[0].value.as_bool(), Some(true));
    assert_eq!(props[1].name, "Count");
    assert_eq!(
        props[1].guid.as_deref(),
        Some("00000001000000020000000300000004")
    );
    assert_eq!(props[1].value.as_i64(), Some(42));
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
    assert_eq!(entries[0].name, "MyInt");
    assert_eq!(entries[0].value.as_i64(), Some(12345));
}

#[test]
fn property_tag_extension_has_external_objects_is_parsed() {
    // UE5.8 adds HasExternalsObjects (0x04) to EPropertyTagExtension. When set,
    // a trailing bool32 (bExperimentalExternalObjects) follows. If not consumed,
    // the value desyncs — so decoding the int proves correct alignment.
    let names = NameMap {
        names: vec![
            "MyInt".to_string(),
            "IntProperty".to_string(),
            "None".to_string(),
        ],
    };

    // Test 0x04 alone (HasExternalsObjects without OverridableInformation).
    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // MyInt
    push_raw_name(&mut d, 1); // IntProperty
    push_i32(&mut d, 0); // type name inner param count
    push_i32(&mut d, 4); // value size
    d.push(0x04); // flags = HasPropertyExtensions
    d.push(0x04); // extension flags = HasExternalsObjects
    push_i32(&mut d, 1); // bExperimentalExternalObjects bool
    push_i32(&mut d, 999); // IntProperty value
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
    assert_eq!(entries[0].name, "MyInt");
    assert_eq!(entries[0].value.as_i64(), Some(999));

    // Test 0x06 (both OverridableInformation + HasExternalsObjects).
    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // MyInt
    push_raw_name(&mut d, 1); // IntProperty
    push_i32(&mut d, 0); // type name inner param count
    push_i32(&mut d, 4); // value size
    d.push(0x04); // flags = HasPropertyExtensions
    d.push(0x06); // extension flags = OverridableInformation | HasExternalsObjects
    d.push(0x00); // override operation
    push_i32(&mut d, 0); // bExperimentalOverridableLogic bool
    push_i32(&mut d, 1); // bExperimentalExternalObjects bool
    push_i32(&mut d, 777); // IntProperty value
    push_raw_name(&mut d, 2); // None

    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "MyInt");
    assert_eq!(entries[0].value.as_i64(), Some(777));
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
        resolve_object: &|_idx: i32| crate::DecodedValue::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "Skipped");
    assert_eq!(entries[0].value["@skipped"].as_bool(), Some(true));
    assert_eq!(entries[1].name, "After");
    assert_eq!(entries[1].value.as_i64(), Some(99));
}
