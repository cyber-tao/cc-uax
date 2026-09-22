//! Whole-package tests: bytes go through `PackageView::parse` — summary, name,
//! import and export tables, export windows — and out through `analyze`, so a
//! header-offset or table-width mistake shows up here rather than being hidden
//! by a hand-assembled `Package`.

use super::common::*;
use crate::model::{AnalysisStatus, AssetView, KnownOpaqueKind, PropertyDecodeStatus};
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
