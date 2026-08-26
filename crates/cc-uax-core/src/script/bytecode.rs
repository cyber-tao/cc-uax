//! Kismet script bytecode disassembly.
//!
//! `UStruct::SerializeExpr` — spelled out in `UObject/ScriptSerialization.inl` —
//! is the authority for this format, and the rules below mirror it expression by
//! expression.
//!
//! Two sizes bracket the stream and they are not the same number.
//! `BytecodeBufferSize` counts the *in-memory* buffer the VM executes, where a
//! pointer occupies `sizeof(ScriptPointerType)` (8) and a name occupies
//! `sizeof(FScriptName)` (12). `SerializedScriptSize` counts the *on-disk* bytes,
//! where the same pointer is a 4-byte `FPackageIndex` and the same name is an
//! 8-byte linker `FName`. Both are tracked here, because agreeing with both is a
//! far stronger statement than consuming the right number of file bytes alone.
//!
//! Across UE5.0–5.8 the only changes to this format were new opcodes; every
//! opcode that already existed kept its encoding. The one layout gate a reader
//! must honour is `LARGE_WORLD_COORDINATES`, which widens the vector, rotation
//! and transform constants.

use crate::package::Package;
use crate::reader::{RAW_NAME_BYTES, Reader};
use crate::version::{custom, ue5};
use anyhow::{Result, bail};
use std::collections::{BTreeMap, BTreeSet};

/// `sizeof(ScriptPointerType)` (`ObjectMacros.h`): the in-memory width of every
/// pointer the bytecode stores, regardless of how it is serialized.
const SCRIPT_POINTER_ICODE_BYTES: u64 = 8;

/// `sizeof(FScriptName)`: two `FNameEntryId`s plus the number.
const SCRIPT_NAME_ICODE_BYTES: u64 = 12;

/// On-disk width of an `FPackageIndex`, which is how a linker archive writes any
/// `UObject*` the bytecode references.
const PACKAGE_INDEX_BYTES: u64 = 4;

/// Expression nesting is shallow in compiled Blueprints; this only exists so a
/// corrupt stream cannot recurse without bound.
const MAX_EXPR_DEPTH: u32 = 256;

/// What a bytecode reference points at, so a consumer can tell a called function
/// from a cast target or an asset constant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum ScriptRefKind {
    /// `EX_ObjectConst`: an asset or object literal.
    Object,
    /// A class literal: cast targets, interface conversions, metaclasses.
    Class,
    /// `EX_FinalFunction`/`EX_CallMath`/`EX_LocalFinalFunction`/delegate calls.
    Function,
    /// `EX_StructConst`'s `UScriptStruct`.
    Struct,
    /// `EX_SoftObjectConst`: a path loaded at runtime, not a linker reference.
    SoftObject,
    /// A virtual call or delegate bound by name rather than by pointer.
    FunctionName,
}

impl ScriptRefKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Object => "object",
            Self::Class => "class",
            Self::Function => "function",
            Self::Struct => "struct",
            Self::SoftObject => "soft_object",
            Self::FunctionName => "function_name",
        }
    }
}

/// Everything one `Script` stream yields: how much of it was consumed, what it is
/// made of, and what it points at.
#[derive(Debug, Clone, Default)]
pub(crate) struct BytecodeSummary {
    pub(crate) expressions: usize,
    /// In-memory bytes the walk accounted for; compared against
    /// `BytecodeBufferSize` as an independent check on the disk-side total.
    pub(crate) icode: u64,
    pub(crate) opcodes: BTreeMap<&'static str, usize>,
    pub(crate) references: BTreeSet<(ScriptRefKind, String)>,
}

pub(crate) struct BytecodeContext<'a> {
    pub(crate) package: &'a Package,
    pub(crate) file_version_ue5: i32,
    /// `FReleaseObjectVersion`, which decides whether an `FFieldPath` carries its
    /// owner. Missing (`-1`) selects the pre-owner layout, as UE does.
    pub(crate) release_object_version: i32,
}

impl BytecodeContext<'_> {
    fn large_world_coordinates(&self) -> bool {
        self.file_version_ue5 >= ue5::LARGE_WORLD_COORDINATES
    }

    fn field_path_has_owner(&self) -> bool {
        self.release_object_version >= custom::RELEASE_FIELD_PATH_OWNER_SERIALIZATION
    }
}

/// Disassemble one `Script` region, consuming exactly `[start, end)`.
pub(crate) fn disassemble(
    reader: &mut Reader,
    end: u64,
    ctx: &BytecodeContext<'_>,
) -> Result<BytecodeSummary> {
    let mut summary = BytecodeSummary::default();
    while reader.pos() < end {
        read_expr(reader, end, ctx, &mut summary, 0)?;
    }
    if reader.pos() != end {
        bail!(
            "script stream overran its declared size by {} byte(s)",
            reader.pos().saturating_sub(end)
        );
    }
    Ok(summary)
}

/// One expression. Returns its opcode so the callers that loop until a terminator
/// (`EX_EndFunctionParms` and friends) can see it, mirroring `SerializeExpr`'s
/// return value.
fn read_expr(
    reader: &mut Reader,
    end: u64,
    ctx: &BytecodeContext<'_>,
    summary: &mut BytecodeSummary,
    depth: u32,
) -> Result<Expr> {
    if depth > MAX_EXPR_DEPTH {
        bail!("script expression nesting exceeded {MAX_EXPR_DEPTH}");
    }
    let token = xfer_u8(reader, end, ctx, summary)?;
    summary.expressions += 1;
    *summary.opcodes.entry(opcode_name(token)).or_default() += 1;
    let mut text = None;

    match token {
        // XFER(uint8) for the conversion kind, then the value.
        EX_CAST => {
            xfer_u8(reader, end, ctx, summary)?;
            read_expr(reader, end, ctx, summary, depth + 1)?;
        }
        EX_OBJ_TO_INTERFACE_CAST
        | EX_CROSS_INTERFACE_CAST
        | EX_INTERFACE_TO_OBJ_CAST
        | EX_META_CAST
        | EX_DYNAMIC_CAST => {
            xfer_object(reader, end, ctx, summary, ScriptRefKind::Class)?;
            read_expr(reader, end, ctx, summary, depth + 1)?;
        }
        // EX_Let falls through to the two-expression cases after its property.
        EX_LET => {
            xfer_field_path(reader, end, ctx, summary)?;
            read_expr(reader, end, ctx, summary, depth + 1)?;
            read_expr(reader, end, ctx, summary, depth + 1)?;
        }
        EX_LET_OBJ
        | EX_LET_WEAK_OBJ_PTR
        | EX_LET_BOOL
        | EX_LET_DELEGATE
        | EX_LET_MULTICAST_DELEGATE
        | EX_ADD_MULTICAST_DELEGATE
        | EX_REMOVE_MULTICAST_DELEGATE
        | EX_ARRAY_GET_BY_REF => {
            read_expr(reader, end, ctx, summary, depth + 1)?;
            read_expr(reader, end, ctx, summary, depth + 1)?;
        }
        EX_LET_VALUE_ON_PERSISTENT_FRAME | EX_STRUCT_MEMBER_CONTEXT => {
            xfer_field_path(reader, end, ctx, summary)?;
            read_expr(reader, end, ctx, summary, depth + 1)?;
        }
        EX_JUMP | EX_PUSH_EXECUTION_FLOW | EX_SKIP_OFFSET_CONST => {
            xfer_code_skip(reader, end, ctx, summary)?;
        }
        EX_COMPUTED_JUMP
        | EX_INTERFACE_CONTEXT
        | EX_RETURN
        | EX_CLEAR_MULTICAST_DELEGATE
        | EX_POP_EXECUTION_FLOW_IF_NOT
        | EX_FIELD_PATH_CONST
        | EX_AUTO_RTFM_ABORT_IF_NOT => {
            read_expr(reader, end, ctx, summary, depth + 1)?;
        }
        EX_LOCAL_VARIABLE
        | EX_INSTANCE_VARIABLE
        | EX_DEFAULT_VARIABLE
        | EX_LOCAL_OUT_VARIABLE
        | EX_CLASS_SPARSE_DATA_VARIABLE
        | EX_PROPERTY_CONST => {
            xfer_field_path(reader, end, ctx, summary)?;
        }
        EX_NOTHING_INT32 | EX_INT_CONST => {
            xfer_bytes(reader, end, ctx, summary, 4, 4)?;
        }
        // Zero-payload opcodes. EX_Breakpoint is rewritten to EX_Tracepoint on
        // load and carries nothing either way.
        EX_NOTHING
        | EX_END_OF_SCRIPT
        | EX_END_FUNCTION_PARMS
        | EX_END_STRUCT_CONST
        | EX_END_ARRAY
        | EX_END_ARRAY_CONST
        | EX_END_SET
        | EX_END_MAP
        | EX_END_SET_CONST
        | EX_END_MAP_CONST
        | EX_INT_ZERO
        | EX_INT_ONE
        | EX_TRUE
        | EX_FALSE
        | EX_NO_OBJECT
        | EX_NO_INTERFACE
        | EX_SELF
        | EX_END_PARM_VALUE
        | EX_POP_EXECUTION_FLOW
        | EX_DEPRECATED_OP_4A
        | EX_WIRE_TRACEPOINT
        | EX_TRACEPOINT
        | EX_BREAKPOINT
        | EX_AUTO_RTFM_ABORT => {}
        // Reads its argument straight out of the in-memory buffer without going
        // through the archive, so it contributes no serialized bytes at all. The
        // iCode advance depends on a byte that is not on disk, which is why the
        // caller stops trusting the iCode total once this appears.
        EX_INSTRUMENTATION_EVENT => {
            bail!("EX_InstrumentationEvent carries no serialized payload to resynchronize from");
        }
        EX_CALL_MATH | EX_LOCAL_FINAL_FUNCTION | EX_FINAL_FUNCTION | EX_CALL_MULTICAST_DELEGATE => {
            xfer_object(reader, end, ctx, summary, ScriptRefKind::Function)?;
            read_until(reader, end, ctx, summary, depth, EX_END_FUNCTION_PARMS)?;
        }
        EX_LOCAL_VIRTUAL_FUNCTION | EX_VIRTUAL_FUNCTION => {
            let name = xfer_name(reader, end, ctx, summary)?;
            summary
                .references
                .insert((ScriptRefKind::FunctionName, name));
            read_until(reader, end, ctx, summary, depth, EX_END_FUNCTION_PARMS)?;
        }
        EX_CLASS_CONTEXT | EX_CONTEXT | EX_CONTEXT_FAIL_SILENT => {
            read_expr(reader, end, ctx, summary, depth + 1)?;
            xfer_code_skip(reader, end, ctx, summary)?;
            xfer_field_path(reader, end, ctx, summary)?;
            read_expr(reader, end, ctx, summary, depth + 1)?;
        }
        EX_INT64_CONST | EX_UINT64_CONST | EX_DOUBLE_CONST => {
            xfer_bytes(reader, end, ctx, summary, 8, 8)?;
        }
        EX_FLOAT_CONST => {
            xfer_bytes(reader, end, ctx, summary, 4, 4)?;
        }
        EX_STRING_CONST => {
            text = Some(xfer_ansi_string(reader, end, ctx, summary)?);
        }
        EX_UNICODE_STRING_CONST => {
            text = Some(xfer_utf16_string(reader, end, ctx, summary)?);
        }
        EX_TEXT_CONST => {
            xfer_text(reader, end, ctx, summary, depth)?;
        }
        EX_OBJECT_CONST => {
            xfer_object(reader, end, ctx, summary, ScriptRefKind::Object)?;
        }
        // On load this is simply the nested string-literal expression; the
        // reference-collector branch in UE only runs while saving.
        EX_SOFT_OBJECT_CONST => {
            let inner = read_expr(reader, end, ctx, summary, depth + 1)?;
            if let Some(path) = inner.text.filter(|path| !path.is_empty()) {
                summary.references.insert((ScriptRefKind::SoftObject, path));
            }
        }
        EX_NAME_CONST => {
            xfer_name(reader, end, ctx, summary)?;
        }
        EX_ROTATION_CONST => {
            let width = if ctx.large_world_coordinates() { 8 } else { 4 };
            xfer_bytes(reader, end, ctx, summary, width * 3, width * 3)?;
        }
        EX_VECTOR_CONST => {
            let width = if ctx.large_world_coordinates() { 8 } else { 4 };
            xfer_bytes(reader, end, ctx, summary, width * 3, width * 3)?;
        }
        EX_VECTOR3F_CONST => {
            xfer_bytes(reader, end, ctx, summary, 12, 12)?;
        }
        EX_TRANSFORM_CONST => {
            // Rotation (4) + Translation (3) + Scale (3).
            let width = if ctx.large_world_coordinates() { 8 } else { 4 };
            xfer_bytes(reader, end, ctx, summary, width * 10, width * 10)?;
        }
        EX_STRUCT_CONST => {
            xfer_object(reader, end, ctx, summary, ScriptRefKind::Struct)?;
            xfer_bytes(reader, end, ctx, summary, 4, 4)?;
            read_until(reader, end, ctx, summary, depth, EX_END_STRUCT_CONST)?;
        }
        // The pre-`CHANGE_SETARRAY_BYTECODE` layout wrote the inner property
        // instead of the target expression, but that predates UE5 entirely.
        EX_SET_ARRAY => {
            read_expr(reader, end, ctx, summary, depth + 1)?;
            read_until(reader, end, ctx, summary, depth, EX_END_ARRAY)?;
        }
        EX_SET_SET => {
            read_expr(reader, end, ctx, summary, depth + 1)?;
            xfer_bytes(reader, end, ctx, summary, 4, 4)?;
            read_until(reader, end, ctx, summary, depth, EX_END_SET)?;
        }
        EX_SET_MAP => {
            read_expr(reader, end, ctx, summary, depth + 1)?;
            xfer_bytes(reader, end, ctx, summary, 4, 4)?;
            read_until(reader, end, ctx, summary, depth, EX_END_MAP)?;
        }
        EX_ARRAY_CONST => {
            xfer_field_path(reader, end, ctx, summary)?;
            xfer_bytes(reader, end, ctx, summary, 4, 4)?;
            read_until(reader, end, ctx, summary, depth, EX_END_ARRAY_CONST)?;
        }
        EX_SET_CONST => {
            xfer_field_path(reader, end, ctx, summary)?;
            xfer_bytes(reader, end, ctx, summary, 4, 4)?;
            read_until(reader, end, ctx, summary, depth, EX_END_SET_CONST)?;
        }
        EX_MAP_CONST => {
            xfer_field_path(reader, end, ctx, summary)?;
            xfer_field_path(reader, end, ctx, summary)?;
            xfer_bytes(reader, end, ctx, summary, 4, 4)?;
            read_until(reader, end, ctx, summary, depth, EX_END_MAP_CONST)?;
        }
        EX_BIT_FIELD_CONST => {
            xfer_field_path(reader, end, ctx, summary)?;
            xfer_u8(reader, end, ctx, summary)?;
        }
        EX_BYTE_CONST | EX_INT_CONST_BYTE => {
            xfer_u8(reader, end, ctx, summary)?;
        }
        EX_JUMP_IF_NOT | EX_SKIP => {
            xfer_code_skip(reader, end, ctx, summary)?;
            read_expr(reader, end, ctx, summary, depth + 1)?;
        }
        EX_ASSERT => {
            xfer_bytes(reader, end, ctx, summary, 2, 2)?;
            xfer_u8(reader, end, ctx, summary)?;
            read_expr(reader, end, ctx, summary, depth + 1)?;
        }
        EX_INSTANCE_DELEGATE => {
            let name = xfer_name(reader, end, ctx, summary)?;
            summary
                .references
                .insert((ScriptRefKind::FunctionName, name));
        }
        EX_BIND_DELEGATE => {
            let name = xfer_name(reader, end, ctx, summary)?;
            summary
                .references
                .insert((ScriptRefKind::FunctionName, name));
            read_expr(reader, end, ctx, summary, depth + 1)?;
            read_expr(reader, end, ctx, summary, depth + 1)?;
        }
        EX_SWITCH_VALUE => {
            let cases = read_u16_tracked(reader, end, ctx, summary)?;
            xfer_code_skip(reader, end, ctx, summary)?;
            read_expr(reader, end, ctx, summary, depth + 1)?;
            for _ in 0..cases {
                read_expr(reader, end, ctx, summary, depth + 1)?;
                xfer_code_skip(reader, end, ctx, summary)?;
                read_expr(reader, end, ctx, summary, depth + 1)?;
            }
            read_expr(reader, end, ctx, summary, depth + 1)?;
        }
        EX_AUTO_RTFM_TRANSACT => {
            xfer_bytes(reader, end, ctx, summary, 4, 4)?;
            xfer_code_skip(reader, end, ctx, summary)?;
            read_until(reader, end, ctx, summary, depth, EX_AUTO_RTFM_STOP_TRANSACT)?;
        }
        EX_AUTO_RTFM_STOP_TRANSACT => {
            xfer_bytes(reader, end, ctx, summary, 4, 4)?;
            xfer_u8(reader, end, ctx, summary)?;
        }
        // UE logs and carries on, but it cannot: an unknown opcode has an unknown
        // payload, so the rest of the stream is no longer addressable.
        other => bail!("unknown script opcode 0x{other:02X}"),
    }

    Ok(Expr { token, text })
}

struct Expr {
    token: u8,
    text: Option<String>,
}

/// `while (SerializeExpr(...) != Terminator)`.
fn read_until(
    reader: &mut Reader,
    end: u64,
    ctx: &BytecodeContext<'_>,
    summary: &mut BytecodeSummary,
    depth: u32,
    terminator: u8,
) -> Result<()> {
    loop {
        if reader.pos() >= end {
            bail!(
                "script stream ended before opcode 0x{terminator:02X} ({})",
                opcode_name(terminator)
            );
        }
        if read_expr(reader, end, ctx, summary, depth + 1)?.token == terminator {
            return Ok(());
        }
    }
}

fn xfer_bytes(
    reader: &mut Reader,
    end: u64,
    _ctx: &BytecodeContext<'_>,
    summary: &mut BytecodeSummary,
    disk: u64,
    icode: u64,
) -> Result<()> {
    reader.ensure_within(end, disk, "script expression payload")?;
    reader.skip(disk)?;
    summary.icode += icode;
    Ok(())
}

fn xfer_u8(
    reader: &mut Reader,
    end: u64,
    _ctx: &BytecodeContext<'_>,
    summary: &mut BytecodeSummary,
) -> Result<u8> {
    let value = reader.read_u8_within(end, "script opcode")?;
    summary.icode += 1;
    Ok(value)
}

fn read_u16_tracked(
    reader: &mut Reader,
    end: u64,
    _ctx: &BytecodeContext<'_>,
    summary: &mut BytecodeSummary,
) -> Result<u16> {
    reader.ensure_within(end, 2, "script uint16")?;
    let value = reader.read_u16()?;
    summary.icode += 2;
    Ok(value)
}

/// `XFER(CodeSkipSizeType)`. `SCRIPT_LIMIT_BYTECODE_TO_64KB` is 0 in every
/// shipped configuration, making this a `uint32`.
fn xfer_code_skip(
    reader: &mut Reader,
    end: u64,
    ctx: &BytecodeContext<'_>,
    summary: &mut BytecodeSummary,
) -> Result<()> {
    xfer_bytes(reader, end, ctx, summary, 4, 4)
}

/// `XFERNAME`: an 8-byte linker `FName` on disk standing in for a 12-byte
/// `FScriptName` in the executed buffer.
fn xfer_name(
    reader: &mut Reader,
    end: u64,
    ctx: &BytecodeContext<'_>,
    summary: &mut BytecodeSummary,
) -> Result<String> {
    reader.ensure_within(end, RAW_NAME_BYTES, "script name")?;
    let raw = reader.read_raw_name()?;
    summary.icode += SCRIPT_NAME_ICODE_BYTES;
    Ok(ctx.package.names.resolve_raw(raw))
}

/// `XFERPTR`/`XFERTOBJPTR` for a `UObject`-derived pointer: an `FPackageIndex`.
fn xfer_object(
    reader: &mut Reader,
    end: u64,
    ctx: &BytecodeContext<'_>,
    summary: &mut BytecodeSummary,
    kind: ScriptRefKind,
) -> Result<()> {
    reader.ensure_within(end, PACKAGE_INDEX_BYTES, "script object reference")?;
    let index = reader.read_i32()?;
    summary.icode += SCRIPT_POINTER_ICODE_BYTES;
    if index != 0 {
        summary
            .references
            .insert((kind, ctx.package.resolve_full_name(index)));
    }
    Ok(())
}

/// `XFERPTR(FProperty*)`/`XFERPTR(FField*)`. A field is not a `UObject`, so
/// `FPropertyProxyArchive` writes it as an `FFieldPath`: the path names, then the
/// owning struct once `FReleaseObjectVersion::FFieldPathOwnerSerialization` is in
/// effect.
fn xfer_field_path(
    reader: &mut Reader,
    end: u64,
    ctx: &BytecodeContext<'_>,
    summary: &mut BytecodeSummary,
) -> Result<()> {
    let count = reader.read_i32_within(end, "script field path length")?;
    if count < 0 {
        bail!("script field path length out of range: {count}");
    }
    let names = count as u64;
    reader.ensure_within(
        end,
        names.saturating_mul(RAW_NAME_BYTES),
        "script field path",
    )?;
    for _ in 0..names {
        reader.read_raw_name()?;
    }
    if ctx.field_path_has_owner() {
        reader.ensure_within(end, PACKAGE_INDEX_BYTES, "script field path owner")?;
        reader.read_i32()?;
    }
    summary.icode += SCRIPT_POINTER_ICODE_BYTES;
    Ok(())
}

/// `XFERSTRING`: bytes up to and including the terminator.
fn xfer_ansi_string(
    reader: &mut Reader,
    end: u64,
    _ctx: &BytecodeContext<'_>,
    summary: &mut BytecodeSummary,
) -> Result<String> {
    let mut bytes = Vec::new();
    loop {
        let byte = reader.read_u8_within(end, "script string constant")?;
        summary.icode += 1;
        if byte == 0 {
            break;
        }
        bytes.push(byte);
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// `XFERUNICODESTRING`: UTF-16 code units up to and including the terminator.
fn xfer_utf16_string(
    reader: &mut Reader,
    end: u64,
    _ctx: &BytecodeContext<'_>,
    summary: &mut BytecodeSummary,
) -> Result<String> {
    let mut units = Vec::new();
    loop {
        reader.ensure_within(end, 2, "script unicode string constant")?;
        let unit = reader.read_u16()?;
        summary.icode += 2;
        if unit == 0 {
            break;
        }
        units.push(unit);
    }
    Ok(String::from_utf16_lossy(&units))
}

/// `XFERTEXT`: a literal-kind byte selecting how many sub-expressions follow.
/// `LocalizedTextWithNotes` deliberately falls through to `LocalizedText` in UE,
/// giving it one extra leading expression.
fn xfer_text(
    reader: &mut Reader,
    end: u64,
    ctx: &BytecodeContext<'_>,
    summary: &mut BytecodeSummary,
    depth: u32,
) -> Result<()> {
    let literal = xfer_u8(reader, end, ctx, summary)?;
    let expressions = match literal {
        TEXT_LITERAL_EMPTY => 0,
        TEXT_LITERAL_LOCALIZED_TEXT => 3,
        TEXT_LITERAL_LOCALIZED_TEXT_WITH_NOTES => 4,
        TEXT_LITERAL_INVARIANT_TEXT | TEXT_LITERAL_LITERAL_STRING => 1,
        TEXT_LITERAL_STRING_TABLE_ENTRY => {
            xfer_object(reader, end, ctx, summary, ScriptRefKind::Object)?;
            2
        }
        other => bail!("unknown EBlueprintTextLiteralType {other}"),
    };
    for _ in 0..expressions {
        read_expr(reader, end, ctx, summary, depth + 1)?;
    }
    Ok(())
}

// EBlueprintTextLiteralType (Script.h).
const TEXT_LITERAL_EMPTY: u8 = 0;
const TEXT_LITERAL_LOCALIZED_TEXT: u8 = 1;
const TEXT_LITERAL_LOCALIZED_TEXT_WITH_NOTES: u8 = 2;
const TEXT_LITERAL_INVARIANT_TEXT: u8 = 3;
const TEXT_LITERAL_LITERAL_STRING: u8 = 4;
const TEXT_LITERAL_STRING_TABLE_ENTRY: u8 = 5;

// EExprToken (Script.h). Gaps in the numbering are unused opcode slots.
const EX_LOCAL_VARIABLE: u8 = 0x00;
const EX_INSTANCE_VARIABLE: u8 = 0x01;
const EX_DEFAULT_VARIABLE: u8 = 0x02;
const EX_RETURN: u8 = 0x04;
const EX_JUMP: u8 = 0x06;
const EX_JUMP_IF_NOT: u8 = 0x07;
const EX_ASSERT: u8 = 0x09;
const EX_NOTHING: u8 = 0x0B;
const EX_NOTHING_INT32: u8 = 0x0C;
const EX_LET: u8 = 0x0F;
const EX_BIT_FIELD_CONST: u8 = 0x11;
const EX_CLASS_CONTEXT: u8 = 0x12;
const EX_META_CAST: u8 = 0x13;
const EX_LET_BOOL: u8 = 0x14;
const EX_END_PARM_VALUE: u8 = 0x15;
const EX_END_FUNCTION_PARMS: u8 = 0x16;
const EX_SELF: u8 = 0x17;
const EX_SKIP: u8 = 0x18;
const EX_CONTEXT: u8 = 0x19;
const EX_CONTEXT_FAIL_SILENT: u8 = 0x1A;
const EX_VIRTUAL_FUNCTION: u8 = 0x1B;
const EX_FINAL_FUNCTION: u8 = 0x1C;
const EX_INT_CONST: u8 = 0x1D;
const EX_FLOAT_CONST: u8 = 0x1E;
const EX_STRING_CONST: u8 = 0x1F;
const EX_OBJECT_CONST: u8 = 0x20;
const EX_NAME_CONST: u8 = 0x21;
const EX_ROTATION_CONST: u8 = 0x22;
const EX_VECTOR_CONST: u8 = 0x23;
const EX_BYTE_CONST: u8 = 0x24;
const EX_INT_ZERO: u8 = 0x25;
const EX_INT_ONE: u8 = 0x26;
const EX_TRUE: u8 = 0x27;
const EX_FALSE: u8 = 0x28;
const EX_TEXT_CONST: u8 = 0x29;
const EX_NO_OBJECT: u8 = 0x2A;
const EX_TRANSFORM_CONST: u8 = 0x2B;
const EX_INT_CONST_BYTE: u8 = 0x2C;
const EX_NO_INTERFACE: u8 = 0x2D;
const EX_DYNAMIC_CAST: u8 = 0x2E;
const EX_STRUCT_CONST: u8 = 0x2F;
const EX_END_STRUCT_CONST: u8 = 0x30;
const EX_SET_ARRAY: u8 = 0x31;
const EX_END_ARRAY: u8 = 0x32;
const EX_PROPERTY_CONST: u8 = 0x33;
const EX_UNICODE_STRING_CONST: u8 = 0x34;
const EX_INT64_CONST: u8 = 0x35;
const EX_UINT64_CONST: u8 = 0x36;
const EX_DOUBLE_CONST: u8 = 0x37;
const EX_CAST: u8 = 0x38;
const EX_SET_SET: u8 = 0x39;
const EX_END_SET: u8 = 0x3A;
const EX_SET_MAP: u8 = 0x3B;
const EX_END_MAP: u8 = 0x3C;
const EX_SET_CONST: u8 = 0x3D;
const EX_END_SET_CONST: u8 = 0x3E;
const EX_MAP_CONST: u8 = 0x3F;
const EX_END_MAP_CONST: u8 = 0x40;
const EX_VECTOR3F_CONST: u8 = 0x41;
const EX_STRUCT_MEMBER_CONTEXT: u8 = 0x42;
const EX_LET_MULTICAST_DELEGATE: u8 = 0x43;
const EX_LET_DELEGATE: u8 = 0x44;
const EX_LOCAL_VIRTUAL_FUNCTION: u8 = 0x45;
const EX_LOCAL_FINAL_FUNCTION: u8 = 0x46;
const EX_LOCAL_OUT_VARIABLE: u8 = 0x48;
const EX_DEPRECATED_OP_4A: u8 = 0x4A;
const EX_INSTANCE_DELEGATE: u8 = 0x4B;
const EX_PUSH_EXECUTION_FLOW: u8 = 0x4C;
const EX_POP_EXECUTION_FLOW: u8 = 0x4D;
const EX_COMPUTED_JUMP: u8 = 0x4E;
const EX_POP_EXECUTION_FLOW_IF_NOT: u8 = 0x4F;
const EX_BREAKPOINT: u8 = 0x50;
const EX_INTERFACE_CONTEXT: u8 = 0x51;
const EX_OBJ_TO_INTERFACE_CAST: u8 = 0x52;
const EX_END_OF_SCRIPT: u8 = 0x53;
const EX_CROSS_INTERFACE_CAST: u8 = 0x54;
const EX_INTERFACE_TO_OBJ_CAST: u8 = 0x55;
const EX_WIRE_TRACEPOINT: u8 = 0x5A;
const EX_SKIP_OFFSET_CONST: u8 = 0x5B;
const EX_ADD_MULTICAST_DELEGATE: u8 = 0x5C;
const EX_CLEAR_MULTICAST_DELEGATE: u8 = 0x5D;
const EX_TRACEPOINT: u8 = 0x5E;
const EX_LET_OBJ: u8 = 0x5F;
const EX_LET_WEAK_OBJ_PTR: u8 = 0x60;
const EX_BIND_DELEGATE: u8 = 0x61;
const EX_REMOVE_MULTICAST_DELEGATE: u8 = 0x62;
const EX_CALL_MULTICAST_DELEGATE: u8 = 0x63;
const EX_LET_VALUE_ON_PERSISTENT_FRAME: u8 = 0x64;
const EX_ARRAY_CONST: u8 = 0x65;
const EX_END_ARRAY_CONST: u8 = 0x66;
const EX_SOFT_OBJECT_CONST: u8 = 0x67;
const EX_CALL_MATH: u8 = 0x68;
const EX_SWITCH_VALUE: u8 = 0x69;
const EX_INSTRUMENTATION_EVENT: u8 = 0x6A;
const EX_ARRAY_GET_BY_REF: u8 = 0x6B;
const EX_CLASS_SPARSE_DATA_VARIABLE: u8 = 0x6C;
const EX_FIELD_PATH_CONST: u8 = 0x6D;
const EX_AUTO_RTFM_TRANSACT: u8 = 0x70;
const EX_AUTO_RTFM_STOP_TRANSACT: u8 = 0x71;
const EX_AUTO_RTFM_ABORT_IF_NOT: u8 = 0x72;
const EX_AUTO_RTFM_ABORT: u8 = 0x73;

pub(crate) fn opcode_name(token: u8) -> &'static str {
    match token {
        EX_LOCAL_VARIABLE => "EX_LocalVariable",
        EX_INSTANCE_VARIABLE => "EX_InstanceVariable",
        EX_DEFAULT_VARIABLE => "EX_DefaultVariable",
        EX_RETURN => "EX_Return",
        EX_JUMP => "EX_Jump",
        EX_JUMP_IF_NOT => "EX_JumpIfNot",
        EX_ASSERT => "EX_Assert",
        EX_NOTHING => "EX_Nothing",
        EX_NOTHING_INT32 => "EX_NothingInt32",
        EX_LET => "EX_Let",
        EX_BIT_FIELD_CONST => "EX_BitFieldConst",
        EX_CLASS_CONTEXT => "EX_ClassContext",
        EX_META_CAST => "EX_MetaCast",
        EX_LET_BOOL => "EX_LetBool",
        EX_END_PARM_VALUE => "EX_EndParmValue",
        EX_END_FUNCTION_PARMS => "EX_EndFunctionParms",
        EX_SELF => "EX_Self",
        EX_SKIP => "EX_Skip",
        EX_CONTEXT => "EX_Context",
        EX_CONTEXT_FAIL_SILENT => "EX_Context_FailSilent",
        EX_VIRTUAL_FUNCTION => "EX_VirtualFunction",
        EX_FINAL_FUNCTION => "EX_FinalFunction",
        EX_INT_CONST => "EX_IntConst",
        EX_FLOAT_CONST => "EX_FloatConst",
        EX_STRING_CONST => "EX_StringConst",
        EX_OBJECT_CONST => "EX_ObjectConst",
        EX_NAME_CONST => "EX_NameConst",
        EX_ROTATION_CONST => "EX_RotationConst",
        EX_VECTOR_CONST => "EX_VectorConst",
        EX_BYTE_CONST => "EX_ByteConst",
        EX_INT_ZERO => "EX_IntZero",
        EX_INT_ONE => "EX_IntOne",
        EX_TRUE => "EX_True",
        EX_FALSE => "EX_False",
        EX_TEXT_CONST => "EX_TextConst",
        EX_NO_OBJECT => "EX_NoObject",
        EX_TRANSFORM_CONST => "EX_TransformConst",
        EX_INT_CONST_BYTE => "EX_IntConstByte",
        EX_NO_INTERFACE => "EX_NoInterface",
        EX_DYNAMIC_CAST => "EX_DynamicCast",
        EX_STRUCT_CONST => "EX_StructConst",
        EX_END_STRUCT_CONST => "EX_EndStructConst",
        EX_SET_ARRAY => "EX_SetArray",
        EX_END_ARRAY => "EX_EndArray",
        EX_PROPERTY_CONST => "EX_PropertyConst",
        EX_UNICODE_STRING_CONST => "EX_UnicodeStringConst",
        EX_INT64_CONST => "EX_Int64Const",
        EX_UINT64_CONST => "EX_UInt64Const",
        EX_DOUBLE_CONST => "EX_DoubleConst",
        EX_CAST => "EX_Cast",
        EX_SET_SET => "EX_SetSet",
        EX_END_SET => "EX_EndSet",
        EX_SET_MAP => "EX_SetMap",
        EX_END_MAP => "EX_EndMap",
        EX_SET_CONST => "EX_SetConst",
        EX_END_SET_CONST => "EX_EndSetConst",
        EX_MAP_CONST => "EX_MapConst",
        EX_END_MAP_CONST => "EX_EndMapConst",
        EX_VECTOR3F_CONST => "EX_Vector3fConst",
        EX_STRUCT_MEMBER_CONTEXT => "EX_StructMemberContext",
        EX_LET_MULTICAST_DELEGATE => "EX_LetMulticastDelegate",
        EX_LET_DELEGATE => "EX_LetDelegate",
        EX_LOCAL_VIRTUAL_FUNCTION => "EX_LocalVirtualFunction",
        EX_LOCAL_FINAL_FUNCTION => "EX_LocalFinalFunction",
        EX_LOCAL_OUT_VARIABLE => "EX_LocalOutVariable",
        EX_DEPRECATED_OP_4A => "EX_DeprecatedOp4A",
        EX_INSTANCE_DELEGATE => "EX_InstanceDelegate",
        EX_PUSH_EXECUTION_FLOW => "EX_PushExecutionFlow",
        EX_POP_EXECUTION_FLOW => "EX_PopExecutionFlow",
        EX_COMPUTED_JUMP => "EX_ComputedJump",
        EX_POP_EXECUTION_FLOW_IF_NOT => "EX_PopExecutionFlowIfNot",
        EX_BREAKPOINT => "EX_Breakpoint",
        EX_INTERFACE_CONTEXT => "EX_InterfaceContext",
        EX_OBJ_TO_INTERFACE_CAST => "EX_ObjToInterfaceCast",
        EX_END_OF_SCRIPT => "EX_EndOfScript",
        EX_CROSS_INTERFACE_CAST => "EX_CrossInterfaceCast",
        EX_INTERFACE_TO_OBJ_CAST => "EX_InterfaceToObjCast",
        EX_WIRE_TRACEPOINT => "EX_WireTracepoint",
        EX_SKIP_OFFSET_CONST => "EX_SkipOffsetConst",
        EX_ADD_MULTICAST_DELEGATE => "EX_AddMulticastDelegate",
        EX_CLEAR_MULTICAST_DELEGATE => "EX_ClearMulticastDelegate",
        EX_TRACEPOINT => "EX_Tracepoint",
        EX_LET_OBJ => "EX_LetObj",
        EX_LET_WEAK_OBJ_PTR => "EX_LetWeakObjPtr",
        EX_BIND_DELEGATE => "EX_BindDelegate",
        EX_REMOVE_MULTICAST_DELEGATE => "EX_RemoveMulticastDelegate",
        EX_CALL_MULTICAST_DELEGATE => "EX_CallMulticastDelegate",
        EX_LET_VALUE_ON_PERSISTENT_FRAME => "EX_LetValueOnPersistentFrame",
        EX_ARRAY_CONST => "EX_ArrayConst",
        EX_END_ARRAY_CONST => "EX_EndArrayConst",
        EX_SOFT_OBJECT_CONST => "EX_SoftObjectConst",
        EX_CALL_MATH => "EX_CallMath",
        EX_SWITCH_VALUE => "EX_SwitchValue",
        EX_INSTRUMENTATION_EVENT => "EX_InstrumentationEvent",
        EX_ARRAY_GET_BY_REF => "EX_ArrayGetByRef",
        EX_CLASS_SPARSE_DATA_VARIABLE => "EX_ClassSparseDataVariable",
        EX_FIELD_PATH_CONST => "EX_FieldPathConst",
        EX_AUTO_RTFM_TRANSACT => "EX_AutoRtfmTransact",
        EX_AUTO_RTFM_STOP_TRANSACT => "EX_AutoRtfmStopTransact",
        EX_AUTO_RTFM_ABORT_IF_NOT => "EX_AutoRtfmAbortIfNot",
        EX_AUTO_RTFM_ABORT => "EX_AutoRtfmAbort",
        _ => "EX_Unknown",
    }
}
