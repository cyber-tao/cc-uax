use super::value::parse_value;
use super::{OVERRIDABLE_SERIALIZATION_BIT, PREVIEW_MAX, ParseCtx, PropertyEntry, to_hex};
use crate::name::NameMap;
use crate::reader::Reader;
use anyhow::{Result, bail};
use serde_json::json;

// FPropertyTag flag bits (EPropertyTagFlags).
const TAG_FLAG_HAS_ARRAY_INDEX: u8 = 0x01;
const TAG_FLAG_HAS_PROPERTY_GUID: u8 = 0x02;
const TAG_FLAG_HAS_PROPERTY_EXTENSIONS: u8 = 0x04;
const TAG_FLAG_HAS_BINARY_OR_NATIVE_SERIALIZE: u8 = 0x08;
const TAG_FLAG_BOOL_TRUE: u8 = 0x10;
const TAG_FLAG_SKIPPED_SERIALIZE: u8 = 0x20;

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
        let ty = Self::build(&flat, &mut pos)?;
        if pos != flat.len() {
            bail!("type name tree did not consume all nodes");
        }
        Ok(ty)
    }

    fn build(flat: &[(String, i32)], pos: &mut usize) -> Result<TypeName> {
        if *pos >= flat.len() {
            bail!("type name tree is incomplete");
        }
        let (name, inner) = flat[*pos].clone();
        *pos += 1;
        let mut params = Vec::new();
        for _ in 0..inner {
            params.push(Self::build(flat, pos)?);
        }
        Ok(TypeName { name, params })
    }

    pub fn display(&self) -> String {
        if self.params.is_empty() {
            self.name.clone()
        } else {
            let inner: Vec<String> = self.params.iter().map(TypeName::display).collect();
            format!("{}({})", self.name, inner.join(","))
        }
    }

    pub(crate) fn param(&self, i: usize) -> Option<&TypeName> {
        self.params.get(i)
    }
}

pub(crate) fn parse_properties(
    r: &mut Reader,
    ctx: &ParseCtx,
    end_limit: u64,
) -> Vec<PropertyEntry> {
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
        let array_index = if flags & TAG_FLAG_HAS_ARRAY_INDEX != 0 {
            match r.read_i32() {
                Ok(i) => i,
                Err(_) => break,
            }
        } else {
            0
        };
        let guid = if flags & TAG_FLAG_HAS_PROPERTY_GUID != 0 {
            match r.read_guid() {
                Ok(g) => Some(g.to_hex()),
                Err(_) => break,
            }
        } else {
            None
        };
        if flags & TAG_FLAG_HAS_PROPERTY_EXTENSIONS != 0 && parse_extensions(r).is_err() {
            break;
        }
        let is_binary_native = flags & TAG_FLAG_HAS_BINARY_OR_NATIVE_SERIALIZE != 0;
        let bool_val = flags & TAG_FLAG_BOOL_TRUE != 0;
        // SkippedSerialize (0x20): the value was intentionally not written (Size == 0),
        // so there is nothing to decode for this property.
        let is_skipped = flags & TAG_FLAG_SKIPPED_SERIALIZE != 0;

        if size < 0 {
            break;
        }
        let value_start = r.pos();
        let aligned = value_start.saturating_add(size as u64);
        if aligned > end_limit {
            break;
        }

        let value = if is_skipped {
            json!({ "@skipped": true })
        } else if type_name.name == "BoolProperty" {
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
    // FPropertyTag::SerializePropertyExtensions in a binary archive writes the uint8
    // extension flags directly (SA_ATTRIBUTE has no presence prefix; the 4-byte
    // presence bool exists only for text archives via SA_OPTIONAL_ATTRIBUTE). If
    // OverridableInformation (0x02) is set, an EOverriddenPropertyOperation byte
    // (uint8) and a 4-byte bExperimentalOverridableLogic bool follow — UE serializes
    // `bool` as a 4-byte int32, hence read_bool32 rather than a single byte.
    let ext = r.read_u8()?;
    if ext & OVERRIDABLE_SERIALIZATION_BIT != 0 {
        let _override_operation = r.read_u8()?;
        let _experimental = r.read_bool32()?;
    }
    Ok(())
}
