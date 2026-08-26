//! `FField` / `FProperty` decoding for `UStruct::SerializeProperties`.
//!
//! Every layout here comes from the matching `Serialize` override in UE5's
//! `CoreUObject`: `FField::Serialize` (Field.cpp), `FProperty::Serialize`
//! (Property.cpp), and one override per concrete property class.
//!
//! The type is chosen by an `FName` written ahead of each field, and the class
//! hierarchy decides which overrides run: `FClassProperty` is an
//! `FObjectProperty`, so it reads `PropertyClass` before its own `MetaClass`.
//! Anything not in the table below is refused rather than guessed at — an
//! unrecognized field name means the rest of the struct is no longer addressable,
//! and inventing a width would silently corrupt everything after it.

use crate::package::Package;
use crate::reader::{FSTRING_LENGTH_BYTES, RAW_NAME_BYTES, Reader};
use anyhow::{Result, bail};

/// Nested fields (an array of maps of arrays) stay shallow in practice; this only
/// bounds a malformed stream.
const MAX_FIELD_DEPTH: u32 = 32;

pub(crate) struct FieldContext<'a> {
    pub(crate) package: &'a Package,
    /// `FField::Serialize` writes its flags word only when the archive keeps
    /// editor-only data.
    pub(crate) filter_editor_only: bool,
}

/// One decoded `FProperty`: the reflected declaration of a Blueprint variable,
/// function parameter, or local.
#[derive(Debug, Clone)]
pub(crate) struct DecodedField {
    pub(crate) name: String,
    /// The `FFieldClass` name, e.g. `ObjectProperty`.
    pub(crate) type_name: String,
    pub(crate) array_dim: i32,
    pub(crate) flags: u64,
    pub(crate) rep_notify_func: Option<String>,
    /// The object this property's type points at: the class for an object
    /// reference, the struct for a struct, the signature for a delegate.
    pub(crate) type_object: Option<String>,
    /// Inner fields: an array's element, a map's key and value, an enum's
    /// underlying integer.
    pub(crate) inner: Vec<DecodedField>,
}

/// `SerializeProperties`: a count, then a type name and body per field.
pub(crate) fn decode_property_list(
    reader: &mut Reader,
    end: u64,
    ctx: &FieldContext<'_>,
) -> Result<Vec<DecodedField>> {
    let count = reader.read_i32_within(end, "ChildProperties count")?;
    if count < 0 {
        bail!("ChildProperties count out of range: {count}");
    }
    // Each field is at least a type name plus a name, so a count that cannot fit
    // is rejected before anything is allocated for it.
    let min_bytes = (count as u64).saturating_mul(RAW_NAME_BYTES * 2);
    reader.ensure_within(end, min_bytes, "ChildProperties")?;
    let mut fields = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let type_name = read_name(reader, end, ctx, "ChildProperties type name")?;
        fields.push(decode_field(reader, end, ctx, type_name, 0)?);
    }
    Ok(fields)
}

/// `SerializeSingleField`: a type name that may be `None`, then the body.
fn decode_single_field(
    reader: &mut Reader,
    end: u64,
    ctx: &FieldContext<'_>,
    depth: u32,
) -> Result<Option<DecodedField>> {
    let type_name = read_name(reader, end, ctx, "inner field type name")?;
    if type_name == "None" {
        return Ok(None);
    }
    Ok(Some(decode_field(reader, end, ctx, type_name, depth + 1)?))
}

fn decode_field(
    reader: &mut Reader,
    end: u64,
    ctx: &FieldContext<'_>,
    type_name: String,
    depth: u32,
) -> Result<DecodedField> {
    if depth > MAX_FIELD_DEPTH {
        bail!("field nesting exceeded {MAX_FIELD_DEPTH}");
    }
    let Some(layout) = field_layout(&type_name) else {
        bail!("unknown property type `{type_name}`");
    };

    // FField::Serialize.
    let name = read_name(reader, end, ctx, "field name")?;
    if !ctx.filter_editor_only {
        reader.ensure_within(end, 4, "field flags")?;
        reader.read_u32()?;
    }
    // The metadata map is written for any package that is not cooked, which is
    // every package in scope here.
    if reader.read_bool32_within(end, "field metadata presence")? {
        read_metadata(reader, end, ctx)?;
    }

    // FProperty::Serialize.
    let array_dim = reader.read_i32_within(end, "field ArrayDim")?;
    reader.read_i32_within(end, "field ElementSize")?;
    reader.ensure_within(end, 8, "field PropertyFlags")?;
    let flags = reader.read_u64()?;
    // RepIndex is rebuilt on demand; the stored value exists for stream
    // compatibility only.
    reader.ensure_within(end, 2, "field RepIndex")?;
    reader.read_u16()?;
    let rep_notify_func = read_name(reader, end, ctx, "field RepNotifyFunc")?;
    reader.ensure_within(end, 1, "field BlueprintReplicationCondition")?;
    reader.read_u8()?;

    // The concrete class's own override.
    let mut type_object = None;
    let mut inner = Vec::new();
    for step in layout {
        match step {
            FieldStep::Object => {
                let object = read_object(reader, end, ctx)?;
                if type_object.is_none() {
                    type_object = object;
                }
            }
            FieldStep::Name => {
                read_name(reader, end, ctx, "field class name")?;
            }
            FieldStep::Bytes(count) => {
                reader.ensure_within(end, *count, "field payload")?;
                reader.skip(*count)?;
            }
            FieldStep::Bool32 => {
                reader.read_bool32_within(end, "field flag")?;
            }
            FieldStep::Field => {
                if let Some(field) = decode_single_field(reader, end, ctx, depth)? {
                    inner.push(field);
                }
            }
        }
    }

    Ok(DecodedField {
        name,
        type_name,
        array_dim,
        flags,
        rep_notify_func: (rep_notify_func != "None").then_some(rep_notify_func),
        type_object,
        inner,
    })
}

/// `TMap<FName, FString>` written by `FField::Serialize` when the field carries
/// editor metadata.
fn read_metadata(reader: &mut Reader, end: u64, ctx: &FieldContext<'_>) -> Result<()> {
    let count = reader.read_i32_within(end, "field metadata count")?;
    if count < 0 {
        bail!("field metadata count out of range: {count}");
    }
    let min_bytes = (count as u64).saturating_mul(RAW_NAME_BYTES + FSTRING_LENGTH_BYTES);
    reader.ensure_within(end, min_bytes, "field metadata")?;
    for _ in 0..count {
        read_name(reader, end, ctx, "field metadata key")?;
        reader.read_fstring_within(end, "field metadata value")?;
    }
    Ok(())
}

fn read_name(reader: &mut Reader, end: u64, ctx: &FieldContext<'_>, what: &str) -> Result<String> {
    let raw = reader.read_raw_name_within(end, what)?;
    Ok(ctx.package.names.resolve_raw(raw))
}

fn read_object(reader: &mut Reader, end: u64, ctx: &FieldContext<'_>) -> Result<Option<String>> {
    let index = reader.read_i32_within(end, "field object reference")?;
    Ok((index != 0).then(|| ctx.package.resolve_full_name(index)))
}

/// One step of a concrete property class's `Serialize` override, in order.
enum FieldStep {
    /// A `UObject*` written as an `FPackageIndex`.
    Object,
    /// An `FName` (used by `FFieldPathProperty`'s `FFieldClass*`).
    Name,
    /// Fixed-width payload with no reference in it.
    Bytes(u64),
    /// A `bool`, which an archive writes as a 32-bit legacy `UBOOL`.
    Bool32,
    /// A nested `SerializeSingleField`.
    Field,
}

/// The override chain for each registered `FFieldClass`, base class first.
///
/// Only classes whose layout is established by UE source appear here; the
/// numeric, string, name and text properties add nothing beyond `FProperty`.
fn field_layout(type_name: &str) -> Option<&'static [FieldStep]> {
    use FieldStep::{Bool32, Bytes, Field, Name, Object};
    const NONE: &[FieldStep] = &[];
    const OBJECT: &[FieldStep] = &[Object];
    const OBJECT_META: &[FieldStep] = &[Object, Object];
    const VERSE_CLASS: &[FieldStep] = &[Object, Object, Bool32, Bool32];
    // FieldSize, ByteOffset, ByteMask, FieldMask, BoolSize, NativeBool.
    const BOOL: &[FieldStep] = &[Bytes(6)];
    const ENUM: &[FieldStep] = &[Object, Field];
    const ONE_FIELD: &[FieldStep] = &[Field];
    const TWO_FIELDS: &[FieldStep] = &[Field, Field];
    const FIELD_PATH: &[FieldStep] = &[Name];

    Some(match type_name {
        "Property" | "NumericProperty" | "Int8Property" | "Int16Property" | "IntProperty"
        | "Int64Property" | "UInt16Property" | "UInt32Property" | "UInt64Property"
        | "FloatProperty" | "DoubleProperty" | "NameProperty" | "StrProperty" | "TextProperty" => {
            NONE
        }
        "BoolProperty" => BOOL,
        "ByteProperty" => OBJECT,
        "EnumProperty" => ENUM,
        "StructProperty" => OBJECT,
        "ArrayProperty" | "SetProperty" | "OptionalProperty" => ONE_FIELD,
        "MapProperty" => TWO_FIELDS,
        "DelegateProperty"
        | "MulticastDelegateProperty"
        | "MulticastInlineDelegateProperty"
        | "MulticastSparseDelegateProperty" => OBJECT,
        "InterfaceProperty" => OBJECT,
        "FieldPathProperty" => FIELD_PATH,
        "ObjectPropertyBase" | "ObjectProperty" | "WeakObjectProperty" | "LazyObjectProperty"
        | "SoftObjectProperty" => OBJECT,
        "ClassProperty" | "SoftClassProperty" => OBJECT_META,
        "VerseClassProperty" => VERSE_CLASS,
        _ => return None,
    })
}
