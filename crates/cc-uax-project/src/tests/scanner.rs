use super::common::{minimal_package, temp_project};
use crate::{
    CachePathPolicy, MountTable, ProjectIndex, ProjectLayout, ProjectScanner,
    ScanDiagnosticSeverity, ScanFailureStage, ScanMode, ScanOptions,
};
use std::collections::BTreeSet;

fn scan_options(mode: ScanMode) -> ScanOptions {
    ScanOptions {
        mode,
        cache: CachePathPolicy::Disabled,
    }
}

fn assert_scan_accounting(index: &ProjectIndex) {
    let failed_asset_count = index
        .failures
        .iter()
        .filter(|failure| {
            matches!(
                failure.stage,
                ScanFailureStage::Read | ScanFailureStage::Parse | ScanFailureStage::Index
            )
        })
        .map(|failure| failure.path.clone())
        .collect::<BTreeSet<_>>()
        .len();
    assert_eq!(index.stats.skipped, 0);
    assert_eq!(
        index.stats.discovered,
        index.stats.indexed + failed_asset_count
    );
}

#[test]
fn strict_returns_partial_index_and_allow_partial_returns_success() {
    let root = temp_project("partial");
    std::fs::write(root.join("Content/Valid.uasset"), minimal_package()).unwrap();
    std::fs::write(root.join("Content/Broken.uasset"), b"not a package").unwrap();
    let scanner = ProjectScanner::new(ProjectLayout::discover(&root).unwrap());

    let error = scanner.scan(scan_options(ScanMode::Strict)).unwrap_err();
    assert_eq!(error.index().stats.discovered, 2);
    assert_eq!(error.index().stats.indexed, 1);
    assert_eq!(error.index().failures.len(), 1);
    assert_eq!(
        error.index().analysis.status,
        cc_uax_core::AnalysisStatus::Partial
    );
    assert_eq!(error.index().analysis.scan_failures, 1);
    assert_scan_accounting(error.index());

    let index = scanner.scan(scan_options(ScanMode::AllowPartial)).unwrap();
    assert_eq!(index.stats.discovered, 2);
    assert_eq!(index.stats.indexed, 1);
    assert_eq!(index.stats.failed, 1);
    assert!(index.asset("/Game/Valid").is_some());
    assert_scan_accounting(&index);

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn world_partition_ownership_is_isolated_by_mount_root() {
    let root = temp_project("world_partition_mount_isolation");
    let game = root.join("Content");
    let plugin = root.join("Plugins/X/Content");
    for mount in [&game, &plugin] {
        let map = mount.join("Maps/Shared.umap");
        let actor = mount.join("__ExternalActors__/Maps/Shared/0/AA/Actor.uasset");
        std::fs::create_dir_all(map.parent().unwrap()).unwrap();
        std::fs::create_dir_all(actor.parent().unwrap()).unwrap();
        std::fs::write(map, minimal_package()).unwrap();
        std::fs::write(actor, minimal_package()).unwrap();
    }

    let layout = ProjectLayout::discover(&root).unwrap();
    let mounts = MountTable::parse(&layout, "/Game=Content,/Plugin=Plugins/X/Content").unwrap();
    let index = ProjectScanner::with_mounts(layout, mounts)
        .scan(scan_options(ScanMode::Strict))
        .unwrap();

    assert_eq!(
        index
            .ownership_root("/Game/__ExternalActors__/Maps/Shared/0/AA/Actor")
            .unwrap(),
        "/Game/Maps/Shared"
    );
    assert_eq!(
        index
            .ownership_root("/Plugin/__ExternalActors__/Maps/Shared/0/AA/Actor")
            .unwrap(),
        "/Plugin/Maps/Shared"
    );
    assert_scan_accounting(&index);

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn resolves_world_partition_actor_and_object_ownership_closure() {
    let root = temp_project("world_partition");
    let content = root.join("Content");
    let map = content.join("Maps/World.umap");
    let actor = content.join("__ExternalActors__/Maps/World/0/AA/Actor.uasset");
    let object = content
        .join("__ExternalObjects__/__ExternalActors__/Maps/World/0/AA/Actor/0/BB/Object.uasset");
    std::fs::create_dir_all(map.parent().unwrap()).unwrap();
    std::fs::create_dir_all(actor.parent().unwrap()).unwrap();
    std::fs::create_dir_all(object.parent().unwrap()).unwrap();
    std::fs::write(&map, minimal_package()).unwrap();
    std::fs::write(&actor, minimal_package()).unwrap();
    std::fs::write(&object, minimal_package()).unwrap();

    let scanner = ProjectScanner::new(ProjectLayout::discover(&root).unwrap());
    let index = scanner.scan(scan_options(ScanMode::Strict)).unwrap();
    let closure = index.closure_for("/Game/Maps/World").unwrap();

    assert_eq!(index.stats.external_actors, 1);
    assert_eq!(index.stats.external_objects, 1);
    assert_eq!(index.stats.owned_external_packages, 2);
    assert_eq!(index.stats.unowned_external_packages, 0);
    assert_scan_accounting(&index);
    assert_eq!(index.analysis.assets, index.stats.indexed);
    assert_eq!(
        index.analysis.complete_assets
            + index.analysis.partial_assets
            + index.analysis.unsupported_assets,
        index.analysis.assets
    );
    let opaque_identities = index
        .assets
        .values()
        .map(|asset| asset.analysis.known_opaque.identities.len())
        .sum::<usize>();
    assert_eq!(
        index.analysis.coverage.known_opaque_regions,
        opaque_identities
    );
    assert_eq!(closure.len(), 3);
    assert!(closure.contains("/Game/Maps/World"));
    assert!(closure.contains("/Game/__ExternalActors__/Maps/World/0/AA/Actor"));
    assert!(closure.contains(
        "/Game/__ExternalObjects__/__ExternalActors__/Maps/World/0/AA/Actor/0/BB/Object"
    ));
    assert_eq!(
        index
            .ownership_root("/Game/__ExternalActors__/Maps/World/0/AA/Actor")
            .unwrap(),
        "/Game/Maps/World"
    );
    assert_eq!(
        index
            .ownership_root(
                "/Game/__ExternalObjects__/__ExternalActors__/Maps/World/0/AA/Actor/0/BB/Object",
            )
            .unwrap(),
        "/Game/Maps/World"
    );

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "requires CC_UAX_TEST_PROJECT to point at a local UE project"]
fn scans_real_project_from_environment() {
    let project = std::env::var_os("CC_UAX_TEST_PROJECT")
        .expect("CC_UAX_TEST_PROJECT must point at a project root or Content directory");
    let scanner = ProjectScanner::new(ProjectLayout::discover(project).unwrap());
    let index = scanner.scan(scan_options(ScanMode::AllowPartial)).unwrap();

    assert!(index.stats.discovered > 0);
    assert_eq!(index.stats.indexed, index.stats.discovered);
    assert_eq!(index.stats.unowned_external_packages, 0);
    assert_scan_accounting(&index);
    assert_eq!(index.analysis.assets, index.stats.indexed);
    assert_eq!(
        index.analysis.complete_assets
            + index.analysis.partial_assets
            + index.analysis.unsupported_assets,
        index.analysis.assets
    );
    assert_eq!(
        index.analysis.coverage.known_opaque_regions,
        index
            .assets
            .values()
            .map(|asset| asset.analysis.known_opaque.identities.len())
            .sum::<usize>()
    );
    assert!(
        index.failures.is_empty(),
        "real project scan failures: {:#?}",
        index.failures
    );
}

#[test]
fn stale_cache_entries_are_reparsed_and_negative_hits_remain_failures() {
    let root = temp_project("cache_invalidation");
    let asset = root.join("Content/Cached.uasset");
    let cache_file = root.join("scan-cache.sqlite");
    std::fs::write(&asset, minimal_package()).unwrap();
    let scanner = ProjectScanner::new(ProjectLayout::discover(&root).unwrap());
    let options = || ScanOptions {
        mode: ScanMode::AllowPartial,
        cache: CachePathPolicy::CustomFile(cache_file.clone()),
    };

    let first = scanner.scan(options()).unwrap();
    assert_eq!(first.stats.cache_hits, 0);
    assert_eq!(first.stats.cache_misses, 1);
    assert!(first.failures.is_empty());

    let cached_valid = scanner.scan(options()).unwrap();
    assert_eq!(cached_valid.stats.cache_hits, 1);
    assert_eq!(cached_valid.stats.cache_misses, 0);
    assert!(cached_valid.failures.is_empty());
    assert_eq!(
        first.asset("/Game/Cached").unwrap().analysis,
        cached_valid.asset("/Game/Cached").unwrap().analysis
    );
    assert_eq!(first.analysis, cached_valid.analysis);

    std::fs::write(&asset, b"broken package").unwrap();
    let stale = scanner.scan(options()).unwrap();
    assert_eq!(stale.stats.cache_hits, 0);
    assert_eq!(stale.stats.cache_misses, 1);
    assert_eq!(stale.failures.len(), 1);
    assert_eq!(stale.stats.indexed, 0);
    assert_scan_accounting(&stale);

    let cached_partial = scanner.scan(options()).unwrap();
    assert_eq!(cached_partial.stats.cache_hits, 1);
    assert_eq!(cached_partial.stats.cached_parse_failures, 1);
    assert_eq!(cached_partial.failures.len(), 1);

    let strict = scanner
        .scan(ScanOptions {
            mode: ScanMode::Strict,
            cache: CachePathPolicy::CustomFile(cache_file),
        })
        .unwrap_err();
    assert_eq!(strict.index().stats.cache_hits, 1);
    assert_eq!(strict.index().stats.cached_parse_failures, 1);
    assert_eq!(strict.index().failures.len(), 1);

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn unavailable_system_cache_warns_without_failing_strict_scan() {
    let root = temp_project("system_cache_warning");
    std::fs::write(root.join("Content/A.uasset"), minimal_package()).unwrap();
    let layout = ProjectLayout::discover(&root).unwrap();
    let cache_file = CachePathPolicy::System.resolve(&layout).unwrap().unwrap();
    let cache_directory = cache_file.parent().unwrap();
    std::fs::create_dir_all(cache_directory.parent().unwrap()).unwrap();
    std::fs::write(cache_directory, b"blocks cache directory creation").unwrap();

    let index = ProjectScanner::new(layout)
        .scan(ScanOptions::default())
        .unwrap();

    assert_eq!(index.stats.indexed, 1);
    assert!(index.failures.is_empty());
    assert_eq!(index.diagnostics.len(), 1);
    assert_eq!(
        index.diagnostics[0].severity,
        ScanDiagnosticSeverity::Warning
    );
    assert_eq!(index.diagnostics[0].stage, ScanFailureStage::Cache);

    std::fs::remove_file(cache_directory).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn unavailable_custom_cache_is_fatal_even_when_partial_results_are_allowed() {
    let root = temp_project("custom_cache_error");
    std::fs::write(root.join("Content/A.uasset"), minimal_package()).unwrap();
    let cache_parent = root.join("cache-parent");
    std::fs::write(&cache_parent, b"not a directory").unwrap();
    let scanner = ProjectScanner::new(ProjectLayout::discover(&root).unwrap());

    let error = scanner
        .scan(ScanOptions {
            mode: ScanMode::AllowPartial,
            cache: CachePathPolicy::CustomFile(cache_parent.join("index.sqlite")),
        })
        .unwrap_err();

    assert_eq!(error.index().stats.indexed, 1);
    assert!(error.index().diagnostics.is_empty());
    assert!(error.index().failures.iter().any(|failure| {
        failure.stage == ScanFailureStage::Cache
            && failure.message.contains("create cache directory")
    }));

    std::fs::remove_dir_all(root).unwrap();
}
