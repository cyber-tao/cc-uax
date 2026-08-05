use super::common::{minimal_package, temp_project};
use crate::scanner::build_project_index;
use crate::{
    AssetAnalysisSummary, AssetKind, AssetOwnership, AssetRecord, MountTable, ProjectEntryPoints,
    ProjectLayout,
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
