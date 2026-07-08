use crate::diagnostic::Diagnostic;
use crate::property::{ParseCtx, parse_text};
use crate::reader::{Guid, Reader};
use crate::summary::PackageFileSummary;
use crate::version::custom;
use anyhow::{Result, bail};

const MAX_PIN_COUNT: i32 = 4096;
/// PinRef on disk: node PackageIndex (i32, 4 bytes) + pin Guid (16 bytes).
const PIN_REF_SIZE: u64 = 20;
const CONTAINER_TYPE_NONE: u8 = 0;
const CONTAINER_TYPE_ARRAY: u8 = 1;
const CONTAINER_TYPE_SET: u8 = 2;
const CONTAINER_TYPE_MAP: u8 = 3;
const PIN_DIRECTION_INPUT: u8 = 0;

#[derive(Clone, Copy, Default)]
pub struct PinSerCtx {
    pub filter_editor_only: bool,
    pub has_source_index: bool,
    pub has_uobject_wrapper: bool,
    pub has_single_precision_float: bool,
}

impl PinSerCtx {
    pub fn from_summary(s: &PackageFileSummary) -> Self {
        let main = s
            .custom_version(custom::UE5_MAIN_STREAM_OBJECT_VERSION)
            .unwrap_or(-1);
        let release = s
            .custom_version(custom::RELEASE_OBJECT_VERSION)
            .unwrap_or(-1);
        let ue5_release = s
            .custom_version(custom::UE5_RELEASE_STREAM_OBJECT_VERSION)
            .unwrap_or(-1);
        PinSerCtx {
            filter_editor_only: s.filter_editor_only(),
            has_source_index: main >= custom::EDGRAPH_PIN_SOURCE_INDEX,
            has_uobject_wrapper: release >= custom::PIN_TYPE_INCLUDES_UOBJECT_WRAPPER_FLAG,
            has_single_precision_float: ue5_release >= custom::SERIALIZE_FLOAT_PIN_SINGLE_PRECISION,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PinRef {
    pub node_index: i32,
    pub pin_id: Guid,
}

#[derive(Debug, Clone)]
pub struct PinTerminalType {
    pub category: String,
    pub sub_category: String,
    pub sub_category_object: i32,
    pub is_const: bool,
    pub is_weak_pointer: bool,
    pub is_uobject_wrapper: bool,
}

#[derive(Debug, Clone)]
pub struct PinType {
    pub category: String,
    pub sub_category: String,
    pub sub_category_object: i32,
    pub container_type: u8,
    pub value_type: Option<PinTerminalType>,
    pub is_reference: bool,
    pub is_weak_pointer: bool,
    pub member_parent: i32,
    pub member_name: String,
    pub member_guid: Guid,
    pub is_const: bool,
    pub is_uobject_wrapper: bool,
    pub serialize_as_single_precision_float: bool,
}

#[derive(Debug, Clone)]
pub struct PinEditorFlags {
    pub hidden: bool,
    pub not_connectable: bool,
    pub default_value_read_only: bool,
    pub default_value_ignored: bool,
    pub advanced_view: bool,
    pub orphaned_pin: bool,
}

#[derive(Debug, Clone)]
pub struct Pin {
    pub pin_id: Guid,
    pub name: String,
    pub direction: u8,
    pub category: String,
    pub sub_category: String,
    pub sub_category_object: i32,
    pub default_value: String,
    pub default_object: i32,
    pub linked_to: Vec<PinRef>,
    pub sub_pins: Vec<PinRef>,
    pub parent_pin: Option<PinRef>,
    pub container_type: u8,
    pub value_type: Option<PinTerminalType>,
    pub is_reference: bool,
    pub is_weak_pointer: bool,
    pub member_parent: i32,
    pub member_name: String,
    pub member_guid: Guid,
    pub is_const: bool,
    pub is_uobject_wrapper: bool,
    pub serialize_as_single_precision_float: bool,
    pub persistent_guid: Option<Guid>,
    pub editor_flags: Option<PinEditorFlags>,
    pub reference_pass_through: Option<PinRef>,
}

#[derive(Debug, Clone)]
pub struct PinParse {
    pub pins: Vec<Pin>,
}

pub fn direction_label(direction: u8) -> &'static str {
    if direction == PIN_DIRECTION_INPUT {
        "input"
    } else {
        "output"
    }
}

pub fn container_type_label(container_type: u8) -> &'static str {
    match container_type {
        CONTAINER_TYPE_NONE => "none",
        CONTAINER_TYPE_ARRAY => "array",
        CONTAINER_TYPE_SET => "set",
        CONTAINER_TYPE_MAP => "map",
        _ => "unknown",
    }
}

#[cfg(test)]
pub(crate) fn parse_node_pins(
    r: &mut Reader,
    end: u64,
    ctx: &ParseCtx,
    vc: &PinSerCtx,
) -> Option<Vec<Pin>> {
    parse_node_pins_report(r, end, ctx, vc, "/pins")
        .ok()
        .map(|parsed| parsed.pins)
}

pub fn parse_node_pins_report(
    r: &mut Reader,
    end: u64,
    ctx: &ParseCtx,
    vc: &PinSerCtx,
    path: &str,
) -> std::result::Result<PinParse, Diagnostic> {
    let start = r.pos();
    match parse_pins_inner(r, end, ctx, vc) {
        Ok(pins) if r.pos() <= end => Ok(PinParse { pins }),
        Ok(_) => {
            let _ = r.seek(start);
            Err(
                Diagnostic::warning("pin_region_overrun", path, "pin parser overran pin region")
                    .with_offset(start),
            )
        }
        Err(err) => {
            let failed_at = r.pos();
            let _ = r.seek(start);
            Err(Diagnostic::warning(
                "pin_parse_failed",
                path,
                format!("pin parser failed: {err:#}"),
            )
            .with_offset(failed_at))
        }
    }
}

fn parse_pins_inner(r: &mut Reader, end: u64, ctx: &ParseCtx, vc: &PinSerCtx) -> Result<Vec<Pin>> {
    let has_object_guid = r.read_bool32()?;
    if has_object_guid {
        let _object_guid = r.read_guid()?;
    }
    let count = r.read_i32()?;
    if !(0..=MAX_PIN_COUNT).contains(&count) {
        bail!("pin count out of range: {count}");
    }
    let mut pins = Vec::with_capacity(count as usize);
    for _ in 0..count {
        if r.read_bool32()? {
            bail!("owning pin entry is null");
        }
        let _wrapper_node = r.read_i32()?;
        let _wrapper_guid = r.read_guid()?;
        let pin = parse_pin(r, ctx, vc)?;
        pins.push(pin);
        if r.pos() > end {
            bail!("pin region overrun");
        }
    }
    Ok(pins)
}

fn parse_pin(r: &mut Reader, ctx: &ParseCtx, vc: &PinSerCtx) -> Result<Pin> {
    let _owning_node = r.read_i32()?;
    let pin_id = r.read_guid()?;
    let name = ctx.names.resolve_raw(r.read_raw_name()?);
    if !vc.filter_editor_only {
        let _friendly_name = parse_text(r, ctx.names, 0)?;
    }
    if vc.has_source_index {
        let _source_index = r.read_i32()?;
    }
    let _tooltip = r.read_fstring()?;
    let direction = r.read_u8()?;
    let pin_type = parse_pin_type(r, ctx, vc)?;
    let default_value = r.read_fstring()?;
    let _autogenerated_default = r.read_fstring()?;
    let default_object = r.read_i32()?;
    let _default_text = parse_text(r, ctx.names, 0)?;
    let linked_to = parse_pin_ref_array(r)?;
    let sub_pins = parse_pin_ref_array(r)?;
    let parent_pin = parse_pin_ref(r)?;
    let reference_pass_through = parse_pin_ref(r)?;
    let (persistent_guid, editor_flags) = if !vc.filter_editor_only {
        let guid = r.read_guid()?;
        let bitfield = r.read_u32()?;
        (
            Some(guid),
            Some(PinEditorFlags {
                hidden: bitfield & (1 << 0) != 0,
                not_connectable: bitfield & (1 << 1) != 0,
                default_value_read_only: bitfield & (1 << 2) != 0,
                default_value_ignored: bitfield & (1 << 3) != 0,
                advanced_view: bitfield & (1 << 4) != 0,
                orphaned_pin: bitfield & (1 << 5) != 0,
            }),
        )
    } else {
        (None, None)
    };
    Ok(Pin {
        pin_id,
        name,
        direction,
        category: pin_type.category,
        sub_category: pin_type.sub_category,
        sub_category_object: pin_type.sub_category_object,
        default_value,
        default_object,
        linked_to,
        sub_pins,
        parent_pin,
        container_type: pin_type.container_type,
        value_type: pin_type.value_type,
        is_reference: pin_type.is_reference,
        is_weak_pointer: pin_type.is_weak_pointer,
        member_parent: pin_type.member_parent,
        member_name: pin_type.member_name,
        member_guid: pin_type.member_guid,
        is_const: pin_type.is_const,
        is_uobject_wrapper: pin_type.is_uobject_wrapper,
        serialize_as_single_precision_float: pin_type.serialize_as_single_precision_float,
        persistent_guid,
        editor_flags,
        reference_pass_through,
    })
}

pub(crate) fn parse_pin_type(r: &mut Reader, ctx: &ParseCtx, vc: &PinSerCtx) -> Result<PinType> {
    let category = ctx.names.resolve_raw(r.read_raw_name()?);
    let sub_category = ctx.names.resolve_raw(r.read_raw_name()?);
    let sub_category_object = r.read_i32()?;
    let container_type = r.read_u8()?;
    let value_type = if container_type == CONTAINER_TYPE_MAP {
        Some(parse_terminal_type(r, ctx, vc)?)
    } else {
        None
    };
    let is_reference = r.read_bool32()?;
    let is_weak_pointer = r.read_bool32()?;
    let member_parent = r.read_i32()?;
    let member_name = ctx.names.resolve_raw(r.read_raw_name()?);
    let member_guid = r.read_guid()?;
    let is_const = r.read_bool32()?;
    let is_uobject_wrapper = if vc.has_uobject_wrapper {
        r.read_bool32()?
    } else {
        false
    };
    let serialize_as_single_precision_float = if vc.has_single_precision_float {
        r.read_bool32()?
    } else {
        false
    };
    Ok(PinType {
        category,
        sub_category,
        sub_category_object,
        container_type,
        value_type,
        is_reference,
        is_weak_pointer,
        member_parent,
        member_name,
        member_guid,
        is_const,
        is_uobject_wrapper,
        serialize_as_single_precision_float,
    })
}

fn parse_terminal_type(r: &mut Reader, ctx: &ParseCtx, vc: &PinSerCtx) -> Result<PinTerminalType> {
    let category = ctx.names.resolve_raw(r.read_raw_name()?);
    let sub_category = ctx.names.resolve_raw(r.read_raw_name()?);
    let sub_category_object = r.read_i32()?;
    let is_const = r.read_bool32()?;
    let is_weak_pointer = r.read_bool32()?;
    let is_uobject_wrapper = if vc.has_uobject_wrapper {
        r.read_bool32()?
    } else {
        false
    };
    Ok(PinTerminalType {
        category,
        sub_category,
        sub_category_object,
        is_const,
        is_weak_pointer,
        is_uobject_wrapper,
    })
}

fn parse_pin_ref_array(r: &mut Reader) -> Result<Vec<PinRef>> {
    let count = r.read_i32()?;
    if count < 0 || (count as u64).saturating_mul(PIN_REF_SIZE) > r.remaining() {
        bail!("pin reference count out of range: {count}");
    }
    let mut refs = Vec::new();
    for _ in 0..count {
        if let Some(pin_ref) = parse_pin_ref(r)? {
            refs.push(pin_ref);
        }
    }
    Ok(refs)
}

fn parse_pin_ref(r: &mut Reader) -> Result<Option<PinRef>> {
    if r.read_bool32()? {
        return Ok(None);
    }
    let node_index = r.read_i32()?;
    let pin_id = r.read_guid()?;
    Ok(Some(PinRef { node_index, pin_id }))
}
