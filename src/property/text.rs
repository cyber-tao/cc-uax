use super::{PREVIEW_MAX, to_hex, validate_count};
use crate::name::NameMap;
use crate::reader::Reader;
use anyhow::{Result, bail};
use serde_json::{Value, json};

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
            validate_count(count, r.remaining(), 5, "FText named-format argument")?;
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
            validate_count(count, r.remaining(), 1, "FText ordered-format argument")?;
            let mut arguments = Vec::with_capacity(count as usize);
            for _ in 0..count {
                arguments.push(parse_format_argument(r, names, depth + 1)?);
            }
            Ok(json!({
                "history": "OrderedFormat", "format": format, "arguments": arguments, "flags": flags
            }))
        }
        3 => {
            // ArgumentFormat: source format text + TArray<FFormatArgumentData>. Each entry
            // carries its own ArgumentName (unlike Named/OrderedFormat's FFormatArgumentValue).
            let format = parse_text(r, names, depth + 1)?;
            let count = r.read_i32()?;
            validate_count(count, r.remaining(), 5, "FText argument-data")?;
            let mut arguments = Vec::with_capacity(count as usize);
            for _ in 0..count {
                let name = r.read_fstring()?;
                let value = parse_format_argument_data(r, names, depth + 1)?;
                arguments.push(json!({ "name": name, "value": value }));
            }
            Ok(json!({
                "history": "ArgumentFormat", "format": format, "arguments": arguments, "flags": flags
            }))
        }
        4 => parse_number_format_history(r, names, "AsNumber", flags, false, depth),
        5 => parse_number_format_history(r, names, "AsPercent", flags, false, depth),
        6 => parse_number_format_history(r, names, "AsCurrency", flags, true, depth),
        7 => {
            // AsDate: SourceDateTime (int64) + DateStyle (int8) + TimeZone + Culture.
            let datetime = r.read_i64()?;
            let date_style = r.read_i8()?;
            let time_zone = r.read_fstring()?;
            let culture = r.read_fstring()?;
            Ok(json!({
                "history": "AsDate", "datetime": datetime, "date_style": date_style,
                "time_zone": time_zone, "culture": culture, "flags": flags
            }))
        }
        8 => {
            // AsTime: SourceDateTime (int64) + TimeStyle (int8) + TimeZone + Culture.
            let datetime = r.read_i64()?;
            let time_style = r.read_i8()?;
            let time_zone = r.read_fstring()?;
            let culture = r.read_fstring()?;
            Ok(json!({
                "history": "AsTime", "datetime": datetime, "time_style": time_style,
                "time_zone": time_zone, "culture": culture, "flags": flags
            }))
        }
        9 => {
            // AsDateTime: int64 + DateStyle + TimeStyle + [CustomPattern when DateStyle==Custom]
            // + TimeZone + Culture.
            let datetime = r.read_i64()?;
            let date_style = r.read_i8()?;
            let time_style = r.read_i8()?;
            let custom_pattern = if date_style == DATE_TIME_STYLE_CUSTOM {
                Some(r.read_fstring()?)
            } else {
                None
            };
            let time_zone = r.read_fstring()?;
            let culture = r.read_fstring()?;
            let mut o = serde_json::Map::new();
            o.insert("history".into(), json!("AsDateTime"));
            o.insert("datetime".into(), json!(datetime));
            o.insert("date_style".into(), json!(date_style));
            o.insert("time_style".into(), json!(time_style));
            if let Some(p) = custom_pattern {
                o.insert("custom_pattern".into(), json!(p));
            }
            o.insert("time_zone".into(), json!(time_zone));
            o.insert("culture".into(), json!(culture));
            o.insert("flags".into(), json!(flags));
            Ok(Value::Object(o))
        }
        10 => {
            // Transform: nested source text + TransformType (uint8).
            let source = parse_text(r, names, depth + 1)?;
            let transform_type = r.read_u8()?;
            Ok(json!({
                "history": "Transform", "source": source,
                "transform_type": transform_type, "flags": flags
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
        12 => {
            // TextGenerator: GeneratorTypeID (FName), then a TArray<uint8> blob when named.
            let type_id = names.resolve_raw(r.read_raw_name()?);
            let mut o = serde_json::Map::new();
            o.insert("history".into(), json!("TextGenerator"));
            o.insert("generator_type_id".into(), json!(type_id.clone()));
            if type_id != "None" && !type_id.is_empty() {
                let count = r.read_i32()?;
                validate_count(count, r.remaining(), 1, "FText generator contents")?;
                let bytes = r.read_bytes(count as usize)?;
                o.insert("contents_size".into(), json!(count));
                if !bytes.is_empty() {
                    let n = bytes.len().min(PREVIEW_MAX);
                    o.insert("contents".into(), json!(to_hex(&bytes[..n])));
                }
            }
            o.insert("flags".into(), json!(flags));
            Ok(Value::Object(o))
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

/// EDateTimeStyle::Custom — gates the CustomPattern field in the AsDateTime history.
const DATE_TIME_STYLE_CUSTOM: i8 = 5;

/// FTextHistory_FormatNumber::Serialize, shared by AsNumber/AsPercent/AsCurrency:
/// an optional leading CurrencyCode, the SourceValue (FFormatArgumentValue), an
/// optional FNumberFormattingOptions, and the target CultureName.
fn parse_number_format_history(
    r: &mut Reader,
    names: &NameMap,
    kind: &str,
    flags: u32,
    has_currency_code: bool,
    depth: usize,
) -> Result<Value> {
    let currency_code = if has_currency_code {
        Some(r.read_fstring()?)
    } else {
        None
    };
    let source_value = parse_format_argument(r, names, depth + 1)?;
    let format_options = if r.read_bool32()? {
        Some(parse_number_formatting_options(r)?)
    } else {
        None
    };
    let culture = r.read_fstring()?;
    let mut o = serde_json::Map::new();
    o.insert("history".into(), json!(kind));
    if let Some(code) = currency_code {
        o.insert("currency_code".into(), json!(code));
    }
    o.insert("source_value".into(), source_value);
    if let Some(opts) = format_options {
        o.insert("format_options".into(), opts);
    }
    o.insert("culture".into(), json!(culture));
    o.insert("flags".into(), json!(flags));
    Ok(Value::Object(o))
}

/// FNumberFormattingOptions::operator<<. AlwaysSign is version-gated on
/// FEditorObjectVersion, but that threshold predates UE5, so any in-scope (UE5)
/// package always serializes it.
fn parse_number_formatting_options(r: &mut Reader) -> Result<Value> {
    let always_sign = r.read_bool32()?;
    let use_grouping = r.read_bool32()?;
    let rounding_mode = r.read_i8()?;
    let minimum_integral_digits = r.read_i32()?;
    let maximum_integral_digits = r.read_i32()?;
    let minimum_fractional_digits = r.read_i32()?;
    let maximum_fractional_digits = r.read_i32()?;
    Ok(json!({
        "always_sign": always_sign,
        "use_grouping": use_grouping,
        "rounding_mode": rounding_mode,
        "minimum_integral_digits": minimum_integral_digits,
        "maximum_integral_digits": maximum_integral_digits,
        "minimum_fractional_digits": minimum_fractional_digits,
        "maximum_fractional_digits": maximum_fractional_digits,
    }))
}

/// FFormatArgumentData::operator<< — like FFormatArgumentValue but the Int case is
/// 64-bit and there is no UInt variant. The type and 64-bit gates predate UE5.
fn parse_format_argument_data(r: &mut Reader, names: &NameMap, depth: usize) -> Result<Value> {
    let arg_type = r.read_u8()?;
    Ok(match arg_type {
        0 => json!(r.read_i64()?),             // Int (64-bit)
        2 => json!(r.read_f32()? as f64),      // Float
        3 => json!(r.read_f64()?),             // Double
        4 => parse_text(r, names, depth + 1)?, // Text
        5 => json!(r.read_u8()?),              // Gender
        _ => Value::Null,
    })
}
