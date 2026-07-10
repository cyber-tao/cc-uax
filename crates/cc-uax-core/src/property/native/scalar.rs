use crate::name::NameMap;
use crate::property::{ParseCtx, ensure_within_value, validate_count};
use crate::reader::Reader;
use crate::structured_value::{Value, json};
use anyhow::Result;
// PerPlatform* / PerQualityLevel* scalar overrides (default + optional map).
pub(super) fn parse_scalar_struct(
    r: &mut Reader,
    name: &str,
    ctx: &ParseCtx,
    value_end: u64,
) -> Result<Option<Value>> {
    let v = match name {
        "PerPlatformFloat" => parse_per_platform(r, ctx.names, ScalarKind::F32, value_end)?,
        "PerPlatformInt" => parse_per_platform(r, ctx.names, ScalarKind::I32, value_end)?,
        "PerPlatformBool" => parse_per_platform(r, ctx.names, ScalarKind::Bool32, value_end)?,
        "PerPlatformFrameRate" => {
            parse_per_platform(r, ctx.names, ScalarKind::FrameRate, value_end)?
        }
        "PerQualityLevelInt" => parse_per_quality_level(r, ScalarKind::I32, value_end)?,
        "PerQualityLevelFloat" => parse_per_quality_level(r, ScalarKind::F32, value_end)?,
        _ => return Ok(None),
    };
    Ok(Some(v))
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
