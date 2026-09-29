//! Whole-package tests: bytes go through `PackageView::parse` — summary, name,
//! import and export tables, export windows — and out through `analyze`, so a
//! header-offset or table-width mistake shows up here rather than being hidden
//! by a hand-assembled `Package`.

use super::common::*;
use crate::model::{
    AnalysisStatus, AssetView, KnownOpaqueKind, OpaqueReason, PropertyDecodeStatus,
};
use crate::version::ue5;
use crate::{CapabilityKind, PackageView};

/// A payload of one legacy or complete-layout `IntProperty` and a `None`
/// terminator (with the control byte from 1011), then the object GUID flag.
fn int_property_payload(builder: &mut PackageBuilder, value: i32) -> (Vec<u8>, (u64, u64)) {
    let fv = builder.file_version_ue5;
    let name = builder.name("Value") as i32;
    let ty = builder.name("IntProperty") as i32;
    let none = builder.name("None") as i32;
    let mut data = Vec::new();
    if fv >= ue5::PROPERTY_TAG_EXTENSION_AND_OVERRIDABLE_SERIALIZATION {
        data.push(0);
    }
    if fv >= ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME {
        push_raw_name(&mut data, name);
        push_raw_name(&mut data, ty);
        push_i32(&mut data, 0);
        push_i32(&mut data, 4);
        data.push(0);
    } else {
        push_legacy_tag_header(&mut data, name, ty, 4);
        push_legacy_tag_tail(&mut data, fv);
    }
    push_i32(&mut data, value);
    push_raw_name(&mut data, none);
    let block = (0, data.len() as u64);
    push_i32(&mut data, 0); // PossiblySerializeObjectGuid: absent
    (data, block)
}

/// Every release's `FileVersionUE5` (per that release's ObjectVersion.h) plus
/// the gate thresholds that change a table or export-row layout, each at
/// threshold-1 and threshold.
fn versions_under_test() -> Vec<(i32, (u16, u16))> {
    let mut versions = vec![
        (1004, (5, 0)),
        (1008, (5, 1)),
        (1009, (5, 3)),
        (1012, (5, 4)),
        (1013, (5, 5)),
        (1017, (5, 6)),
        (1018, (5, 8)),
    ];
    for gate in [
        ue5::OPTIONAL_RESOURCES,
        ue5::ADD_SOFTOBJECTPATH_LIST,
        ue5::SCRIPT_SERIALIZATION_OFFSET,
        ue5::PROPERTY_TAG_EXTENSION_AND_OVERRIDABLE_SERIALIZATION,
        ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
        ue5::REMOVE_OBJECT_EXPORT_PACKAGE_GUID,
        ue5::METADATA_SERIALIZATION_OFFSET,
        ue5::VERSE_CELLS,
        ue5::TRACK_OBJECT_EXPORT_IS_INHERITED,
        ue5::PACKAGE_SAVED_HASH,
        ue5::IMPORT_TYPE_HIERARCHIES,
    ] {
        for version in [gate - 1, gate] {
            if (ue5::INITIAL_VERSION..=ue5::HIGHEST).contains(&version) {
                versions.push((version, (5, 6)));
            }
        }
    }
    versions.sort_unstable();
    versions.dedup();
    versions
}

#[test]
fn a_tagged_export_round_trips_through_every_version_gate() {
    for (fv, engine) in versions_under_test() {
        let mut builder = PackageBuilder::new(fv, engine);
        let actor = builder.script_class("/Script/Engine", "Actor");
        let (payload, block) = int_property_payload(&mut builder, 42);
        let object_name = builder.name("Actor_0");
        builder.exports.push(ExportSpec {
            class_index: actor,
            outer_index: 0,
            object_name,
            payload,
            script_range: Some(block),
        });
        let bytes = builder.build();

        let view = PackageView::parse(&bytes)
            .unwrap_or_else(|error| panic!("FileVersionUE5={fv}: {error}"));
        let analysis = view.analyze(AssetView::Full);
        assert_eq!(analysis.summary.file_version_ue5, fv);
        assert_eq!(analysis.exports.len(), 1, "{fv}");
        let export = &analysis.exports[0];
        assert_eq!(export.class, "/Script/Engine.Actor", "{fv}");
        assert_eq!(export.name, "Actor_0", "{fv}");
        assert_eq!(
            export.property_status,
            Some(PropertyDecodeStatus::Complete),
            "{fv}: {:#?}",
            analysis.diagnostics
        );
        assert_eq!(export.properties[0].value.as_i64(), Some(42), "{fv}");
        assert!(
            analysis.diagnostics.is_empty(),
            "{fv}: {:#?}",
            analysis.diagnostics
        );
        assert_eq!(analysis.coverage.unclassified_bytes, 0, "{fv}");
        assert!(
            analysis.known_opaque.is_empty(),
            "{fv}: {:#?}",
            analysis.known_opaque
        );
        assert_eq!(analysis.status, AnalysisStatus::Complete, "{fv}");
        // The import that names the class is a script reference the tables record.
        assert!(
            analysis
                .references
                .scripts
                .iter()
                .any(|reference| reference == "/Script/Engine"),
            "{fv}: {:?}",
            analysis.references
        );
    }
}

/// Complete-layout tag helpers (`FileVersionUE5` >= PROPERTY_TAG_COMPLETE_TYPE_NAME).
fn push_object_property(data: &mut Vec<u8>, name: i32, type_name: i32, target: i32) {
    push_raw_name(data, name);
    push_raw_name(data, type_name);
    push_i32(data, 0);
    push_i32(data, 4);
    data.push(0);
    push_i32(data, target);
}

fn push_object_array_property(
    data: &mut Vec<u8>,
    name: i32,
    array_type: i32,
    element_type: i32,
    targets: &[i32],
) {
    push_raw_name(data, name);
    push_raw_name(data, array_type);
    push_i32(data, 1); // one type parameter
    push_raw_name(data, element_type);
    push_i32(data, 0);
    push_i32(data, 4 + 4 * targets.len() as i32);
    data.push(0);
    push_i32(data, targets.len() as i32);
    for target in targets {
        push_i32(data, *target);
    }
}

fn push_name_property(data: &mut Vec<u8>, name: i32, type_name: i32, value: i32) {
    push_raw_name(data, name);
    push_raw_name(data, type_name);
    push_i32(data, 0);
    push_i32(data, 8);
    data.push(0);
    push_raw_name(data, value);
}

/// The StateTree adapter is otherwise tested from hand-built `AssetExport`s.
/// Here the tree, its editor data and one state are real exports with real
/// tagged blocks, so object references resolve through the export table and the
/// adapter sees exactly what the decoder produces.
#[test]
fn a_state_tree_is_assembled_from_real_exports() {
    let mut builder = PackageBuilder::new(ue5::HIGHEST, (5, 7));
    let tree_class = builder.script_class("/Script/StateTreeModule", "StateTree");
    let editor_data_class =
        builder.script_class("/Script/StateTreeEditorModule", "StateTreeEditorData");
    let state_class = builder.script_class("/Script/StateTreeEditorModule", "StateTreeState");
    let none = builder.name("None") as i32;
    let object_property = builder.name("ObjectProperty") as i32;
    let array_property = builder.name("ArrayProperty") as i32;
    let name_property = builder.name("NameProperty") as i32;
    let editor_data_field = builder.name("EditorData") as i32;
    let sub_trees_field = builder.name("SubTrees") as i32;
    let name_field = builder.name("Name") as i32;
    let root_label = builder.name("Root") as i32;

    // Export 1: the tree, pointing at export 2. Export 2: editor data whose
    // SubTrees names export 3. Export 3: the root state.
    let block = |body: &mut dyn FnMut(&mut Vec<u8>)| {
        let mut data = vec![0u8]; // object serialization control
        body(&mut data);
        push_raw_name(&mut data, none);
        let end = data.len() as u64;
        push_i32(&mut data, 0); // object guid absent
        (data, (0, end))
    };
    let (tree_payload, tree_block) =
        block(&mut |data| push_object_property(data, editor_data_field, object_property, 2));
    let (editor_payload, editor_block) = block(&mut |data| {
        push_object_array_property(data, sub_trees_field, array_property, object_property, &[3])
    });
    let (state_payload, state_block) =
        block(&mut |data| push_name_property(data, name_field, name_property, root_label));

    let tree_name = builder.name("ST_Test");
    let editor_name = builder.name("StateTreeEditorData_0");
    let state_name = builder.name("StateTreeState_0");
    builder.exports.push(ExportSpec {
        class_index: tree_class,
        outer_index: 0,
        object_name: tree_name,
        payload: tree_payload,
        script_range: Some(tree_block),
    });
    builder.exports.push(ExportSpec {
        class_index: editor_data_class,
        outer_index: 1,
        object_name: editor_name,
        payload: editor_payload,
        script_range: Some(editor_block),
    });
    builder.exports.push(ExportSpec {
        class_index: state_class,
        outer_index: 2,
        object_name: state_name,
        payload: state_payload,
        script_range: Some(state_block),
    });
    let bytes = builder.build();

    let analysis = PackageView::parse(&bytes).unwrap().analyze(AssetView::Full);
    assert!(
        analysis.diagnostics.is_empty(),
        "{:#?}",
        analysis.diagnostics
    );
    assert_eq!(
        analysis.state_tree_graphs.len(),
        1,
        "{:#?}",
        analysis.exports
    );
    let graph = &analysis.state_tree_graphs[0];
    assert_eq!(graph.editor_data_index, Some(2));
    assert_eq!(graph.root_state_indices, vec![3]);
    assert_eq!(graph.states.len(), 1);
    assert_eq!(graph.states[0].name, "Root");
    assert_eq!(graph.unresolved_state_references, 0);
    assert_eq!(analysis.coverage.state_tree_graphs_decoded, 1);
    assert_eq!(analysis.coverage.state_tree_states_decoded, 1);
    assert!(
        analysis
            .capabilities
            .iter()
            .any(
                |capability| capability.kind == CapabilityKind::StateTreeSemantics
                    && capability.status == AnalysisStatus::Complete
            ),
        "{:#?}",
        analysis.capabilities
    );
    assert_eq!(analysis.status, AnalysisStatus::Complete);
}

/// The three export shapes that are decided by the class rather than by the
/// bytes — a native-only payload, a URigVMLink, and an import-data prefix — go
/// through the real table readers on both sides of `SCRIPT_SERIALIZATION_OFFSET`.
#[test]
fn class_decided_export_shapes_survive_the_real_parse_path() {
    for fv in [ue5::SCRIPT_SERIALIZATION_OFFSET - 1, ue5::HIGHEST] {
        let mut builder = PackageBuilder::new(fv, (5, 7));
        let hierarchy = builder.script_class("/Script/ControlRig", "RigHierarchy");
        let link = builder.script_class("/Script/RigVMDeveloper", "RigVMLink");
        let mut hierarchy_payload = Vec::new();
        push_i32(&mut hierarchy_payload, 0x26);
        hierarchy_payload.extend_from_slice(&[0xAB; 12]);
        let mut link_payload = Vec::new();
        push_fstring(&mut link_payload, "A.Out");
        push_fstring(&mut link_payload, "B.In");
        let hierarchy_name = builder.name("RigHierarchy_0");
        let link_name = builder.name("RigVMLink_0");
        builder.exports.push(ExportSpec {
            class_index: hierarchy,
            outer_index: 0,
            object_name: hierarchy_name,
            payload: hierarchy_payload.clone(),
            script_range: None,
        });
        builder.exports.push(ExportSpec {
            class_index: link,
            outer_index: 0,
            object_name: link_name,
            payload: link_payload,
            script_range: None,
        });
        let bytes = builder.build();

        let analysis = PackageView::parse(&bytes)
            .unwrap_or_else(|error| panic!("{fv}: {error}"))
            .analyze(AssetView::Full);
        assert!(
            analysis.diagnostics.is_empty(),
            "{fv}: {:#?}",
            analysis.diagnostics
        );
        assert_eq!(
            analysis.exports[0].property_status,
            Some(PropertyDecodeStatus::NativeOnly),
            "{fv}"
        );
        assert_eq!(analysis.exports[1].property_status, None, "{fv}");
        let class_payload = analysis
            .known_opaque
            .iter()
            .filter(|region| region.kind == KnownOpaqueKind::ClassPayload)
            .collect::<Vec<_>>();
        assert_eq!(class_payload.len(), 1, "{fv}: {:#?}", analysis.known_opaque);
        assert_eq!(
            class_payload[0].byte_range.as_ref().unwrap().size,
            hierarchy_payload.len() as u64
        );
        assert_eq!(
            analysis.coverage.class_payload_bytes,
            hierarchy_payload.len() as u64
        );
        assert_eq!(analysis.coverage.unattributed_tail_bytes, 0, "{fv}");
        assert_eq!(analysis.coverage.unclassified_bytes, 0, "{fv}");
        // The hierarchy payload is the rig_hierarchy gap and nothing else is.
        let gaps: Vec<_> = analysis
            .capabilities
            .iter()
            .filter(|capability| capability.status != AnalysisStatus::Complete)
            .map(|capability| capability.kind)
            .collect();
        assert_eq!(gaps, [CapabilityKind::RigHierarchy], "{fv}");
    }
}

// A legacy `TMap` tag names no key or value struct, and a Blueprint variable is
// declared by nothing but its own generated class. These build the class export
// and an instance of it as real exports of one package (`FileVersionUE5` 1009,
// where the tags are legacy), so the export order, the class walk and the tag
// loop are exercised together.

/// One `FField` of the builder's FilterEditorOnly package, which writes no flags
/// word: type name, field name, then `FProperty`'s fixed block.
fn push_reflected_field(data: &mut Vec<u8>, type_name: i32, name: i32, none: i32) {
    push_raw_name(data, type_name);
    push_raw_name(data, name);
    push_i32(data, 0); // bHasMetaData, a 32-bit legacy bool
    push_i32(data, 1); // ArrayDim
    push_i32(data, 8); // ElementSize
    push_u64(data, 0x0000_0004); // PropertyFlags
    push_u16(data, 0); // RepIndex
    push_raw_name(data, none); // RepNotifyFunc
    data.push(0); // BlueprintReplicationCondition
}

/// A package with `Default__BP_Test_C`, an instance whose `ColorMap` is a legacy
/// `MapProperty(NameProperty, StructProperty)` with one `FName -> FLinearColor`
/// entry, and `BP_Test_C`, the generated class that declares it. `class_first`
/// puts the class export before the instance; otherwise it comes after, which is
/// the order UE writes them in. `instance_uses_local_class` points the instance at
/// the class export, or at an imported class when the class lives elsewhere.
fn legacy_map_package(class_first: bool, instance_uses_local_class: bool) -> Vec<u8> {
    let mut builder = PackageBuilder::new(1009, (5, 3));
    builder.custom_versions = vec![
        (
            crate::version::custom::CORE_OBJECT_VERSION,
            crate::version::custom::CORE_FPROPERTIES,
        ),
        (
            crate::version::custom::FRAMEWORK_OBJECT_VERSION,
            crate::version::custom::FRAMEWORK_REMOVE_UFIELD_NEXT,
        ),
    ];
    let generated_class = builder.script_class("/Script/Engine", "BlueprintGeneratedClass");
    let linear_color = builder.script_class("/Script/CoreUObject", "LinearColor");
    let color_map = builder.name("ColorMap") as i32;
    let map_property = builder.name("MapProperty") as i32;
    let name_property = builder.name("NameProperty") as i32;
    let struct_property = builder.name("StructProperty") as i32;
    let none = builder.name("None") as i32;
    let red = builder.name("Red") as i32;
    let class_name = builder.name("BP_Test_C");
    let instance_name = builder.name("Default__BP_Test_C");

    // The instance: one legacy map tag, then the object GUID flag.
    let mut map_payload = Vec::new();
    push_i32(&mut map_payload, 0); // NumToRemove
    push_i32(&mut map_payload, 1); // Num
    push_raw_name(&mut map_payload, red);
    for channel in [1.0f32, 0.5, 0.25, 1.0] {
        push_f32(&mut map_payload, channel);
    }
    let mut instance = Vec::new();
    push_legacy_tag_header(
        &mut instance,
        color_map,
        map_property,
        map_payload.len() as i32,
    );
    push_raw_name(&mut instance, name_property); // key type
    push_raw_name(&mut instance, struct_property); // value type, no struct name
    push_legacy_tag_tail(&mut instance, 1009);
    instance.extend_from_slice(&map_payload);
    push_raw_name(&mut instance, none);
    push_i32(&mut instance, 0); // PossiblySerializeObjectGuid: absent

    // The class: an empty tagged block, the GUID flag, then `UStruct` and `UClass`.
    let mut class = Vec::new();
    push_raw_name(&mut class, none);
    push_i32(&mut class, 0);
    push_i32(&mut class, 0); // SuperStruct
    push_i32(&mut class, 0); // Children count
    push_i32(&mut class, 1); // ChildProperties count
    push_reflected_field(&mut class, map_property, color_map, none);
    push_reflected_field(&mut class, name_property, color_map, none); // key
    push_reflected_field(&mut class, struct_property, color_map, none); // value
    push_i32(&mut class, linear_color); // the value struct
    push_i32(&mut class, 0); // BytecodeBufferSize
    push_i32(&mut class, 0); // SerializedScriptSize
    push_i32(&mut class, 0); // FuncMap count
    push_u32(&mut class, 0); // ClassFlags
    push_i32(&mut class, 0); // ClassWithin
    push_raw_name(&mut class, none); // ClassConfigName
    push_i32(&mut class, 0); // ClassGeneratedBy
    push_i32(&mut class, 0); // Interfaces count
    push_u32(&mut class, 0); // bDeprecatedForceScriptOrder
    push_raw_name(&mut class, none); // reserved name
    push_u32(&mut class, 0); // bCooked
    push_i32(&mut class, 0); // ClassDefaultObject

    // Export package indices are 1-based in table order.
    let (instance_index, class_index) = if class_first { (2, 1) } else { (1, 2) };
    let instance_class = if instance_uses_local_class {
        class_index
    } else {
        generated_class
    };
    let instance_spec = ExportSpec {
        class_index: instance_class,
        outer_index: 0,
        object_name: instance_name,
        payload: instance,
        script_range: None,
    };
    let class_spec = ExportSpec {
        class_index: generated_class,
        outer_index: 0,
        object_name: class_name,
        payload: class,
        script_range: None,
    };
    debug_assert_eq!(instance_index + class_index, 3);
    if class_first {
        builder.exports.push(class_spec);
        builder.exports.push(instance_spec);
    } else {
        builder.exports.push(instance_spec);
        builder.exports.push(class_spec);
    }
    builder.build()
}

#[test]
fn a_cdo_resolves_a_legacy_map_from_its_class_export_even_when_the_class_comes_later() {
    let bytes = legacy_map_package(false, true);

    let analysis = PackageView::parse(&bytes)
        .expect("the package parses")
        .analyze(AssetView::Full);

    // The instance is export 1 and stays first in the report although the class
    // export after it is decoded before it.
    assert_eq!(analysis.exports[0].index, 1);
    assert_eq!(analysis.exports[0].name, "Default__BP_Test_C");
    assert_eq!(analysis.exports[1].index, 2);
    let color_map = &analysis.exports[0].properties[0];
    assert_eq!(color_map.name, "ColorMap");
    assert!(
        !color_map.value.is_opaque(),
        "{:#?}\n{:#?}",
        color_map.value,
        analysis.diagnostics
    );
    assert_eq!(color_map.value[0]["key"].as_str(), Some("Red"));
    assert_eq!(
        color_map.type_name,
        "MapProperty(NameProperty,StructProperty(LinearColor))"
    );
    let color = &color_map.value[0]["value"];
    for (channel, expected) in [("r", 1.0), ("g", 0.5), ("b", 0.25), ("a", 1.0)] {
        assert_eq!(
            color[channel].as_f64(),
            Some(expected),
            "{channel}: {color:#?}"
        );
    }
    assert_eq!(
        analysis.exports[0].property_status,
        Some(PropertyDecodeStatus::Complete),
        "{:#?}",
        analysis.diagnostics
    );
    assert!(
        !analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "property_tag_missing_inner_struct_name"),
        "{:#?}",
        analysis.diagnostics
    );
    let tagged = analysis
        .capabilities
        .iter()
        .find(|capability| capability.kind == CapabilityKind::TaggedProperties)
        .expect("tagged properties are reported");
    assert_eq!(tagged.status, AnalysisStatus::Complete, "{tagged:#?}");

    // The class before the instance gives the same result.
    let class_first = PackageView::parse(&legacy_map_package(true, true))
        .expect("the package parses")
        .analyze(AssetView::Full);
    assert_eq!(class_first.exports[0].index, 1);
    assert_eq!(class_first.exports[1].name, "Default__BP_Test_C");
    assert!(!class_first.exports[1].properties[0].value.is_opaque());
}

#[test]
fn a_cdo_whose_class_is_imported_keeps_the_legacy_map_opaque() {
    let bytes = legacy_map_package(false, false);

    let analysis = PackageView::parse(&bytes)
        .expect("the package parses")
        .analyze(AssetView::Full);

    let opaque = analysis.exports[0].properties[0]
        .value
        .as_opaque()
        .expect("no class of this package declares the variable, so nothing is guessed");
    assert_eq!(opaque.reason, OpaqueReason::MissingInnerStructName);
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "property_tag_missing_inner_struct_name"),
        "{:#?}",
        analysis.diagnostics
    );
}
