use crate::property::ParseCtx;
use crate::reader::Reader;
use anyhow::Result;
use serde_json::{Value, json};

// Blueprint pin type embedded as a native struct payload.
pub(super) fn parse_graph_pin_struct(
    r: &mut Reader,
    name: &str,
    ctx: &ParseCtx,
) -> Result<Option<Value>> {
    let v = match name {
        "EdGraphPinType" => {
            let pin_type = crate::pin::parse_pin_type(r, ctx, &ctx.pins)?;
            let mut o = serde_json::Map::new();
            o.insert("category".into(), json!(pin_type.category));
            o.insert("sub_category".into(), json!(pin_type.sub_category));
            o.insert(
                "sub_category_object".into(),
                (ctx.resolve_object)(pin_type.sub_category_object),
            );
            o.insert(
                "container_type".into(),
                json!(crate::pin::container_type_label(pin_type.container_type)),
            );
            if let Some(value_type) = &pin_type.value_type {
                o.insert(
                    "value_type".into(),
                    pin_terminal_type_to_json(value_type, ctx),
                );
            }
            o.insert("is_reference".into(), json!(pin_type.is_reference));
            o.insert("is_weak_pointer".into(), json!(pin_type.is_weak_pointer));
            if pin_type.member_parent != 0
                || !pin_type.member_name.is_empty()
                || !pin_type.member_guid.is_zero()
            {
                let mut member = serde_json::Map::new();
                if pin_type.member_parent != 0 {
                    member.insert(
                        "parent".into(),
                        (ctx.resolve_object)(pin_type.member_parent),
                    );
                }
                if !pin_type.member_name.is_empty() {
                    member.insert("name".into(), json!(pin_type.member_name));
                }
                if !pin_type.member_guid.is_zero() {
                    member.insert("guid".into(), json!(pin_type.member_guid.to_hex()));
                }
                o.insert("member_reference".into(), Value::Object(member));
            }
            o.insert("is_const".into(), json!(pin_type.is_const));
            o.insert(
                "is_uobject_wrapper".into(),
                json!(pin_type.is_uobject_wrapper),
            );
            o.insert(
                "serialize_as_single_precision_float".into(),
                json!(pin_type.serialize_as_single_precision_float),
            );
            Value::Object(o)
        }
        _ => return Ok(None),
    };
    Ok(Some(v))
}

fn pin_terminal_type_to_json(ty: &crate::pin::PinTerminalType, ctx: &ParseCtx) -> Value {
    let mut o = serde_json::Map::new();
    o.insert("category".into(), json!(ty.category));
    o.insert("sub_category".into(), json!(ty.sub_category));
    o.insert(
        "sub_category_object".into(),
        (ctx.resolve_object)(ty.sub_category_object),
    );
    o.insert("is_const".into(), json!(ty.is_const));
    o.insert("is_weak_pointer".into(), json!(ty.is_weak_pointer));
    o.insert("is_uobject_wrapper".into(), json!(ty.is_uobject_wrapper));
    Value::Object(o)
}
