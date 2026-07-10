use crate::property::ParseCtx;
use crate::reader::Reader;
use anyhow::Result;
use serde_json::{Value, json};

const MATERIAL_INPUT_USES_LINEAR_COLOR: i32 = 171;

// Material expression inputs (FExpressionInput + FMaterialInput<T> constants).
pub(super) fn parse_material_input_struct(
    r: &mut Reader,
    name: &str,
    ctx: &ParseCtx,
) -> Result<Option<Value>> {
    let v = match name {
        "ExpressionInput" | "MaterialAttributesInput" => {
            Value::Object(parse_expression_input(r, ctx)?)
        }
        "ScalarMaterialInput" => {
            let mut o = parse_expression_input(r, ctx)?;
            o.insert("use_constant".into(), json!(r.read_bool32()?));
            o.insert("constant".into(), json!(r.read_f32()? as f64));
            Value::Object(o)
        }
        "Vector2MaterialInput" => {
            let mut o = parse_expression_input(r, ctx)?;
            o.insert("use_constant".into(), json!(r.read_bool32()?));
            o.insert(
                "constant".into(),
                json!({ "x": r.read_f32()?, "y": r.read_f32()? }),
            );
            Value::Object(o)
        }
        "VectorMaterialInput" => {
            let mut o = parse_expression_input(r, ctx)?;
            o.insert("use_constant".into(), json!(r.read_bool32()?));
            o.insert(
                "constant".into(),
                json!({ "x": r.read_f32()?, "y": r.read_f32()?, "z": r.read_f32()? }),
            );
            Value::Object(o)
        }
        "ColorMaterialInput" => {
            let mut o = parse_expression_input(r, ctx)?;
            o.insert("use_constant".into(), json!(r.read_bool32()?));
            if ctx.serialization.fortnite_main_version < MATERIAL_INPUT_USES_LINEAR_COLOR {
                o.insert("constant".into(), json!({ "packed_bgra": r.read_u32()? }));
            } else {
                o.insert(
                    "constant".into(),
                    json!({
                        "r": r.read_f32()?, "g": r.read_f32()?, "b": r.read_f32()?, "a": r.read_f32()?
                    }),
                );
            }
            Value::Object(o)
        }
        "ShadingModelMaterialInput" | "SubstrateMaterialInput" => {
            let mut o = parse_expression_input(r, ctx)?;
            o.insert("use_constant".into(), json!(r.read_bool32()?));
            o.insert("constant".into(), json!(r.read_u32()?));
            Value::Object(o)
        }
        _ => return Ok(None),
    };
    Ok(Some(v))
}

fn parse_expression_input(
    r: &mut Reader,
    ctx: &ParseCtx,
) -> Result<serde_json::Map<String, Value>> {
    let expression = r.read_i32()?;
    let output_index = r.read_i32()?;
    let input_name = ctx.names.resolve_raw(r.read_raw_name()?);
    let mask = r.read_i32()?;
    let mask_r = r.read_i32()?;
    let mask_g = r.read_i32()?;
    let mask_b = r.read_i32()?;
    let mask_a = r.read_i32()?;
    let mut o = serde_json::Map::new();
    o.insert("expression".into(), (ctx.resolve_object)(expression));
    o.insert("output_index".into(), json!(output_index));
    o.insert("input_name".into(), json!(input_name));
    o.insert("mask".into(), json!([mask, mask_r, mask_g, mask_b, mask_a]));
    Ok(o)
}
