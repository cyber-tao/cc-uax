use super::common::*;
use crate::name::NameMap;
use crate::object::{ObjectImport, PackageIndex};
use crate::package::Package;
use crate::reader::Reader;
use crate::script::bytecode::{BytecodeContext, ScriptRefKind, disassemble};
use crate::script::field::{FieldContext, decode_property_list};
use crate::script::{ScriptStructContext, decode_script_struct};
use crate::version::{custom, ue5};

// Opcodes exercised below, spelled out so a test reads like the stream it builds.
const EX_LOCAL_VARIABLE: u8 = 0x00;
const EX_RETURN: u8 = 0x04;
const EX_NOTHING: u8 = 0x0B;
const EX_END_FUNCTION_PARMS: u8 = 0x16;
const EX_FINAL_FUNCTION: u8 = 0x1C;
const EX_INT_CONST: u8 = 0x1D;
const EX_STRING_CONST: u8 = 0x1F;
const EX_OBJECT_CONST: u8 = 0x20;
const EX_NAME_CONST: u8 = 0x21;
const EX_VECTOR_CONST: u8 = 0x23;
const EX_UNICODE_STRING_CONST: u8 = 0x34;
const EX_END_OF_SCRIPT: u8 = 0x53;
const EX_SOFT_OBJECT_CONST: u8 = 0x67;
const EX_SWITCH_VALUE: u8 = 0x69;

fn names() -> NameMap {
    NameMap {
        names: vec![
            "None".into(),              // 0
            "MyFunc".into(),            // 1
            "Target".into(),            // 2
            "ObjectProperty".into(),    // 3
            "ArrayProperty".into(),     // 4
            "Items".into(),             // 5
            "IntProperty".into(),       // 6
            "Package".into(),           // 7
            "/Game/Fx/NS_Spark".into(), // 8
        ],
    }
}

/// A package whose single import is `/Game/Fx/NS_Spark`, so object references in
/// a test stream resolve to a readable name.
fn package() -> Package {
    let base = Package::parse(&build_minimal_package()).unwrap();
    Package {
        summary: base.summary,
        names: names(),
        imports: vec![ObjectImport {
            class_package: raw(0),
            class_name: raw(7),
            outer_index: PackageIndex(0),
            object_name: raw(8),
            package_name: None,
        }],
        exports: Vec::new(),
        soft_object_paths: Vec::new(),
        soft_object_path_error: None,
        soft_package_references: Vec::new(),
        soft_package_reference_error: None,
    }
}

fn raw(index: i32) -> crate::reader::RawName {
    crate::reader::RawName { index, number: 0 }
}

/// The fixture package's summary carries no custom versions and sets
/// FilterEditorOnly, so the modern editor-package layout is selected explicitly
/// here rather than inherited from it.
fn editor_struct_ctx(package: &Package) -> ScriptStructContext<'_> {
    ScriptStructContext {
        filter_editor_only: false,
        framework_version: custom::FRAMEWORK_REMOVE_UFIELD_NEXT,
        core_object_version: custom::CORE_FPROPERTIES,
        release_object_version: custom::RELEASE_FIELD_PATH_OWNER_SERIALIZATION,
        ..ScriptStructContext::new(package)
    }
}

fn bytecode_ctx<'a>(package: &'a Package, file_version_ue5: i32) -> BytecodeContext<'a> {
    BytecodeContext {
        package,
        file_version_ue5,
        release_object_version: custom::RELEASE_FIELD_PATH_OWNER_SERIALIZATION,
    }
}

fn run(
    script: &[u8],
    package: &Package,
    file_version_ue5: i32,
) -> crate::script::bytecode::BytecodeSummary {
    let mut reader = Reader::new(script);
    disassemble(
        &mut reader,
        script.len() as u64,
        &bytecode_ctx(package, file_version_ue5),
    )
    .expect("stream should disassemble")
}

/// An `FFieldPath` as `FPropertyProxyArchive` writes it: the path names, then the
/// owning struct.
fn push_field_path(script: &mut Vec<u8>, names: &[i32]) {
    push_i32(script, names.len() as i32);
    for name in names {
        push_raw_name(script, *name);
    }
    push_i32(script, 0); // owner
}

#[test]
fn consumes_every_expression_and_agrees_with_both_declared_sizes() {
    let package = package();
    let mut script = Vec::new();
    script.push(EX_LOCAL_VARIABLE);
    push_field_path(&mut script, &[2]);
    script.push(EX_INT_CONST);
    push_i32(&mut script, 7);
    script.push(EX_NAME_CONST);
    push_raw_name(&mut script, 1);
    script.push(EX_OBJECT_CONST);
    push_i32(&mut script, -1);
    script.push(EX_END_OF_SCRIPT);

    let summary = run(&script, &package, ue5::HIGHEST);

    assert_eq!(summary.expressions, 5);
    // Disk: 1+16 | 1+4 | 1+8 | 1+4 | 1  = 37 bytes, all consumed by the walk.
    assert_eq!(script.len(), 37);
    // In-memory: a field path is one 8-byte pointer and a name is 12 bytes, so
    // the executed buffer is wider than the file.
    assert_eq!(summary.icode, (1 + 8) + (1 + 4) + (1 + 12) + (1 + 8) + 1);
    assert_eq!(summary.opcodes.get("EX_ObjectConst"), Some(&1));
    assert!(
        summary
            .references
            .contains(&(ScriptRefKind::Object, "/Game/Fx/NS_Spark".to_string()))
    );
}

#[test]
fn a_call_consumes_parameters_up_to_its_terminator() {
    let package = package();
    let mut script = Vec::new();
    script.push(EX_FINAL_FUNCTION);
    push_i32(&mut script, -1); // stack node
    script.push(EX_INT_CONST);
    push_i32(&mut script, 1);
    script.push(EX_NOTHING);
    script.push(EX_END_FUNCTION_PARMS);

    let summary = run(&script, &package, ue5::HIGHEST);

    assert_eq!(summary.expressions, 4);
    assert_eq!(summary.opcodes.get("EX_FinalFunction"), Some(&1));
    assert!(
        summary
            .references
            .contains(&(ScriptRefKind::Function, "/Game/Fx/NS_Spark".to_string()))
    );
}

// The only layout gate a reader has to honour inside an expression: below
// LARGE_WORLD_COORDINATES a vector constant is three floats, at and above it
// three doubles.
#[test]
fn vector_constants_follow_the_large_world_coordinates_gate() {
    let package = package();
    let narrow = {
        let mut script = vec![EX_VECTOR_CONST];
        script.extend_from_slice(&[0u8; 12]);
        script
    };
    let wide = {
        let mut script = vec![EX_VECTOR_CONST];
        script.extend_from_slice(&[0u8; 24]);
        script
    };

    run(&narrow, &package, ue5::LARGE_WORLD_COORDINATES - 1);
    run(&wide, &package, ue5::LARGE_WORLD_COORDINATES);

    // Reading the narrow form with the wide layout overruns the declared size.
    let mut reader = Reader::new(&narrow);
    assert!(
        disassemble(
            &mut reader,
            narrow.len() as u64,
            &bytecode_ctx(&package, ue5::LARGE_WORLD_COORDINATES)
        )
        .is_err()
    );
}

#[test]
fn a_soft_object_constant_yields_the_path_its_string_literal_holds() {
    let package = package();
    let mut script = vec![EX_SOFT_OBJECT_CONST, EX_STRING_CONST];
    script.extend_from_slice(b"/Game/Runtime/Loaded.Loaded\0");

    let summary = run(&script, &package, ue5::HIGHEST);

    assert!(summary.references.contains(&(
        ScriptRefKind::SoftObject,
        "/Game/Runtime/Loaded.Loaded".to_string()
    )));
}

#[test]
fn a_unicode_string_constant_ends_at_its_two_byte_terminator() {
    let package = package();
    let mut script = vec![EX_UNICODE_STRING_CONST];
    for unit in "Hi".encode_utf16() {
        push_u16(&mut script, unit);
    }
    push_u16(&mut script, 0);
    script.push(EX_END_OF_SCRIPT);

    let summary = run(&script, &package, ue5::HIGHEST);

    assert_eq!(summary.expressions, 2);
    assert_eq!(summary.opcodes.get("EX_UnicodeStringConst"), Some(&1));
}

// EX_SwitchValue reads its case count first and then that many triples; getting
// the count wrong silently desynchronizes the rest of the function.
#[test]
fn switch_value_reads_exactly_its_declared_case_count() {
    let package = package();
    let mut script = vec![EX_SWITCH_VALUE];
    push_u16(&mut script, 2); // cases
    push_i32(&mut script, 0); // end offset
    script.push(EX_NOTHING); // index term
    for _ in 0..2 {
        script.push(EX_NOTHING); // case index value
        push_i32(&mut script, 0); // next-case offset
        script.push(EX_NOTHING); // case term
    }
    script.push(EX_NOTHING); // default term

    let summary = run(&script, &package, ue5::HIGHEST);

    assert_eq!(summary.expressions, 1 + 1 + 4 + 1);
}

#[test]
fn an_unknown_opcode_stops_the_walk_instead_of_guessing_a_width() {
    let package = package();
    let script = [0xFEu8, 0x00, 0x00];
    let mut reader = Reader::new(&script);

    let error = disassemble(
        &mut reader,
        script.len() as u64,
        &bytecode_ctx(&package, ue5::HIGHEST),
    )
    .expect_err("an unknown opcode has an unknown payload");

    assert!(format!("{error:#}").contains("unknown script opcode 0xFE"));
}

#[test]
fn a_truncated_expression_is_an_error_rather_than_a_short_read() {
    let package = package();
    // EX_IntConst promises four bytes and only two are present.
    let script = [EX_INT_CONST, 0x01, 0x02];
    let mut reader = Reader::new(&script);

    assert!(
        disassemble(
            &mut reader,
            script.len() as u64,
            &bytecode_ctx(&package, ue5::HIGHEST)
        )
        .is_err()
    );
}

#[test]
fn a_call_missing_its_terminator_reports_the_missing_opcode() {
    let package = package();
    let mut script = vec![EX_FINAL_FUNCTION];
    push_i32(&mut script, 0);
    script.push(EX_NOTHING);

    let mut reader = Reader::new(&script);
    let error = disassemble(
        &mut reader,
        script.len() as u64,
        &bytecode_ctx(&package, ue5::HIGHEST),
    )
    .expect_err("the parameter list never closes");

    assert!(format!("{error:#}").contains("EX_EndFunctionParms"));
}

/// One `FField`: name, editor flags, no metadata, then `FProperty`'s fixed block.
fn push_field_header(data: &mut Vec<u8>, name: i32) {
    push_raw_name(data, name);
    push_u32(data, 0); // FField flags (present when editor data is kept)
    push_u32(data, 0); // bHasMetaData, written as a 32-bit legacy bool
    push_i32(data, 1); // ArrayDim
    push_i32(data, 8); // ElementSize
    push_u64(data, 0x0000_0004); // PropertyFlags
    push_u16(data, 0); // RepIndex
    push_raw_name(data, 0); // RepNotifyFunc = None
    data.push(0); // BlueprintReplicationCondition
}

#[test]
fn child_properties_decode_with_their_nested_inner_fields() {
    let package = package();
    let ctx = FieldContext {
        package: &package,
        filter_editor_only: false,
    };
    let mut data = Vec::new();
    push_i32(&mut data, 2); // ChildProperties count

    push_raw_name(&mut data, 3); // ObjectProperty
    push_field_header(&mut data, 2); // Target
    push_i32(&mut data, -1); // PropertyClass

    push_raw_name(&mut data, 4); // ArrayProperty
    push_field_header(&mut data, 5); // Items
    push_raw_name(&mut data, 6); // Inner type: IntProperty
    push_field_header(&mut data, 5);

    let mut reader = Reader::new(&data);
    let fields =
        decode_property_list(&mut reader, data.len() as u64, &ctx).expect("fields should decode");

    assert_eq!(reader.pos(), data.len() as u64);
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].name, "Target");
    assert_eq!(fields[0].type_name, "ObjectProperty");
    assert_eq!(fields[0].type_object.as_deref(), Some("/Game/Fx/NS_Spark"));
    assert_eq!(fields[1].type_name, "ArrayProperty");
    assert_eq!(fields[1].inner.len(), 1);
    assert_eq!(fields[1].inner[0].type_name, "IntProperty");
}

#[test]
fn an_unrecognized_property_type_is_refused_rather_than_skipped() {
    let package = package();
    let ctx = FieldContext {
        package: &package,
        filter_editor_only: false,
    };
    let mut data = Vec::new();
    push_i32(&mut data, 1);
    push_raw_name(&mut data, 5); // "Items" is not a field class name
    push_field_header(&mut data, 2);

    let mut reader = Reader::new(&data);
    let error = decode_property_list(&mut reader, data.len() as u64, &ctx)
        .expect_err("an unknown field width would desynchronize everything after it");

    assert!(format!("{error:#}").contains("unknown property type `Items`"));
}

#[test]
fn a_function_struct_decodes_its_shape_script_and_flags() {
    let package = package();
    let ctx = editor_struct_ctx(&package);

    let script = vec![EX_RETURN, EX_NOTHING, EX_END_OF_SCRIPT];

    let mut data = Vec::new();
    push_i32(&mut data, -1); // SuperStruct
    push_i32(&mut data, 0); // Children count
    push_i32(&mut data, 1); // ChildProperties count
    push_raw_name(&mut data, 3); // ObjectProperty
    push_field_header(&mut data, 2); // Target
    push_i32(&mut data, 0); // PropertyClass
    push_i32(&mut data, 3); // BytecodeBufferSize
    push_i32(&mut data, script.len() as i32); // SerializedScriptSize
    data.extend_from_slice(&script);
    push_u32(&mut data, 0x0000_0800); // FunctionFlags: Event
    push_i32(&mut data, 0); // EventGraphFunction
    push_i32(&mut data, 0); // EventGraphCallOffset

    let mut reader = Reader::new(&data);
    let decoded = decode_script_struct(
        &mut reader,
        data.len() as u64,
        "/Script/CoreUObject.Function",
        &ctx,
    )
    .expect("the function struct should decode");

    assert_eq!(decoded.end, data.len() as u64);
    assert_eq!(decoded.super_struct.as_deref(), Some("/Game/Fx/NS_Spark"));
    assert_eq!(decoded.properties.len(), 1);
    let code = decoded.bytecode.expect("the function carries script");
    assert!(code.failure.is_none());
    assert!(code.sizes_agree());
    assert_eq!(code.serialized_size, 3);
    let function = decoded
        .function
        .expect("a Function export has UFunction data");
    assert_eq!(
        crate::script::function_flag_names(function.flags),
        ["Event"]
    );
}

// FUNC_Net inserts a replication offset before the event-graph fields; missing it
// shifts everything after it.
#[test]
fn a_networked_function_reads_its_replication_offset() {
    let package = package();
    let ctx = editor_struct_ctx(&package);

    let mut data = Vec::new();
    push_i32(&mut data, 0); // SuperStruct
    push_i32(&mut data, 0); // Children count
    push_i32(&mut data, 0); // ChildProperties count
    push_i32(&mut data, 0); // BytecodeBufferSize
    push_i32(&mut data, 0); // SerializedScriptSize
    push_u32(&mut data, 0x0000_0040); // FunctionFlags: Net
    push_u16(&mut data, 0); // RepOffset
    push_i32(&mut data, 0); // EventGraphFunction
    push_i32(&mut data, 0); // EventGraphCallOffset

    let mut reader = Reader::new(&data);
    let decoded = decode_script_struct(
        &mut reader,
        data.len() as u64,
        "/Script/CoreUObject.Function",
        &ctx,
    )
    .expect("a networked function should decode");

    assert_eq!(decoded.end, data.len() as u64);
    assert!(decoded.bytecode.is_none());
}

// Below FCoreObjectVersion::FProperties there is no ChildProperties block at all,
// and a missing custom version selects that older layout.
#[test]
fn a_package_without_the_fproperties_version_has_no_child_properties_block() {
    let package = package();
    let ctx = ScriptStructContext {
        core_object_version: custom::CORE_FPROPERTIES - 1,
        ..editor_struct_ctx(&package)
    };

    let mut data = Vec::new();
    push_i32(&mut data, 0); // SuperStruct
    push_i32(&mut data, 0); // Children count
    push_i32(&mut data, 0); // BytecodeBufferSize
    push_i32(&mut data, 0); // SerializedScriptSize
    push_u32(&mut data, 0); // FunctionFlags
    push_i32(&mut data, 0); // EventGraphFunction
    push_i32(&mut data, 0); // EventGraphCallOffset

    let mut reader = Reader::new(&data);
    let decoded = decode_script_struct(
        &mut reader,
        data.len() as u64,
        "/Script/CoreUObject.Function",
        &ctx,
    )
    .expect("the legacy layout should decode");

    assert_eq!(decoded.end, data.len() as u64);
    assert!(decoded.properties.is_empty());
}

#[test]
fn a_generated_class_decodes_its_uclass_block() {
    let package = package();
    let ctx = editor_struct_ctx(&package);

    let mut data = Vec::new();
    push_i32(&mut data, 0); // SuperStruct
    push_i32(&mut data, 0); // Children count
    push_i32(&mut data, 0); // ChildProperties count
    push_i32(&mut data, 0); // BytecodeBufferSize
    push_i32(&mut data, 0); // SerializedScriptSize
    // UClass::Serialize
    push_i32(&mut data, 1); // FuncMap count
    push_raw_name(&mut data, 1); // MyFunc
    push_i32(&mut data, 0); // function object
    push_u32(&mut data, 0); // ClassFlags
    push_i32(&mut data, 0); // ClassWithin
    push_raw_name(&mut data, 0); // ClassConfigName
    push_i32(&mut data, -1); // ClassGeneratedBy
    push_i32(&mut data, 1); // Interfaces count
    push_i32(&mut data, -1); // interface class
    push_i32(&mut data, 0); // pointer offset
    push_u32(&mut data, 0); // bImplementedByK2
    push_u32(&mut data, 0); // bDeprecatedForceScriptOrder
    push_raw_name(&mut data, 0); // reserved name
    push_u32(&mut data, 0); // bCooked
    push_i32(&mut data, -1); // ClassDefaultObject

    let mut reader = Reader::new(&data);
    let decoded = decode_script_struct(
        &mut reader,
        data.len() as u64,
        "/Script/Engine.BlueprintGeneratedClass",
        &ctx,
    )
    .expect("a generated class should decode");

    assert_eq!(decoded.end, data.len() as u64);
    assert!(decoded.function.is_none());
    let class = decoded.class.expect("a class export has UClass data");
    assert_eq!(class.functions.len(), 1);
    assert_eq!(class.functions[0].0, "MyFunc");
    assert_eq!(
        class.class_generated_by.as_deref(),
        Some("/Game/Fx/NS_Spark")
    );
    assert_eq!(class.interfaces, ["/Game/Fx/NS_Spark"]);
    assert_eq!(class.default_object.as_deref(), Some("/Game/Fx/NS_Spark"));
}
