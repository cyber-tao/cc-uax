use super::ensure_tagged_payload_parsed;
use crate::property::{ParseCtx, entries_to_values, parse_properties_report, parse_soft_object};
use crate::reader::Reader;
use crate::structured_value::{Map, Value, json};
use crate::version::custom;
use anyhow::Result;

/// The structs `SerializeExpressionInput` (MaterialShared.cpp) serialises: it returns
/// false, i.e. tagged serialization, while `FCoreObjectVersion` is below
/// `MaterialInputNativeSerialize`.
fn is_material_input_struct(name: &str) -> bool {
    matches!(
        name,
        "ExpressionInput"
            | "MaterialAttributesInput"
            | "ScalarMaterialInput"
            | "Vector2MaterialInput"
            | "VectorMaterialInput"
            | "ColorMaterialInput"
            | "ShadingModelMaterialInput"
            | "SubstrateMaterialInput"
            | "StrataMaterialInput"
    )
}

// Material expression inputs (FExpressionInput + FMaterialInput<T> constants).
pub(super) fn parse_material_input_struct(
    r: &mut Reader,
    name: &str,
    ctx: &ParseCtx,
) -> Result<Option<Value>> {
    if is_material_input_struct(name)
        && ctx.serialization.core_object_version < custom::CORE_MATERIAL_INPUT_NATIVE_SERIALIZE
    {
        return Ok(None);
    }
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
            if ctx.serialization.fortnite_main_version < custom::MATERIAL_INPUT_USES_LINEAR_COLOR {
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
        // `FStrataMaterialInput` is the UE5.0–5.3 name of `FSubstrateMaterialInput`
        // (MaterialShared.cpp: `SerializeMaterialInput<uint32>` in both).
        "ShadingModelMaterialInput" | "SubstrateMaterialInput" | "StrataMaterialInput" => {
            let mut o = parse_expression_input(r, ctx)?;
            o.insert("use_constant".into(), json!(r.read_bool32()?));
            o.insert("constant".into(), json!(r.read_u32()?));
            Value::Object(o)
        }
        _ => return Ok(None),
    };
    Ok(Some(v))
}

fn parse_expression_input(r: &mut Reader, ctx: &ParseCtx) -> Result<Map> {
    let expression = r.read_i32()?;
    let output_index = r.read_i32()?;
    // `InputName` is an FName from `FFrameworkObjectVersion::PinsStoreFName`, an
    // FString before it (`SerializeExpressionInput`).
    let input_name =
        if ctx.serialization.framework_object_version >= custom::FRAMEWORK_PINS_STORE_FNAME {
            ctx.names.resolve_raw(r.read_raw_name()?)
        } else {
            r.read_fstring()?
        };
    let mask = r.read_i32()?;
    let mask_r = r.read_i32()?;
    let mask_g = r.read_i32()?;
    let mask_b = r.read_i32()?;
    let mask_a = r.read_i32()?;
    let mut o = Map::new();
    o.insert("expression".into(), (ctx.resolve_object)(expression));
    o.insert("output_index".into(), json!(output_index));
    o.insert("input_name".into(), json!(input_name));
    o.insert("mask".into(), json!([mask, mask_r, mask_g, mask_b, mask_a]));
    Ok(o)
}

/// `FMaterialOverrideNanite::Serialize` (MaterialOverrideNanite.cpp; the legacy
/// layout is all of UE5.1's `Serialize`). While
/// `FFortniteReleaseBranchCustomObjectVersion` is below
/// `NaniteMaterialOverrideUsesEditorOnly` it writes `OverrideMaterialRef` (an
/// `FSoftObjectPath`), `bEnableOverride` (a 4-byte bool) and `OverrideMaterial`
/// (a `UObject*`) and returns true. From it on it writes `bSerializeAsCookedData`
/// (4 bytes), a `UObject*` only when that is set, and returns false, so the rest
/// of the window is the default tagged serialization.
pub(super) fn parse_material_override_struct(
    r: &mut Reader,
    name: &str,
    ctx: &ParseCtx,
    value_end: u64,
) -> Result<Option<Value>> {
    if name != "MaterialOverrideNanite" {
        return Ok(None);
    }
    if ctx.serialization.fortnite_release_version
        < custom::FORTNITE_RELEASE_NANITE_MATERIAL_OVERRIDE_USES_EDITOR_ONLY
    {
        let override_material_ref = parse_soft_object(r, ctx, value_end)?;
        let enable_override = r.read_bool32_within(value_end, "MaterialOverrideNanite enable")?;
        let override_material = (ctx.resolve_object)(
            r.read_i32_within(value_end, "MaterialOverrideNanite override material")?,
        );
        return Ok(Some(json!({
            "@struct": "MaterialOverrideNanite",
            "override_material_ref": override_material_ref,
            "enable_override": enable_override,
            "override_material": override_material,
        })));
    }
    let mut out = Map::new();
    out.insert("@struct".into(), json!("MaterialOverrideNanite"));
    let cooked = r.read_bool32_within(value_end, "MaterialOverrideNanite cooked flag")?;
    out.insert("cooked".into(), json!(cooked));
    if cooked {
        let material = (ctx.resolve_object)(
            r.read_i32_within(value_end, "MaterialOverrideNanite cooked material")?,
        );
        out.insert("override_material".into(), material);
    }
    let nested = parse_properties_report(r, ctx, value_end, "/properties");
    ensure_tagged_payload_parsed(&nested.status, "MaterialOverrideNanite")?;
    out.insert("properties".into(), entries_to_values(&nested.entries));
    Ok(Some(Value::Object(out)))
}
