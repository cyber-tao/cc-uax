mod native;
mod tag;
mod text;
mod value;

use crate::name::NameMap;
use crate::pin::PinSerCtx;
use crate::reader::Reader;
use anyhow::{Result, bail};
use serde_json::{Value, json};

pub use tag::TypeName;
pub(crate) use text::parse_text;
pub(crate) use value::read_soft_object_path;

pub(crate) const PREVIEW_MAX: usize = 64;
const MAX_DYNAMIC_COUNT: i32 = 1_000_000;

// The "overridable serialization" bit, shared by EClassSerializationControlExtension
// (object control byte) and EPropertyTagExtension (per-tag extension flags).
pub(crate) const OVERRIDABLE_SERIALIZATION_BIT: u8 = 0x02;

pub struct ParseCtx<'a> {
    pub names: &'a NameMap,
    pub resolve_object: &'a dyn Fn(i32) -> Value,
    pub pins: PinSerCtx,
    pub soft_object_paths: &'a [Value],
    /// FNiagaraCustomVersion of the package (-1 when absent), gating Niagara decoders.
    pub niagara_version: i32,
    /// FFortniteMainBranchObjectVersion of the package (-1 when absent), gating the
    /// MovieScene channel bShowCurve field.
    pub fortnite_main_version: i32,
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
    // A UClass tagged-property block opens with a serialization-control byte
    // (EClassSerializationControlExtension, uint8). When OverridableSerialization-
    // Information (0x02) is set, an EOverriddenPropertyOperation byte (uint8) follows.
    if ue5_version >= crate::version::ue5::PROPERTY_TAG_EXTENSION_AND_OVERRIDABLE_SERIALIZATION {
        let control = match r.read_u8() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        if control & OVERRIDABLE_SERIALIZATION_BIT != 0 && r.read_u8().is_err() {
            return Vec::new();
        }
    }
    parse_properties(r, ctx, end_limit)
}

pub fn parse_properties(r: &mut Reader, ctx: &ParseCtx, end_limit: u64) -> Vec<PropertyEntry> {
    tag::parse_properties(r, ctx, end_limit)
}

pub(crate) fn validate_count(
    count: i32,
    remaining: u64,
    min_bytes_per_item: u64,
    label: &str,
) -> Result<()> {
    if !(0..=MAX_DYNAMIC_COUNT).contains(&count) {
        bail!("{label} count out of range: {count}");
    }
    if (count as u64).saturating_mul(min_bytes_per_item) > remaining {
        bail!("{label} count out of range: {count}");
    }
    Ok(())
}

pub(crate) fn ensure_within_value(r: &Reader, value_end: u64, label: &str) -> Result<()> {
    if r.pos() > value_end {
        bail!("{label} overran declared value window");
    }
    Ok(())
}

pub(crate) fn to_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
}
