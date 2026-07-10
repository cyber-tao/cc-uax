use crate::property::{
    ParseCtx, PropertyParseStatus, entries_to_values, parse_properties_report, validate_count,
};
use crate::reader::Reader;
use crate::structured_value::{Map, Value, json};
use anyhow::{Result, bail};

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
        "PCGPoint" => parse_pcg_point(r, ctx, value_end)?,
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
            parse_pcg_point_array_property(r, value_end, num_points, kind, name)?,
        );
    }

    if r.pos() != value_end {
        bail!(
            "PCGPointArray ended at byte {}, expected {value_end}",
            r.pos()
        );
    }

    Ok(Value::Object(o))
}

fn parse_pcg_point_array_property(
    r: &mut Reader,
    value_end: u64,
    expected_num_values: i32,
    kind: PcgValueKind,
    label: &str,
) -> Result<Value> {
    let num_values = r.read_i32()?;
    if num_values < 0 {
        bail!("PCGPointArray {label} NumValues out of range: {num_values}");
    }
    if num_values != expected_num_values {
        bail!(
            "PCGPointArray {label} NumValues {num_values} does not match NumPoints {expected_num_values}"
        );
    }
    let default = read_pcg_value(r, kind)?;

    let values_count = r.read_i32()?;
    let remaining = value_end.saturating_sub(r.pos());
    validate_count(values_count, remaining, kind.size_bytes(), label)?;
    if values_count != 0 && values_count != num_values {
        bail!(
            "PCGPointArray {label} allocated value count {values_count} does not match NumValues {num_values}"
        );
    }

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

fn parse_pcg_point(r: &mut Reader, ctx: &ParseCtx, value_end: u64) -> Result<Value> {
    if ctx.serialization.fortnite_release_version
        < crate::version::custom::PCG_POINT_STRUCTURED_SERIALIZER
    {
        // The structured serializer returns false before this custom version,
        // so UScriptStruct falls back to ordinary tagged-property serialization.
        let parsed = parse_properties_report(r, ctx, value_end, "/pcg_point");
        ensure_complete_tagged_payload(r, value_end, &parsed.status, "PCGPoint")?;
        let mut o = Map::new();
        o.insert("@struct".into(), json!("PCGPoint"));
        o.insert("serialization".into(), json!("legacy_tagged"));
        o.insert("properties".into(), entries_to_values(&parsed.entries));
        if parsed.status.is_output_relevant() {
            o.insert("property_status".into(), json!(parsed.status.as_str()));
        }
        if !parsed.diagnostics.is_empty() {
            o.insert("property_diagnostics".into(), json!(parsed.diagnostics));
        }
        return Ok(Value::Object(o));
    }

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
    // FPCGPoint's structured format omits fields equal to the C++ defaults.
    // Materialize those defaults so downstream consumers see one canonical
    // shape regardless of the serialize mask.
    o.insert("density".into(), json!(1.0));
    o.insert(
        "bounds_min".into(),
        json!({ "x": -1.0, "y": -1.0, "z": -1.0 }),
    );
    o.insert("bounds_max".into(), json!({ "x": 1.0, "y": 1.0, "z": 1.0 }));
    o.insert(
        "color".into(),
        json!({ "x": 1.0, "y": 1.0, "z": 1.0, "w": 1.0 }),
    );
    o.insert("steepness".into(), json!(0.5));
    o.insert("seed".into(), json!(0));
    o.insert("metadata_entry".into(), json!(-1));
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

    if r.pos() != value_end {
        bail!("PCGPoint ended at byte {}, expected {value_end}", r.pos());
    }
    Ok(Value::Object(o))
}

fn ensure_complete_tagged_payload(
    r: &Reader,
    value_end: u64,
    status: &PropertyParseStatus,
    name: &str,
) -> Result<()> {
    if matches!(
        status,
        PropertyParseStatus::NonTaggedPayload | PropertyParseStatus::FailedAfterEntries
    ) {
        bail!("{name} tagged payload is malformed ({})", status.as_str());
    }
    if r.pos() != value_end {
        bail!(
            "{name} tagged payload ended at byte {}, expected {value_end}",
            r.pos()
        );
    }
    Ok(())
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
