use crate::name::NameMap;
use crate::property::{
    PREVIEW_MAX, ParseCtx, ensure_within_value, entries_to_values, parse_properties, to_hex,
    validate_count,
};
use crate::reader::Reader;
use crate::structured_value::{Map, Value, json};
use anyhow::Result;

// Niagara core variable/type structs (modern registry format only).
pub(super) fn parse_niagara_struct(
    r: &mut Reader,
    name: &str,
    ctx: &ParseCtx,
    value_end: u64,
) -> Result<Option<Value>> {
    let v = match name {
        "NiagaraDataInterfaceGPUParamInfo" => parse_niagara_gpu_param_info(r, ctx, value_end)?,
        // Niagara core variable types (modern format only). FNiagaraTypeDefinition
        // serializes via SerializeTaggedProperties, so it reuses parse_properties.
        "NiagaraTypeDefinition" if niagara_modern(ctx) => {
            let nested = parse_properties(r, ctx, value_end);
            json!({ "@struct": "NiagaraTypeDefinition", "properties": entries_to_values(&nested) })
        }
        "NiagaraVariableBase" if niagara_modern(ctx) => {
            Value::Object(parse_niagara_variable_base(r, ctx, value_end)?)
        }
        "NiagaraVariable" if niagara_modern(ctx) => {
            let mut o = parse_niagara_variable_base(r, ctx, value_end)?;
            // VarData: TArray<uint8> (the variable's default-value blob).
            let count = r.read_i32()?;
            let remaining = value_end.saturating_sub(r.pos());
            validate_count(count, remaining, 1, "NiagaraVariable data")?;
            let bytes = r.read_bytes(count as usize)?;
            o.insert("data_size".into(), json!(count));
            if !bytes.is_empty() {
                let n = bytes.len().min(PREVIEW_MAX);
                o.insert("data".into(), json!(to_hex(&bytes[..n])));
            }
            Value::Object(o)
        }
        "NiagaraVariableWithOffset" if niagara_modern(ctx) => {
            let mut o = parse_niagara_variable_base(r, ctx, value_end)?;
            o.insert("offset".into(), json!(r.read_i32()?));
            Value::Object(o)
        }
        _ => return Ok(None),
    };
    Ok(Some(v))
}

fn niagara_modern(ctx: &ParseCtx) -> bool {
    ctx.serialization.niagara_version
        >= crate::version::custom::NIAGARA_VARIABLES_USE_TYPE_DEF_REGISTRY
}

fn read_name(r: &mut Reader, names: &NameMap) -> Result<String> {
    Ok(names.resolve_raw(r.read_raw_name()?))
}

fn parse_niagara_gpu_param_info(r: &mut Reader, ctx: &ParseCtx, value_end: u64) -> Result<Value> {
    let mut o = Map::new();
    o.insert(
        "data_interface_hlsl_symbol".into(),
        json!(r.read_fstring()?),
    );
    o.insert("di_class_name".into(), json!(r.read_fstring()?));
    if ctx.serialization.niagara_version
        >= crate::version::custom::NIAGARA_ADD_GENERATED_FUNCTIONS_TO_GPU_PARAM_INFO
    {
        o.insert(
            "generated_functions".into(),
            parse_niagara_generated_functions(r, ctx, value_end)?,
        );
    }
    Ok(Value::Object(o))
}

fn parse_niagara_generated_functions(
    r: &mut Reader,
    ctx: &ParseCtx,
    value_end: u64,
) -> Result<Value> {
    let count = r.read_i32()?;
    let remaining = value_end.saturating_sub(r.pos());
    validate_count(count, remaining, 16, "Niagara generated function")?;
    let mut functions = Vec::with_capacity(count as usize);
    for _ in 0..count {
        functions.push(parse_niagara_generated_function(r, ctx, value_end)?);
        ensure_within_value(r, value_end, "Niagara generated function")?;
    }
    Ok(Value::Array(functions))
}

fn parse_niagara_generated_function(
    r: &mut Reader,
    ctx: &ParseCtx,
    value_end: u64,
) -> Result<Value> {
    let mut o = Map::new();
    o.insert("definition_name".into(), json!(read_name(r, ctx.names)?));
    o.insert("instance_name".into(), json!(r.read_fstring()?));
    o.insert(
        "specifiers".into(),
        parse_name_pair_array(r, ctx.names, value_end, "Niagara function specifier")?,
    );
    if ctx.serialization.niagara_version
        >= crate::version::custom::NIAGARA_ADD_VARIADIC_PARAMETERS_TO_GPU_FUNCTION_INFO
    {
        o.insert(
            "variadic_inputs".into(),
            parse_niagara_variable_references(r, ctx, value_end, "Niagara variadic input")?,
        );
        o.insert(
            "variadic_outputs".into(),
            parse_niagara_variable_references(r, ctx, value_end, "Niagara variadic output")?,
        );
    }
    if ctx.serialization.niagara_version
        >= crate::version::custom::NIAGARA_SERIALIZE_USAGE_BITMASK_TO_GPU_FUNCTION_INFO
    {
        o.insert("misc_usage_bitmask".into(), json!(r.read_u16()?));
    }
    Ok(Value::Object(o))
}

fn parse_name_pair_array(
    r: &mut Reader,
    names: &NameMap,
    value_end: u64,
    label: &str,
) -> Result<Value> {
    let count = r.read_i32()?;
    let remaining = value_end.saturating_sub(r.pos());
    validate_count(count, remaining, 16, label)?;
    let mut pairs = Vec::with_capacity(count as usize);
    for _ in 0..count {
        pairs.push(json!({
            "key": read_name(r, names)?,
            "value": read_name(r, names)?
        }));
        ensure_within_value(r, value_end, label)?;
    }
    Ok(Value::Array(pairs))
}

fn parse_niagara_variable_references(
    r: &mut Reader,
    ctx: &ParseCtx,
    value_end: u64,
    label: &str,
) -> Result<Value> {
    let count = r.read_i32()?;
    let remaining = value_end.saturating_sub(r.pos());
    validate_count(count, remaining, 12, label)?;
    let mut refs = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let name = read_name(r, ctx.names)?;
        let underlying_type = r.read_i32()?;
        refs.push(json!({
            "name": name,
            "underlying_type": (ctx.resolve_object)(underlying_type)
        }));
        ensure_within_value(r, value_end, label)?;
    }
    Ok(Value::Array(refs))
}

/// FNiagaraVariableBase::Serialize (modern): `Ar << Name; Ar << TypeDefHandle;`
/// where TypeDefHandle serializes a full FNiagaraTypeDefinition via tagged
/// properties. Leaves the reader positioned right after the type definition.
fn parse_niagara_variable_base(r: &mut Reader, ctx: &ParseCtx, value_end: u64) -> Result<Map> {
    let name = ctx.names.resolve_raw(r.read_raw_name()?);
    let type_def = parse_properties(r, ctx, value_end);
    let mut o = Map::new();
    o.insert("name".into(), json!(name));
    o.insert(
        "type".into(),
        json!({ "@struct": "NiagaraTypeDefinition", "properties": entries_to_values(&type_def) }),
    );
    Ok(o)
}
