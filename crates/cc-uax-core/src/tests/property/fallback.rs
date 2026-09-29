use super::super::common::*;
use crate::model::OpaqueReason;
use crate::name::NameMap;
use crate::pin::PinSerCtx;
use crate::property::{ParseCtx, parse_properties, parse_properties_report};
use crate::reader::Reader;

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
        resolve_object: &|_idx: i32| crate::DecodedValue::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        soft_object_paths_unavailable: false,
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
        nested_diagnostics: Default::default(),
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let opaque = entries[0]
        .value
        .as_opaque()
        .expect("count out of range stays opaque");
    assert_eq!(opaque.reason, OpaqueReason::UndecodedValue);
    assert_eq!(opaque.byte_range.preview, "41420f00");
    assert_eq!(opaque.byte_range.size, 4);
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
        resolve_object: &|_idx: i32| crate::DecodedValue::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        soft_object_paths_unavailable: false,
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
        nested_diagnostics: Default::default(),
    };
    let mut r = Reader::new(&d);
    let report = parse_properties_report(&mut r, &ctx, d.len() as u64, "/exports/0/properties");

    assert_eq!(report.entries.len(), 1);
    let opaque = report.entries[0].value.as_opaque().unwrap();
    assert_eq!(opaque.reason, OpaqueReason::UndecodedValue);
    assert_eq!(
        opaque.type_name.as_deref(),
        Some("ArrayProperty(IntProperty)")
    );
    assert_eq!(opaque.byte_range.start, value_start);
    assert_eq!(opaque.byte_range.end, value_start + 4);
    assert_eq!(opaque.byte_range.size, 4);
    assert_eq!(opaque.byte_range.preview, "41420f00");
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
        resolve_object: &|_idx: i32| crate::DecodedValue::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        soft_object_paths_unavailable: false,
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
        nested_diagnostics: Default::default(),
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
        resolve_object: &|_idx: i32| crate::DecodedValue::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        soft_object_paths_unavailable: false,
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
        nested_diagnostics: Default::default(),
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
        resolve_object: &|_idx: i32| crate::DecodedValue::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        soft_object_paths_unavailable: false,
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
        nested_diagnostics: Default::default(),
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].value["@struct"].as_str(),
        Some("VMExternalFunctionBindingInfo")
    );
    assert!(!entries[0].value.is_opaque());
    let props = entries[0].value["properties"].as_array().unwrap();
    assert_eq!(props[0]["name"].as_str(), Some("NumOutputs"));
    assert_eq!(props[0]["value"].as_i64(), Some(2));
}

#[test]
fn value_crossing_its_window_falls_back_through_the_read_limit() {
    let names = NameMap {
        names: vec![
            "Location".to_string(),
            "StructProperty".to_string(),
            "Vector".to_string(),
            "Count".to_string(),
            "IntProperty".to_string(),
            "None".to_string(),
        ],
    };
    let mut d = Vec::new();
    // A double FVector needs 24 bytes; the tag only declares 12.
    push_raw_name(&mut d, 0); // Location
    push_raw_name(&mut d, 1); // StructProperty
    push_i32(&mut d, 1); // one type parameter
    push_raw_name(&mut d, 2); // Vector
    push_i32(&mut d, 0);
    push_i32(&mut d, 12); // declared size
    d.push(0x08); // HasBinaryOrNativeSerialize
    let value_start = d.len() as u64;
    for _ in 0..3 {
        push_f32(&mut d, 1.0);
    }
    push_raw_name(&mut d, 3); // Count
    push_raw_name(&mut d, 4); // IntProperty
    push_i32(&mut d, 0);
    push_i32(&mut d, 4);
    d.push(0);
    push_i32(&mut d, 42);
    push_raw_name(&mut d, 5); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| crate::DecodedValue::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        soft_object_paths_unavailable: false,
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
        nested_diagnostics: Default::default(),
    };
    let mut r = Reader::new(&d);
    let report = parse_properties_report(&mut r, &ctx, d.len() as u64, "/properties");

    assert_eq!(report.entries.len(), 2);
    assert!(report.entries[0].value.is_opaque());
    assert_eq!(report.entries[1].name, "Count");
    assert_eq!(report.entries[1].value.as_i64(), Some(42));

    let diag = report
        .diagnostics
        .iter()
        .find(|diag| diag.code == "property_value_fallback")
        .expect("fallback diagnostic should be emitted");
    assert_eq!(diag.path, "/properties/Location");
    assert_eq!(diag.offset, Some(value_start));
    assert!(diag.message.contains("failed to decode property"));
    assert!(!diag.message.contains("past its declared value window"));
    assert!(diag.message.contains("read limit"), "{}", diag.message);
}

fn complete_name_ctx<'a>(names: &'a NameMap) -> ParseCtx<'a> {
    ParseCtx {
        names,
        resolve_object: &|_idx: i32| crate::DecodedValue::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        soft_object_paths_unavailable: false,
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
        nested_diagnostics: Default::default(),
    }
}

/// `Nums: ArrayProperty(<inner>)` followed by `Count: IntProperty = 42`.
fn array_then_count(inner_idx: i32, payload: &[u8]) -> Vec<u8> {
    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // Nums
    push_raw_name(&mut d, 1); // ArrayProperty
    push_i32(&mut d, 1); // one type parameter
    push_raw_name(&mut d, inner_idx);
    push_i32(&mut d, 0);
    push_i32(&mut d, payload.len() as i32);
    d.push(0); // flags
    d.extend_from_slice(payload);
    push_raw_name(&mut d, 3); // Count
    push_raw_name(&mut d, 2); // IntProperty
    push_i32(&mut d, 0);
    push_i32(&mut d, 4);
    d.push(0);
    push_i32(&mut d, 42);
    push_raw_name(&mut d, 4); // None
    d
}

fn array_then_count_names() -> NameMap {
    NameMap {
        names: vec![
            "Nums".to_string(),          // 0
            "ArrayProperty".to_string(), // 1
            "IntProperty".to_string(),   // 2
            "Count".to_string(),         // 3
            "None".to_string(),          // 4
            "StrProperty".to_string(),   // 5
        ],
    }
}

#[test]
fn an_underconsumed_fixed_width_container_becomes_an_opaque_value() {
    let names = array_then_count_names();
    let mut payload = Vec::new();
    push_i32(&mut payload, 2);
    push_i32(&mut payload, 10);
    push_i32(&mut payload, 20);
    payload.extend_from_slice(&[0xAA, 0xBB]); // two bytes no element accounts for
    let d = array_then_count(2, &payload);
    let value_start = 37; // name, type, param count, inner name, inner count, size, flags
    let ctx = complete_name_ctx(&names);
    let mut r = Reader::new(&d);

    let report = parse_properties_report(&mut r, &ctx, d.len() as u64, "/properties");

    assert_eq!(report.entries.len(), 2);
    let opaque = report.entries[0].value.as_opaque().expect("opaque value");
    assert_eq!(opaque.reason, OpaqueReason::ValueUnderconsumed);
    assert_eq!(opaque.byte_range.start, value_start);
    assert_eq!(opaque.byte_range.end, value_start + 14);
    assert_eq!(opaque.byte_range.size, 14);
    let diagnostic = report
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "property_value_fallback")
        .unwrap_or_else(|| panic!("{:#?}", report.diagnostics));
    assert!(
        diagnostic.message.contains("left 2 undecoded byte(s)")
            && diagnostic.message.contains("must fill its window"),
        "{}",
        diagnostic.message
    );
    assert!(
        !report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "property_value_incomplete")
    );
    assert_eq!(report.entries[1].name, "Count");
    assert_eq!(report.entries[1].value.as_i64(), Some(42));
}

#[test]
fn an_underconsumed_variable_width_container_keeps_its_decoded_elements() {
    let names = array_then_count_names();
    let mut payload = Vec::new();
    push_i32(&mut payload, 1);
    push_fstring(&mut payload, "Hi");
    payload.extend_from_slice(&[0xAA, 0xBB]);
    let d = array_then_count(5, &payload);
    let ctx = complete_name_ctx(&names);
    let mut r = Reader::new(&d);

    let report = parse_properties_report(&mut r, &ctx, d.len() as u64, "/properties");

    assert!(!report.entries[0].value.is_opaque());
    assert_eq!(report.entries[0].value[0].as_str(), Some("Hi"));
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "property_value_incomplete"),
        "{:#?}",
        report.diagnostics
    );
    assert_eq!(report.entries[1].value.as_i64(), Some(42));
}

#[test]
fn nested_diagnostics_stay_with_the_element_that_raised_them() {
    let names = NameMap {
        names: vec![
            "M".to_string(),              // 0
            "MapProperty".to_string(),    // 1
            "NameProperty".to_string(),   // 2
            "StructProperty".to_string(), // 3
            "MyStruct".to_string(),       // 4
            "X".to_string(),              // 5
            "IntProperty".to_string(),    // 6
            "Q".to_string(),              // 7
            "None".to_string(),           // 8
            "K1".to_string(),             // 9
            "K2".to_string(),             // 10
        ],
    };
    let mut payload = Vec::new();
    push_i32(&mut payload, 0); // NumToRemove
    push_i32(&mut payload, 2);
    // Entry 1: X is an IntProperty whose declared size cannot hold an int32.
    push_raw_name(&mut payload, 9);
    push_raw_name(&mut payload, 5);
    push_raw_name(&mut payload, 6);
    push_i32(&mut payload, 0);
    push_i32(&mut payload, 2);
    payload.push(0);
    payload.extend_from_slice(&[1, 0]);
    push_raw_name(&mut payload, 8);
    // Entry 2: Q decodes cleanly.
    push_raw_name(&mut payload, 10);
    push_raw_name(&mut payload, 7);
    push_raw_name(&mut payload, 6);
    push_i32(&mut payload, 0);
    push_i32(&mut payload, 4);
    payload.push(0);
    push_i32(&mut payload, 5);
    push_raw_name(&mut payload, 8);

    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // M
    push_raw_name(&mut d, 1); // MapProperty
    push_i32(&mut d, 2); // key and value
    push_raw_name(&mut d, 2);
    push_i32(&mut d, 0);
    push_raw_name(&mut d, 3);
    push_i32(&mut d, 1);
    push_raw_name(&mut d, 4);
    push_i32(&mut d, 0);
    push_i32(&mut d, payload.len() as i32);
    d.push(0);
    d.extend_from_slice(&payload);
    push_raw_name(&mut d, 8);
    let ctx = complete_name_ctx(&names);
    let mut r = Reader::new(&d);

    let report = parse_properties_report(&mut r, &ctx, d.len() as u64, "/exports/0/properties");

    let paths: Vec<&str> = report
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.path.as_str())
        .collect();
    assert_eq!(paths, ["/exports/0/properties/M/properties/X"], "{paths:?}");
}
