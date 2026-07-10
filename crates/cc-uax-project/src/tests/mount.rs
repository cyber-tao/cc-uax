use super::common::{minimal_package, temp_project};
use crate::{
    CachePathPolicy, MountTable, ProjectLayout, ProjectScanner, ScanFailureStage, ScanMode,
    ScanOptions, package_path_from_relative,
};

fn no_cache() -> ScanOptions {
    ScanOptions {
        mode: ScanMode::Strict,
        cache: CachePathPolicy::Disabled,
    }
}

#[test]
fn converts_mounted_disk_paths_to_canonical_package_paths() {
    assert_eq!(
        package_path_from_relative("Maps\\Main.UMAP", "Game/").unwrap(),
        "/Game/Maps/Main"
    );
    assert_eq!(
        package_path_from_relative("Widgets/W_HUD.uasset", "/Plugin").unwrap(),
        "/Plugin/Widgets/W_HUD"
    );
}

#[test]
fn scans_game_plugin_and_engine_mounts_once() {
    let root = temp_project("multi_mount");
    let plugin = root.join("Plugins/X/Content");
    let engine = root.join("Engine/Content");
    let unmapped = root.join("Unmapped");
    std::fs::create_dir_all(&plugin).unwrap();
    std::fs::create_dir_all(&engine).unwrap();
    std::fs::create_dir_all(&unmapped).unwrap();
    std::fs::write(root.join("Content/GameAsset.uasset"), minimal_package()).unwrap();
    std::fs::write(plugin.join("PluginAsset.uasset"), minimal_package()).unwrap();
    std::fs::write(engine.join("EngineAsset.uasset"), minimal_package()).unwrap();
    std::fs::write(unmapped.join("Ignored.uasset"), minimal_package()).unwrap();

    let layout = ProjectLayout::discover(&root).unwrap();
    let mounts = MountTable::parse(
        &layout,
        "/Game=Content,/Plugin=Plugins/X/Content,/Engine=Engine/Content",
    )
    .unwrap();
    let index = ProjectScanner::with_mounts(layout, mounts)
        .scan(no_cache())
        .unwrap();

    assert_eq!(index.stats.discovered, 3);
    assert_eq!(index.stats.indexed, 3);
    assert_eq!(index.stats.skipped, 0);
    assert!(index.asset("/Game/GameAsset").is_some());
    assert!(index.asset("/Plugin/PluginAsset").is_some());
    assert!(index.asset("/Engine/EngineAsset").is_some());
    assert!(index.asset("/Game/Ignored").is_none());

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn duplicate_disk_root_is_a_structured_mount_failure() {
    let root = temp_project("duplicate_mount_root");
    std::fs::write(root.join("Content/A.uasset"), minimal_package()).unwrap();
    let layout = ProjectLayout::discover(&root).unwrap();
    let mounts = MountTable::parse(&layout, "/Game=Content,/Plugin=Content").unwrap();
    let error = ProjectScanner::with_mounts(layout, mounts)
        .scan(no_cache())
        .unwrap_err();

    assert_eq!(error.index().stats.indexed, 1);
    assert_eq!(error.index().stats.skipped, 0);
    assert!(
        error
            .index()
            .failures
            .iter()
            .any(|failure| failure.stage == ScanFailureStage::Mount)
    );

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn overlapping_mounts_that_map_the_same_package_fail_indexing() {
    let root = temp_project("duplicate_package");
    let content_asset = root.join("Content/Foo/A.uasset");
    let other = root.join("Other");
    std::fs::create_dir_all(content_asset.parent().unwrap()).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(content_asset, minimal_package()).unwrap();
    std::fs::write(other.join("A.uasset"), minimal_package()).unwrap();
    let layout = ProjectLayout::discover(&root).unwrap();
    let mounts = MountTable::parse(&layout, "/Game=Content,/Game/Foo=Other").unwrap();
    let error = ProjectScanner::with_mounts(layout, mounts)
        .scan(no_cache())
        .unwrap_err();

    assert_eq!(error.index().stats.discovered, 2);
    assert_eq!(error.index().stats.indexed, 1);
    assert_eq!(error.index().stats.skipped, 0);
    assert!(error.index().failures.iter().any(|failure| {
        failure.stage == ScanFailureStage::Index
            && failure.message.contains("duplicate package path")
    }));

    std::fs::remove_dir_all(root).unwrap();
}
