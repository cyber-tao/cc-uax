use super::{PREVIEW_MAX, ParseCtx, to_hex, validate_count};
use crate::reader::Reader;
use crate::structured_value::{Map, Value, json};
use crate::version::custom;
use anyhow::{Result, bail};

/// Decode an `FText`. Every read is bounded by `end`, the declared end of the
/// value this text lives in: an `FText` history is a chain of length-prefixed
/// strings and counted arrays, so a corrupt prefix would otherwise size a read
/// against the rest of the file rather than the property.
pub(crate) fn parse_text(r: &mut Reader, ctx: &ParseCtx, end: u64, depth: usize) -> Result<Value> {
    if depth > 32 {
        bail!("FText nesting too deep");
    }
    let flags = r.read_u32_within(end, "FText flags")?;
    let history_type = r.read_i8_within(end, "FText history type")?;
    match history_type {
        -1 => {
            let has_culture_invariant =
                r.read_i32_within(end, "FText culture invariant flag")? != 0;
            if has_culture_invariant {
                let s = r.read_fstring_within(end, "FText culture invariant string")?;
                Ok(json!({ "text": s, "flags": flags }))
            } else {
                Ok(json!({ "text": Value::Null, "flags": flags }))
            }
        }
        0 => {
            let namespace = r.read_fstring_within(end, "FText namespace")?;
            let key = r.read_fstring_within(end, "FText key")?;
            let source = r.read_fstring_within(end, "FText source")?;
            // UE5.8 editor Base history appends DevNotes when FortniteMain
            // >= AddDevNotesToFText and the archive is not FilterEditorOnly.
            if ctx.serialization.fortnite_main_version >= custom::ADD_DEV_NOTES_TO_FTEXT
                && !ctx.pins.filter_editor_only
            {
                let _dev_notes = r.read_fstring_within(end, "FText dev notes")?;
            }
            Ok(json!({
                "text": source, "namespace": namespace, "key": key, "flags": flags
            }))
        }
        1 => {
            // NamedFormat: source format text + TMap<FString, FFormatArgumentValue>.
            let format = parse_text(r, ctx, end, depth + 1)?;
            let count = r.read_i32_within(end, "FText named-format argument count")?;
            validate_count(
                count,
                end.saturating_sub(r.pos()),
                5,
                "FText named-format argument",
            )?;
            let mut arguments = Vec::with_capacity(count as usize);
            for _ in 0..count {
                let name = r.read_fstring_within(end, "FText argument name")?;
                let value = parse_format_argument(r, ctx, end, depth + 1)?;
                arguments.push(json!({ "name": name, "value": value }));
            }
            Ok(json!({
                "history": "NamedFormat", "format": format, "arguments": arguments, "flags": flags
            }))
        }
        2 => {
            // OrderedFormat: source format text + TArray<FFormatArgumentValue>.
            let format = parse_text(r, ctx, end, depth + 1)?;
            let count = r.read_i32_within(end, "FText ordered-format argument count")?;
            validate_count(
                count,
                end.saturating_sub(r.pos()),
                1,
                "FText ordered-format argument",
            )?;
            let mut arguments = Vec::with_capacity(count as usize);
            for _ in 0..count {
                arguments.push(parse_format_argument(r, ctx, end, depth + 1)?);
            }
            Ok(json!({
                "history": "OrderedFormat", "format": format, "arguments": arguments, "flags": flags
            }))
        }
        3 => {
            // ArgumentFormat: source format text + TArray<FFormatArgumentData>. Each entry
            // carries its own ArgumentName (unlike Named/OrderedFormat's FFormatArgumentValue).
            let format = parse_text(r, ctx, end, depth + 1)?;
            let count = r.read_i32_within(end, "FText argument-data count")?;
            validate_count(count, end.saturating_sub(r.pos()), 5, "FText argument-data")?;
            let mut arguments = Vec::with_capacity(count as usize);
            for _ in 0..count {
                let name = r.read_fstring_within(end, "FText argument name")?;
                let value = parse_format_argument_data(r, ctx, end, depth + 1)?;
                arguments.push(json!({ "name": name, "value": value }));
            }
            Ok(json!({
                "history": "ArgumentFormat", "format": format, "arguments": arguments, "flags": flags
            }))
        }
        4 => parse_number_format_history(r, ctx, end, "AsNumber", flags, false, depth),
        5 => parse_number_format_history(r, ctx, end, "AsPercent", flags, false, depth),
        6 => parse_number_format_history(r, ctx, end, "AsCurrency", flags, true, depth),
        7 => {
            // AsDate: SourceDateTime (int64) + DateStyle (int8) + TimeZone + Culture.
            let datetime = r.read_i64_within(end, "FText date value")?;
            let date_style = r.read_i8_within(end, "FText date style")?;
            let time_zone = r.read_fstring_within(end, "FText time zone")?;
            let culture = r.read_fstring_within(end, "FText culture")?;
            Ok(json!({
                "history": "AsDate", "datetime": datetime, "date_style": date_style,
                "time_zone": time_zone, "culture": culture, "flags": flags
            }))
        }
        8 => {
            // AsTime: SourceDateTime (int64) + TimeStyle (int8) + TimeZone + Culture.
            let datetime = r.read_i64_within(end, "FText time value")?;
            let time_style = r.read_i8_within(end, "FText time style")?;
            let time_zone = r.read_fstring_within(end, "FText time zone")?;
            let culture = r.read_fstring_within(end, "FText culture")?;
            Ok(json!({
                "history": "AsTime", "datetime": datetime, "time_style": time_style,
                "time_zone": time_zone, "culture": culture, "flags": flags
            }))
        }
        9 => {
            // AsDateTime: int64 + DateStyle + TimeStyle + [CustomPattern when DateStyle==Custom]
            // + TimeZone + Culture.
            let datetime = r.read_i64_within(end, "FText datetime value")?;
            let date_style = r.read_i8_within(end, "FText date style")?;
            let time_style = r.read_i8_within(end, "FText time style")?;
            let custom_pattern = if date_style == DATE_TIME_STYLE_CUSTOM {
                Some(r.read_fstring_within(end, "FText custom pattern")?)
            } else {
                None
            };
            let time_zone = r.read_fstring_within(end, "FText time zone")?;
            let culture = r.read_fstring_within(end, "FText culture")?;
            let mut o = Map::new();
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
            let source = parse_text(r, ctx, end, depth + 1)?;
            let transform_type = r.read_u8_within(end, "FText transform type")?;
            Ok(json!({
                "history": "Transform", "source": source,
                "transform_type": transform_type, "flags": flags
            }))
        }
        11 => {
            // StringTableEntry: TableId (FName) + Key (FString).
            let table_id = ctx
                .names
                .resolve_raw(r.read_raw_name_within(end, "FText string table id")?);
            let key = r.read_fstring_within(end, "FText string table key")?;
            Ok(json!({
                "history": "StringTableEntry", "table_id": table_id, "key": key, "flags": flags
            }))
        }
        12 => {
            // TextGenerator: GeneratorTypeID (FName), then a TArray<uint8> blob when named.
            let type_id = ctx
                .names
                .resolve_raw(r.read_raw_name_within(end, "FText generator type id")?);
            let mut o = Map::new();
            o.insert("history".into(), json!("TextGenerator"));
            o.insert("generator_type_id".into(), json!(type_id.clone()));
            if type_id != "None" && !type_id.is_empty() {
                let count = r.read_i32_within(end, "FText generator contents count")?;
                validate_count(
                    count,
                    end.saturating_sub(r.pos()),
                    1,
                    "FText generator contents",
                )?;
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

fn parse_format_argument(r: &mut Reader, ctx: &ParseCtx, end: u64, depth: usize) -> Result<Value> {
    let arg_type = r.read_i8_within(end, "FText format argument type")?;
    Ok(match arg_type {
        // Int
        0 => json!(r.read_i64_within(end, "FText argument int")?),
        // UInt / Gender (Gender is stored as a UInt)
        1 | 5 => json!(r.read_u64_within(end, "FText argument uint")?),
        2 => json!(r.read_f32_within(end, "FText argument float")? as f64),
        3 => json!(r.read_f64_within(end, "FText argument double")?),
        4 => parse_text(r, ctx, end, depth + 1)?,
        other => bail!("unknown FText format argument type: {other}"),
    })
}

/// EDateTimeStyle::Custom — gates the CustomPattern field in the AsDateTime history.
const DATE_TIME_STYLE_CUSTOM: i8 = 5;

/// FTextHistory_FormatNumber::Serialize, shared by AsNumber/AsPercent/AsCurrency:
/// an optional leading CurrencyCode, the SourceValue (FFormatArgumentValue), an
/// optional FNumberFormattingOptions, and the target CultureName.
#[allow(clippy::too_many_arguments)]
fn parse_number_format_history(
    r: &mut Reader,
    ctx: &ParseCtx,
    end: u64,
    kind: &str,
    flags: u32,
    has_currency_code: bool,
    depth: usize,
) -> Result<Value> {
    let currency_code = if has_currency_code {
        Some(r.read_fstring_within(end, "FText currency code")?)
    } else {
        None
    };
    let source_value = parse_format_argument(r, ctx, end, depth + 1)?;
    let format_options = if r.read_bool32_within(end, "FText format options flag")? {
        Some(parse_number_formatting_options(r, end)?)
    } else {
        None
    };
    let culture = r.read_fstring_within(end, "FText culture")?;
    let mut o = Map::new();
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
fn parse_number_formatting_options(r: &mut Reader, end: u64) -> Result<Value> {
    let always_sign = r.read_bool32_within(end, "FText always sign")?;
    let use_grouping = r.read_bool32_within(end, "FText use grouping")?;
    let rounding_mode = r.read_i8_within(end, "FText rounding mode")?;
    let minimum_integral_digits = r.read_i32_within(end, "FText minimum integral digits")?;
    let maximum_integral_digits = r.read_i32_within(end, "FText maximum integral digits")?;
    let minimum_fractional_digits = r.read_i32_within(end, "FText minimum fractional digits")?;
    let maximum_fractional_digits = r.read_i32_within(end, "FText maximum fractional digits")?;
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

/// FFormatArgumentData::operator<< — like FFormatArgumentValue but there is no
/// UInt variant, and Int is i64 only from TextFormatArgumentData64bitSupport.
fn parse_format_argument_data(
    r: &mut Reader,
    ctx: &ParseCtx,
    end: u64,
    depth: usize,
) -> Result<Value> {
    let arg_type = r.read_u8_within(end, "FFormatArgumentData type")?;
    Ok(match arg_type {
        0 => {
            if ctx.serialization.ue5_release_stream_version
                >= custom::TEXT_FORMAT_ARGUMENT_DATA_64BIT_SUPPORT
            {
                json!(r.read_i64_within(end, "FFormatArgumentData int64")?)
            } else {
                json!(i64::from(
                    r.read_i32_within(end, "FFormatArgumentData int32")?
                ))
            }
        }
        2 => json!(r.read_f32_within(end, "FFormatArgumentData float")? as f64),
        3 => json!(r.read_f64_within(end, "FFormatArgumentData double")?),
        4 => parse_text(r, ctx, end, depth + 1)?,
        5 => json!(r.read_u8_within(end, "FFormatArgumentData gender")?),
        other => bail!("unknown FFormatArgumentData type: {other}"),
    })
}
