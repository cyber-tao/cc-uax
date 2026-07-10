use crate::property::ParseCtx;
use crate::reader::Reader;
use anyhow::{Result, bail};
use serde_json::{Value, json};
// Sequencer MovieScene channels/ranges.
pub(super) fn parse_sequencer_struct(
    r: &mut Reader,
    name: &str,
    ctx: &ParseCtx,
    value_end: u64,
) -> Result<Option<Value>> {
    let v = match name {
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
        _ => return Ok(None),
    };
    Ok(Some(v))
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
    if ctx.serialization.fortnite_main_version
        >= crate::version::custom::SERIALIZE_FLOAT_CHANNEL_SHOW_CURVE
    {
        out.insert("show_curve".into(), json!(r.read_bool32()?));
    }
    Ok(Value::Object(out))
}
