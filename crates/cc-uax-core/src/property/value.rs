use super::native::{ensure_tagged_payload_parsed, is_tagged_fallback_struct, parse_native_struct};
use super::tag::read_inner_array_struct_name;
use super::text::parse_text;
use super::{
    PREVIEW_MAX, ParseCtx, TypeName, ensure_within_value, entries_to_values,
    parse_properties_report, to_hex, validate_count,
};
use crate::model::{OpaqueByteRange, OpaqueReason, OpaqueValue};
use crate::name::NameMap;
use crate::reader::{RAW_NAME_BYTES, Reader};
use crate::structured_value::{Value, json};
use crate::version::{ue4, ue5};
use anyhow::{Result, bail};

pub(crate) fn parse_value(
    r: &mut Reader,
    ty: &TypeName,
    ctx: &ParseCtx,
    prefer_native: bool,
    value_end: u64,
) -> Result<Value> {
    let v = match ty.name.as_str() {
        "BoolProperty" => json!(r.read_u8_within(value_end, "BoolProperty")? != 0),
        "Int8Property" => json!(r.read_i8_within(value_end, "Int8Property")?),
        "Int16Property" => json!(r.read_i16_within(value_end, "Int16Property")?),
        "IntProperty" => json!(r.read_i32_within(value_end, "IntProperty")?),
        "Int64Property" => json!(r.read_i64_within(value_end, "Int64Property")?),
        "ByteProperty" => {
            if has_enum_param(ty) {
                let raw = r.read_raw_name_within(value_end, "ByteProperty enum")?;
                if is_legacy_unnamed_enum(ty) && !ctx.names.is_valid(raw) {
                    bail!(
                        "ByteProperty enum name index {} is outside the name table",
                        raw.index
                    );
                }
                json!(ctx.names.resolve_raw(raw))
            } else {
                json!(r.read_u8_within(value_end, "ByteProperty")?)
            }
        }
        "UInt16Property" => json!(r.read_u16_within(value_end, "UInt16Property")?),
        "UInt32Property" => json!(r.read_u32_within(value_end, "UInt32Property")?),
        "UInt64Property" => json!(r.read_u64_within(value_end, "UInt64Property")?),
        "FloatProperty" => json!(r.read_f32_within(value_end, "FloatProperty")? as f64),
        "DoubleProperty" => json!(r.read_f64_within(value_end, "DoubleProperty")?),
        "EnumProperty" => json!(
            ctx.names
                .resolve_raw(r.read_raw_name_within(value_end, "EnumProperty")?)
        ),
        "NameProperty" => json!(
            ctx.names
                .resolve_raw(r.read_raw_name_within(value_end, "NameProperty")?)
        ),
        "StrProperty" => json!(r.read_fstring_within(value_end, "StrProperty")?),
        "Utf8StrProperty" | "AnsiStrProperty" => {
            json!(r.read_narrow_string_within(value_end, &ty.name)?)
        }
        "TextProperty" => parse_text(r, ctx, value_end, 0)?,
        "ObjectProperty" | "ClassProperty" | "WeakObjectProperty" | "ObjectPtrProperty"
        | "ClassPtrProperty" | "InterfaceProperty" => {
            let idx = r.read_i32_within(value_end, "object reference")?;
            (ctx.resolve_object)(idx)
        }
        "LazyObjectProperty" => {
            // FLinkerSave::operator<<(FLazyObjectPtr&) writes the 16-byte
            // FUniqueObjectGuid, not a package index.
            json!({
                "lazy_object_guid": r.read_guid_within(value_end, "LazyObjectProperty")?.to_hex()
            })
        }
        "DelegateProperty" => {
            let object = r.read_i32_within(value_end, "delegate object")?;
            let function = ctx
                .names
                .resolve_raw(r.read_raw_name_within(value_end, "delegate function")?);
            json!({ "object": (ctx.resolve_object)(object), "function": function })
        }
        "MulticastInlineDelegateProperty" | "MulticastSparseDelegateProperty" => {
            let count = r.read_i32_within(value_end, "delegate invocation count")?;
            let remaining = value_end.saturating_sub(r.pos());
            validate_count(count, remaining, 12, "delegate invocation")?;
            let mut arr = Vec::with_capacity(count as usize);
            for _ in 0..count {
                let object = r.read_i32_within(value_end, "delegate object")?;
                let function = ctx
                    .names
                    .resolve_raw(r.read_raw_name_within(value_end, "delegate function")?);
                arr.push(json!({ "object": (ctx.resolve_object)(object), "function": function }));
            }
            Value::Array(arr)
        }
        "SoftObjectProperty" | "SoftClassProperty" => parse_soft_object(r, ctx, value_end)?,
        // FFieldPath::operator<< writes the FName path array then the owner
        // UStruct. UE gates the owner on FFortniteMainBranchObjectVersion or
        // FReleaseObjectVersion reaching FFieldPathOwnerSerialization; every
        // in-scope UE5 package satisfies both, so it is always present here.
        "FieldPathProperty" => {
            let count = r.read_i32_within(value_end, "FieldPath length")?;
            let remaining = value_end.saturating_sub(r.pos());
            validate_count(count, remaining, RAW_NAME_BYTES, "FieldPath segment")?;
            let mut path = Vec::with_capacity(count as usize);
            for _ in 0..count {
                path.push(
                    ctx.names
                        .resolve_raw(r.read_raw_name_within(value_end, "FieldPath segment")?),
                );
            }
            let owner = r.read_i32_within(value_end, "FieldPath owner")?;
            json!({ "path": path, "owner": (ctx.resolve_object)(owner) })
        }
        "OptionalProperty" => {
            // FOptionalProperty::SerializeItem encodes presence via the binary
            // structured-archive optional field (a 4-byte UBOOL), then the inner
            // value when set (UE5.7 serializes the value directly).
            let inner = ty
                .param(0)
                .ok_or_else(|| anyhow::anyhow!("OptionalProperty missing inner type"))?;
            if r.read_bool32_within(value_end, "OptionalProperty presence")? {
                parse_value(r, inner, ctx, prefer_native, value_end)?
            } else {
                Value::Null
            }
        }
        "StructProperty" => {
            let struct_name = ty.param(0).map(|p| p.name.as_str()).unwrap_or("");
            parse_struct(r, struct_name, ctx, prefer_native, value_end)?
        }
        "ArrayProperty" | "SetProperty" | "MapProperty" => {
            if has_legacy_bare_byte_slot(ctx, ty) {
                return parse_legacy_byte_container(r, ty, ctx, prefer_native, value_end);
            }
            parse_container(r, ty, ctx, prefer_native, value_end)?
        }
        _ => bail!("unknown property type: {}", ty.name),
    };
    Ok(v)
}

/// The `TArray`/`TSet`/`TMap` arms of [`parse_value`]. Kept apart so the legacy
/// byte-width probe can decode a container under an assumed layout without
/// re-entering its own dispatch.
fn parse_container(
    r: &mut Reader,
    ty: &TypeName,
    ctx: &ParseCtx,
    prefer_native: bool,
    value_end: u64,
) -> Result<Value> {
    match ty.name.as_str() {
        "ArrayProperty" => {
            let inner = ty
                .param(0)
                .ok_or_else(|| anyhow::anyhow!("ArrayProperty missing element type"))?;
            parse_array(r, inner, ctx, prefer_native, value_end)
        }
        "SetProperty" => {
            let inner = ty
                .param(0)
                .ok_or_else(|| anyhow::anyhow!("SetProperty missing element type"))?;
            discard_removed_elements(
                r,
                inner,
                ctx,
                prefer_native,
                value_end,
                "Set removed element",
            )?;
            parse_collection(r, inner, ctx, prefer_native, value_end)
        }
        "MapProperty" => {
            let key_ty = ty
                .param(0)
                .ok_or_else(|| anyhow::anyhow!("MapProperty missing key type"))?;
            let val_ty = ty
                .param(1)
                .ok_or_else(|| anyhow::anyhow!("MapProperty missing value type"))?;
            parse_map(r, key_ty, val_ty, ctx, prefer_native, value_end)
        }
        _ => bail!("{} is not a container property", ty.name),
    }
}

/// Placeholder enum name for a legacy container whose `ByteProperty` elements
/// turn out to be enum names: the tag never recorded which enum.
const LEGACY_UNNAMED_ENUM: &str = "<enum>";

fn is_legacy_unnamed_enum(ty: &TypeName) -> bool {
    ty.params
        .first()
        .is_some_and(|param| param.name == LEGACY_UNNAMED_ENUM)
}

fn is_bare_byte(ty: &TypeName) -> bool {
    ty.name == "ByteProperty" && ty.params.is_empty()
}

/// Whether a container tag below `PROPERTY_TAG_COMPLETE_TYPE_NAME` records a
/// bare `ByteProperty` as one of its element, key or value types. Such a tag says
/// nothing about whether the slot is a `uint8` or a `TEnumAsByte<E>`.
fn has_legacy_bare_byte_slot(ctx: &ParseCtx, ty: &TypeName) -> bool {
    ctx.file_version_ue5 < ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME
        && matches!(
            ty.name.as_str(),
            "ArrayProperty" | "SetProperty" | "MapProperty"
        )
        && ty.params.iter().any(is_bare_byte)
}

/// Decodes a legacy container whose `ByteProperty` slots are ambiguous.
///
/// `FByteProperty::SerializeItem` writes an enum-backed byte as an 8-byte
/// `FName` and a plain one as a single byte, and below
/// `PROPERTY_TAG_COMPLETE_TYPE_NAME` the tag records neither. The tag's `Size`
/// is the criterion: every byte/enum-name assignment of the bare slots is decoded
/// from the value start, and only one that fills the declared window exactly (with
/// name indices the table contains) counts. Several fitting layouts with
/// different values are reported as ambiguous instead of guessed.
fn parse_legacy_byte_container(
    r: &mut Reader,
    ty: &TypeName,
    ctx: &ParseCtx,
    prefer_native: bool,
    value_end: u64,
) -> Result<Value> {
    let start = r.pos();
    let mark = ctx.nested_diagnostics.borrow().len();
    let slots: Vec<usize> = ty
        .params
        .iter()
        .enumerate()
        .filter(|(_, param)| is_bare_byte(param))
        .map(|(index, _)| index)
        .collect();

    struct Fit {
        value: Value,
        diagnostics: Vec<crate::diagnostic::Diagnostic>,
        layout: String,
    }
    let mut fits: Vec<Fit> = Vec::new();
    for assignment in 0..(1u32 << slots.len()) {
        let mut candidate = ty.clone();
        let mut layout = Vec::new();
        for (bit, &slot) in slots.iter().enumerate() {
            let is_enum = assignment & (1 << bit) != 0;
            if is_enum {
                candidate.params[slot] = TypeName::byte_enum_of(LEGACY_UNNAMED_ENUM.to_string());
            }
            layout.push(format!(
                "{}={}",
                legacy_slot_label(ty, slot),
                if is_enum { "enum names" } else { "bytes" }
            ));
        }
        let _ = r.seek(start);
        ctx.nested_diagnostics.borrow_mut().truncate(mark);
        let decoded = parse_container(r, &candidate, ctx, prefer_native, value_end);
        let diagnostics = ctx.nested_diagnostics.borrow_mut().split_off(mark);
        if let Ok(value) = decoded
            && r.pos() == value_end
        {
            fits.push(Fit {
                value,
                diagnostics,
                layout: layout.join(", "),
            });
        }
    }

    let window = value_end.saturating_sub(start);
    let Some(first) = fits.first() else {
        let _ = r.seek(start);
        bail!(
            "legacy {} fills its {window}-byte value neither with byte nor with enum-name elements",
            ty.display()
        );
    };
    if fits.iter().all(|fit| fit.value == first.value) {
        let fit = fits.swap_remove(0);
        ctx.nested_diagnostics.borrow_mut().extend(fit.diagnostics);
        r.seek(value_end)?;
        return Ok(fit.value);
    }

    r.seek(start)?;
    let preview = r.read_bytes(window.min(PREVIEW_MAX as u64) as usize)?;
    r.seek(value_end)?;
    let layouts: Vec<String> = fits.iter().map(|fit| format!("[{}]", fit.layout)).collect();
    Ok(Value::Opaque(OpaqueValue {
        reason: OpaqueReason::AmbiguousLegacyByteWidth,
        message: format!(
            "legacy {} fills its {window}-byte value under more than one layout with different values: {}",
            ty.display(),
            layouts.join(" or ")
        ),
        type_name: Some(ty.display()),
        byte_range: OpaqueByteRange {
            start,
            end: value_end,
            size: window,
            preview: to_hex(&preview),
        },
    }))
}

fn legacy_slot_label(ty: &TypeName, slot: usize) -> &'static str {
    match (ty.name.as_str(), slot) {
        ("MapProperty", 0) => "key",
        ("MapProperty", _) => "value",
        _ => "element",
    }
}

fn has_enum_param(ty: &TypeName) -> bool {
    ty.params
        .first()
        .map(|p| !p.name.is_empty() && p.name != "None")
        .unwrap_or(false)
}

/// `TArray` payload: element count, then — below
/// [`ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME`] and for a struct element — the inner
/// `FPropertyTag` UE writes to name the element struct, then the elements
/// (`FArrayProperty::SerializeItem`). The container's own tag records only the
/// element's *property* type, so that inner tag is the only place the
/// `UScriptStruct` name appears on disk.
fn parse_array(
    r: &mut Reader,
    inner: &TypeName,
    ctx: &ParseCtx,
    prefer_native: bool,
    value_end: u64,
) -> Result<Value> {
    let count = r.read_i32_within(value_end, "array element count")?;
    let remaining_in_value = value_end.saturating_sub(r.pos());
    validate_count(count, remaining_in_value, 1, "collection element")?;
    let named_inner = if inner_array_tag_is_serialized(ctx, inner) {
        let struct_name = read_inner_array_struct_name(r, ctx, value_end)?;
        Some(TypeName::struct_of(struct_name))
    } else {
        None
    };
    let inner = named_inner.as_ref().unwrap_or(inner);
    read_elements(r, inner, ctx, prefer_native, value_end, count)
}

/// Whether `FArrayProperty::SerializeItem` wrote an inner `FPropertyTag` for this
/// element type. From [`ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME`] on, the struct name
/// lives in the container tag's type tree instead and no inner tag is written.
fn inner_array_tag_is_serialized(ctx: &ParseCtx, inner: &TypeName) -> bool {
    ctx.file_version_ue5 < ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME
        && ctx.file_version_ue4 >= ue4::INNER_ARRAY_TAG_INFO
        && inner.name == "StructProperty"
        && inner.params.is_empty()
}

fn parse_collection(
    r: &mut Reader,
    inner: &TypeName,
    ctx: &ParseCtx,
    prefer_native: bool,
    value_end: u64,
) -> Result<Value> {
    let count = r.read_i32_within(value_end, "collection element count")?;
    let remaining_in_value = value_end.saturating_sub(r.pos());
    validate_count(count, remaining_in_value, 1, "collection element")?;
    read_elements(r, inner, ctx, prefer_native, value_end, count)
}

fn read_elements(
    r: &mut Reader,
    inner: &TypeName,
    ctx: &ParseCtx,
    prefer_native: bool,
    value_end: u64,
    count: i32,
) -> Result<Value> {
    let mut arr = Vec::with_capacity(count as usize);
    for _ in 0..count {
        arr.push(parse_value(r, inner, ctx, prefer_native, value_end)?);
        ensure_within_value(r, value_end, "collection element")?;
    }
    Ok(Value::Array(arr))
}

/// TSet/TMap delta saves serialize NumToRemove followed by that many key payloads
/// (keys removed relative to the archetype); the loader reads and discards them
/// before the element/pair entries (FSetProperty/FMapProperty::SerializeItem).
fn discard_removed_elements(
    r: &mut Reader,
    key_ty: &TypeName,
    ctx: &ParseCtx,
    prefer_native: bool,
    value_end: u64,
    label: &str,
) -> Result<()> {
    let num_to_remove = r.read_i32_within(value_end, "removed element count")?;
    let remaining = value_end.saturating_sub(r.pos());
    validate_count(num_to_remove, remaining, 1, label)?;
    for _ in 0..num_to_remove {
        let _ = parse_value(r, key_ty, ctx, prefer_native, value_end)?;
        ensure_within_value(r, value_end, label)?;
    }
    Ok(())
}

fn parse_map(
    r: &mut Reader,
    key_ty: &TypeName,
    val_ty: &TypeName,
    ctx: &ParseCtx,
    prefer_native: bool,
    value_end: u64,
) -> Result<Value> {
    discard_removed_elements(r, key_ty, ctx, prefer_native, value_end, "Map removed key")?;
    let count = r.read_i32_within(value_end, "map element count")?;
    let remaining_in_value = value_end.saturating_sub(r.pos());
    validate_count(count, remaining_in_value, 2, "Map element")?;
    let mut arr = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let key = parse_value(r, key_ty, ctx, prefer_native, value_end)?;
        ensure_within_value(r, value_end, "Map key")?;
        let value = parse_value(r, val_ty, ctx, prefer_native, value_end)?;
        ensure_within_value(r, value_end, "Map value")?;
        arr.push(json!({ "key": key, "value": value }));
    }
    Ok(Value::Array(arr))
}

fn parse_struct(
    r: &mut Reader,
    struct_name: &str,
    ctx: &ParseCtx,
    prefer_native_for_unknown: bool,
    value_end: u64,
) -> Result<Value> {
    if struct_name == "SoftObjectPath" || struct_name == "SoftClassPath" {
        return parse_soft_object(r, ctx, value_end);
    }
    if let Some(v) = parse_native_struct(r, struct_name, ctx, value_end)? {
        return Ok(v);
    }
    if prefer_native_for_unknown && !is_tagged_fallback_struct(struct_name) {
        bail!("unknown native struct: {struct_name}");
    }
    // The block has to have parsed. Its failure paths seek to `value_end`, so
    // without consulting the status the caller sees a cursor that looks like a
    // clean decode and records an opaque payload as a decoded empty struct.
    //
    // Only the status, not the position: a struct whose serializer writes its own
    // data after the tagged block ends cleanly short of `value_end`, and the tag
    // loop already reports that gap as `property_value_incomplete` while keeping
    // the properties that did decode. Demanding exact consumption here threw that
    // evidence away instead.
    let nested = parse_properties_report(r, ctx, value_end, "/properties");
    ensure_tagged_payload_parsed(&nested.status, struct_name)?;
    Ok(json!({ "@struct": struct_name, "properties": entries_to_values(&nested.entries) }))
}

/// Decode an `FSoftObjectPath` value. When the package carries a soft-object-path
/// list the reference serializes as an int32 index into that list; otherwise the
/// path is written inline (see [`read_soft_object_path`]).
pub(crate) fn parse_soft_object(r: &mut Reader, ctx: &ParseCtx, value_end: u64) -> Result<Value> {
    // A package whose header declared a soft-object-path list but whose table
    // could not be read is not the same as a package without one. UE keys the
    // same choice on the loaded list (FLinkerLoad::operator<<(FSoftObjectPath&)),
    // but a table it cannot read is a critical load error, so the "list present
    // yet empty" state never reaches this decision there. Decoding the 4-byte
    // index as an inline path would silently misread every soft reference, so the
    // value stays opaque with a reason instead.
    if ctx.soft_object_paths_unavailable {
        bail!(
            "the package declares a soft object path list that could not be read, so the int32 index this value serializes as cannot be resolved"
        );
    }
    // When the package has a soft object path list, soft references serialize as
    // an int32 index into that list; otherwise the path is serialized inline.
    if !ctx.soft_object_paths.is_empty() {
        let index = r.read_i32_within(value_end, "soft object path index")?;
        let index = usize::try_from(index)
            .map_err(|_| anyhow::anyhow!("soft object path index out of range: {index}"))?;
        return ctx
            .soft_object_paths
            .get(index)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("soft object path index out of range: {index}"));
    }
    read_soft_object_path(r, ctx.names, ctx.file_version_ue5, value_end)
}

/// Decode an inline `FSoftObjectPath` (`SerializePathWithoutFixup`). Before
/// `FSOFTOBJECTPATH_REMOVE_ASSET_PATH_FNAMES` the path is a single FName holding
/// the whole asset path; from that version on it is an `FTopLevelAssetPath`
/// (package name, asset name) pair.
pub(crate) fn read_soft_object_path(
    r: &mut Reader,
    names: &NameMap,
    file_version_ue5: i32,
    value_end: u64,
) -> Result<Value> {
    let asset_path = if file_version_ue5 >= ue5::FSOFTOBJECTPATH_REMOVE_ASSET_PATH_FNAMES {
        let package_name =
            names.resolve_raw(r.read_raw_name_within(value_end, "soft object package name")?);
        let asset_name =
            names.resolve_raw(r.read_raw_name_within(value_end, "soft object asset name")?);
        if asset_name.is_empty() || asset_name == "None" {
            package_name
        } else {
            format!("{package_name}.{asset_name}")
        }
    } else {
        names.resolve_raw(r.read_raw_name_within(value_end, "soft object path")?)
    };
    let sub_path = r.read_fstring_within(value_end, "soft object sub path")?;
    if sub_path.is_empty() {
        Ok(json!({ "asset_path": asset_path }))
    } else {
        Ok(json!({ "asset_path": asset_path, "sub_path": sub_path }))
    }
}
