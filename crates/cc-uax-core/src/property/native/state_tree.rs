use super::gameplay::preview_payload;
use crate::property::{ParseCtx, entries_to_json, parse_properties_report};
use crate::reader::Reader;
use anyhow::Result;
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
    let mut o = Map::new();
    o.insert("@struct".into(), json!("StateTreeInstanceStorage"));
    o.insert("properties".into(), entries_to_json(&parsed.entries));
    if parsed.status.is_output_relevant() {
        o.insert("property_status".into(), json!(parsed.status.as_str()));
    }
    if !parsed.diagnostics.is_empty() {
        o.insert("property_diagnostics".into(), json!(parsed.diagnostics));
    }
    if r.pos() < value_end {
        o.insert(
            "payload_tail".into(),
            preview_payload(r, r.pos(), value_end)?,
        );
    }
    Ok(Value::Object(o))
}
