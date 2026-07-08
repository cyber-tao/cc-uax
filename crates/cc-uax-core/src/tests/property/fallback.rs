use super::super::common::*;
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
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
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
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
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
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
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
        resolve_object: &|_idx: i32| serde_json::Value::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: -1,
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
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
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
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
