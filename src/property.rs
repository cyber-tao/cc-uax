use crate::name::NameMap;
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
            && control & 0x02 != 0 {
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
        if aligned > end_limit.max(r.len()) {
            break;
        }

        let value = if type_name.name == "BoolProperty" {
            json!(bool_val)
        } else {
            match parse_value(r, &type_name, ctx, is_binary_native) {
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
        "TextProperty" => parse_text(r)?,
        "ObjectProperty" | "ClassProperty" | "WeakObjectProperty" | "LazyObjectProperty"
        | "ObjectPtrProperty" | "ClassPtrProperty" | "InterfaceProperty" => {
            let idx = r.read_i32()?;
            (ctx.resolve_object)(idx)
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
            parse_struct(r, struct_name, ctx, prefer_native)?
        }
        "ArrayProperty" => {
            let inner = ty
                .param(0)
                .ok_or_else(|| anyhow::anyhow!("ArrayProperty missing element type"))?;
            parse_collection(r, inner, ctx, false)?
        }
        "SetProperty" => {
            let inner = ty
                .param(0)
                .ok_or_else(|| anyhow::anyhow!("SetProperty missing element type"))?;
            let _num_to_remove = r.read_i32()?;
            parse_collection(r, inner, ctx, true)?
        }
        "MapProperty" => {
            let key_ty = ty
                .param(0)
                .ok_or_else(|| anyhow::anyhow!("MapProperty missing key type"))?;
            let val_ty = ty
                .param(1)
                .ok_or_else(|| anyhow::anyhow!("MapProperty missing value type"))?;
            parse_map(r, key_ty, val_ty, ctx)?
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
) -> Result<Value> {
    let count = r.read_i32()?;
    if count < 0 || count as u64 > r.remaining() {
        bail!("collection element count out of range: {count}");
    }
    let mut arr = Vec::with_capacity(count as usize);
    for _ in 0..count {
        arr.push(parse_element(r, inner, ctx)?);
    }
    Ok(Value::Array(arr))
}

fn parse_map(
    r: &mut Reader,
    key_ty: &TypeName,
    val_ty: &TypeName,
    ctx: &ParseCtx,
) -> Result<Value> {
    let _num_to_remove = r.read_i32()?;
    let count = r.read_i32()?;
    if count < 0 || count as u64 > r.remaining() {
        bail!("Map element count out of range: {count}");
    }
    let mut arr = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let key = parse_element(r, key_ty, ctx)?;
        let value = parse_element(r, val_ty, ctx)?;
        arr.push(json!({ "key": key, "value": value }));
    }
    Ok(Value::Array(arr))
}

fn parse_element(r: &mut Reader, ty: &TypeName, ctx: &ParseCtx) -> Result<Value> {
    parse_value(r, ty, ctx, false)
}

fn parse_struct(
    r: &mut Reader,
    struct_name: &str,
    ctx: &ParseCtx,
    prefer_native_for_unknown: bool,
) -> Result<Value> {
    if struct_name == "SoftObjectPath" || struct_name == "SoftClassPath" {
        return parse_soft_object(r, ctx);
    }
    if let Some(v) = parse_native_struct(r, struct_name)? {
        return Ok(v);
    }
    if prefer_native_for_unknown {
        bail!("unknown native struct: {struct_name}");
    }
    let nested = parse_properties(r, ctx, r.len());
    Ok(json!({ "@struct": struct_name, "properties": entries_to_json(&nested) }))
}

fn parse_native_struct(r: &mut Reader, name: &str) -> Result<Option<Value>> {
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
        _ => return Ok(None),
    };
    Ok(Some(v))
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

fn parse_text(r: &mut Reader) -> Result<Value> {
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
        other => Ok(json!({ "history_type": other, "flags": flags })),
    }
}

fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}
