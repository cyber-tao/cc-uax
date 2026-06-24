use crate::name::NameMap;
use crate::pin::PinSerCtx;
use crate::reader::Reader;
use anyhow::{Result, bail};
use serde_json::{Value, json};

const PREVIEW_MAX: usize = 64;

#[derive(Debug, Clone)]
pub struct TypeName {
    pub name: String,
    pub params: Vec<TypeName>,
}

impl TypeName {
    pub fn parse(r: &mut Reader, names: &NameMap) -> Result<Self> {
        let mut flat: Vec<(String, i32)> = Vec::new();
        let mut remaining: i64 = 1;
        let mut guard = 0usize;
        while remaining > 0 {
            let name = names.resolve_raw(r.read_raw_name()?);
            let inner = r.read_i32()?;
            if !(0..=4096).contains(&inner) {
                bail!("type name inner parameter count out of range: {inner}");
            }
            flat.push((name, inner));
            remaining += inner as i64 - 1;
            guard += 1;
            if guard > 8192 {
                bail!("too many type name nodes, data may be corrupt");
            }
        }
        let mut pos = 0usize;
        Ok(Self::build(&flat, &mut pos))
    }

    fn build(flat: &[(String, i32)], pos: &mut usize) -> TypeName {
        if *pos >= flat.len() {
            return TypeName {
                name: String::new(),
                params: Vec::new(),
            };
        }
        let (name, inner) = flat[*pos].clone();
        *pos += 1;
        let mut params = Vec::new();
        for _ in 0..inner {
            if *pos >= flat.len() {
                break;
            }
            params.push(Self::build(flat, pos));
        }
        TypeName { name, params }
    }

    pub fn display(&self) -> String {
        if self.params.is_empty() {
            self.name.clone()
        } else {
            let inner: Vec<String> = self.params.iter().map(TypeName::display).collect();
            format!("{}({})", self.name, inner.join(","))
        }
    }

    fn param(&self, i: usize) -> Option<&TypeName> {
        self.params.get(i)
    }
}

pub struct ParseCtx<'a> {
    pub names: &'a NameMap,
    pub resolve_object: &'a dyn Fn(i32) -> Value,
    pub pins: PinSerCtx,
}

#[derive(Debug, Clone)]
pub struct PropertyEntry {
    pub name: String,
    pub type_str: String,
    pub array_index: i32,
    pub value: Value,
    pub guid: Option<String>,
}

pub fn entries_to_json(props: &[PropertyEntry]) -> Value {
    let arr: Vec<Value> = props
        .iter()
        .map(|e| {
            let mut o = serde_json::Map::new();
            o.insert("name".into(), json!(e.name));
            o.insert("type".into(), json!(e.type_str));
            if e.array_index != 0 {
                o.insert("array_index".into(), json!(e.array_index));
            }
            o.insert("value".into(), e.value.clone());
            if let Some(g) = &e.guid {
                o.insert("guid".into(), json!(g));
            }
            Value::Object(o)
        })
        .collect();
    Value::Array(arr)
}

pub fn parse_object_properties(
    r: &mut Reader,
    ctx: &ParseCtx,
    end_limit: u64,
    ue5_version: i32,
) -> Vec<PropertyEntry> {
    if ue5_version >= crate::version::ue5::PROPERTY_TAG_EXTENSION_AND_OVERRIDABLE_SERIALIZATION
        && let Ok(control) = r.read_u8()
        && control & 0x02 != 0
    {
        let _ = r.read_u8();
    }
    parse_properties(r, ctx, end_limit)
}

pub fn parse_properties(r: &mut Reader, ctx: &ParseCtx, end_limit: u64) -> Vec<PropertyEntry> {
    let mut entries = Vec::new();
    let mut guard = 0usize;
    loop {
        guard += 1;
        if guard > 1_000_000 {
            break;
        }
        if r.pos() + 8 > end_limit {
            break;
        }
        let name_raw = match r.read_raw_name() {
            Ok(n) => n,
            Err(_) => break,
        };
        let name = ctx.names.resolve_raw(name_raw);
        if name == "None" || name.is_empty() {
            break;
        }
        let type_name = match TypeName::parse(r, ctx.names) {
            Ok(t) => t,
            Err(_) => break,
        };
        let size = match r.read_i32() {
            Ok(s) => s,
            Err(_) => break,
        };
        let flags = match r.read_u8() {
            Ok(f) => f,
            Err(_) => break,
        };
        let array_index = if flags & 0x01 != 0 {
            r.read_i32().unwrap_or(0)
        } else {
            0
        };
        let guid = if flags & 0x02 != 0 {
            r.read_guid().ok().map(|g| g.to_hex())
        } else {
            None
        };
        if flags & 0x04 != 0 && parse_extensions(r).is_err() {
            break;
        }
        let is_binary_native = flags & 0x08 != 0;
        let bool_val = flags & 0x10 != 0;

        if size < 0 {
            break;
        }
        let value_start = r.pos();
        let aligned = value_start.saturating_add(size as u64);
        if aligned > end_limit {
            break;
        }

        let value = if type_name.name == "BoolProperty" {
            json!(bool_val)
        } else {
            match parse_value(r, &type_name, ctx, is_binary_native, aligned) {
                Ok(v) => v,
                Err(_) => {
                    let _ = r.seek(value_start);
                    let n = (size as usize).min(PREVIEW_MAX);
                    let preview = r.read_bytes(n).unwrap_or_default();
                    json!({ "@unparsed": to_hex(&preview), "size": size })
                }
            }
        };

        if r.seek(aligned).is_err() {
            entries.push(PropertyEntry {
                name,
                type_str: type_name.display(),
                array_index,
                value,
                guid,
            });
            break;
        }

        entries.push(PropertyEntry {
            name,
            type_str: type_name.display(),
            array_index,
            value,
            guid,
        });
    }
    entries
}

fn parse_extensions(r: &mut Reader) -> Result<()> {
    let ext = r.read_u8()?;
    if ext & 0x02 != 0 {
        let _override_operation = r.read_u8()?;
        let _experimental = r.read_u8()?;
    }
    Ok(())
}

fn parse_value(
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
        "ObjectProperty" | "ClassProperty" | "WeakObjectProperty" | "LazyObjectProperty"
        | "ObjectPtrProperty" | "ClassPtrProperty" | "InterfaceProperty" => {
            let idx = r.read_i32()?;
            (ctx.resolve_object)(idx)
        }
        "DelegateProperty" => {
            let object = r.read_i32()?;
            let function = ctx.names.resolve_raw(r.read_raw_name()?);
            json!({ "object": (ctx.resolve_object)(object), "function": function })
        }
        "MulticastInlineDelegateProperty" | "MulticastSparseDelegateProperty" => {
            let count = r.read_i32()?;
            let remaining = value_end.saturating_sub(r.pos());
            if count < 0 || (count as u64).saturating_mul(12) > remaining {
                bail!("delegate invocation count out of range: {count}");
            }
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
        "StructProperty" => {
            let struct_name = ty.param(0).map(|p| p.name.as_str()).unwrap_or("");
            parse_struct(r, struct_name, ctx, prefer_native, value_end)?
        }
        "ArrayProperty" => {
            let inner = ty
                .param(0)
                .ok_or_else(|| anyhow::anyhow!("ArrayProperty missing element type"))?;
            parse_collection(r, inner, ctx, false, prefer_native, value_end)?
        }
        "SetProperty" => {
            let inner = ty
                .param(0)
                .ok_or_else(|| anyhow::anyhow!("SetProperty missing element type"))?;
            let _num_to_remove = r.read_i32()?;
            parse_collection(r, inner, ctx, true, prefer_native, value_end)?
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
    _is_set: bool,
    prefer_native: bool,
    value_end: u64,
) -> Result<Value> {
    let count = r.read_i32()?;
    let remaining_in_value = value_end.saturating_sub(r.pos());
    if count < 0 || count as u64 > remaining_in_value {
        bail!("collection element count out of range: {count}");
    }
    let mut arr = Vec::with_capacity(count as usize);
    for _ in 0..count {
        arr.push(parse_element(r, inner, ctx, prefer_native, value_end)?);
    }
    Ok(Value::Array(arr))
}

fn parse_map(
    r: &mut Reader,
    key_ty: &TypeName,
    val_ty: &TypeName,
    ctx: &ParseCtx,
    prefer_native: bool,
    value_end: u64,
) -> Result<Value> {
    let _num_to_remove = r.read_i32()?;
    let count = r.read_i32()?;
    let remaining_in_value = value_end.saturating_sub(r.pos());
    if count < 0 || count as u64 > remaining_in_value {
        bail!("Map element count out of range: {count}");
    }
    let mut arr = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let key = parse_element(r, key_ty, ctx, prefer_native, value_end)?;
        let value = parse_element(r, val_ty, ctx, prefer_native, value_end)?;
        arr.push(json!({ "key": key, "value": value }));
    }
    Ok(Value::Array(arr))
}

fn parse_element(
    r: &mut Reader,
    ty: &TypeName,
    ctx: &ParseCtx,
    prefer_native: bool,
    value_end: u64,
) -> Result<Value> {
    parse_value(r, ty, ctx, prefer_native, value_end)
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
    if prefer_native_for_unknown {
        bail!("unknown native struct: {struct_name}");
    }
    let nested = parse_properties(r, ctx, value_end);
    Ok(json!({ "@struct": struct_name, "properties": entries_to_json(&nested) }))
}

fn parse_native_struct(
    r: &mut Reader,
    name: &str,
    ctx: &ParseCtx,
    value_end: u64,
) -> Result<Option<Value>> {
    let v = match name {
        "Vector"
        | "Vector_NetQuantize"
        | "Vector_NetQuantize10"
        | "Vector_NetQuantize100"
        | "Vector_NetQuantizeNormal" => {
            json!({ "x": r.read_f64()?, "y": r.read_f64()?, "z": r.read_f64()? })
        }
        "Vector3f" => json!({ "x": r.read_f32()?, "y": r.read_f32()?, "z": r.read_f32()? }),
        "Vector2D" => json!({ "x": r.read_f64()?, "y": r.read_f64()? }),
        "Vector2f" => json!({ "x": r.read_f32()?, "y": r.read_f32()? }),
        "Vector4" => json!({
            "x": r.read_f64()?, "y": r.read_f64()?, "z": r.read_f64()?, "w": r.read_f64()?
        }),
        "Rotator" => json!({
            "pitch": r.read_f64()?, "yaw": r.read_f64()?, "roll": r.read_f64()?
        }),
        "Quat" => json!({
            "x": r.read_f64()?, "y": r.read_f64()?, "z": r.read_f64()?, "w": r.read_f64()?
        }),
        "IntPoint" => json!({ "x": r.read_i32()?, "y": r.read_i32()? }),
        "IntVector" => json!({ "x": r.read_i32()?, "y": r.read_i32()?, "z": r.read_i32()? }),
        "Guid" => json!(r.read_guid()?.to_hex()),
        "Color" => json!({
            "b": r.read_u8()?, "g": r.read_u8()?, "r": r.read_u8()?, "a": r.read_u8()?
        }),
        "LinearColor" => json!({
            "r": r.read_f32()?, "g": r.read_f32()?, "b": r.read_f32()?, "a": r.read_f32()?
        }),
        "DateTime" | "Timespan" => json!(r.read_i64()?),
        "Transform" => {
            let rot = json!({
                "x": r.read_f64()?, "y": r.read_f64()?, "z": r.read_f64()?, "w": r.read_f64()?
            });
            let trans = json!({ "x": r.read_f64()?, "y": r.read_f64()?, "z": r.read_f64()? });
            let scale = json!({ "x": r.read_f64()?, "y": r.read_f64()?, "z": r.read_f64()? });
            json!({ "rotation": rot, "translation": trans, "scale3d": scale })
        }
        "Transform3f" => {
            let rot = json!({
                "x": r.read_f32()?, "y": r.read_f32()?, "z": r.read_f32()?, "w": r.read_f32()?
            });
            let trans = json!({ "x": r.read_f32()?, "y": r.read_f32()?, "z": r.read_f32()? });
            let scale = json!({ "x": r.read_f32()?, "y": r.read_f32()?, "z": r.read_f32()? });
            json!({ "rotation": rot, "translation": trans, "scale3d": scale })
        }
        "Box" => {
            let min = json!({ "x": r.read_f64()?, "y": r.read_f64()?, "z": r.read_f64()? });
            let max = json!({ "x": r.read_f64()?, "y": r.read_f64()?, "z": r.read_f64()? });
            let is_valid = r.read_u8()? != 0;
            json!({ "min": min, "max": max, "is_valid": is_valid })
        }
        "Box2D" => {
            let min = json!({ "x": r.read_f64()?, "y": r.read_f64()? });
            let max = json!({ "x": r.read_f64()?, "y": r.read_f64()? });
            let is_valid = r.read_bool32()?;
            json!({ "min": min, "max": max, "is_valid": is_valid })
        }
        "FrameNumber" => json!({ "value": r.read_i32()? }),
        "FrameRate" => json!({ "numerator": r.read_i32()?, "denominator": r.read_i32()? }),
        "IntVector2" => json!({ "x": r.read_i32()?, "y": r.read_i32()? }),
        "IntVector4" => json!({
            "x": r.read_i32()?, "y": r.read_i32()?, "z": r.read_i32()?, "w": r.read_i32()?
        }),
        "DeprecateSlateVector2D" => json!({ "x": r.read_f32()?, "y": r.read_f32()? }),
        "RichCurveKey" => {
            let interp_mode = r.read_u8()?;
            let tangent_mode = r.read_u8()?;
            let tangent_weight_mode = r.read_u8()?;
            json!({
                "interp_mode": interp_mode,
                "tangent_mode": tangent_mode,
                "tangent_weight_mode": tangent_weight_mode,
                "time": r.read_f32()? as f64,
                "value": r.read_f32()? as f64,
                "arrive_tangent": r.read_f32()? as f64,
                "arrive_tangent_weight": r.read_f32()? as f64,
                "leave_tangent": r.read_f32()? as f64,
                "leave_tangent_weight": r.read_f32()? as f64,
            })
        }
        "PerPlatformFloat" => parse_per_platform(r, ctx.names, ScalarKind::F32, value_end)?,
        "PerPlatformInt" => parse_per_platform(r, ctx.names, ScalarKind::I32, value_end)?,
        "PerPlatformBool" => parse_per_platform(r, ctx.names, ScalarKind::Bool32, value_end)?,
        "PerPlatformFrameRate" => {
            parse_per_platform(r, ctx.names, ScalarKind::FrameRate, value_end)?
        }
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
            o.insert(
                "constant".into(),
                json!({
                    "r": r.read_f32()?, "g": r.read_f32()?, "b": r.read_f32()?, "a": r.read_f32()?
                }),
            );
            Value::Object(o)
        }
        "ShadingModelMaterialInput" | "SubstrateMaterialInput" => {
            let mut o = parse_expression_input(r, ctx)?;
            o.insert("use_constant".into(), json!(r.read_bool32()?));
            o.insert("constant".into(), json!(r.read_u32()?));
            Value::Object(o)
        }
        "MovieSceneFrameRange" => {
            let lower_type = r.read_u8()?;
            let lower = r.read_i32()?;
            let upper_type = r.read_u8()?;
            let upper = r.read_i32()?;
            json!({
                "lower_bound_type": lower_type,
                "lower_bound": lower,
                "upper_bound_type": upper_type,
                "upper_bound": upper,
            })
        }
        "MovieSceneFloatChannel" => parse_movie_scene_channel(r, false, value_end)?,
        "MovieSceneDoubleChannel" => parse_movie_scene_channel(r, true, value_end)?,
        "PerQualityLevelInt" => parse_per_quality_level(r, ScalarKind::I32, value_end)?,
        "PerQualityLevelFloat" => parse_per_quality_level(r, ScalarKind::F32, value_end)?,
        "EdGraphPinType" => {
            let (category, sub_category, sub_category_object) =
                crate::pin::parse_pin_type(r, ctx, &ctx.pins)?;
            json!({
                "category": category,
                "sub_category": sub_category,
                "sub_category_object": (ctx.resolve_object)(sub_category_object),
            })
        }
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
        _ => return Ok(None),
    };
    Ok(Some(v))
}

#[derive(Clone, Copy)]
enum ScalarKind {
    F32,
    I32,
    Bool32,
    FrameRate,
}

fn read_scalar(r: &mut Reader, kind: ScalarKind) -> Result<Value> {
    Ok(match kind {
        ScalarKind::F32 => json!(r.read_f32()? as f64),
        ScalarKind::I32 => json!(r.read_i32()?),
        ScalarKind::Bool32 => json!(r.read_bool32()?),
        ScalarKind::FrameRate => {
            json!({ "numerator": r.read_i32()?, "denominator": r.read_i32()? })
        }
    })
}

fn parse_per_platform(
    r: &mut Reader,
    names: &NameMap,
    kind: ScalarKind,
    value_end: u64,
) -> Result<Value> {
    let cooked = r.read_bool32()?;
    let default = read_scalar(r, kind)?;
    let mut per_platform = Vec::new();
    if !cooked {
        let count = r.read_i32()?;
        let remaining = value_end.saturating_sub(r.pos());
        if count < 0 || count as u64 > remaining {
            bail!("PerPlatform map count out of range: {count}");
        }
        for _ in 0..count {
            let key = names.resolve_raw(r.read_raw_name()?);
            let value = read_scalar(r, kind)?;
            per_platform.push(json!({ "platform": key, "value": value }));
        }
    }
    Ok(json!({ "default": default, "per_platform": per_platform }))
}

fn parse_per_quality_level(r: &mut Reader, kind: ScalarKind, value_end: u64) -> Result<Value> {
    let cooked = r.read_bool32()?;
    let default = read_scalar(r, kind)?;
    let mut per_quality = Vec::new();
    if !cooked {
        let count = r.read_i32()?;
        let remaining = value_end.saturating_sub(r.pos());
        if count < 0 || count as u64 > remaining {
            bail!("PerQualityLevel map count out of range: {count}");
        }
        for _ in 0..count {
            let quality_level = r.read_i32()?;
            let value = read_scalar(r, kind)?;
            per_quality.push(json!({ "quality_level": quality_level, "value": value }));
        }
    }
    Ok(json!({ "default": default, "per_quality": per_quality }))
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

fn parse_movie_scene_channel(r: &mut Reader, is_double: bool, value_end: u64) -> Result<Value> {
    let pre_infinity = r.read_u8()?;
    let post_infinity = r.read_u8()?;

    // Times: serialized element size (sizeof FFrameNumber == 4), count, then raw int32 data.
    let times_elem_size = r.read_i32()?;
    if times_elem_size != 4 {
        bail!("unexpected MovieScene time element size: {times_elem_size}");
    }
    let times_count = r.read_i32()?;
    let remaining = value_end.saturating_sub(r.pos());
    if times_count < 0 || (times_count as u64).saturating_mul(4) > remaining {
        bail!("MovieScene times count out of range: {times_count}");
    }
    let mut times = Vec::with_capacity(times_count as usize);
    for _ in 0..times_count {
        times.push(json!(r.read_i32()?));
    }

    // Values: serialized element size (POD struct dumped with padding), count, raw data.
    let value_size: u64 = if is_double { 8 } else { 4 };
    let val_elem_size = r.read_i32()?;
    if (val_elem_size as i64) < (value_size + 22) as i64 || val_elem_size > 64 {
        bail!("unexpected MovieScene value element size: {val_elem_size}");
    }
    let val_count = r.read_i32()?;
    let remaining = value_end.saturating_sub(r.pos());
    if val_count < 0 || (val_count as u64).saturating_mul(val_elem_size as u64) > remaining {
        bail!("MovieScene values count out of range: {val_count}");
    }
    let mut values = Vec::with_capacity(val_count as usize);
    for _ in 0..val_count {
        let elem_start = r.pos();
        let value = if is_double {
            json!(r.read_f64()?)
        } else {
            json!(r.read_f32()? as f64)
        };
        let arrive_tangent = r.read_f32()? as f64;
        let leave_tangent = r.read_f32()? as f64;
        let arrive_tangent_weight = r.read_f32()? as f64;
        let leave_tangent_weight = r.read_f32()? as f64;
        let tangent_weight_mode = r.read_u8()?;
        // InterpMode/TangentMode follow the 20-byte tangent block (past its padding).
        r.seek(elem_start + value_size + 20)?;
        let interp_mode = r.read_u8()?;
        let tangent_mode = r.read_u8()?;
        r.seek(elem_start + val_elem_size as u64)?;
        values.push(json!({
            "value": value,
            "interp_mode": interp_mode,
            "tangent_mode": tangent_mode,
            "tangent_weight_mode": tangent_weight_mode,
            "arrive_tangent": arrive_tangent,
            "leave_tangent": leave_tangent,
            "arrive_tangent_weight": arrive_tangent_weight,
            "leave_tangent_weight": leave_tangent_weight,
        }));
    }

    let default_value = if is_double {
        json!(r.read_f64()?)
    } else {
        json!(r.read_f32()? as f64)
    };
    let has_default_value = r.read_bool32()?;
    let tick_numerator = r.read_i32()?;
    let tick_denominator = r.read_i32()?;
    let mut out = serde_json::Map::new();
    out.insert("pre_infinity_extrap".into(), json!(pre_infinity));
    out.insert("post_infinity_extrap".into(), json!(post_infinity));
    out.insert("times".into(), Value::Array(times));
    out.insert("values".into(), Value::Array(values));
    out.insert("default_value".into(), default_value);
    out.insert("has_default_value".into(), json!(has_default_value));
    out.insert(
        "tick_resolution".into(),
        json!({ "numerator": tick_numerator, "denominator": tick_denominator }),
    );
    // bShowCurve is serialized for current engine versions; consume only if present.
    if r.pos() + 4 <= value_end {
        out.insert("show_curve".into(), json!(r.read_bool32()?));
    }
    Ok(Value::Object(out))
}

fn parse_soft_object(r: &mut Reader, ctx: &ParseCtx) -> Result<Value> {
    let package_name = ctx.names.resolve_raw(r.read_raw_name()?);
    let asset_name = ctx.names.resolve_raw(r.read_raw_name()?);
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

pub(crate) fn parse_text(r: &mut Reader, names: &NameMap, depth: usize) -> Result<Value> {
    if depth > 32 {
        bail!("FText nesting too deep");
    }
    let flags = r.read_u32()?;
    let history_type = r.read_i8()?;
    match history_type {
        -1 => {
            let has_culture_invariant = r.read_i32()? != 0;
            if has_culture_invariant {
                let s = r.read_fstring()?;
                Ok(json!({ "text": s, "flags": flags }))
            } else {
                Ok(json!({ "text": Value::Null, "flags": flags }))
            }
        }
        0 => {
            let namespace = r.read_fstring()?;
            let key = r.read_fstring()?;
            let source = r.read_fstring()?;
            Ok(json!({
                "text": source, "namespace": namespace, "key": key, "flags": flags
            }))
        }
        1 => {
            // NamedFormat: source format text + TMap<FString, FFormatArgumentValue>.
            let format = parse_text(r, names, depth + 1)?;
            let count = r.read_i32()?;
            if count < 0 || count as u64 > r.remaining() {
                bail!("FText named-format argument count out of range: {count}");
            }
            let mut arguments = Vec::with_capacity(count as usize);
            for _ in 0..count {
                let name = r.read_fstring()?;
                let value = parse_format_argument(r, names, depth + 1)?;
                arguments.push(json!({ "name": name, "value": value }));
            }
            Ok(json!({
                "history": "NamedFormat", "format": format, "arguments": arguments, "flags": flags
            }))
        }
        2 => {
            // OrderedFormat: source format text + TArray<FFormatArgumentValue>.
            let format = parse_text(r, names, depth + 1)?;
            let count = r.read_i32()?;
            if count < 0 || count as u64 > r.remaining() {
                bail!("FText ordered-format argument count out of range: {count}");
            }
            let mut arguments = Vec::with_capacity(count as usize);
            for _ in 0..count {
                arguments.push(parse_format_argument(r, names, depth + 1)?);
            }
            Ok(json!({
                "history": "OrderedFormat", "format": format, "arguments": arguments, "flags": flags
            }))
        }
        11 => {
            // StringTableEntry: TableId (FName) + Key (FString).
            let table_id = names.resolve_raw(r.read_raw_name()?);
            let key = r.read_fstring()?;
            Ok(json!({
                "history": "StringTableEntry", "table_id": table_id, "key": key, "flags": flags
            }))
        }
        other => bail!("unsupported FText history type: {other}"),
    }
}

fn parse_format_argument(r: &mut Reader, names: &NameMap, depth: usize) -> Result<Value> {
    let arg_type = r.read_i8()?;
    Ok(match arg_type {
        0 => json!(r.read_i64()?),             // Int
        1 | 5 => json!(r.read_u64()?),         // UInt / Gender
        2 => json!(r.read_f32()? as f64),      // Float
        3 => json!(r.read_f64()?),             // Double
        4 => parse_text(r, names, depth + 1)?, // Text
        other => bail!("unknown FText format argument type: {other}"),
    })
}

fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}
