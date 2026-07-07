use crate::property::{
    PREVIEW_MAX, ParseCtx, ensure_within_value, entries_to_json, parse_properties, to_hex,
    validate_count,
};
use crate::reader::Reader;
use anyhow::Result;
use serde_json::{Map, Value, json};

// Mesh / cloth / property-bag structs (sampled or hex-tailed payloads).
pub(super) fn parse_mesh_cloth_struct(
    r: &mut Reader,
    name: &str,
    ctx: &ParseCtx,
    value_end: u64,
) -> Result<Option<Value>> {
    let v = match name {
        "SkeletalMeshSamplingLODBuiltData" => {
            parse_skeletal_mesh_sampling_lod_built_data(r, value_end)?
        }
        "ClothLODDataCommon" => parse_cloth_lod_data_common(r, ctx, value_end)?,
        "ClothTetherData" => parse_cloth_tether_data(r, ctx, value_end)?,
        "GroomDataflowSettings" => {
            parse_tagged_struct_with_payload(r, name, ctx, value_end, "rest_collection")?
        }
        "InstancedPropertyBag" => parse_instanced_property_bag(r, value_end)?,
        _ => return Ok(None),
    };
    Ok(Some(v))
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
