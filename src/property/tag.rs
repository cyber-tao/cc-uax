use super::value::parse_value;
use super::{
    OVERRIDABLE_SERIALIZATION_BIT, PREVIEW_MAX, ParseCtx, PropertyEntry, PropertyParse, to_hex,
};
use crate::diagnostic::Diagnostic;
use crate::name::NameMap;
use crate::reader::Reader;
use crate::version::{ue4, ue5};
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
    fn leaf(name: String) -> Self {
        TypeName {
            name,
            params: Vec::new(),
        }
    }

    fn with_params(name: String, params: Vec<TypeName>) -> Self {
        TypeName { name, params }
    }

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

struct PropertyTag {
    name: String,
    type_name: TypeName,
    size: i32,
    array_index: i32,
    guid: Option<String>,
    is_binary_native: bool,
    bool_val: bool,
    is_skipped: bool,
}

pub(crate) fn parse_properties_report(
    r: &mut Reader,
    ctx: &ParseCtx,
    end_limit: u64,
    path: &str,
) -> PropertyParse {
    let mut entries = Vec::new();
    let mut diagnostics = Vec::new();
    let mut guard = 0usize;
    loop {
        guard += 1;
        if guard > 1_000_000 {
            diagnostics.push(
                Diagnostic::warning(
                    "property_guard_limit_reached",
                    path,
                    "stopped after 1000000 properties; data may be corrupt",
                )
                .with_offset(r.pos()),
            );
            break;
        }
        if r.pos() + 8 > end_limit {
            break;
        }
        let tag_start = r.pos();
        let prop_index_path = format!("{path}/{}", entries.len());
        let tag = match read_property_tag(r, ctx, end_limit) {
            Ok(Some(tag)) => tag,
            Ok(None) => break,
            Err(err) => {
                if entries.is_empty() {
                    let _ = r.seek(end_limit);
                    break;
                }
                diagnostics.push(
                    Diagnostic::warning(
                        "property_tag_parse_failed",
                        prop_index_path,
                        format!("failed to parse property tag: {err:#}"),
                    )
                    .with_offset(tag_start),
                );
                break;
            }
        };
        let prop_path = format!("{path}/{}", tag.name);

        if tag.size < 0 {
            diagnostics.push(
                Diagnostic::warning(
                    "property_negative_size",
                    prop_path,
                    format!("property '{}' has negative size {}", tag.name, tag.size),
                )
                .with_offset(tag_start),
            );
            break;
        }
        let value_start = r.pos();
        let aligned = value_start.saturating_add(tag.size as u64);
        if aligned > end_limit {
            if entries.is_empty() {
                let _ = r.seek(end_limit);
                break;
            }
            diagnostics.push(
                Diagnostic::warning(
                    "property_value_overruns_window",
                    prop_path,
                    format!(
                        "property '{}' value range [{value_start}, {aligned}) exceeds end {end_limit}",
                        tag.name
                    ),
                )
                .with_offset(value_start),
            );
            break;
        }

        // SkippedSerialize (0x20): the value was intentionally not written (Size == 0),
        // so there is nothing to decode for this property.
        let value = if tag.is_skipped {
            json!({ "@skipped": true })
        } else if tag.type_name.name == "BoolProperty" {
            json!(tag.bool_val)
        } else {
            match parse_value(r, &tag.type_name, ctx, tag.is_binary_native, aligned) {
                Ok(v) if r.pos() <= aligned => v,
                Ok(_) => {
                    let consumed_to = r.pos();
                    let _ = r.seek(value_start);
                    let n = (tag.size as usize).min(PREVIEW_MAX);
                    let preview = r.read_bytes(n).unwrap_or_default();
                    diagnostics.push(
                        Diagnostic::warning(
                            "property_value_fallback",
                            prop_path.clone(),
                            format!(
                                "decoded property '{}' as {} past its declared value window: read to {consumed_to}, expected end {aligned}",
                                tag.name,
                                tag.type_name.display()
                            ),
                        )
                        .with_offset(value_start)
                        .with_context(json!({
                            "property": tag.name.clone(),
                            "type": tag.type_name.display(),
                            "size": tag.size,
                            "preview": to_hex(&preview),
                            "declared_end": aligned,
                            "consumed_to": consumed_to,
                        })),
                    );
                    json!({ "@unparsed": to_hex(&preview), "size": tag.size })
                }
                Err(err) => {
                    let _ = r.seek(value_start);
                    let n = (tag.size as usize).min(PREVIEW_MAX);
                    let preview = r.read_bytes(n).unwrap_or_default();
                    diagnostics.push(
                        Diagnostic::warning(
                            "property_value_fallback",
                            prop_path.clone(),
                            format!(
                                "failed to decode property '{}' as {}: {err:#}",
                                tag.name,
                                tag.type_name.display()
                            ),
                        )
                        .with_offset(value_start)
                        .with_context(json!({
                            "property": tag.name.clone(),
                            "type": tag.type_name.display(),
                            "size": tag.size,
                            "preview": to_hex(&preview),
                        })),
                    );
                    json!({ "@unparsed": to_hex(&preview), "size": tag.size })
                }
            }
        };

        if r.seek(aligned).is_err() {
            entries.push(PropertyEntry {
                name: tag.name,
                type_str: tag.type_name.display(),
                array_index: tag.array_index,
                value,
                guid: tag.guid,
            });
            break;
        }

        entries.push(PropertyEntry {
            name: tag.name,
            type_str: tag.type_name.display(),
            array_index: tag.array_index,
            value,
            guid: tag.guid,
        });
    }
    PropertyParse {
        entries,
        diagnostics,
    }
}

fn read_property_tag(
    r: &mut Reader,
    ctx: &ParseCtx,
    end_limit: u64,
) -> Result<Option<PropertyTag>> {
    let name_raw = r.read_raw_name()?;
    let name = ctx.names.resolve_raw(name_raw);
    if name == "None" || name.is_empty() {
        return Ok(None);
    }
    if ctx.file_version_ue5 >= ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME {
        read_complete_property_tag(r, ctx, name).map(Some)
    } else {
        read_legacy_property_tag(r, ctx, end_limit, name).map(Some)
    }
}

fn read_complete_property_tag(r: &mut Reader, ctx: &ParseCtx, name: String) -> Result<PropertyTag> {
    let type_name = TypeName::parse(r, ctx.names)?;
    let size = r.read_i32()?;
    let flags = r.read_u8()?;
    let array_index = if flags & TAG_FLAG_HAS_ARRAY_INDEX != 0 {
        r.read_i32()?
    } else {
        0
    };
    let guid = if flags & TAG_FLAG_HAS_PROPERTY_GUID != 0 {
        Some(r.read_guid()?.to_hex())
    } else {
        None
    };
    if flags & TAG_FLAG_HAS_PROPERTY_EXTENSIONS != 0 {
        parse_extensions(r)?;
    }
    Ok(PropertyTag {
        name,
        type_name,
        size,
        array_index,
        guid,
        is_binary_native: flags & TAG_FLAG_HAS_BINARY_OR_NATIVE_SERIALIZE != 0,
        bool_val: flags & TAG_FLAG_BOOL_TRUE != 0,
        is_skipped: flags & TAG_FLAG_SKIPPED_SERIALIZE != 0,
    })
}

fn read_legacy_property_tag(
    r: &mut Reader,
    ctx: &ParseCtx,
    _end_limit: u64,
    name: String,
) -> Result<PropertyTag> {
    let property_type = ctx.names.resolve_raw(r.read_raw_name()?);
    let size = r.read_i32()?;
    let array_index = r.read_i32()?;
    let (type_name, bool_val) = read_legacy_type_name(r, ctx, &property_type)?;
    let guid = if ctx.file_version_ue4 >= ue4::PROPERTY_GUID_IN_PROPERTY_TAG {
        if r.read_u8()? != 0 {
            Some(r.read_guid()?.to_hex())
        } else {
            None
        }
    } else {
        None
    };
    if ctx.file_version_ue5 >= ue5::PROPERTY_TAG_EXTENSION_AND_OVERRIDABLE_SERIALIZATION {
        parse_extensions(r)?;
    }
    Ok(PropertyTag {
        name,
        type_name,
        size,
        array_index,
        guid,
        is_binary_native: false,
        bool_val,
        is_skipped: false,
    })
}

fn read_legacy_type_name(
    r: &mut Reader,
    ctx: &ParseCtx,
    property_type: &str,
) -> Result<(TypeName, bool)> {
    let ty = match property_type {
        "StructProperty" => {
            let struct_name = ctx.names.resolve_raw(r.read_raw_name()?);
            if ctx.file_version_ue4 >= ue4::STRUCT_GUID_IN_PROPERTY_TAG {
                let _struct_guid = r.read_guid()?;
            }
            TypeName::with_params(property_type.to_string(), vec![TypeName::leaf(struct_name)])
        }
        "BoolProperty" => {
            let bool_val = r.read_u8()? != 0;
            return Ok((TypeName::leaf(property_type.to_string()), bool_val));
        }
        "ByteProperty" => {
            let enum_name = ctx.names.resolve_raw(r.read_raw_name()?);
            let params = if enum_name.is_empty() || enum_name == "None" {
                Vec::new()
            } else {
                vec![TypeName::leaf(enum_name)]
            };
            TypeName::with_params(property_type.to_string(), params)
        }
        "EnumProperty" => {
            let enum_name = ctx.names.resolve_raw(r.read_raw_name()?);
            TypeName::with_params(
                property_type.to_string(),
                vec![
                    TypeName::leaf(enum_name),
                    TypeName::leaf("ByteProperty".into()),
                ],
            )
        }
        "ArrayProperty" => {
            let inner = if ctx.file_version_ue4 >= ue4::INNER_ARRAY_TAG_INFO {
                ctx.names.resolve_raw(r.read_raw_name()?)
            } else {
                "None".to_string()
            };
            TypeName::with_params(property_type.to_string(), vec![TypeName::leaf(inner)])
        }
        "OptionalProperty" => {
            let inner = ctx.names.resolve_raw(r.read_raw_name()?);
            TypeName::with_params(property_type.to_string(), vec![TypeName::leaf(inner)])
        }
        "SetProperty" if ctx.file_version_ue4 >= ue4::PROPERTY_TAG_SET_MAP_SUPPORT => {
            let inner = ctx.names.resolve_raw(r.read_raw_name()?);
            TypeName::with_params(property_type.to_string(), vec![TypeName::leaf(inner)])
        }
        "MapProperty" if ctx.file_version_ue4 >= ue4::PROPERTY_TAG_SET_MAP_SUPPORT => {
            let key = ctx.names.resolve_raw(r.read_raw_name()?);
            let value = ctx.names.resolve_raw(r.read_raw_name()?);
            TypeName::with_params(
                property_type.to_string(),
                vec![TypeName::leaf(key), TypeName::leaf(value)],
            )
        }
        _ => TypeName::leaf(property_type.to_string()),
    };
    Ok((ty, false))
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
