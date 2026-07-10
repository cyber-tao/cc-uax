use crate::property::{ParseCtx, PropertyParseStatus, entries_to_json, parse_properties_report};
use crate::reader::Reader;
use anyhow::{Result, bail};
use serde_json::{Map, Value, json};

// Runtime StateTree structs with custom serializers.
pub(super) fn parse_state_tree_struct(
    r: &mut Reader,
    name: &str,
    ctx: &ParseCtx,
    value_end: u64,
) -> Result<Option<Value>> {
    let v = match name {
        // FStateTreeInstanceData::Serialize writes the FStateTreeInstanceStorage
        // payload directly through StaticStruct()->SerializeItem().
        "StateTreeInstanceData" => parse_state_tree_instance_data(r, ctx, value_end)?,
        "StateTreeInstanceStorage" => parse_state_tree_instance_storage(r, ctx, value_end)?,
        _ => return Ok(None),
    };
    Ok(Some(v))
}

fn parse_state_tree_instance_data(r: &mut Reader, ctx: &ParseCtx, value_end: u64) -> Result<Value> {
    if ctx.serialization.state_tree_instance_storage_version
        < crate::version::custom::STATE_TREE_INSTANCE_STORAGE_ADDED_CUSTOM_SERIALIZATION
    {
        // Before AddedCustomSerialization, Serialize() delegates to the
        // tagged properties of FStateTreeInstanceData itself. In editor data
        // this carries InstanceStorage_DEPRECATED as an InstancedStruct.
        let parsed = parse_properties_report(r, ctx, value_end, "/state_tree_instance_data");
        ensure_complete_tagged_payload(r, value_end, &parsed.status, "StateTreeInstanceData")?;
        let mut o = Map::new();
        o.insert("@struct".into(), json!("StateTreeInstanceData"));
        o.insert("serialization".into(), json!("legacy_tagged"));
        o.insert("properties".into(), entries_to_json(&parsed.entries));
        if parsed.status.is_output_relevant() {
            o.insert("property_status".into(), json!(parsed.status.as_str()));
        }
        if !parsed.diagnostics.is_empty() {
            o.insert("property_diagnostics".into(), json!(parsed.diagnostics));
        }
        return Ok(Value::Object(o));
    }

    Ok(json!({
        "@struct": "StateTreeInstanceData",
        "storage": parse_state_tree_instance_storage(r, ctx, value_end)?
    }))
}

fn parse_state_tree_instance_storage(
    r: &mut Reader,
    ctx: &ParseCtx,
    value_end: u64,
) -> Result<Value> {
    let parsed = parse_properties_report(r, ctx, value_end, "/state_tree_instance_storage");
    ensure_complete_tagged_payload(r, value_end, &parsed.status, "StateTreeInstanceStorage")?;
    let mut o = Map::new();
    o.insert("@struct".into(), json!("StateTreeInstanceStorage"));
    o.insert("properties".into(), entries_to_json(&parsed.entries));
    if parsed.status.is_output_relevant() {
        o.insert("property_status".into(), json!(parsed.status.as_str()));
    }
    if !parsed.diagnostics.is_empty() {
        o.insert("property_diagnostics".into(), json!(parsed.diagnostics));
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
