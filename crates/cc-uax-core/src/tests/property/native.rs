use super::super::common::*;
use crate::name::NameMap;
use crate::pin::PinSerCtx;
use crate::property::{ParseCtx, parse_properties};
use crate::reader::Reader;

fn push_test_struct_property(v: &mut Vec<u8>, property_idx: i32, struct_idx: i32, value: &[u8]) {
    push_raw_name(v, property_idx);
    push_raw_name(v, 1); // StructProperty
    push_i32(v, 1); // one type parameter
    push_raw_name(v, struct_idx);
    push_i32(v, 0);
    push_i32(v, value.len() as i32);
    v.push(0x08); // HasBinaryOrNativeSerialize
    v.extend_from_slice(value);
}

fn build_name_property_value(
    property_idx: i32,
    name_property_idx: i32,
    value_name_idx: i32,
    none_idx: i32,
) -> Vec<u8> {
    let mut value = Vec::new();
    push_raw_name(&mut value, property_idx);
    push_raw_name(&mut value, name_property_idx);
    push_i32(&mut value, 0); // type name inner param count
    push_i32(&mut value, 8); // RawName payload size
    value.push(0); // flags
    push_raw_name(&mut value, value_name_idx);
    push_raw_name(&mut value, none_idx);
    value
}

enum PcgTestKind {
    Transform,
    Float(f32),
    Vector([f64; 3]),
    Vector4([f64; 4]),
    Int32(i32),
    Int64(i64),
}

fn push_pcg_point_array_property(
    v: &mut Vec<u8>,
    kind: PcgTestKind,
    num_values: i32,
    allocated_count: i32,
) {
    push_i32(v, num_values);
    match kind {
        PcgTestKind::Transform => push_pcg_transform(v),
        PcgTestKind::Float(x) => push_f32(v, x),
        PcgTestKind::Vector(x) => push_pcg_vector(v, x),
        PcgTestKind::Vector4(x) => {
            for value in x {
                push_f64(v, value);
            }
        }
        PcgTestKind::Int32(x) => push_i32(v, x),
        PcgTestKind::Int64(x) => push_i64(v, x),
    }
    push_i32(v, allocated_count);
}

fn push_pcg_transform(v: &mut Vec<u8>) {
    for x in [0.0, 0.0, 0.0, 1.0] {
        push_f64(v, x);
    }
    push_pcg_vector(v, [1.0, 2.0, 3.0]);
    push_pcg_vector(v, [1.0, 1.0, 1.0]);
}

fn push_pcg_vector(v: &mut Vec<u8>, values: [f64; 3]) {
    for x in values {
        push_f64(v, x);
    }
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
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let unparsed = entries[0].value.get("@unparsed").and_then(|v| v.as_str());
    assert_eq!(unparsed, Some("01000000aabbccdd"));
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
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
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
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].value["min"]["x"].as_f64(), Some(1.0));
    assert_eq!(entries[0].value["max"]["y"].as_f64(), Some(4.0));
    assert_eq!(entries[0].value["is_valid"].as_bool(), Some(true));
}

// Wrap raw FText `value` bytes as a single TextProperty, parse it, return the value.

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
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
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
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
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
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
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
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
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
            crate::version::custom::NIAGARA_SERIALIZE_USAGE_BITMASK_TO_GPU_FUNCTION_INFO,
        fortnite_main_version: -1,
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
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
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].value["implementation"].as_str(), Some("empty"));
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
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
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
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
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
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
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
fn material_color_input_uses_packed_color_before_linear_color_version() {
    let names = NameMap {
        names: vec![
            "EmissiveColor".to_string(),
            "StructProperty".to_string(),
            "ColorMaterialInput".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    push_i32(&mut value, -5); // expression object index
    push_i32(&mut value, 0); // output index
    push_raw_name(&mut value, 3); // input name "None"
    for m in [0, 0, 0, 0, 0] {
        push_i32(&mut value, m);
    }
    push_i32(&mut value, 1); // use constant
    push_u32(&mut value, 0xAABBCCDD); // legacy FColor payload
    assert_eq!(value.len(), 44);
    let d = build_struct_property(2, 3, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|idx: i32| serde_json::json!({ "index": idx }),
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        niagara_version: -1,
        fortnite_main_version: 76,
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let v = &entries[0].value;
    assert_eq!(v["expression"]["index"].as_i64(), Some(-5));
    assert_eq!(v["use_constant"].as_bool(), Some(true));
    assert_eq!(v["constant"]["packed_bgra"].as_u64(), Some(0xAABBCCDD));
    assert!(v.get("@unparsed").is_none());
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
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
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
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
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
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
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
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
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
fn native_struct_instanced_struct_container_decodes_items() {
    let names = NameMap {
        names: vec![
            "Data".to_string(),
            "StructProperty".to_string(),
            "InstancedStructContainer".to_string(),
            "Inner".to_string(),
            "IntProperty".to_string(),
            "None".to_string(),
        ],
    };
    // First item stores tagged properties, second is an empty/null item.
    let mut inner = Vec::new();
    push_raw_name(&mut inner, 3); // Inner
    push_raw_name(&mut inner, 4); // IntProperty
    push_i32(&mut inner, 0); // type name inner param count
    push_i32(&mut inner, 4); // size
    inner.push(0); // flags
    push_i32(&mut inner, 42);
    push_raw_name(&mut inner, 5); // None

    let mut value = Vec::new();
    value.push(0); // FInstancedStructContainer version
    push_i32(&mut value, 2); // item count
    push_i32(&mut value, -7); // item 0 script struct
    push_i32(&mut value, 0); // item 1 null script struct
    push_i32(&mut value, inner.len() as i32); // item 0 serial size
    value.extend_from_slice(&inner);
    push_i32(&mut value, 0); // item 1 serial size
    let d = build_struct_property(2, 5, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|idx: i32| serde_json::json!({ "index": idx }),
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
    assert_eq!(v["version"].as_u64(), Some(0));
    assert_eq!(v["item_count"].as_i64(), Some(2));
    assert_eq!(v["items"][0]["script_struct"]["index"].as_i64(), Some(-7));
    assert!(v["items"][0].get("@unparsed").is_none());
    let props = v["items"][0]["properties"].as_array().unwrap();
    assert_eq!(props[0]["name"].as_str(), Some("Inner"));
    assert_eq!(props[0]["value"].as_i64(), Some(42));
    assert_eq!(v["items"][1]["serial_size"].as_i64(), Some(0));
}

#[test]
fn native_struct_state_tree_instance_data_decodes_storage() {
    let names = NameMap {
        names: vec![
            "Data".to_string(),
            "StructProperty".to_string(),
            "StateTreeInstanceData".to_string(),
            "InstanceStructs".to_string(),
            "InstancedStructContainer".to_string(),
            "None".to_string(),
        ],
    };
    let mut container = Vec::new();
    container.push(0); // FInstancedStructContainer version
    push_i32(&mut container, 0); // item count

    let mut storage = Vec::new();
    push_test_struct_property(&mut storage, 3, 4, &container);
    push_raw_name(&mut storage, 5); // None
    let d = build_struct_property(2, 5, &storage);

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
    let storage = &entries[0].value["storage"];
    assert_eq!(
        storage["@struct"].as_str(),
        Some("StateTreeInstanceStorage")
    );
    let props = storage["properties"].as_array().unwrap();
    assert_eq!(props[0]["name"].as_str(), Some("InstanceStructs"));
    assert_eq!(props[0]["value"]["item_count"].as_i64(), Some(0));
    assert!(entries[0].value.get("@unparsed").is_none());
}

#[test]
fn pcg_input_and_output_selectors_parse_as_tagged_properties() {
    let names = NameMap {
        names: vec![
            "InputSelector".to_string(),
            "StructProperty".to_string(),
            "PCGAttributePropertyInputSelector".to_string(),
            "AttributeName".to_string(),
            "NameProperty".to_string(),
            "Height".to_string(),
            "OutputSelector".to_string(),
            "PCGAttributePropertyOutputSelector".to_string(),
            "None".to_string(),
        ],
    };
    let selector_value = build_name_property_value(3, 4, 5, 8);
    let mut d = Vec::new();
    push_test_struct_property(&mut d, 0, 2, &selector_value);
    push_test_struct_property(&mut d, 6, 7, &selector_value);
    push_raw_name(&mut d, 8); // None

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

    assert_eq!(entries.len(), 2);
    assert_eq!(
        entries[0].value["@struct"].as_str(),
        Some("PCGAttributePropertyInputSelector")
    );
    assert_eq!(
        entries[1].value["@struct"].as_str(),
        Some("PCGAttributePropertyOutputSelector")
    );
    for entry in &entries {
        let props = entry.value["properties"].as_array().unwrap();
        assert_eq!(props[0]["name"].as_str(), Some("AttributeName"));
        assert_eq!(props[0]["value"].as_str(), Some("Height"));
        assert!(entry.value.get("@unparsed").is_none());
    }
}

#[test]
fn native_struct_pcg_point_array_decodes_channels() {
    let names = NameMap {
        names: vec![
            "PointArray".to_string(),
            "StructProperty".to_string(),
            "PCGPointArray".to_string(),
            "None".to_string(),
        ],
    };

    let mut value = Vec::new();
    push_i32(&mut value, 1); // NumPoints
    push_pcg_point_array_property(&mut value, PcgTestKind::Transform, 1, 0);
    push_pcg_point_array_property(&mut value, PcgTestKind::Float(0.5), 1, 0);
    push_pcg_point_array_property(&mut value, PcgTestKind::Vector([0.0, 0.0, 0.0]), 1, 0);
    push_pcg_point_array_property(&mut value, PcgTestKind::Vector([1.0, 1.0, 1.0]), 1, 0);
    push_pcg_point_array_property(&mut value, PcgTestKind::Vector4([1.0, 1.0, 1.0, 1.0]), 1, 0);
    push_pcg_point_array_property(&mut value, PcgTestKind::Float(0.0), 1, 0);
    push_pcg_point_array_property(&mut value, PcgTestKind::Int32(123), 1, 0);
    push_pcg_point_array_property(&mut value, PcgTestKind::Int64(456), 1, 0);
    let d = build_struct_property(2, 3, &value);

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
    assert_eq!(v["num_points"].as_i64(), Some(1));
    assert_eq!(v["transform"]["num_values"].as_i64(), Some(1));
    assert_eq!(
        v["transform"]["default"]["rotation"]["w"].as_f64(),
        Some(1.0)
    );
    assert_eq!(v["density"]["default"].as_f64(), Some(0.5));
    assert_eq!(v["seed"]["default"].as_i64(), Some(123));
    assert_eq!(v["metadata_entry"]["default"].as_i64(), Some(456));
    assert!(v.get("@unparsed").is_none());
    assert!(v.get("payload_tail").is_none());
}

#[test]
fn native_struct_pcg_point_decodes_structured_mask() {
    let names = NameMap {
        names: vec![
            "Point".to_string(),
            "StructProperty".to_string(),
            "PCGPoint".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    value.push(1 << 4); // Steepness is serialized after the transform.
    push_pcg_transform(&mut value);
    push_f32(&mut value, 0.25);
    let d = build_struct_property(2, 3, &value);

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
    assert_eq!(v["serialize_mask"].as_u64(), Some(1 << 4));
    assert_eq!(v["transform"]["rotation"]["w"].as_f64(), Some(1.0));
    assert_eq!(v["steepness"].as_f64(), Some(0.25));
    assert!(v.get("@unparsed").is_none());
}

#[test]
fn niagara_variant_parses_as_tagged_properties() {
    let names = NameMap {
        names: vec![
            "Variant".to_string(),
            "StructProperty".to_string(),
            "NiagaraVariant".to_string(),
            "Object".to_string(),
            "ObjectProperty".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    push_raw_name(&mut value, 3); // Object
    push_raw_name(&mut value, 4); // ObjectProperty
    push_i32(&mut value, 0); // type name inner param count
    push_i32(&mut value, 4); // size
    value.push(0); // flags
    push_i32(&mut value, -9);
    push_raw_name(&mut value, 5); // None
    let d = build_struct_property(2, 5, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|idx: i32| serde_json::json!({ "index": idx }),
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
    assert_eq!(entries[0].value["@struct"].as_str(), Some("NiagaraVariant"));
    let props = entries[0].value["properties"].as_array().unwrap();
    assert_eq!(props[0]["name"].as_str(), Some("Object"));
    assert_eq!(props[0]["value"]["index"].as_i64(), Some(-9));
    assert!(entries[0].value.get("@unparsed").is_none());
}

#[test]
fn state_tree_reference_parses_as_tagged_properties() {
    let names = NameMap {
        names: vec![
            "Ref".to_string(),
            "StructProperty".to_string(),
            "StateTreeReference".to_string(),
            "StateTree".to_string(),
            "ObjectProperty".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    push_raw_name(&mut value, 3); // StateTree
    push_raw_name(&mut value, 4); // ObjectProperty
    push_i32(&mut value, 0); // type name inner param count
    push_i32(&mut value, 4); // size
    value.push(0); // flags
    push_i32(&mut value, -2);
    push_raw_name(&mut value, 5); // None
    let d = build_struct_property(2, 5, &value);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|idx: i32| serde_json::json!({ "index": idx }),
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
        Some("StateTreeReference")
    );
    let props = entries[0].value["properties"].as_array().unwrap();
    assert_eq!(props[0]["name"].as_str(), Some("StateTree"));
    assert_eq!(props[0]["value"]["index"].as_i64(), Some(-2));
    assert!(entries[0].value.get("@unparsed").is_none());
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
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
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
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
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
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
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
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
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
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
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
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
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
