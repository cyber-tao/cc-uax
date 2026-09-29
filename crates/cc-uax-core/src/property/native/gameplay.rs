use crate::model::{OpaqueByteRange, OpaqueReason, OpaqueValue};
use crate::property::{
    PREVIEW_MAX, ParseCtx, PropertyParseStatus, entries_to_values, parse_properties_report, to_hex,
    validate_count,
};
use crate::reader::Reader;
use crate::structured_value::{Map, Value, json};
use anyhow::{Result, bail};

// Gameplay / generic engine structs with custom native serialization.
pub(super) fn parse_gameplay_struct(
    r: &mut Reader,
    name: &str,
    ctx: &ParseCtx,
    value_end: u64,
) -> Result<Option<Value>> {
    let v = match name {
        "InstancedStruct" => parse_instanced_struct(r, ctx, value_end)?,
        "InstancedStructContainer" => parse_instanced_struct_container(r, ctx, value_end)?,
        "UniversalObjectLocatorFragment" => {
            let start = r.pos();
            opaque_range(
                r,
                start,
                value_end,
                OpaqueReason::RegistryDependentPayload,
                name,
                "the locator fragment payload is interpreted by a runtime-registered locator type"
                    .into(),
            )?
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
        _ => return Ok(None),
    };
    Ok(Some(v))
}

fn parse_instanced_struct(r: &mut Reader, ctx: &ParseCtx, value_end: u64) -> Result<Value> {
    let mut o = Map::new();
    if ctx.serialization.instanced_struct_version
        < crate::version::custom::INSTANCED_STRUCT_CUSTOM_VERSION_ADDED
    {
        // Before FInstancedStructCustomVersion, editor archives prefixed the
        // payload with 0xABABABAB while non-editor archives wrote only the
        // legacy uint8 version. Probe the header exactly as UE5.7 does.
        const LEGACY_EDITOR_HEADER: u32 = 0xABAB_ABAB;
        let header_offset = r.pos();
        let has_editor_header = if value_end.saturating_sub(header_offset) >= 4 {
            if r.read_u32()? == LEGACY_EDITOR_HEADER {
                true
            } else {
                r.seek(header_offset)?;
                false
            }
        } else {
            false
        };
        let legacy_version = r.read_u8()?;
        o.insert("legacy_version".into(), json!(legacy_version));
        o.insert("legacy_editor_header".into(), json!(has_editor_header));
    }

    let script_struct = r.read_i32()?;
    let script_struct = (ctx.resolve_object)(script_struct);
    o.insert("script_struct".into(), script_struct.clone());
    append_serialized_struct_payload(r, ctx, value_end, &script_struct, &mut o)?;
    if r.pos() != value_end {
        bail!(
            "InstancedStruct ended at byte {}, expected {value_end}",
            r.pos()
        );
    }
    Ok(Value::Object(o))
}

fn parse_instanced_struct_container(
    r: &mut Reader,
    ctx: &ParseCtx,
    value_end: u64,
) -> Result<Value> {
    // FInstancedStructContainer::Serialize writes Version(uint8), item count,
    // then all script struct object refs, then each item payload size + payload.
    let version = r.read_u8()?;
    if version != 0 {
        bail!("InstancedStructContainer version out of range: {version}");
    }
    let count = r.read_i32()?;
    let remaining = value_end.saturating_sub(r.pos());
    validate_count(count, remaining, 8, "InstancedStructContainer item")?;

    let mut script_structs = Vec::with_capacity(count as usize);
    for _ in 0..count {
        script_structs.push(r.read_i32()?);
    }

    let mut items = Vec::with_capacity(count as usize);
    for (index, script_struct) in script_structs.into_iter().enumerate() {
        let mut item = Map::new();
        item.insert("index".into(), json!(index));
        let script_struct = (ctx.resolve_object)(script_struct);
        item.insert("script_struct".into(), script_struct.clone());
        append_serialized_struct_payload(r, ctx, value_end, &script_struct, &mut item)?;
        items.push(Value::Object(item));
    }

    if r.pos() != value_end {
        bail!(
            "InstancedStructContainer ended at byte {}, expected {value_end}",
            r.pos()
        );
    }
    Ok(json!({
        "version": version,
        "item_count": count,
        "items": items
    }))
}

pub(super) fn append_serialized_struct_payload(
    r: &mut Reader,
    ctx: &ParseCtx,
    value_end: u64,
    script_struct: &Value,
    out: &mut Map,
) -> Result<()> {
    let serial_size = r.read_i32()?;
    if serial_size < 0 {
        bail!("serialized struct size out of range: {serial_size}");
    }
    out.insert("serial_size".into(), json!(serial_size));

    let struct_type = script_struct
        .get("ref")
        .and_then(Value::as_str)
        .unwrap_or("InstancedStruct");
    let payload_start = r.pos();
    let payload_end = payload_start.saturating_add(serial_size as u64);
    if payload_end > value_end {
        bail!("serialized struct payload exceeds value window: {serial_size}");
    }
    if serial_size == 0 {
        return Ok(());
    }

    let parsed = r.with_limit(payload_end, |r| {
        parse_properties_report(r, ctx, payload_end, "/serialized_struct/properties")
    });
    if !parsed.entries.is_empty() {
        out.insert("properties".into(), entries_to_values(&parsed.entries));
    }
    // The diagnostics themselves reach the report through the enclosing tag loop
    // (`ParseCtx::nested_diagnostics`); only the status is worth repeating here.
    if parsed.status.is_output_relevant() || !parsed.diagnostics.is_empty() {
        out.insert("property_status".into(), json!(parsed.status.as_str()));
    }

    if parsed.entries.is_empty() && parsed.status == PropertyParseStatus::NonTaggedPayload {
        let payload = opaque_range(
            r,
            payload_start,
            payload_end,
            OpaqueReason::RegistryDependentPayload,
            struct_type,
            format!(
                "{struct_type} payload is not a tagged-property block, so its layout comes from the struct's native serializer"
            ),
        )?;
        out.insert("payload".into(), payload);
        return Ok(());
    }

    if r.pos() < payload_end {
        let tail_start = r.pos();
        let payload = opaque_range(
            r,
            tail_start,
            payload_end,
            OpaqueReason::PayloadTail,
            struct_type,
            format!("{struct_type} payload has bytes after its decoded tagged properties"),
        )?;
        out.insert("payload_tail".into(), payload);
    } else {
        r.seek(payload_end)?;
    }
    Ok(())
}

/// Reads the preview of `[start, end)`, leaves the reader at `end`, and returns
/// the range as an opaque value so the bytes stay attributable in the report.
pub(super) fn opaque_range(
    r: &mut Reader,
    start: u64,
    end: u64,
    reason: OpaqueReason,
    type_name: &str,
    message: String,
) -> Result<Value> {
    let size = end.saturating_sub(start);
    r.seek(start)?;
    let preview_len = size.min(PREVIEW_MAX as u64) as usize;
    let preview = r.read_bytes(preview_len)?;
    r.seek(end)?;
    Ok(Value::Opaque(OpaqueValue {
        reason,
        message,
        type_name: Some(type_name.to_owned()),
        byte_range: OpaqueByteRange {
            start,
            end,
            size,
            preview: to_hex(&preview),
        },
    }))
}
