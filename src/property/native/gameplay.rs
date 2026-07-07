use crate::property::{ParseCtx, entries_to_json, parse_properties, validate_count};
use crate::reader::Reader;
use anyhow::{Result, bail};
use serde_json::{Value, json};

// Gameplay / generic engine structs with custom native serialization.
pub(super) fn parse_gameplay_struct(
    r: &mut Reader,
    name: &str,
    ctx: &ParseCtx,
    value_end: u64,
) -> Result<Option<Value>> {
    let v = match name {
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
        _ => return Ok(None),
    };
    Ok(Some(v))
}
