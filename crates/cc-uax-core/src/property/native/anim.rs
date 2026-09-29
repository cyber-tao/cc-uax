use super::ensure_tagged_payload_parsed;
use crate::property::{
    ParseCtx, entries_to_values, parse_properties_report, parse_soft_object, validate_count,
};
use crate::reader::Reader;
use crate::structured_value::{Value, json};
use crate::version::{custom, ue4};
use anyhow::{Result, bail};

/// One `FVector3f` position or scale key.
const VECTOR3F_KEY_BYTES: i32 = 12;
/// One `FQuat4f` rotation key.
const QUAT4F_KEY_BYTES: i32 = 16;

// Animation structs whose `Serialize` writes a fixed binary field layout
// (`Ar << Field`), as opposed to `SerializeTaggedProperties`.
pub(super) fn parse_anim_struct(
    r: &mut Reader,
    name: &str,
    ctx: &ParseCtx,
    value_end: u64,
) -> Result<Option<Value>> {
    let v = match name {
        // FAnimationAttributeIdentifier::Serialize returns true and writes:
        //   Ar << Name << BoneName << BoneIndex << ScriptStructPath.
        "AnimationAttributeIdentifier" => {
            let name = ctx
                .names
                .resolve_raw(r.read_raw_name_within(value_end, "attribute name")?);
            let bone_name = ctx
                .names
                .resolve_raw(r.read_raw_name_within(value_end, "attribute bone name")?);
            let bone_index = r.read_i32_within(value_end, "attribute bone index")?;
            let script_struct_path = parse_soft_object(r, ctx, value_end)?;
            json!({
                "name": name,
                "bone_name": bone_name,
                "bone_index": bone_index,
                "script_struct_path": script_struct_path,
            })
        }
        "RawAnimSequenceTrack" => match parse_raw_anim_sequence_track(r, ctx, value_end)? {
            Some(v) => v,
            None => return Ok(None),
        },
        "SmartName" => parse_smart_name(r, ctx, value_end)?,
        "AttributeCurve" => parse_attribute_curve(r, ctx, value_end)?,
        _ => return Ok(None),
    };
    Ok(Some(v))
}

/// `FRawAnimSequenceTrack::Serialize` and `operator<<` (AnimTypes.h, UE5.1+; 5.0
/// declares no serializer). `Serialize` returns false, so the payload is tagged
/// properties, while `FUE5ReleaseStreamObjectVersion` is below
/// `RawAnimSequenceTrackSerializer`; from it on the payload is `PosKeys`
/// (`FVector3f`), `RotKeys` (`FQuat4f`) and, from
/// `VER_UE4_ANIM_SUPPORT_NONUNIFORM_SCALE_ANIMATION`, `ScaleKeys` (`FVector3f`),
/// each a `TArray::BulkSerialize` (Containers/Array.h): `int32 element size`,
/// `int32 count`, then `count` elements.
///
/// Only the counts are kept. A track runs to megabytes of floats per asset, and
/// the counts plus exact consumption of the window are the evidence.
fn parse_raw_anim_sequence_track(
    r: &mut Reader,
    ctx: &ParseCtx,
    value_end: u64,
) -> Result<Option<Value>> {
    if ctx.serialization.ue5_release_stream_version
        < custom::UE5_RELEASE_RAW_ANIM_SEQUENCE_TRACK_SERIALIZER
    {
        return Ok(None);
    }
    let pos_keys = skip_bulk_array(
        r,
        value_end,
        VECTOR3F_KEY_BYTES,
        "RawAnimSequenceTrack PosKeys",
    )?;
    let rot_keys = skip_bulk_array(
        r,
        value_end,
        QUAT4F_KEY_BYTES,
        "RawAnimSequenceTrack RotKeys",
    )?;
    let scale_keys = if ctx.file_version_ue4 >= ue4::ANIM_SUPPORT_NONUNIFORM_SCALE_ANIMATION {
        skip_bulk_array(
            r,
            value_end,
            VECTOR3F_KEY_BYTES,
            "RawAnimSequenceTrack ScaleKeys",
        )?
    } else {
        0
    };
    Ok(Some(json!({
        "@struct": "RawAnimSequenceTrack",
        "serialization": "bulk",
        "pos_keys": pos_keys,
        "rot_keys": rot_keys,
        "scale_keys": scale_keys,
    })))
}

/// Reads one `TArray::BulkSerialize` header, checks the element size against the
/// one the layout expects, and steps over the elements. Returns the count.
fn skip_bulk_array(
    r: &mut Reader,
    value_end: u64,
    expected_element_bytes: i32,
    label: &str,
) -> Result<i32> {
    let element_bytes = r.read_i32_within(value_end, label)?;
    if element_bytes != expected_element_bytes {
        bail!("{label} serialized element size {element_bytes}, expected {expected_element_bytes}");
    }
    let count = r.read_i32_within(value_end, label)?;
    let remaining = value_end.saturating_sub(r.pos());
    validate_count(count, remaining, expected_element_bytes as u64, label)?;
    let bytes = (count as u64) * (expected_element_bytes as u64);
    r.ensure_within(value_end, bytes, label)?;
    r.skip(bytes)?;
    Ok(count)
}

/// `FSmartName::Serialize` (SmartName.cpp; identical in UE5.0–5.8): the display
/// `FName`, then a `uint16` UID while `FAnimPhysObjectVersion` is below
/// `RemoveUIDFromSmartNameSerialize`, then an `FGuid` while it is below
/// `SmartNameRefactorForDeterministicCooking`. The transacting/duplicate branch
/// never occurs in a saved package. A package without the custom version reads
/// `-1`, which selects both legacy fields.
fn parse_smart_name(r: &mut Reader, ctx: &ParseCtx, value_end: u64) -> Result<Value> {
    let display_name = ctx
        .names
        .resolve_raw(r.read_raw_name_within(value_end, "SmartName display name")?);
    let mut out = crate::structured_value::Map::new();
    out.insert("@struct".into(), json!("SmartName"));
    out.insert("display_name".into(), json!(display_name));
    if ctx.serialization.anim_phys_version < custom::ANIM_PHYS_REMOVE_UID_FROM_SMART_NAME_SERIALIZE
    {
        out.insert(
            "uid".into(),
            json!(r.read_u16_within(value_end, "SmartName UID")?),
        );
    }
    if ctx.serialization.anim_phys_version
        < custom::ANIM_PHYS_SMART_NAME_REFACTOR_FOR_DETERMINISTIC_COOKING
    {
        r.read_guid_within(value_end, "SmartName GUID")?;
    }
    Ok(Value::Object(out))
}

/// `FAttributeCurve::Serialize` (AttributeCurve.cpp; identical in UE5.0–5.8):
/// `Ar << Keys` (an `int32` count and, per `FAttributeKey::operator<<`, only each
/// key's `float Time`), then `Ar << ScriptStructPath` (an `FSoftObjectPath`), and
/// when that path is not null each key's value in key order through
/// `ScriptStruct->SerializeItem`. The attribute structs are plain USTRUCTs, so
/// each value is a self-delimiting tagged block.
fn parse_attribute_curve(r: &mut Reader, ctx: &ParseCtx, value_end: u64) -> Result<Value> {
    let count = r.read_i32_within(value_end, "AttributeCurve key count")?;
    let remaining = value_end.saturating_sub(r.pos());
    validate_count(count, remaining, 4, "AttributeCurve key")?;
    let mut times = Vec::with_capacity(count as usize);
    for _ in 0..count {
        times.push(f64::from(
            r.read_f32_within(value_end, "AttributeCurve key time")?,
        ));
    }
    let script_struct = parse_soft_object(r, ctx, value_end)?;
    let path_is_null = script_struct
        .get("sub_path")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
        && script_struct
            .get("asset_path")
            .and_then(Value::as_str)
            .is_none_or(|path| path.is_empty() || path == "None");
    let struct_name = script_struct
        .get("asset_path")
        .and_then(Value::as_str)
        .map(|path| path.rsplit(['.', '/']).next().unwrap_or(path).to_owned())
        .unwrap_or_default();

    let mut keys = Vec::with_capacity(times.len());
    for time in times {
        let value = if path_is_null {
            Value::Null
        } else {
            let nested = parse_properties_report(r, ctx, value_end, "/properties");
            ensure_tagged_payload_parsed(&nested.status, "AttributeCurve key value")?;
            json!({
                "@struct": struct_name,
                "properties": entries_to_values(&nested.entries),
            })
        };
        keys.push(json!({ "time": time, "value": value }));
    }
    Ok(json!({
        "@struct": "AttributeCurve",
        "script_struct": script_struct,
        "keys": keys,
    }))
}
