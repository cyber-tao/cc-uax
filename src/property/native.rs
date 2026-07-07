use super::{
    PREVIEW_MAX, ParseCtx, ensure_within_value, entries_to_json, parse_properties, to_hex,
    validate_count,
};
use crate::name::NameMap;
use crate::reader::Reader;
use anyhow::{Result, bail};
use serde_json::{Map, Value, json};

pub(crate) fn is_tagged_fallback_struct(name: &str) -> bool {
    matches!(
        name,
        "ConstraintInstance"
            | "Timeline"
            | "AnimNotifyEvent"
            | "PostProcessSettings"
            | "HierarchicalSimplification"
            // FAlphaBlend / FAnimCurveBase-derived curves declare WithSerializer but
            // their Serialize returns false, so the payload is tagged properties.
            | "AlphaBlend"
            | "FloatCurve"
            | "TransformCurve"
            | "VectorCurve"
            // FGameplayEffectModifierMagnitude::Serialize also returns false; the
            // landscape per-layer struct has no custom serializer (the enclosing map
            // carries the native flag), so both are tagged-property payloads.
            | "GameplayEffectModifierMagnitude"
            | "LandscapeLayerComponentData"
            // FVMExternalFunctionBindingInfo::Serialize calls SerializeTaggedProperties.
            | "VMExternalFunctionBindingInfo"
    )
}

pub(crate) fn parse_native_struct(
    r: &mut Reader,
    name: &str,
    ctx: &ParseCtx,
    value_end: u64,
) -> Result<Option<Value>> {
    let v = match name {
        // Note: FVector_NetQuantize* subclasses only declare WithNetSerializer, so
        // their package payload is tagged properties — do not decode them natively.
        "Vector" => {
            json!({ "x": r.read_f64()?, "y": r.read_f64()?, "z": r.read_f64()? })
        }
        "Vector3f" => json!({ "x": r.read_f32()?, "y": r.read_f32()?, "z": r.read_f32()? }),
        "Vector2D" => json!({ "x": r.read_f64()?, "y": r.read_f64()? }),
        "Vector2f" => json!({ "x": r.read_f32()?, "y": r.read_f32()? }),
        "Vector4" => json!({
            "x": r.read_f64()?, "y": r.read_f64()?, "z": r.read_f64()?, "w": r.read_f64()?
        }),
        "Vector4f" => json!({
            "x": r.read_f32()?, "y": r.read_f32()?, "z": r.read_f32()?, "w": r.read_f32()?
        }),
        "Rotator" => json!({
            "pitch": r.read_f64()?, "yaw": r.read_f64()?, "roll": r.read_f64()?
        }),
        "Quat" => json!({
            "x": r.read_f64()?, "y": r.read_f64()?, "z": r.read_f64()?, "w": r.read_f64()?
        }),
        "IntPoint" => json!({ "x": r.read_i32()?, "y": r.read_i32()? }),
        "IntVector" => json!({ "x": r.read_i32()?, "y": r.read_i32()?, "z": r.read_i32()? }),
        "Guid" => json!(r.read_guid()?.to_hex()),
        "Color" => json!({
            "b": r.read_u8()?, "g": r.read_u8()?, "r": r.read_u8()?, "a": r.read_u8()?
        }),
        "LinearColor" => json!({
            "r": r.read_f32()?, "g": r.read_f32()?, "b": r.read_f32()?, "a": r.read_f32()?
        }),
        "DateTime" | "Timespan" => json!(r.read_i64()?),
        "Transform" => {
            let rot = json!({
                "x": r.read_f64()?, "y": r.read_f64()?, "z": r.read_f64()?, "w": r.read_f64()?
            });
            let trans = json!({ "x": r.read_f64()?, "y": r.read_f64()?, "z": r.read_f64()? });
            let scale = json!({ "x": r.read_f64()?, "y": r.read_f64()?, "z": r.read_f64()? });
            json!({ "rotation": rot, "translation": trans, "scale3d": scale })
        }
        "Transform3f" => {
            let rot = json!({
                "x": r.read_f32()?, "y": r.read_f32()?, "z": r.read_f32()?, "w": r.read_f32()?
            });
            let trans = json!({ "x": r.read_f32()?, "y": r.read_f32()?, "z": r.read_f32()? });
            let scale = json!({ "x": r.read_f32()?, "y": r.read_f32()?, "z": r.read_f32()? });
            json!({ "rotation": rot, "translation": trans, "scale3d": scale })
        }
        "Box" => {
            let min = json!({ "x": r.read_f64()?, "y": r.read_f64()?, "z": r.read_f64()? });
            let max = json!({ "x": r.read_f64()?, "y": r.read_f64()?, "z": r.read_f64()? });
            let is_valid = r.read_u8()? != 0;
            json!({ "min": min, "max": max, "is_valid": is_valid })
        }
        "Box2D" => {
            let min = json!({ "x": r.read_f64()?, "y": r.read_f64()? });
            let max = json!({ "x": r.read_f64()?, "y": r.read_f64()? });
            // TBox2::Serialize writes bIsValid as a single uint8 (not a 4-byte UBOOL).
            let is_valid = r.read_u8()? != 0;
            json!({ "min": min, "max": max, "is_valid": is_valid })
        }
        "Box2f" => {
            let min = json!({ "x": r.read_f32()?, "y": r.read_f32()? });
            let max = json!({ "x": r.read_f32()?, "y": r.read_f32()? });
            let is_valid = r.read_u8()? != 0;
            json!({ "min": min, "max": max, "is_valid": is_valid })
        }
        "FrameNumber" => json!({ "value": r.read_i32()? }),
        // FrameRate deliberately has no arm: TStructOpsTypeTraits<FFrameRate> keeps
        // WithSerializer disabled (UE keeps the generic UPROPERTY layout for existing
        // assets), so a StructProperty(FrameRate) payload is tagged properties.
        // ScalarKind::FrameRate below still covers the genuinely native contexts
        // (PerPlatformFrameRate, MovieScene channel tick resolution).
        "IntVector2" => json!({ "x": r.read_i32()?, "y": r.read_i32()? }),
        "IntVector4" => json!({
            "x": r.read_i32()?, "y": r.read_i32()?, "z": r.read_i32()?, "w": r.read_i32()?
        }),
        "DeprecateSlateVector2D" => json!({ "x": r.read_f32()?, "y": r.read_f32()? }),
        "RichCurveKey" => {
            let interp_mode = r.read_u8()?;
            let tangent_mode = r.read_u8()?;
            let tangent_weight_mode = r.read_u8()?;
            json!({
                "interp_mode": interp_mode,
                "tangent_mode": tangent_mode,
                "tangent_weight_mode": tangent_weight_mode,
                "time": r.read_f32()? as f64,
                "value": r.read_f32()? as f64,
                "arrive_tangent": r.read_f32()? as f64,
                "arrive_tangent_weight": r.read_f32()? as f64,
                "leave_tangent": r.read_f32()? as f64,
                "leave_tangent_weight": r.read_f32()? as f64,
            })
        }
        "PerPlatformFloat" => parse_per_platform(r, ctx.names, ScalarKind::F32, value_end)?,
        "PerPlatformInt" => parse_per_platform(r, ctx.names, ScalarKind::I32, value_end)?,
        "PerPlatformBool" => parse_per_platform(r, ctx.names, ScalarKind::Bool32, value_end)?,
        "PerPlatformFrameRate" => {
            parse_per_platform(r, ctx.names, ScalarKind::FrameRate, value_end)?
        }
        "ExpressionInput" | "MaterialAttributesInput" => {
            Value::Object(parse_expression_input(r, ctx)?)
        }
        "ScalarMaterialInput" => {
            let mut o = parse_expression_input(r, ctx)?;
            o.insert("use_constant".into(), json!(r.read_bool32()?));
            o.insert("constant".into(), json!(r.read_f32()? as f64));
            Value::Object(o)
        }
        "Vector2MaterialInput" => {
            let mut o = parse_expression_input(r, ctx)?;
            o.insert("use_constant".into(), json!(r.read_bool32()?));
            o.insert(
                "constant".into(),
                json!({ "x": r.read_f32()?, "y": r.read_f32()? }),
            );
            Value::Object(o)
        }
        "VectorMaterialInput" => {
            let mut o = parse_expression_input(r, ctx)?;
            o.insert("use_constant".into(), json!(r.read_bool32()?));
            o.insert(
                "constant".into(),
                json!({ "x": r.read_f32()?, "y": r.read_f32()?, "z": r.read_f32()? }),
            );
            Value::Object(o)
        }
        "ColorMaterialInput" => {
            let mut o = parse_expression_input(r, ctx)?;
            o.insert("use_constant".into(), json!(r.read_bool32()?));
            o.insert(
                "constant".into(),
                json!({
                    "r": r.read_f32()?, "g": r.read_f32()?, "b": r.read_f32()?, "a": r.read_f32()?
                }),
            );
            Value::Object(o)
        }
        "ShadingModelMaterialInput" | "SubstrateMaterialInput" => {
            let mut o = parse_expression_input(r, ctx)?;
            o.insert("use_constant".into(), json!(r.read_bool32()?));
            o.insert("constant".into(), json!(r.read_u32()?));
            Value::Object(o)
        }
        "MovieSceneFrameRange" => {
            let lower_type = r.read_u8()?;
            let lower = r.read_i32()?;
            let upper_type = r.read_u8()?;
            let upper = r.read_i32()?;
            json!({
                "lower_bound_type": lower_type,
                "lower_bound": lower,
                "upper_bound_type": upper_type,
                "upper_bound": upper,
            })
        }
        "MovieSceneFloatChannel" => parse_movie_scene_channel(r, ctx, false, value_end)?,
        "MovieSceneDoubleChannel" => parse_movie_scene_channel(r, ctx, true, value_end)?,
        "PerQualityLevelInt" => parse_per_quality_level(r, ScalarKind::I32, value_end)?,
        "PerQualityLevelFloat" => parse_per_quality_level(r, ScalarKind::F32, value_end)?,
        "EdGraphPinType" => {
            let pin_type = crate::pin::parse_pin_type(r, ctx, &ctx.pins)?;
            let mut o = serde_json::Map::new();
            o.insert("category".into(), json!(pin_type.category));
            o.insert("sub_category".into(), json!(pin_type.sub_category));
            o.insert(
                "sub_category_object".into(),
                (ctx.resolve_object)(pin_type.sub_category_object),
            );
            o.insert(
                "container_type".into(),
                json!(crate::pin::container_type_label(pin_type.container_type)),
            );
            if let Some(value_type) = &pin_type.value_type {
                o.insert(
                    "value_type".into(),
                    pin_terminal_type_to_json(value_type, ctx),
                );
            }
            o.insert("is_reference".into(), json!(pin_type.is_reference));
            o.insert("is_weak_pointer".into(), json!(pin_type.is_weak_pointer));
            if pin_type.member_parent != 0
                || !pin_type.member_name.is_empty()
                || !pin_type.member_guid.is_zero()
            {
                let mut member = serde_json::Map::new();
                if pin_type.member_parent != 0 {
                    member.insert(
                        "parent".into(),
                        (ctx.resolve_object)(pin_type.member_parent),
                    );
                }
                if !pin_type.member_name.is_empty() {
                    member.insert("name".into(), json!(pin_type.member_name));
                }
                if !pin_type.member_guid.is_zero() {
                    member.insert("guid".into(), json!(pin_type.member_guid.to_hex()));
                }
                o.insert("member_reference".into(), Value::Object(member));
            }
            o.insert("is_const".into(), json!(pin_type.is_const));
            o.insert(
                "is_uobject_wrapper".into(),
                json!(pin_type.is_uobject_wrapper),
            );
            o.insert(
                "serialize_as_single_precision_float".into(),
                json!(pin_type.serialize_as_single_precision_float),
            );
            Value::Object(o)
        }
        "InstancedStruct" => {
            // Modern format (>= CustomVersionAdded): no legacy header/version prefix.
            let script_struct = r.read_i32()?;
            let serial_size = r.read_i32()?;
            if serial_size < 0 {
                bail!("InstancedStruct serial size out of range: {serial_size}");
            }
            let inner_end = r.pos().saturating_add(serial_size as u64);
            if inner_end > value_end {
                bail!("InstancedStruct serial size exceeds value window: {serial_size}");
            }
            let nested = parse_properties(r, ctx, inner_end);
            r.seek(inner_end)?;
            json!({
                "script_struct": (ctx.resolve_object)(script_struct),
                "properties": entries_to_json(&nested)
            })
        }
        "GameplayEffectVersion" => {
            // FGameplayEffectVersion::Serialize writes the EGameplayEffectVersion byte.
            let v = r.read_u8()?;
            let name = match v {
                0 => "Monolithic",
                1 => "Modular53",
                2 => "AbilitiesComponent53",
                _ => "Unknown",
            };
            json!({ "current_version": v, "name": name })
        }
        "Spline" => {
            // FSpline::SerializeLoad writes an int8 implementation tag, followed by
            // variant data only when it is non-zero (legacy/new spline payloads,
            // not yet structured here).
            let impl_id = r.read_i8()?;
            match impl_id {
                0 => json!({ "implementation": "empty" }),
                _ => bail!("FSpline implementation {impl_id} not yet structured"),
            }
        }
        "GameplayTagContainer" => {
            // FGameplayTagContainer::Serialize writes the TArray<FGameplayTag>;
            // each FGameplayTag serializes as its single TagName (FName).
            let count = r.read_i32()?;
            let remaining = value_end.saturating_sub(r.pos());
            validate_count(count, remaining, 8, "GameplayTagContainer tag")?;
            let mut tags = Vec::with_capacity(count as usize);
            for _ in 0..count {
                tags.push(json!(ctx.names.resolve_raw(r.read_raw_name()?)));
            }
            json!({ "tags": tags })
        }
        "SkeletalMeshSamplingLODBuiltData" => {
            parse_skeletal_mesh_sampling_lod_built_data(r, value_end)?
        }
        "ClothLODDataCommon" => parse_cloth_lod_data_common(r, ctx, value_end)?,
        "ClothTetherData" => parse_cloth_tether_data(r, ctx, value_end)?,
        "GroomDataflowSettings" => {
            parse_tagged_struct_with_payload(r, name, ctx, value_end, "rest_collection")?
        }
        "InstancedPropertyBag" => parse_instanced_property_bag(r, value_end)?,
        "NiagaraDataInterfaceGPUParamInfo" => parse_niagara_gpu_param_info(r, ctx, value_end)?,
        // Niagara core variable types (modern format only). FNiagaraTypeDefinition
        // serializes via SerializeTaggedProperties, so it reuses parse_properties.
        "NiagaraTypeDefinition" if niagara_modern(ctx) => {
            let nested = parse_properties(r, ctx, value_end);
            json!({ "@struct": "NiagaraTypeDefinition", "properties": entries_to_json(&nested) })
        }
        "NiagaraVariableBase" if niagara_modern(ctx) => {
            Value::Object(parse_niagara_variable_base(r, ctx, value_end)?)
        }
        "NiagaraVariable" if niagara_modern(ctx) => {
            let mut o = parse_niagara_variable_base(r, ctx, value_end)?;
            // VarData: TArray<uint8> (the variable's default-value blob).
            let count = r.read_i32()?;
            let remaining = value_end.saturating_sub(r.pos());
            validate_count(count, remaining, 1, "NiagaraVariable data")?;
            let bytes = r.read_bytes(count as usize)?;
            o.insert("data_size".into(), json!(count));
            if !bytes.is_empty() {
                let n = bytes.len().min(PREVIEW_MAX);
                o.insert("data".into(), json!(to_hex(&bytes[..n])));
            }
            Value::Object(o)
        }
        "NiagaraVariableWithOffset" if niagara_modern(ctx) => {
            let mut o = parse_niagara_variable_base(r, ctx, value_end)?;
            o.insert("offset".into(), json!(r.read_i32()?));
            Value::Object(o)
        }
        _ => return Ok(None),
    };
    Ok(Some(v))
}

fn pin_terminal_type_to_json(ty: &crate::pin::PinTerminalType, ctx: &ParseCtx) -> Value {
    let mut o = serde_json::Map::new();
    o.insert("category".into(), json!(ty.category));
    o.insert("sub_category".into(), json!(ty.sub_category));
    o.insert(
        "sub_category_object".into(),
        (ctx.resolve_object)(ty.sub_category_object),
    );
    o.insert("is_const".into(), json!(ty.is_const));
    o.insert("is_weak_pointer".into(), json!(ty.is_weak_pointer));
    o.insert("is_uobject_wrapper".into(), json!(ty.is_uobject_wrapper));
    Value::Object(o)
}

fn niagara_modern(ctx: &ParseCtx) -> bool {
    ctx.niagara_version >= crate::version::custom::NIAGARA_VARIABLES_USE_TYPE_DEF_REGISTRY
}

fn read_name(r: &mut Reader, names: &NameMap) -> Result<String> {
    Ok(names.resolve_raw(r.read_raw_name()?))
}

fn parse_tagged_struct_with_payload(
    r: &mut Reader,
    name: &str,
    ctx: &ParseCtx,
    value_end: u64,
    payload_key: &str,
) -> Result<Value> {
    let nested = parse_properties(r, ctx, value_end);
    let mut o = Map::new();
    o.insert("@struct".into(), json!(name));
    o.insert("properties".into(), entries_to_json(&nested));
    if r.pos() < value_end {
        let payload_size = value_end - r.pos();
        let preview_len = payload_size.min(PREVIEW_MAX as u64) as usize;
        let preview = r.read_bytes(preview_len)?;
        if r.pos() < value_end {
            r.seek(value_end)?;
        }
        o.insert(
            payload_key.into(),
            json!({ "size": payload_size, "preview": to_hex(&preview) }),
        );
    }
    Ok(Value::Object(o))
}

fn parse_skeletal_mesh_sampling_lod_built_data(r: &mut Reader, value_end: u64) -> Result<Value> {
    Ok(json!({
        "area_weighted_triangle_sampler": parse_weighted_random_sampler(r, value_end)?
    }))
}

fn parse_weighted_random_sampler(r: &mut Reader, value_end: u64) -> Result<Value> {
    let prob = read_f32_array(r, value_end, "WeightedRandomSampler prob")?;
    let alias = read_i32_array(r, value_end, "WeightedRandomSampler alias")?;
    let total_weight = r.read_f32()? as f64;
    Ok(json!({
        "prob": prob,
        "alias": alias,
        "total_weight": total_weight
    }))
}

fn read_f32_array(r: &mut Reader, value_end: u64, label: &str) -> Result<Value> {
    let count = r.read_i32()?;
    let remaining = value_end.saturating_sub(r.pos());
    validate_count(count, remaining, 4, label)?;
    let mut values = Vec::with_capacity(count as usize);
    for _ in 0..count {
        values.push(json!(r.read_f32()? as f64));
    }
    Ok(Value::Array(values))
}

fn read_i32_array(r: &mut Reader, value_end: u64, label: &str) -> Result<Value> {
    let count = r.read_i32()?;
    let remaining = value_end.saturating_sub(r.pos());
    validate_count(count, remaining, 4, label)?;
    let mut values = Vec::with_capacity(count as usize);
    for _ in 0..count {
        values.push(json!(r.read_i32()?));
    }
    Ok(Value::Array(values))
}

fn parse_cloth_lod_data_common(r: &mut Reader, ctx: &ParseCtx, value_end: u64) -> Result<Value> {
    let nested = parse_properties(r, ctx, value_end);
    let mut o = Map::new();
    o.insert("@struct".into(), json!("ClothLODDataCommon"));
    o.insert("properties".into(), entries_to_json(&nested));
    o.insert(
        "transition_up_skin_data".into(),
        parse_mesh_to_mesh_vert_data_array(r, value_end, "TransitionUpSkinData")?,
    );
    o.insert(
        "transition_down_skin_data".into(),
        parse_mesh_to_mesh_vert_data_array(r, value_end, "TransitionDownSkinData")?,
    );
    Ok(Value::Object(o))
}

fn parse_cloth_tether_data(r: &mut Reader, ctx: &ParseCtx, value_end: u64) -> Result<Value> {
    const TETHER_TUPLE_SIZE: u64 = 12;
    const BATCH_SAMPLE_LIMIT: usize = 4;
    const TETHER_SAMPLE_LIMIT: usize = 4;

    let nested = parse_properties(r, ctx, value_end);
    let batch_count = r.read_i32()?;
    let remaining = value_end.saturating_sub(r.pos());
    validate_count(batch_count, remaining, 4, "ClothTetherData batch")?;
    let mut batch_sample = Vec::new();
    let mut tether_total = 0i64;
    for _ in 0..batch_count {
        let tether_count = r.read_i32()?;
        let remaining = value_end.saturating_sub(r.pos());
        validate_count(
            tether_count,
            remaining,
            TETHER_TUPLE_SIZE,
            "ClothTetherData tether",
        )?;
        tether_total += i64::from(tether_count);
        if batch_sample.len() < BATCH_SAMPLE_LIMIT {
            let mut tether_sample = Vec::new();
            for _ in 0..tether_count {
                if tether_sample.len() < TETHER_SAMPLE_LIMIT {
                    tether_sample.push(parse_cloth_tether_tuple(r)?);
                } else {
                    r.skip(TETHER_TUPLE_SIZE)?;
                }
                ensure_within_value(r, value_end, "ClothTetherData tether")?;
            }
            batch_sample.push(json!({
                "count": tether_count,
                "sample": tether_sample,
                "sample_truncated": (tether_count as usize) > tether_sample.len()
            }));
        } else {
            r.skip((tether_count as u64).saturating_mul(TETHER_TUPLE_SIZE))?;
            ensure_within_value(r, value_end, "ClothTetherData batch")?;
        }
    }
    Ok(json!({
        "@struct": "ClothTetherData",
        "properties": entries_to_json(&nested),
        "batch_count": batch_count,
        "tether_count": tether_total,
        "batch_sample": batch_sample,
        "batch_sample_truncated": (batch_count as usize) > batch_sample.len()
    }))
}

fn parse_cloth_tether_tuple(r: &mut Reader) -> Result<Value> {
    Ok(json!({
        "start": r.read_i32()?,
        "end": r.read_i32()?,
        "length": r.read_f32()? as f64
    }))
}

fn parse_instanced_property_bag(r: &mut Reader, value_end: u64) -> Result<Value> {
    let has_data = r.read_bool32()?;
    let mut o = Map::new();
    o.insert("has_data".into(), json!(has_data));
    if r.pos() < value_end {
        let payload_size = value_end - r.pos();
        let preview_len = payload_size.min(PREVIEW_MAX as u64) as usize;
        let preview = r.read_bytes(preview_len)?;
        if r.pos() < value_end {
            r.seek(value_end)?;
        }
        o.insert(
            "serialized_data".into(),
            json!({ "size": payload_size, "preview": to_hex(&preview) }),
        );
    }
    Ok(Value::Object(o))
}

fn parse_mesh_to_mesh_vert_data_array(
    r: &mut Reader,
    value_end: u64,
    label: &str,
) -> Result<Value> {
    const MESH_TO_MESH_VERT_DATA_SIZE: u64 = 64;
    const SAMPLE_LIMIT: usize = 4;

    let count = r.read_i32()?;
    let remaining = value_end.saturating_sub(r.pos());
    validate_count(count, remaining, MESH_TO_MESH_VERT_DATA_SIZE, label)?;
    let mut sample = Vec::new();
    for _ in 0..count {
        if sample.len() < SAMPLE_LIMIT {
            sample.push(parse_mesh_to_mesh_vert_data(r)?);
        } else {
            r.skip(MESH_TO_MESH_VERT_DATA_SIZE)?;
        }
        ensure_within_value(r, value_end, label)?;
    }
    Ok(json!({
        "count": count,
        "sample": sample,
        "sample_truncated": (count as usize) > sample.len()
    }))
}

fn parse_mesh_to_mesh_vert_data(r: &mut Reader) -> Result<Value> {
    let position_bary_coords_and_dist = read_vector4f(r)?;
    let normal_bary_coords_and_dist = read_vector4f(r)?;
    let tangent_bary_coords_and_dist = read_vector4f(r)?;
    let source_mesh_vert_indices =
        json!([r.read_u16()?, r.read_u16()?, r.read_u16()?, r.read_u16()?]);
    Ok(json!({
        "position_bary_coords_and_dist": position_bary_coords_and_dist,
        "normal_bary_coords_and_dist": normal_bary_coords_and_dist,
        "tangent_bary_coords_and_dist": tangent_bary_coords_and_dist,
        "source_mesh_vert_indices": source_mesh_vert_indices,
        "weight": r.read_f32()? as f64,
        "padding": r.read_u32()?
    }))
}

fn read_vector4f(r: &mut Reader) -> Result<Value> {
    Ok(json!({
        "x": r.read_f32()? as f64,
        "y": r.read_f32()? as f64,
        "z": r.read_f32()? as f64,
        "w": r.read_f32()? as f64
    }))
}

fn parse_niagara_gpu_param_info(r: &mut Reader, ctx: &ParseCtx, value_end: u64) -> Result<Value> {
    let mut o = Map::new();
    o.insert(
        "data_interface_hlsl_symbol".into(),
        json!(r.read_fstring()?),
    );
    o.insert("di_class_name".into(), json!(r.read_fstring()?));
    if ctx.niagara_version
        >= crate::version::custom::NIAGARA_ADD_GENERATED_FUNCTIONS_TO_GPU_PARAM_INFO
    {
        o.insert(
            "generated_functions".into(),
            parse_niagara_generated_functions(r, ctx, value_end)?,
        );
    }
    Ok(Value::Object(o))
}

fn parse_niagara_generated_functions(
    r: &mut Reader,
    ctx: &ParseCtx,
    value_end: u64,
) -> Result<Value> {
    let count = r.read_i32()?;
    let remaining = value_end.saturating_sub(r.pos());
    validate_count(count, remaining, 16, "Niagara generated function")?;
    let mut functions = Vec::with_capacity(count as usize);
    for _ in 0..count {
        functions.push(parse_niagara_generated_function(r, ctx, value_end)?);
        ensure_within_value(r, value_end, "Niagara generated function")?;
    }
    Ok(Value::Array(functions))
}

fn parse_niagara_generated_function(
    r: &mut Reader,
    ctx: &ParseCtx,
    value_end: u64,
) -> Result<Value> {
    let mut o = Map::new();
    o.insert("definition_name".into(), json!(read_name(r, ctx.names)?));
    o.insert("instance_name".into(), json!(r.read_fstring()?));
    o.insert(
        "specifiers".into(),
        parse_name_pair_array(r, ctx.names, value_end, "Niagara function specifier")?,
    );
    if ctx.niagara_version
        >= crate::version::custom::NIAGARA_ADD_VARIADIC_PARAMETERS_TO_GPU_FUNCTION_INFO
    {
        o.insert(
            "variadic_inputs".into(),
            parse_niagara_variable_references(r, ctx, value_end, "Niagara variadic input")?,
        );
        o.insert(
            "variadic_outputs".into(),
            parse_niagara_variable_references(r, ctx, value_end, "Niagara variadic output")?,
        );
    }
    if ctx.niagara_version
        >= crate::version::custom::NIAGARA_SERIALIZE_USAGE_BITMASK_TO_GPU_FUNCTION_INFO
    {
        o.insert("misc_usage_bitmask".into(), json!(r.read_u16()?));
    }
    Ok(Value::Object(o))
}

fn parse_name_pair_array(
    r: &mut Reader,
    names: &NameMap,
    value_end: u64,
    label: &str,
) -> Result<Value> {
    let count = r.read_i32()?;
    let remaining = value_end.saturating_sub(r.pos());
    validate_count(count, remaining, 16, label)?;
    let mut pairs = Vec::with_capacity(count as usize);
    for _ in 0..count {
        pairs.push(json!({
            "key": read_name(r, names)?,
            "value": read_name(r, names)?
        }));
        ensure_within_value(r, value_end, label)?;
    }
    Ok(Value::Array(pairs))
}

fn parse_niagara_variable_references(
    r: &mut Reader,
    ctx: &ParseCtx,
    value_end: u64,
    label: &str,
) -> Result<Value> {
    let count = r.read_i32()?;
    let remaining = value_end.saturating_sub(r.pos());
    validate_count(count, remaining, 12, label)?;
    let mut refs = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let name = read_name(r, ctx.names)?;
        let underlying_type = r.read_i32()?;
        refs.push(json!({
            "name": name,
            "underlying_type": (ctx.resolve_object)(underlying_type)
        }));
        ensure_within_value(r, value_end, label)?;
    }
    Ok(Value::Array(refs))
}

/// FNiagaraVariableBase::Serialize (modern): `Ar << Name; Ar << TypeDefHandle;`
/// where TypeDefHandle serializes a full FNiagaraTypeDefinition via tagged
/// properties. Leaves the reader positioned right after the type definition.
fn parse_niagara_variable_base(
    r: &mut Reader,
    ctx: &ParseCtx,
    value_end: u64,
) -> Result<serde_json::Map<String, Value>> {
    let name = ctx.names.resolve_raw(r.read_raw_name()?);
    let type_def = parse_properties(r, ctx, value_end);
    let mut o = serde_json::Map::new();
    o.insert("name".into(), json!(name));
    o.insert(
        "type".into(),
        json!({ "@struct": "NiagaraTypeDefinition", "properties": entries_to_json(&type_def) }),
    );
    Ok(o)
}

#[derive(Clone, Copy)]
enum ScalarKind {
    F32,
    I32,
    Bool32,
    FrameRate,
}

fn read_scalar(r: &mut Reader, kind: ScalarKind) -> Result<Value> {
    Ok(match kind {
        ScalarKind::F32 => json!(r.read_f32()? as f64),
        ScalarKind::I32 => json!(r.read_i32()?),
        ScalarKind::Bool32 => json!(r.read_bool32()?),
        ScalarKind::FrameRate => {
            json!({ "numerator": r.read_i32()?, "denominator": r.read_i32()? })
        }
    })
}

fn parse_per_platform(
    r: &mut Reader,
    names: &NameMap,
    kind: ScalarKind,
    value_end: u64,
) -> Result<Value> {
    let cooked = r.read_bool32()?;
    let default = read_scalar(r, kind)?;
    let mut per_platform = Vec::new();
    if !cooked {
        let count = r.read_i32()?;
        let remaining = value_end.saturating_sub(r.pos());
        validate_count(count, remaining, 12, "PerPlatform map")?;
        for _ in 0..count {
            let key = names.resolve_raw(r.read_raw_name()?);
            let value = read_scalar(r, kind)?;
            per_platform.push(json!({ "platform": key, "value": value }));
            ensure_within_value(r, value_end, "PerPlatform map entry")?;
        }
    }
    Ok(json!({ "default": default, "per_platform": per_platform }))
}

fn parse_per_quality_level(r: &mut Reader, kind: ScalarKind, value_end: u64) -> Result<Value> {
    let cooked = r.read_bool32()?;
    let default = read_scalar(r, kind)?;
    let mut per_quality = Vec::new();
    if !cooked {
        let count = r.read_i32()?;
        let remaining = value_end.saturating_sub(r.pos());
        validate_count(count, remaining, 8, "PerQualityLevel map")?;
        for _ in 0..count {
            let quality_level = r.read_i32()?;
            let value = read_scalar(r, kind)?;
            per_quality.push(json!({ "quality_level": quality_level, "value": value }));
            ensure_within_value(r, value_end, "PerQualityLevel map entry")?;
        }
    }
    Ok(json!({ "default": default, "per_quality": per_quality }))
}

fn parse_expression_input(
    r: &mut Reader,
    ctx: &ParseCtx,
) -> Result<serde_json::Map<String, Value>> {
    let expression = r.read_i32()?;
    let output_index = r.read_i32()?;
    let input_name = ctx.names.resolve_raw(r.read_raw_name()?);
    let mask = r.read_i32()?;
    let mask_r = r.read_i32()?;
    let mask_g = r.read_i32()?;
    let mask_b = r.read_i32()?;
    let mask_a = r.read_i32()?;
    let mut o = serde_json::Map::new();
    o.insert("expression".into(), (ctx.resolve_object)(expression));
    o.insert("output_index".into(), json!(output_index));
    o.insert("input_name".into(), json!(input_name));
    o.insert("mask".into(), json!([mask, mask_r, mask_g, mask_b, mask_a]));
    Ok(o)
}

fn parse_movie_scene_channel(
    r: &mut Reader,
    ctx: &ParseCtx,
    is_double: bool,
    value_end: u64,
) -> Result<Value> {
    let pre_infinity = r.read_u8()?;
    let post_infinity = r.read_u8()?;

    // Times: serialized element size (sizeof FFrameNumber == 4), count, then raw int32 data.
    let times_elem_size = r.read_i32()?;
    if times_elem_size != 4 {
        bail!("unexpected MovieScene time element size: {times_elem_size}");
    }
    let times_count = r.read_i32()?;
    let remaining = value_end.saturating_sub(r.pos());
    if times_count < 0 || (times_count as u64).saturating_mul(4) > remaining {
        bail!("MovieScene times count out of range: {times_count}");
    }
    let mut times = Vec::with_capacity(times_count as usize);
    for _ in 0..times_count {
        times.push(json!(r.read_i32()?));
    }

    // Values: serialized element size (POD struct dumped with padding), count, raw data.
    let value_size: u64 = if is_double { 8 } else { 4 };
    let val_elem_size = r.read_i32()?;
    if (val_elem_size as i64) < (value_size + 22) as i64 || val_elem_size > 64 {
        bail!("unexpected MovieScene value element size: {val_elem_size}");
    }
    let val_count = r.read_i32()?;
    let remaining = value_end.saturating_sub(r.pos());
    if val_count < 0 || (val_count as u64).saturating_mul(val_elem_size as u64) > remaining {
        bail!("MovieScene values count out of range: {val_count}");
    }
    let mut values = Vec::with_capacity(val_count as usize);
    for _ in 0..val_count {
        let elem_start = r.pos();
        let value = if is_double {
            json!(r.read_f64()?)
        } else {
            json!(r.read_f32()? as f64)
        };
        let arrive_tangent = r.read_f32()? as f64;
        let leave_tangent = r.read_f32()? as f64;
        let arrive_tangent_weight = r.read_f32()? as f64;
        let leave_tangent_weight = r.read_f32()? as f64;
        let tangent_weight_mode = r.read_u8()?;
        // InterpMode/TangentMode follow the 20-byte tangent block (past its padding).
        r.seek(elem_start + value_size + 20)?;
        let interp_mode = r.read_u8()?;
        let tangent_mode = r.read_u8()?;
        r.seek(elem_start + val_elem_size as u64)?;
        values.push(json!({
            "value": value,
            "interp_mode": interp_mode,
            "tangent_mode": tangent_mode,
            "tangent_weight_mode": tangent_weight_mode,
            "arrive_tangent": arrive_tangent,
            "leave_tangent": leave_tangent,
            "arrive_tangent_weight": arrive_tangent_weight,
            "leave_tangent_weight": leave_tangent_weight,
        }));
    }

    let default_value = if is_double {
        json!(r.read_f64()?)
    } else {
        json!(r.read_f32()? as f64)
    };
    let has_default_value = r.read_bool32()?;
    let tick_numerator = r.read_i32()?;
    let tick_denominator = r.read_i32()?;
    let mut out = serde_json::Map::new();
    out.insert("pre_infinity_extrap".into(), json!(pre_infinity));
    out.insert("post_infinity_extrap".into(), json!(post_infinity));
    out.insert("times".into(), Value::Array(times));
    out.insert("values".into(), Value::Array(values));
    out.insert("default_value".into(), default_value);
    out.insert("has_default_value".into(), json!(has_default_value));
    out.insert(
        "tick_resolution".into(),
        json!({ "numerator": tick_numerator, "denominator": tick_denominator }),
    );
    // bShowCurve is gated on FFortniteMainBranchObjectVersion; a position-based
    // heuristic would misread when the channel sits inside an array (value_end then
    // spans the remaining elements, not just this channel).
    if ctx.fortnite_main_version >= crate::version::custom::SERIALIZE_FLOAT_CHANNEL_SHOW_CURVE {
        out.insert("show_curve".into(), json!(r.read_bool32()?));
    }
    Ok(Value::Object(out))
}
