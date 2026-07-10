use crate::{AnalysisStatus, AssetAnalysis, AssetView, CapabilityKind, PackageView};
use std::path::{Path, PathBuf};

#[test]
#[ignore = "requires CC_UAX_STACKOBOT_CONTENT to point at the external StackOBot Content directory"]
fn stackobot_pcg_and_state_tree_semantic_counts_match_real_assets() {
    let content = PathBuf::from(
        std::env::var_os("CC_UAX_STACKOBOT_CONTENT")
            .expect("CC_UAX_STACKOBOT_CONTENT must point at StackOBot/Content"),
    );

    let pickup = analyze(&content, "StackOBot/Blueprints/PCG/PCG_PickupSpline.uasset");
    let pickup_graph = pickup.pcg_graphs.first().expect("Pickup PCG graph");
    assert_eq!(pickup_graph.nodes_array_count, 5);
    assert_eq!(pickup_graph.default_node_count, 2);
    assert_eq!(pickup_graph.nodes.len(), 7);
    assert_eq!(pickup_graph.base_node_export_count, 6);
    assert_eq!(
        pickup_graph
            .nodes
            .iter()
            .flat_map(|node| &node.pins)
            .count(),
        135
    );
    assert_eq!(pickup_graph.edges.len(), 4);
    assert!(
        pickup_graph
            .nodes
            .iter()
            .any(|node| node.class == "/Script/PCG.PCGSpawnActorNode")
    );
    assert_eq!(pickup_graph.unresolved_node_references, 0);
    assert_eq!(pickup_graph.unresolved_pin_references, 0);
    assert_eq!(pickup_graph.unresolved_edge_references, 0);
    assert_capability(
        &pickup,
        CapabilityKind::PcgSemantics,
        AnalysisStatus::Complete,
    );

    let under_rock = analyze(&content, "StackOBot/Blueprints/PCG/PCG_UnderRock.uasset");
    let under_rock_graph = under_rock.pcg_graphs.first().expect("UnderRock PCG graph");
    assert_eq!(under_rock_graph.nodes_array_count, 67);
    assert_eq!(under_rock_graph.default_node_count, 2);
    assert_eq!(under_rock_graph.nodes.len(), 69);
    assert_eq!(under_rock_graph.base_node_export_count, 69);
    assert_eq!(
        under_rock_graph
            .nodes
            .iter()
            .flat_map(|node| &node.pins)
            .count(),
        713
    );
    assert_eq!(under_rock_graph.edges.len(), 74);
    let property_bag_paths = under_rock
        .known_opaque
        .iter()
        .filter(|opaque| opaque.type_name.as_deref() == Some("InstancedPropertyBag"))
        .map(|opaque| opaque.path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        property_bag_paths,
        vec![
            "/exports/108/properties/UserParameters",
            "/exports/120/properties/DefaultValues/PropertyBag",
            "/exports/121/properties/DefaultValues/PropertyBag",
        ]
    );
    assert_capability(
        &under_rock,
        CapabilityKind::PcgSemantics,
        AnalysisStatus::Partial,
    );

    let state_tree = analyze(&content, "StackOBot/AI/STree_Bug.uasset");
    let graph = state_tree
        .state_tree_graphs
        .first()
        .expect("StateTree semantic graph");
    assert_eq!(graph.states.len(), 13);
    assert_eq!(
        graph
            .states
            .iter()
            .map(|state| state.tasks.len())
            .sum::<usize>(),
        20
    );
    assert_eq!(
        graph
            .states
            .iter()
            .map(|state| state.enter_conditions.len())
            .sum::<usize>(),
        3
    );
    assert_eq!(
        graph
            .states
            .iter()
            .map(|state| state.transitions.len())
            .sum::<usize>(),
        12
    );
    assert_eq!(
        graph
            .states
            .iter()
            .flat_map(|state| &state.transitions)
            .map(|transition| transition.conditions.len())
            .sum::<usize>(),
        6
    );
    assert_eq!(graph.unresolved_state_references, 0);
    assert_capability(
        &state_tree,
        CapabilityKind::StateTreeSemantics,
        AnalysisStatus::Complete,
    );
}

fn analyze(content: &Path, relative: &str) -> AssetAnalysis {
    let bytes = std::fs::read(content.join(relative)).unwrap();
    PackageView::parse(&bytes)
        .unwrap()
        .analyze(AssetView::Logic)
}

fn assert_capability(analysis: &AssetAnalysis, kind: CapabilityKind, expected: AnalysisStatus) {
    let capability = analysis
        .capabilities
        .iter()
        .find(|capability| capability.kind == kind)
        .unwrap_or_else(|| panic!("missing {kind:?} capability"));
    assert_eq!(capability.status, expected);
}
