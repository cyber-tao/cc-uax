//! `UStruct`, `UFunction` and `UClass` serializer decoding.
//!
//! These classes write their own data after the tagged-property block, which is
//! why a compiled Blueprint always left an export tail behind. The order is fixed
//! by the `Super::Serialize` chain in `Class.cpp`: `UObject` (tagged properties
//! and the object GUID) runs first, then `UField`, then `UStruct`, then the
//! concrete class.
//!
//! `UStruct::Serialize` ends by writing two sizes and then the script itself.
//! They are not interchangeable: `BytecodeBufferSize` is the length of the buffer
//! the VM executes and `SerializedScriptSize` is the number of bytes on disk. The
//! second is what bounds the region here, and `FStructScriptLoader` reads it
//! exactly that way.

pub(crate) mod bytecode;
pub(crate) mod field;

use crate::package::Package;
use crate::reader::Reader;
use crate::version::custom;
use anyhow::{Result, bail};
use bytecode::{BytecodeContext, BytecodeSummary};
use field::{DecodedField, FieldContext, decode_property_list};

/// Whether a class writes compiled script bytecode through `UStruct::Serialize`.
/// Every Blueprint function and generated class does.
pub(crate) fn is_script_bytecode_class(class: &str) -> bool {
    let Some(simple) = class.rsplit(['.', '/']).next() else {
        return false;
    };
    matches!(
        simple,
        "Function" | "DelegateFunction" | "SparseDelegateFunction" | "Class"
    ) || simple.ends_with("GeneratedClass")
}

/// Whether the export is a `UFunction`, which appends its own fields after the
/// script. `UClass` appends a different block, handled as a named remainder.
fn is_function_class(class: &str) -> bool {
    let Some(simple) = class.rsplit(['.', '/']).next() else {
        return false;
    };
    matches!(
        simple,
        "Function" | "DelegateFunction" | "SparseDelegateFunction"
    )
}

#[derive(Debug, Clone)]
pub(crate) struct DecodedScriptStruct {
    pub(crate) super_struct: Option<String>,
    pub(crate) children: Vec<String>,
    pub(crate) properties: Vec<DecodedField>,
    pub(crate) bytecode: Option<DecodedBytecode>,
    pub(crate) function: Option<DecodedFunction>,
    pub(crate) class: Option<DecodedClass>,
    /// Offset the decode reached.
    pub(crate) end: u64,
}

#[derive(Debug, Clone)]
pub(crate) struct DecodedBytecode {
    /// `BytecodeBufferSize`: the in-memory length the VM executes.
    pub(crate) buffer_size: u32,
    /// `SerializedScriptSize`: the on-disk length, which bounds the region.
    pub(crate) serialized_size: u32,
    /// Present when the whole region disassembled.
    pub(crate) summary: Option<BytecodeSummary>,
    /// Why it did not, when it did not. The region is skipped exactly either way,
    /// because its length is declared, so a failure here costs the bytecode
    /// evidence and nothing else.
    pub(crate) failure: Option<String>,
}

impl DecodedBytecode {
    /// Whether the walk agreed with both declared sizes. The disk total is
    /// enforced by the bounded read; this adds the in-memory total, which is
    /// derived independently from each expression's pointer and name widths.
    pub(crate) fn sizes_agree(&self) -> bool {
        self.summary
            .as_ref()
            .is_some_and(|summary| summary.icode == u64::from(self.buffer_size))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DecodedFunction {
    pub(crate) flags: u32,
    pub(crate) event_graph_function: Option<String>,
    pub(crate) event_graph_call_offset: i32,
}

/// `UClass::Serialize` after the `UStruct` block.
#[derive(Debug, Clone)]
pub(crate) struct DecodedClass {
    /// `FuncMap`: the class's functions by name.
    pub(crate) functions: Vec<(String, String)>,
    pub(crate) class_flags: u32,
    pub(crate) class_within: Option<String>,
    pub(crate) class_config_name: String,
    /// The Blueprint asset this class was generated from.
    pub(crate) class_generated_by: Option<String>,
    pub(crate) interfaces: Vec<String>,
    pub(crate) default_object: Option<String>,
}

/// `EFunctionFlags::FUNC_Net`, which adds the replication offset.
const FUNC_NET: u32 = 0x0000_0040;

pub(crate) struct ScriptStructContext<'a> {
    pub(crate) package: &'a Package,
    pub(crate) filter_editor_only: bool,
    /// `FFrameworkObjectVersion`; below `RemoveUField_Next` both `UField::Next`
    /// and a single-pointer `Children` are still written.
    pub(crate) framework_version: i32,
    /// `FCoreObjectVersion`; `ChildProperties` exists from `FProperties` on.
    pub(crate) core_object_version: i32,
    pub(crate) release_object_version: i32,
    pub(crate) file_version_ue5: i32,
}

impl<'a> ScriptStructContext<'a> {
    pub(crate) fn new(package: &'a Package) -> Self {
        Self {
            package,
            filter_editor_only: package.summary.filter_editor_only(),
            framework_version: package
                .summary
                .custom_version(custom::FRAMEWORK_OBJECT_VERSION)
                .unwrap_or(-1),
            core_object_version: package
                .summary
                .custom_version(custom::CORE_OBJECT_VERSION)
                .unwrap_or(-1),
            release_object_version: package
                .summary
                .custom_version(custom::RELEASE_OBJECT_VERSION)
                .unwrap_or(-1),
            file_version_ue5: package.summary.file_version_ue5,
        }
    }

    fn keeps_ufield_next(&self) -> bool {
        self.framework_version < custom::FRAMEWORK_REMOVE_UFIELD_NEXT
    }

    fn has_child_properties(&self) -> bool {
        self.core_object_version >= custom::CORE_FPROPERTIES
    }
}

/// Decode the `UStruct` serializer starting at the reader's position, bounded by
/// `end` (the export's serial end).
pub(crate) fn decode_script_struct(
    reader: &mut Reader,
    end: u64,
    class_full: &str,
    ctx: &ScriptStructContext<'_>,
) -> Result<DecodedScriptStruct> {
    // UField::Serialize.
    if ctx.keeps_ufield_next() {
        reader.read_i32_within(end, "UField Next")?;
    }

    let super_struct = read_object(reader, end, ctx, "SuperStruct")?;

    let children = if ctx.keeps_ufield_next() {
        // The pre-RemoveUField_Next layout stores the head of a linked list.
        read_object(reader, end, ctx, "Children")?
            .into_iter()
            .collect()
    } else {
        let count = reader.read_i32_within(end, "Children count")?;
        if count < 0 {
            bail!("Children count out of range: {count}");
        }
        reader.ensure_within(end, (count as u64).saturating_mul(4), "Children")?;
        let mut children = Vec::with_capacity(count as usize);
        for _ in 0..count {
            if let Some(child) = read_object(reader, end, ctx, "Children entry")? {
                children.push(child);
            }
        }
        children
    };

    let properties = if ctx.has_child_properties() {
        let field_ctx = FieldContext {
            package: ctx.package,
            filter_editor_only: ctx.filter_editor_only,
        };
        decode_property_list(reader, end, &field_ctx)?
    } else {
        Vec::new()
    };

    let bytecode = decode_bytecode(reader, end, ctx)?;

    let function = if is_function_class(class_full) {
        Some(decode_function(reader, end, ctx)?)
    } else {
        None
    };
    let class = if function.is_none() {
        Some(decode_class(reader, end, ctx)?)
    } else {
        None
    };

    Ok(DecodedScriptStruct {
        super_struct,
        children,
        properties,
        bytecode,
        function,
        class,
        end: reader.pos(),
    })
}

fn decode_bytecode(
    reader: &mut Reader,
    end: u64,
    ctx: &ScriptStructContext<'_>,
) -> Result<Option<DecodedBytecode>> {
    let buffer_size = reader.read_i32_within(end, "BytecodeBufferSize")?;
    let serialized_size = reader.read_i32_within(end, "SerializedScriptSize")?;
    if buffer_size < 0 || serialized_size < 0 {
        bail!("script sizes out of range: buffer {buffer_size}, serialized {serialized_size}");
    }
    if buffer_size == 0 && serialized_size == 0 {
        return Ok(None);
    }
    let start = reader.pos();
    reader.ensure_within(end, serialized_size as u64, "script bytecode")?;
    let script_end = start + serialized_size as u64;

    let bytecode_ctx = BytecodeContext {
        package: ctx.package,
        file_version_ue5: ctx.file_version_ue5,
        release_object_version: ctx.release_object_version,
    };
    let (summary, failure) = match bytecode::disassemble(reader, script_end, &bytecode_ctx) {
        Ok(summary) => (Some(summary), None),
        Err(error) => (None, Some(format!("{error:#}"))),
    };
    // The region's length is declared, so a failed walk still leaves the stream
    // positioned correctly for whatever the class writes next.
    reader.seek(script_end)?;

    Ok(Some(DecodedBytecode {
        buffer_size: buffer_size as u32,
        serialized_size: serialized_size as u32,
        summary,
        failure,
    }))
}

/// `UFunction::Serialize` after `Super::Serialize`.
fn decode_function(
    reader: &mut Reader,
    end: u64,
    ctx: &ScriptStructContext<'_>,
) -> Result<DecodedFunction> {
    reader.ensure_within(end, 4, "FunctionFlags")?;
    let flags = reader.read_u32()?;
    if flags & FUNC_NET != 0 {
        reader.ensure_within(end, 2, "function RepOffset")?;
        reader.read_i16()?;
    }
    // Written unconditionally for UE5: the gate is a UE4 version far below the
    // supported floor, and UE serializes the pair even when the fast-call feature
    // is compiled out, to keep the stream in sync.
    let event_graph_function = read_object(reader, end, ctx, "EventGraphFunction")?;
    let event_graph_call_offset = reader.read_i32_within(end, "EventGraphCallOffset")?;
    Ok(DecodedFunction {
        flags,
        event_graph_function,
        event_graph_call_offset,
    })
}

/// `UClass::Serialize` after `Super::Serialize`.
///
/// The interface array's read-then-seek-back dance in UE only applies below
/// `VER_UE4_UCLASS_SERIALIZE_INTERFACES_AFTER_LINKING`, far below the supported
/// floor, so the fields are simply in file order here. `SparseClassDataStruct` is
/// serialized only for archives that are neither loading nor saving, so it never
/// reaches a package.
fn decode_class(
    reader: &mut Reader,
    end: u64,
    ctx: &ScriptStructContext<'_>,
) -> Result<DecodedClass> {
    // FuncMap: TMap<FName, TObjectPtr<UFunction>>.
    let count = reader.read_i32_within(end, "FuncMap count")?;
    if count < 0 {
        bail!("FuncMap count out of range: {count}");
    }
    reader.ensure_within(
        end,
        (count as u64).saturating_mul(FUNC_MAP_ENTRY_BYTES),
        "FuncMap",
    )?;
    let mut functions = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let raw = reader.read_raw_name()?;
        let name = ctx.package.names.resolve_raw(raw);
        let target = read_object(reader, end, ctx, "FuncMap entry")?.unwrap_or_default();
        functions.push((name, target));
    }

    reader.ensure_within(end, 4, "ClassFlags")?;
    let class_flags = reader.read_u32()?;
    let class_within = read_object(reader, end, ctx, "ClassWithin")?;
    let class_config_name = {
        let raw = reader.read_raw_name_within(end, "ClassConfigName")?;
        ctx.package.names.resolve_raw(raw)
    };
    let class_generated_by = read_object(reader, end, ctx, "ClassGeneratedBy")?;

    // TArray<FImplementedInterface>: class, pointer offset, bImplementedByK2.
    let interface_count = reader.read_i32_within(end, "Interfaces count")?;
    if interface_count < 0 {
        bail!("Interfaces count out of range: {interface_count}");
    }
    reader.ensure_within(
        end,
        (interface_count as u64).saturating_mul(IMPLEMENTED_INTERFACE_BYTES),
        "Interfaces",
    )?;
    let mut interfaces = Vec::with_capacity(interface_count as usize);
    for _ in 0..interface_count {
        let class = read_object(reader, end, ctx, "interface class")?;
        reader.read_i32()?;
        reader.read_u32()?;
        if let Some(class) = class {
            interfaces.push(class);
        }
    }

    reader.read_bool32_within(end, "bDeprecatedForceScriptOrder")?;
    reader.read_raw_name_within(end, "UClass reserved name")?;
    reader.read_bool32_within(end, "bCooked")?;
    let default_object = read_object(reader, end, ctx, "ClassDefaultObject")?;

    Ok(DecodedClass {
        functions,
        class_flags,
        class_within,
        class_config_name,
        class_generated_by,
        interfaces,
        default_object,
    })
}

/// `FName` key plus an `FPackageIndex` value.
const FUNC_MAP_ENTRY_BYTES: u64 = 12;
/// `FImplementedInterface`: class index, pointer offset, and a 32-bit legacy bool.
const IMPLEMENTED_INTERFACE_BYTES: u64 = 12;

fn read_object(
    reader: &mut Reader,
    end: u64,
    ctx: &ScriptStructContext<'_>,
    what: &str,
) -> Result<Option<String>> {
    let index = reader.read_i32_within(end, what)?;
    Ok((index != 0).then(|| ctx.package.resolve_full_name(index)))
}

/// Named flags for a decoded `UFunction`, so a consumer does not have to carry
/// UE's `EFunctionFlags` table to read the report.
pub(crate) fn function_flag_names(flags: u32) -> Vec<&'static str> {
    const FLAGS: [(u32, &str); 24] = [
        (0x0000_0001, "Final"),
        (0x0000_0002, "RequiredAPI"),
        (0x0000_0004, "BlueprintAuthorityOnly"),
        (0x0000_0008, "BlueprintCosmetic"),
        (0x0000_0040, "Net"),
        (0x0000_0080, "NetReliable"),
        (0x0000_0100, "NetRequest"),
        (0x0000_0200, "Exec"),
        (0x0000_0400, "Native"),
        (0x0000_0800, "Event"),
        (0x0000_1000, "NetResponse"),
        (0x0000_2000, "Static"),
        (0x0000_4000, "NetMulticast"),
        (0x0000_8000, "UbergraphFunction"),
        (0x0001_0000, "MulticastDelegate"),
        (0x0002_0000, "Public"),
        (0x0004_0000, "Private"),
        (0x0008_0000, "Protected"),
        (0x0010_0000, "Delegate"),
        (0x0020_0000, "NetServer"),
        (0x0040_0000, "HasOutParms"),
        (0x0080_0000, "HasDefaults"),
        (0x0100_0000, "NetClient"),
        (0x0200_0000, "DLLImport"),
    ];
    FLAGS
        .iter()
        .filter(|(bit, _)| flags & bit != 0)
        .map(|(_, name)| *name)
        .collect()
}
