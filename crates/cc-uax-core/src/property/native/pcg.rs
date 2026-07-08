use super::gameplay::preview_payload;
use crate::property::{ParseCtx, validate_count};
use crate::reader::Reader;
use anyhow::{Result, bail};
use serde_json::{Map, Value, json};

const POINT_ARRAY_SAMPLE_MAX: i32 = 3;

#[derive(Clone, Copy)]
enum PcgValueKind {
    Transform,
    Float,
    Vector,
    Vector4,
    Int32,
    Int64,
}

impl PcgValueKind {
    fn size_bytes(self) -> u64 {
        match self {
            PcgValueKind::Transform => 80,
            PcgValueKind::Float => 4,
            PcgValueKind::Vector => 24,
            PcgValueKind::Vector4 => 32,
            PcgValueKind::Int32 => 4,
            PcgValueKind::Int64 => 8,
        }
    }
}

pub(super) fn parse_pcg_struct(
    r: &mut Reader,
    name: &str,
    ctx: &ParseCtx,
    value_end: u64,
) -> Result<Option<Value>> {
    let v = match name {
        "PCGPointArray" => parse_pcg_point_array(r, value_end)?,
        "PCGPoint" => parse_pcg_point(r, value_end)?,
        "PCGDataPtrWrapper" => {
            let data = r.read_i32()?;
            json!({ "data": (ctx.resolve_object)(data) })
        }
        _ => return Ok(None),
    };
    Ok(Some(v))
}

fn parse_pcg_point_array(r: &mut Reader, value_end: u64) -> Result<Value> {
    let num_points = r.read_i32()?;
    if num_points < 0 {
        bail!("PCGPointArray NumPoints out of range: {num_points}");
    }

    let mut o = Map::new();
    o.insert("num_points".into(), json!(num_points));

    let fields = [
        ("transform", PcgValueKind::Transform),
        ("density", PcgValueKind::Float),
        ("bounds_min", PcgValueKind::Vector),
        ("bounds_max", PcgValueKind::Vector),
        ("color", PcgValueKind::Vector4),
        ("steepness", PcgValueKind::Float),
        ("seed", PcgValueKind::Int32),
        ("metadata_entry", PcgValueKind::Int64),
    ];

    for (name, kind) in fields {
        o.insert(
            name.into(),
            parse_pcg_point_array_property(r, value_end, kind, name)?,
        );
    }

    if r.pos() < value_end {
        o.insert(
            "payload_tail".into(),
            preview_payload(r, r.pos(), value_end)?,
        );
    }

    Ok(Value::Object(o))
}

fn parse_pcg_point_array_property(
    r: &mut Reader,
    value_end: u64,
    kind: PcgValueKind,
    label: &str,
) -> Result<Value> {
    let num_values = r.read_i32()?;
    if num_values < 0 {
        bail!("PCGPointArray {label} NumValues out of range: {num_values}");
    }
    let default = read_pcg_value(r, kind)?;

    let values_count = r.read_i32()?;
    let remaining = value_end.saturating_sub(r.pos());
    validate_count(values_count, remaining, kind.size_bytes(), label)?;

    let sample_count = values_count.min(POINT_ARRAY_SAMPLE_MAX);
    let mut sample = Vec::with_capacity(sample_count as usize);
    for _ in 0..sample_count {
        sample.push(read_pcg_value(r, kind)?);
    }

    let skipped = values_count - sample_count;
    if skipped > 0 {
        r.skip((skipped as u64).saturating_mul(kind.size_bytes()))?;
    }

    Ok(json!({
        "num_values": num_values,
        "default": default,
        "allocated": values_count > 0,
        "values_count": values_count,
        "values_sample": sample
    }))
}

fn parse_pcg_point(r: &mut Reader, value_end: u64) -> Result<Value> {
    // FPCGPoint structured serializer writes a uint8 field mask, then the
    // transform, then only non-default fields indicated by the mask.
    let mask = r.read_u8()?;
    if mask & !0x7f != 0 {
        bail!("PCGPoint serialize mask out of range: {mask}");
    }

    let mut o = Map::new();
    o.insert("serialize_mask".into(), json!(mask));
    o.insert(
        "transform".into(),
        read_pcg_value(r, PcgValueKind::Transform)?,
    );
    if mask & (1 << 0) != 0 {
        o.insert("density".into(), read_pcg_value(r, PcgValueKind::Float)?);
    }
    if mask & (1 << 1) != 0 {
        o.insert(
            "bounds_min".into(),
            read_pcg_value(r, PcgValueKind::Vector)?,
        );
    }
    if mask & (1 << 2) != 0 {
        o.insert(
            "bounds_max".into(),
            read_pcg_value(r, PcgValueKind::Vector)?,
        );
    }
    if mask & (1 << 3) != 0 {
        o.insert("color".into(), read_pcg_value(r, PcgValueKind::Vector4)?);
    }
    if mask & (1 << 4) != 0 {
        o.insert("steepness".into(), read_pcg_value(r, PcgValueKind::Float)?);
    }
    if mask & (1 << 5) != 0 {
        o.insert("seed".into(), read_pcg_value(r, PcgValueKind::Int32)?);
    }
    if mask & (1 << 6) != 0 {
        o.insert(
            "metadata_entry".into(),
            read_pcg_value(r, PcgValueKind::Int64)?,
        );
    }

    if r.pos() < value_end {
        o.insert(
            "payload_tail".into(),
            preview_payload(r, r.pos(), value_end)?,
        );
    }
    Ok(Value::Object(o))
}

fn read_pcg_value(r: &mut Reader, kind: PcgValueKind) -> Result<Value> {
    match kind {
        PcgValueKind::Transform => Ok(json!({
            "rotation": read_quat(r)?,
            "translation": read_vector(r)?,
            "scale": read_vector(r)?
        })),
        PcgValueKind::Float => Ok(json!(r.read_f32()? as f64)),
        PcgValueKind::Vector => read_vector(r),
        PcgValueKind::Vector4 => Ok(json!({
            "x": r.read_f64()?,
            "y": r.read_f64()?,
            "z": r.read_f64()?,
            "w": r.read_f64()?
        })),
        PcgValueKind::Int32 => Ok(json!(r.read_i32()?)),
        PcgValueKind::Int64 => Ok(json!(r.read_i64()?)),
    }
}

fn read_quat(r: &mut Reader) -> Result<Value> {
    Ok(json!({
        "x": r.read_f64()?,
        "y": r.read_f64()?,
        "z": r.read_f64()?,
        "w": r.read_f64()?
    }))
}

fn read_vector(r: &mut Reader) -> Result<Value> {
    Ok(json!({
        "x": r.read_f64()?,
        "y": r.read_f64()?,
        "z": r.read_f64()?
    }))
}
