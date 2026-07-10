use super::native::{is_tagged_fallback_struct, parse_native_struct};
use super::text::parse_text;
use super::{
    ParseCtx, TypeName, ensure_within_value, entries_to_values, parse_properties, validate_count,
};
use crate::name::NameMap;
use crate::reader::Reader;
use crate::structured_value::{Value, json};
use anyhow::{Result, bail};

pub(crate) fn parse_value(
    r: &mut Reader,
    ty: &TypeName,
    ctx: &ParseCtx,
    prefer_native: bool,
    value_end: u64,
) -> Result<Value> {
    let v = match ty.name.as_str() {
        "BoolProperty" => json!(r.read_u8()? != 0),
        "Int8Property" => json!(r.read_i8()?),
        "Int16Property" => json!(r.read_i16()?),
        "IntProperty" => json!(r.read_i32()?),
        "Int64Property" => json!(r.read_i64()?),
        "ByteProperty" => {
            if has_enum_param(ty) {
                json!(ctx.names.resolve_raw(r.read_raw_name()?))
            } else {
                json!(r.read_u8()?)
            }
        }
        "UInt16Property" => json!(r.read_u16()?),
        "UInt32Property" => json!(r.read_u32()?),
        "UInt64Property" => json!(r.read_u64()?),
        "FloatProperty" => json!(r.read_f32()? as f64),
        "DoubleProperty" => json!(r.read_f64()?),
        "EnumProperty" => json!(ctx.names.resolve_raw(r.read_raw_name()?)),
        "NameProperty" => json!(ctx.names.resolve_raw(r.read_raw_name()?)),
        "StrProperty" => json!(r.read_fstring()?),
        "TextProperty" => parse_text(r, ctx.names, 0)?,
        "ObjectProperty" | "ClassProperty" | "WeakObjectProperty" | "ObjectPtrProperty"
        | "ClassPtrProperty" | "InterfaceProperty" => {
            let idx = r.read_i32()?;
            (ctx.resolve_object)(idx)
        }
        "LazyObjectProperty" => {
            // FLinkerSave::operator<<(FLazyObjectPtr&) writes the 16-byte
            // FUniqueObjectGuid, not a package index.
            json!({ "lazy_object_guid": r.read_guid()?.to_hex() })
        }
        "DelegateProperty" => {
            let object = r.read_i32()?;
            let function = ctx.names.resolve_raw(r.read_raw_name()?);
            json!({ "object": (ctx.resolve_object)(object), "function": function })
        }
        "MulticastInlineDelegateProperty" | "MulticastSparseDelegateProperty" => {
            let count = r.read_i32()?;
            let remaining = value_end.saturating_sub(r.pos());
            validate_count(count, remaining, 12, "delegate invocation")?;
            let mut arr = Vec::with_capacity(count as usize);
            for _ in 0..count {
                let object = r.read_i32()?;
                let function = ctx.names.resolve_raw(r.read_raw_name()?);
                arr.push(json!({ "object": (ctx.resolve_object)(object), "function": function }));
            }
            Value::Array(arr)
        }
        "SoftObjectProperty" | "SoftClassProperty" => parse_soft_object(r, ctx)?,
        "FieldPathProperty" => {
            let count = r.read_i32()?;
            if !(0..=4096).contains(&count) {
                bail!("FieldPath length out of range: {count}");
            }
            let mut path = Vec::with_capacity(count as usize);
            for _ in 0..count {
                path.push(ctx.names.resolve_raw(r.read_raw_name()?));
            }
            let owner = r.read_i32()?;
            json!({ "path": path, "owner": (ctx.resolve_object)(owner) })
        }
        "OptionalProperty" => {
            // FOptionalProperty::SerializeItem encodes presence via the binary
            // structured-archive optional field (a 4-byte UBOOL), then the inner
            // value when set (UE5.7 serializes the value directly).
            let inner = ty
                .param(0)
                .ok_or_else(|| anyhow::anyhow!("OptionalProperty missing inner type"))?;
            if r.read_bool32()? {
                parse_value(r, inner, ctx, prefer_native, value_end)?
            } else {
                Value::Null
            }
        }
        "StructProperty" => {
            let struct_name = ty.param(0).map(|p| p.name.as_str()).unwrap_or("");
            parse_struct(r, struct_name, ctx, prefer_native, value_end)?
        }
        "ArrayProperty" => {
            let inner = ty
                .param(0)
                .ok_or_else(|| anyhow::anyhow!("ArrayProperty missing element type"))?;
            parse_collection(r, inner, ctx, prefer_native, value_end)?
        }
        "SetProperty" => {
            let inner = ty
                .param(0)
                .ok_or_else(|| anyhow::anyhow!("SetProperty missing element type"))?;
            discard_removed_elements(
                r,
                inner,
                ctx,
                prefer_native,
                value_end,
                "Set removed element",
            )?;
            parse_collection(r, inner, ctx, prefer_native, value_end)?
        }
        "MapProperty" => {
            let key_ty = ty
                .param(0)
                .ok_or_else(|| anyhow::anyhow!("MapProperty missing key type"))?;
            let val_ty = ty
                .param(1)
                .ok_or_else(|| anyhow::anyhow!("MapProperty missing value type"))?;
            parse_map(r, key_ty, val_ty, ctx, prefer_native, value_end)?
        }
        _ => bail!("unknown property type: {}", ty.name),
    };
    Ok(v)
}

fn has_enum_param(ty: &TypeName) -> bool {
    ty.params
        .first()
        .map(|p| !p.name.is_empty() && p.name != "None")
        .unwrap_or(false)
}

fn parse_collection(
    r: &mut Reader,
    inner: &TypeName,
    ctx: &ParseCtx,
    prefer_native: bool,
    value_end: u64,
) -> Result<Value> {
    let count = r.read_i32()?;
    let remaining_in_value = value_end.saturating_sub(r.pos());
    validate_count(count, remaining_in_value, 1, "collection element")?;
    let mut arr = Vec::with_capacity(count as usize);
    for _ in 0..count {
        arr.push(parse_value(r, inner, ctx, prefer_native, value_end)?);
        ensure_within_value(r, value_end, "collection element")?;
    }
    Ok(Value::Array(arr))
}

/// TSet/TMap delta saves serialize NumToRemove followed by that many key payloads
/// (keys removed relative to the archetype); the loader reads and discards them
/// before the element/pair entries (FSetProperty/FMapProperty::SerializeItem).
fn discard_removed_elements(
    r: &mut Reader,
    key_ty: &TypeName,
    ctx: &ParseCtx,
    prefer_native: bool,
    value_end: u64,
    label: &str,
) -> Result<()> {
    let num_to_remove = r.read_i32()?;
    let remaining = value_end.saturating_sub(r.pos());
    validate_count(num_to_remove, remaining, 1, label)?;
    for _ in 0..num_to_remove {
        let _ = parse_value(r, key_ty, ctx, prefer_native, value_end)?;
        ensure_within_value(r, value_end, label)?;
    }
    Ok(())
}

fn parse_map(
    r: &mut Reader,
    key_ty: &TypeName,
    val_ty: &TypeName,
    ctx: &ParseCtx,
    prefer_native: bool,
    value_end: u64,
) -> Result<Value> {
    discard_removed_elements(r, key_ty, ctx, prefer_native, value_end, "Map removed key")?;
    let count = r.read_i32()?;
    let remaining_in_value = value_end.saturating_sub(r.pos());
    validate_count(count, remaining_in_value, 2, "Map element")?;
    let mut arr = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let key = parse_value(r, key_ty, ctx, prefer_native, value_end)?;
        ensure_within_value(r, value_end, "Map key")?;
        let value = parse_value(r, val_ty, ctx, prefer_native, value_end)?;
        ensure_within_value(r, value_end, "Map value")?;
        arr.push(json!({ "key": key, "value": value }));
    }
    Ok(Value::Array(arr))
}

fn parse_struct(
    r: &mut Reader,
    struct_name: &str,
    ctx: &ParseCtx,
    prefer_native_for_unknown: bool,
    value_end: u64,
) -> Result<Value> {
    if struct_name == "SoftObjectPath" || struct_name == "SoftClassPath" {
        return parse_soft_object(r, ctx);
    }
    if let Some(v) = parse_native_struct(r, struct_name, ctx, value_end)? {
        return Ok(v);
    }
    if prefer_native_for_unknown && !is_tagged_fallback_struct(struct_name) {
        bail!("unknown native struct: {struct_name}");
    }
    let nested = parse_properties(r, ctx, value_end);
    Ok(json!({ "@struct": struct_name, "properties": entries_to_values(&nested) }))
}

/// Structs that declare `WithSerializer` (so their property tag carries the
/// `HasBinaryOrNativeSerialize` flag) but whose `Serialize` returns `false` to
/// only register a custom version — their payload is still tagged properties.
fn parse_soft_object(r: &mut Reader, ctx: &ParseCtx) -> Result<Value> {
    // When the package has a soft object path list, soft references serialize as
    // an int32 index into that list; otherwise the path is serialized inline.
    if !ctx.soft_object_paths.is_empty() {
        let index = r.read_i32()?;
        let index = usize::try_from(index)
            .map_err(|_| anyhow::anyhow!("soft object path index out of range: {index}"))?;
        return ctx
            .soft_object_paths
            .get(index)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("soft object path index out of range: {index}"));
    }
    read_soft_object_path(r, ctx.names)
}

pub(crate) fn read_soft_object_path(r: &mut Reader, names: &NameMap) -> Result<Value> {
    let package_name = names.resolve_raw(r.read_raw_name()?);
    let asset_name = names.resolve_raw(r.read_raw_name()?);
    let sub_path = r.read_fstring()?;
    let asset_path = if asset_name.is_empty() || asset_name == "None" {
        package_name
    } else {
        format!("{package_name}.{asset_name}")
    };
    if sub_path.is_empty() {
        Ok(json!({ "asset_path": asset_path }))
    } else {
        Ok(json!({ "asset_path": asset_path, "sub_path": sub_path }))
    }
}
