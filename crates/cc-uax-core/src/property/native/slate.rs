use super::ensure_tagged_payload_parsed;
use crate::property::{ParseCtx, entries_to_values, parse_properties_report};
use crate::reader::Reader;
use crate::structured_value::{Value, json};
use crate::version::custom;
use anyhow::{Result, bail};

// Slate structs whose `Serialize` writes a native prefix before falling back to
// tagged properties.
pub(super) fn parse_slate_struct(
    r: &mut Reader,
    name: &str,
    ctx: &ParseCtx,
    value_end: u64,
) -> Result<Option<Value>> {
    let v = match name {
        "FontData" => match parse_font_data(r, ctx, value_end)? {
            Some(v) => v,
            None => return Ok(None),
        },
        _ => return Ok(None),
    };
    Ok(Some(v))
}

/// `FFontData::Serialize` (SlateCore CompositeFont.cpp; identical in UE5.0-5.8).
/// Below `FEditorObjectVersion::AddedFontFaceAssets` it returns false, so the
/// payload is tagged. From it on it writes `bool bIsCooked` (4 bytes); an
/// uncooked payload then runs `SerializeTaggedProperties`. The cooked layout
/// (`FontFaceAsset` and, without one, filename, hinting and loading policy) is
/// never written into an editor package and is not decoded.
fn parse_font_data(r: &mut Reader, ctx: &ParseCtx, value_end: u64) -> Result<Option<Value>> {
    if ctx.serialization.editor_version < custom::EDITOR_ADDED_FONT_FACE_ASSETS {
        return Ok(None);
    }
    if r.read_bool32_within(value_end, "FontData bIsCooked")? {
        bail!("cooked FFontData payload in an editor package");
    }
    let nested = parse_properties_report(r, ctx, value_end, "/properties");
    ensure_tagged_payload_parsed(&nested.status, "FontData")?;
    Ok(Some(json!({
        "@struct": "FontData",
        "properties": entries_to_values(&nested.entries),
    })))
}
