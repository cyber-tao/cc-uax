use super::common::{minimal_package, temp_project};
use crate::scanner::build_project_index;
use crate::{
    AssetAnalysisSummary, AssetKind, AssetOwnership, AssetRecord, ConfigReference, MountTable,
    ProjectEntryPoints, ProjectLayout,
};
use cc_uax_core::{AssetView, PackageView};
use std::collections::BTreeSet;

#[test]
fn builds_forward_and_reverse_adjacency_with_canonical_case() {
    let root = temp_project("graph");
    let layout = ProjectLayout::discover(&root).unwrap();
    let bytes = minimal_package();
    let analysis = AssetAnalysisSummary::from_analysis(
        &PackageView::parse(&bytes).unwrap().analyze(AssetView::Full),
    );
    let record = |package: &str, references: &[&str]| AssetRecord {
        package_path: package.to_string(),
        mount_root: "/Game".to_string(),
        file_path: root.join(format!(
            "Content/{}.uasset",
            package.trim_start_matches("/Game/")
        )),
        relative_path: format!("{}.uasset", package.trim_start_matches("/Game/")),
        asset_kind: AssetKind::Asset,
        ownership: AssetOwnership::ProjectAsset,
        forward_references: references.iter().map(|value| value.to_string()).collect(),
        value_references: BTreeSet::new(),
        owned_sublevels: BTreeSet::new(),
        analysis: analysis.clone(),
    };
    let index = build_project_index(
        layout.clone(),
        MountTable::default_for(&layout),
        ProjectEntryPoints::default(),
        vec![record("/Game/A", &["/game/b"]), record("/Game/B", &[])],
        Vec::new(),
        Vec::new(),
        2,
    );

    assert_eq!(
        index.forward_references("/game/a").unwrap(),
        &BTreeSet::from(["/Game/B".to_string()])
    );
    assert_eq!(
        index.reverse_referencers("/game/b").unwrap(),
        &BTreeSet::from(["/Game/A".to_string()])
    );

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn self_reference_is_excluded_from_adjacency_so_the_asset_can_be_isolated() {
    let root = temp_project("selfref");
    let layout = ProjectLayout::discover(&root).unwrap();
    let bytes = minimal_package();
    let analysis = AssetAnalysisSummary::from_analysis(
        &PackageView::parse(&bytes).unwrap().analyze(AssetView::Full),
    );
    let record = |package: &str, references: &[&str]| AssetRecord {
        package_path: package.to_string(),
        mount_root: "/Game".to_string(),
        file_path: root.join(format!(
            "Content/{}.uasset",
            package.trim_start_matches("/Game/")
        )),
        relative_path: format!("{}.uasset", package.trim_start_matches("/Game/")),
        asset_kind: AssetKind::Asset,
        ownership: AssetOwnership::ProjectAsset,
        forward_references: references.iter().map(|value| value.to_string()).collect(),
        value_references: BTreeSet::new(),
        owned_sublevels: BTreeSet::new(),
        analysis: analysis.clone(),
    };
    // /Game/Loner cites only itself (as UE writes a Blueprint's own GeneratedClass),
    // and nothing else references it.
    let index = build_project_index(
        layout.clone(),
        MountTable::default_for(&layout),
        ProjectEntryPoints::default(),
        vec![record("/Game/Loner", &["/game/loner"])],
        Vec::new(),
        Vec::new(),
        1,
    );

    // The self-edge is gone from both directions.
    assert!(index.forward_references("/Game/Loner").unwrap().is_empty());
    assert!(index.reverse_referencers("/Game/Loner").is_none());
    // With no real edges, the asset is correctly reported as isolated.
    assert!(
        index
            .reachability
            .isolated_project_assets
            .contains("/Game/Loner")
    );

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn repeated_external_references_normalize_to_a_single_case() {
    let root = temp_project("extref");
    let layout = ProjectLayout::discover(&root).unwrap();
    let bytes = minimal_package();
    let analysis = AssetAnalysisSummary::from_analysis(
        &PackageView::parse(&bytes).unwrap().analyze(AssetView::Full),
    );
    let record = |package: &str, references: &[&str]| AssetRecord {
        package_path: package.to_string(),
        mount_root: "/Game".to_string(),
        file_path: root.join(format!(
            "Content/{}.uasset",
            package.trim_start_matches("/Game/")
        )),
        relative_path: format!("{}.uasset", package.trim_start_matches("/Game/")),
        asset_kind: AssetKind::Asset,
        ownership: AssetOwnership::ProjectAsset,
        forward_references: references.iter().map(|value| value.to_string()).collect(),
        value_references: BTreeSet::new(),
        owned_sublevels: BTreeSet::new(),
        analysis: analysis.clone(),
    };
    // /Engine/Shared has no project record; A and B cite it with different casing.
    let index = build_project_index(
        layout.clone(),
        MountTable::default_for(&layout),
        ProjectEntryPoints::default(),
        vec![
            record("/Game/A", &["/Engine/Shared"]),
            record("/Game/B", &["/engine/shared"]),
        ],
        Vec::new(),
        Vec::new(),
        2,
    );

    let a_refs = index.forward_references("/Game/A").unwrap();
    let b_refs = index.forward_references("/Game/B").unwrap();
    assert_eq!(a_refs, b_refs);
    assert_eq!(a_refs, &BTreeSet::from(["/Engine/Shared".to_string()]));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn level_instance_sublevels_join_the_owner_closure() {
    let root = temp_project("li_closure");
    let layout = ProjectLayout::discover(&root).unwrap();
    let bytes = minimal_package();
    let analysis = AssetAnalysisSummary::from_analysis(
        &PackageView::parse(&bytes).unwrap().analyze(AssetView::Full),
    );
    let record = |package: &str, owned: &[&str]| AssetRecord {
        package_path: package.to_string(),
        mount_root: "/Game".to_string(),
        file_path: root.join(format!(
            "Content/{}.uasset",
            package.trim_start_matches("/Game/")
        )),
        relative_path: format!("{}.uasset", package.trim_start_matches("/Game/")),
        asset_kind: AssetKind::Map,
        ownership: AssetOwnership::ProjectAsset,
        forward_references: BTreeSet::new(),
        value_references: BTreeSet::new(),
        owned_sublevels: owned.iter().map(|value| value.to_string()).collect(),
        analysis: analysis.clone(),
    };
    let index = build_project_index(
        layout.clone(),
        MountTable::default_for(&layout),
        ProjectEntryPoints::default(),
        vec![
            record("/Game/Maps/World", &["/Game/Maps/Sub"]),
            record("/Game/Maps/Sub", &[]),
        ],
        Vec::new(),
        Vec::new(),
        2,
    );

    let closure = index
        .ownership_closure
        .get("/Game/Maps/World")
        .expect("host map should own its Level Instance sublevel");
    assert!(closure.contains("/Game/Maps/World"));
    assert!(closure.contains("/Game/Maps/Sub"));

    std::fs::remove_dir_all(root).unwrap();
}

// An asset path typed into a graph pin is not a typed reference, so the linker
// tables never record it. Following those value-level edges is what keeps the
// target out of `unreachable_project_assets`, and the difference has to be
// reported rather than silently folded into the adjacency maps.
#[test]
fn value_level_edges_reach_a_package_the_linker_tables_do_not_record() {
    let root = temp_project("value_refs");
    let layout = ProjectLayout::discover(&root).unwrap();
    let bytes = minimal_package();
    let analysis = AssetAnalysisSummary::from_analysis(
        &PackageView::parse(&bytes).unwrap().analyze(AssetView::Full),
    );
    let record = |package: &str, value_references: &[&str]| AssetRecord {
        package_path: package.to_string(),
        mount_root: "/Game".to_string(),
        file_path: root.join(format!(
            "Content/{}.uasset",
            package.trim_start_matches("/Game/")
        )),
        relative_path: format!("{}.uasset", package.trim_start_matches("/Game/")),
        asset_kind: AssetKind::Asset,
        ownership: AssetOwnership::ProjectAsset,
        forward_references: BTreeSet::new(),
        value_references: value_references
            .iter()
            .map(|value| value.to_string())
            .collect(),
        owned_sublevels: BTreeSet::new(),
        analysis: analysis.clone(),
    };
    let mut entry_points = ProjectEntryPoints::default();
    entry_points.defaults.insert(
        "GameDefaultMap".to_string(),
        ConfigReference {
            key: "GameDefaultMap".to_string(),
            source: "Config/DefaultEngine.ini".to_string(),
            object_path: "/Game/Root.Root".to_string(),
            package_path: "/Game/Root".to_string(),
        },
    );
    let index = build_project_index(
        layout.clone(),
        MountTable::default_for(&layout),
        entry_points,
        vec![
            // Cites the widget only through a decoded value, with different casing.
            record("/Game/Root", &["/game/ui/w_popup"]),
            record("/Game/UI/W_Popup", &[]),
        ],
        Vec::new(),
        Vec::new(),
        2,
    );

    // Table adjacency stays exactly what the linker recorded.
    assert!(index.forward_references("/Game/Root").unwrap().is_empty());
    assert_eq!(
        index.value_references.get("/Game/Root"),
        Some(&BTreeSet::from(["/Game/UI/W_Popup".to_string()])),
        "the value edge is canonicalized and kept in its own map"
    );

    let reachability = &index.reachability;
    assert!(
        reachability
            .reachable_runtime_packages
            .contains("/Game/UI/W_Popup")
    );
    assert!(
        !reachability
            .unreachable_project_assets
            .contains("/Game/UI/W_Popup")
    );
    assert_eq!(
        reachability.value_reference_only_reachable,
        BTreeSet::from(["/Game/UI/W_Popup".to_string()]),
        "the widget is reachable only because the value edge was followed"
    );
    assert!(
        reachability.isolated_project_assets.is_empty(),
        "a value edge is a real edge in both directions"
    );

    std::fs::remove_dir_all(root).unwrap();
}

// A value-level path that names nothing the scan knows must not invent a target,
// or a stale pin string would keep resurrecting a package that no longer exists.
#[test]
fn an_unresolvable_value_path_creates_no_edge() {
    let root = temp_project("value_refs_unresolved");
    let layout = ProjectLayout::discover(&root).unwrap();
    let bytes = minimal_package();
    let analysis = AssetAnalysisSummary::from_analysis(
        &PackageView::parse(&bytes).unwrap().analyze(AssetView::Full),
    );
    let index = build_project_index(
        layout.clone(),
        MountTable::default_for(&layout),
        ProjectEntryPoints::default(),
        vec![AssetRecord {
            package_path: "/Game/Root".to_string(),
            mount_root: "/Game".to_string(),
            file_path: root.join("Content/Root.uasset"),
            relative_path: "Root.uasset".to_string(),
            asset_kind: AssetKind::Asset,
            ownership: AssetOwnership::ProjectAsset,
            forward_references: BTreeSet::new(),
            value_references: BTreeSet::from(["/Game/Deleted/Gone".to_string()]),
            owned_sublevels: BTreeSet::new(),
            analysis,
        }],
        Vec::new(),
        Vec::new(),
        1,
    );

    assert!(index.value_references.is_empty());
    assert!(index.reachability.value_reference_only_reachable.is_empty());

    std::fs::remove_dir_all(root).unwrap();
}
