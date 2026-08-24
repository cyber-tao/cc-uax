use super::value::parse_value;
use super::{
    OVERRIDABLE_SERIALIZATION_BIT, PREVIEW_MAX, ParseCtx, PropertyEntry, PropertyParse,
    PropertyParseStatus, to_hex,
};
use crate::diagnostic::Diagnostic;
use crate::name::NameMap;
use crate::reader::{RAW_NAME_BYTES, Reader};
use crate::structured_value::{Value, json};
use crate::version::{ue4, ue5};
use anyhow::{Result, bail};

const MAX_TYPE_NAME_DEPTH: usize = 64;
/// `known_opaque` reason for a set element or map key/value whose struct name the
/// legacy `FPropertyTag` layout never wrote (see [`has_unnamed_inner_struct`]).
const MISSING_INNER_STRUCT_NAME_REASON: &str = "the legacy property tag does not record a set/map element struct name, and unlike TArray the payload carries no inner tag, so the element cannot be decoded without a reflection registry";

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

/// True when a container element inside `ty` is typed only as `StructProperty`,
/// with no `UScriptStruct` name, *and* the payload carries no inner tag to
/// recover it from.
///
/// The legacy `FPropertyTag` layout (below
/// [`ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME`]) records only a container element's
/// *property* type. `FArrayProperty::SerializeItem` compensates by writing a full
/// inner `FPropertyTag` into the payload, so array elements stay decodable — see
/// [`read_inner_array_struct_name`]. `FSetProperty::SerializeItem` and
/// `FMapProperty::SerializeItem` write no such tag, so a set element or map
/// key/value struct really is recoverable only from UE's reflection registry.
fn has_unnamed_inner_struct(ty: &TypeName) -> bool {
    let element_struct_name_is_in_payload = ty.name == "ArrayProperty";
    ty.params.iter().any(|param| {
        (!element_struct_name_is_in_payload
            && param.name == "StructProperty"
            && param.params.is_empty())
            || has_unnamed_inner_struct(param)
    })
}

/// Read the inner `FPropertyTag` that `FArrayProperty::SerializeItem` writes
/// between a struct array's element count and its elements, and return the
/// element's `UScriptStruct` name.
///
/// UE writes this tag whenever the archive predates
/// [`ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME`], is at least
/// [`ue4::INNER_ARRAY_TAG_INFO`], and the inner property is a struct
/// (PropertyArray.cpp). It is written unconditionally, before the zero-element
/// early-out, and it carries the struct name the container's own tag never
/// records.
pub(super) fn read_inner_array_struct_name(
    r: &mut Reader,
    ctx: &ParseCtx,
    end_limit: u64,
) -> Result<String> {
    let name = ctx
        .names
        .resolve_raw(r.read_raw_name_within(end_limit, "inner array tag name")?);
    if name == "None" || name.is_empty() {
        bail!("inner array property tag is a None terminator");
    }
    let tag = read_legacy_property_tag(r, ctx, end_limit, name)?;
    if tag.type_name.name != "StructProperty" {
        bail!(
            "inner array property tag declares {} rather than StructProperty",
            tag.type_name.name
        );
    }
    match tag.type_name.param(0) {
        Some(param) if !param.name.is_empty() && param.name != "None" => Ok(param.name.clone()),
        _ => bail!("inner array property tag carries no struct name"),
    }
}

fn fallback_code(unnamed_inner_struct: bool) -> &'static str {
    if unnamed_inner_struct {
        "property_tag_missing_inner_struct_name"
    } else {
        "property_value_fallback"
    }
}

/// The value kept for a property whose payload could not be decoded: an
/// `@unparsed` preview normally, or a named opaque region when the cause is the
/// legacy tag's missing inner struct name.
fn fallback_value(unnamed_inner_struct: bool, preview: &[u8], size: i32) -> Value {
    if unnamed_inner_struct {
        json!({
            "status": "opaque",
            "reason": MISSING_INNER_STRUCT_NAME_REASON,
            "size": size,
            "preview": to_hex(preview),
        })
    } else {
        json!({ "@unparsed": to_hex(preview), "size": size })
    }
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

    /// A `StructProperty` carrying a resolved `UScriptStruct` name, shaped the same
    /// way a complete type name would carry it.
    pub(super) fn struct_of(struct_name: String) -> Self {
        Self::with_params("StructProperty".to_string(), vec![Self::leaf(struct_name)])
    }

    pub fn parse(r: &mut Reader, names: &NameMap, end_limit: u64) -> Result<Self> {
        let mut flat: Vec<(String, i32)> = Vec::new();
        let mut remaining: i64 = 1;
        let mut guard = 0usize;
        while remaining > 0 {
            let name = names.resolve_raw(r.read_raw_name_within(end_limit, "type name node")?);
            let inner = r.read_i32_within(end_limit, "type name parameter count")?;
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
        let ty = Self::build(&flat, &mut pos, 0)?;
        if pos != flat.len() {
            bail!("type name tree did not consume all nodes");
        }
        Ok(ty)
    }

    fn build(flat: &[(String, i32)], pos: &mut usize, depth: usize) -> Result<TypeName> {
        if depth > MAX_TYPE_NAME_DEPTH {
            bail!("type name nesting exceeds {MAX_TYPE_NAME_DEPTH}");
        }
        if *pos >= flat.len() {
            bail!("type name tree is incomplete");
        }
        let (name, inner) = flat[*pos].clone();
        *pos += 1;
        let mut params = Vec::new();
        for _ in 0..inner {
            params.push(Self::build(flat, pos, depth + 1)?);
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
    let mut status = PropertyParseStatus::Complete;
    // Highest offset this loop actually consumed as evidence. It only advances
    // past a completed tag/value pair or the `None` terminator, so a failed parse
    // never donates its leftover cursor to the export's decoded byte total.
    let mut decoded_end = None;
    let mut guard = 0usize;
    loop {
        guard += 1;
        if guard > 1_000_000 {
            status = PropertyParseStatus::FailedAfterEntries;
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
        if r.pos().saturating_add(RAW_NAME_BYTES) > end_limit {
            status = if entries.is_empty() {
                if r.pos() >= end_limit {
                    PropertyParseStatus::Empty
                } else {
                    PropertyParseStatus::NonTaggedPayload
                }
            } else {
                diagnostics.push(
                    Diagnostic::warning(
                        "property_tag_terminator_missing",
                        path,
                        "property list ended before a complete None tag could be read",
                    )
                    .with_offset(r.pos()),
                );
                PropertyParseStatus::FailedAfterEntries
            };
            let _ = r.seek(end_limit);
            break;
        }
        let tag_start = r.pos();
        let prop_index_path = format!("{path}/{}", entries.len());
        let tag = match read_property_tag(r, ctx, end_limit) {
            Ok(Some(tag)) => tag,
            Ok(None) => {
                if entries.is_empty() {
                    status = PropertyParseStatus::Empty;
                }
                // The terminator is consumed evidence: the block closed here.
                decoded_end = Some(r.pos());
                break;
            }
            Err(err) => {
                if entries.is_empty() {
                    status = PropertyParseStatus::NonTaggedPayload;
                    let _ = r.seek(end_limit);
                    break;
                }
                status = PropertyParseStatus::FailedAfterEntries;
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
            status = PropertyParseStatus::FailedAfterEntries;
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
                status = PropertyParseStatus::NonTaggedPayload;
                let _ = r.seek(end_limit);
                break;
            }
            status = PropertyParseStatus::FailedAfterEntries;
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
                Ok(v) if r.pos() == aligned => v,
                Ok(v) if r.pos() < aligned => {
                    // Decoder stopped before the declared window end; retain the gap as evidence.
                    let consumed_to = r.pos();
                    let gap = aligned - consumed_to;
                    let preview_len = (gap as usize).min(PREVIEW_MAX);
                    let preview = r.read_bytes(preview_len).unwrap_or_default();
                    diagnostics.push(
                        Diagnostic::warning(
                            "property_value_incomplete",
                            prop_path.clone(),
                            format!(
                                "decoded property '{}' as {} left {gap} undecoded byte(s): read to {consumed_to}, declared end {aligned}",
                                tag.name,
                                tag.type_name.display()
                            ),
                        )
                        .with_offset(consumed_to)
                        .with_context(json!({
                            "property": tag.name.clone(),
                            "type": tag.type_name.display(),
                            "size": tag.size,
                            "declared_end": aligned,
                            "consumed_to": consumed_to,
                            "unconsumed_bytes": gap,
                            "preview": to_hex(&preview),
                        })),
                    );
                    v
                }
                Ok(_) => {
                    let consumed_to = r.pos();
                    let _ = r.seek(value_start);
                    let n = (tag.size as usize).min(PREVIEW_MAX);
                    let preview = r.read_bytes(n).unwrap_or_default();
                    let unnamed_struct = has_unnamed_inner_struct(&tag.type_name);
                    diagnostics.push(
                        Diagnostic::warning(
                            fallback_code(unnamed_struct),
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
                    fallback_value(unnamed_struct, &preview, tag.size)
                }
                Err(err) => {
                    let _ = r.seek(value_start);
                    let n = (tag.size as usize).min(PREVIEW_MAX);
                    let preview = r.read_bytes(n).unwrap_or_default();
                    let unnamed_struct = has_unnamed_inner_struct(&tag.type_name);
                    diagnostics.push(
                        Diagnostic::warning(
                            fallback_code(unnamed_struct),
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
                    fallback_value(unnamed_struct, &preview, tag.size)
                }
            }
        };

        let resynced = r.seek(aligned).is_ok();
        entries.push(PropertyEntry {
            name: tag.name,
            type_str: tag.type_name.display(),
            array_index: tag.array_index,
            value,
            guid: tag.guid,
        });
        if !resynced {
            break;
        }
        decoded_end = Some(aligned);
    }
    PropertyParse {
        entries,
        diagnostics,
        status,
        decoded_end,
    }
}

fn read_property_tag(
    r: &mut Reader,
    ctx: &ParseCtx,
    end_limit: u64,
) -> Result<Option<PropertyTag>> {
    let name_raw = r.read_raw_name_within(end_limit, "tag name")?;
    let name = ctx.names.resolve_raw(name_raw);
    if name == "None" || name.is_empty() {
        return Ok(None);
    }
    if ctx.file_version_ue5 >= ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME {
        read_complete_property_tag(r, ctx, end_limit, name).map(Some)
    } else {
        read_legacy_property_tag(r, ctx, end_limit, name).map(Some)
    }
}

fn read_complete_property_tag(
    r: &mut Reader,
    ctx: &ParseCtx,
    end_limit: u64,
    name: String,
) -> Result<PropertyTag> {
    let type_name = TypeName::parse(r, ctx.names, end_limit)?;
    let size = r.read_i32_within(end_limit, "tag size")?;
    let flags = r.read_u8_within(end_limit, "tag flags")?;
    let array_index = if flags & TAG_FLAG_HAS_ARRAY_INDEX != 0 {
        r.read_i32_within(end_limit, "tag array index")?
    } else {
        0
    };
    let guid = if flags & TAG_FLAG_HAS_PROPERTY_GUID != 0 {
        Some(r.read_guid_within(end_limit, "tag property guid")?.to_hex())
    } else {
        None
    };
    if flags & TAG_FLAG_HAS_PROPERTY_EXTENSIONS != 0 {
        parse_extensions(r, end_limit)?;
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
    end_limit: u64,
    name: String,
) -> Result<PropertyTag> {
    let property_type = ctx
        .names
        .resolve_raw(r.read_raw_name_within(end_limit, "tag type")?);
    let size = r.read_i32_within(end_limit, "tag size")?;
    let array_index = r.read_i32_within(end_limit, "tag array index")?;
    let (type_name, bool_val) = read_legacy_type_name(r, ctx, end_limit, &property_type)?;
    // FPropertyTag stores HasPropertyGuid as uint8 (PropertyTag.h), not a
    // 4-byte FArchive::SerializeBool. Reading it as bool32 desyncs every
    // following tag on UE5.0–5.4 packages (FileVersionUE5 < 1012).
    let guid = if ctx.file_version_ue4 >= ue4::PROPERTY_GUID_IN_PROPERTY_TAG {
        if r.read_u8_within(end_limit, "tag has property guid")? != 0 {
            Some(r.read_guid_within(end_limit, "tag property guid")?.to_hex())
        } else {
            None
        }
    } else {
        None
    };
    if ctx.file_version_ue5 >= ue5::PROPERTY_TAG_EXTENSION_AND_OVERRIDABLE_SERIALIZATION {
        parse_extensions(r, end_limit)?;
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
    end_limit: u64,
    property_type: &str,
) -> Result<(TypeName, bool)> {
    let name = |r: &mut Reader, field: &str| -> Result<String> {
        Ok(ctx
            .names
            .resolve_raw(r.read_raw_name_within(end_limit, field)?))
    };
    let ty = match property_type {
        "StructProperty" => {
            let struct_name = name(r, "tag struct name")?;
            if ctx.file_version_ue4 >= ue4::STRUCT_GUID_IN_PROPERTY_TAG {
                let _struct_guid = r.read_guid_within(end_limit, "tag struct guid")?;
            }
            TypeName::with_params(property_type.to_string(), vec![TypeName::leaf(struct_name)])
        }
        "BoolProperty" => {
            // FPropertyTag::BoolVal is uint8 on disk (PropertyTag.h, UE5.0–5.8).
            let bool_val = r.read_u8_within(end_limit, "tag bool value")? != 0;
            return Ok((TypeName::leaf(property_type.to_string()), bool_val));
        }
        "ByteProperty" => {
            let enum_name = name(r, "tag enum name")?;
            let params = if enum_name.is_empty() || enum_name == "None" {
                Vec::new()
            } else {
                vec![TypeName::leaf(enum_name)]
            };
            TypeName::with_params(property_type.to_string(), params)
        }
        "EnumProperty" => {
            let enum_name = name(r, "tag enum name")?;
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
                name(r, "tag array inner type")?
            } else {
                "None".to_string()
            };
            TypeName::with_params(property_type.to_string(), vec![TypeName::leaf(inner)])
        }
        "OptionalProperty" => {
            let inner = name(r, "tag optional inner type")?;
            TypeName::with_params(property_type.to_string(), vec![TypeName::leaf(inner)])
        }
        "SetProperty" if ctx.file_version_ue4 >= ue4::PROPERTY_TAG_SET_MAP_SUPPORT => {
            let inner = name(r, "tag set element type")?;
            TypeName::with_params(property_type.to_string(), vec![TypeName::leaf(inner)])
        }
        "MapProperty" if ctx.file_version_ue4 >= ue4::PROPERTY_TAG_SET_MAP_SUPPORT => {
            let key = name(r, "tag map key type")?;
            let value = name(r, "tag map value type")?;
            TypeName::with_params(
                property_type.to_string(),
                vec![TypeName::leaf(key), TypeName::leaf(value)],
            )
        }
        _ => TypeName::leaf(property_type.to_string()),
    };
    Ok((ty, false))
}

fn parse_extensions(r: &mut Reader, end_limit: u64) -> Result<()> {
    // FPropertyTag::SerializePropertyExtensions in a binary archive writes the uint8
    // extension flags directly (SA_ATTRIBUTE has no presence prefix; the 4-byte
    // presence bool exists only for text archives via SA_OPTIONAL_ATTRIBUTE). If
    // OverridableInformation (0x02) is set, an EOverriddenPropertyOperation byte
    // (uint8) and a 4-byte bExperimentalOverridableLogic bool follow — UE serializes
    // `bool` as a 4-byte int32, hence read_bool32 rather than a single byte.
    // If HasExternalsObjects (0x04) is set (UE5.8+, CPF_ExperimentalExternalObjects),
    // a trailing bExperimentalExternalObjects bool32 follows.
    const HAS_EXTERNAL_OBJECTS_BIT: u8 = 0x04;
    let ext = r.read_u8_within(end_limit, "tag extension flags")?;
    if ext & OVERRIDABLE_SERIALIZATION_BIT != 0 {
        let _override_operation = r.read_u8_within(end_limit, "tag override operation")?;
        let _experimental = r.read_bool32_within(end_limit, "tag overridable logic flag")?;
    }
    if ext & HAS_EXTERNAL_OBJECTS_BIT != 0 {
        let _external_objects = r.read_bool32_within(end_limit, "tag external objects flag")?;
    }
    Ok(())
}
