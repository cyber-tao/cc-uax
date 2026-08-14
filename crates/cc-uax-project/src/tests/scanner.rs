use super::common::{minimal_package, package_with_soft_refs, temp_project, ue4_package};
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

// UE5 projects routinely contain UE4 assets that were never resaved. Such a
// package is real evidence about the project, so it is indexed as `unsupported`
// and does not abort a strict scan; only an unreadable package is a failure.
#[test]
fn out_of_scope_packages_are_indexed_as_unsupported_and_do_not_fail_strict_mode() {
    let root = temp_project("unsupported");
    std::fs::write(root.join("Content/Valid.uasset"), minimal_package()).unwrap();
    std::fs::write(root.join("Content/Legacy.uasset"), ue4_package()).unwrap();
    let scanner = ProjectScanner::new(ProjectLayout::discover(&root).unwrap());

    let index = scanner.scan(scan_options(ScanMode::Strict)).unwrap();
    assert_eq!(index.stats.discovered, 2);
    assert_eq!(index.stats.indexed, 2);
    assert_eq!(index.stats.failed, 0);
    assert!(index.failures.is_empty(), "{:#?}", index.failures);
    assert_eq!(index.analysis.unsupported_assets, 1);
    assert_eq!(index.analysis.complete_assets, 1);
    assert_eq!(index.analysis.status, cc_uax_core::AnalysisStatus::Partial);
    assert_scan_accounting(&index);

    let legacy = index.asset("/Game/Legacy").expect("UE4 asset is indexed");
    assert_eq!(
        legacy.analysis.status,
        cc_uax_core::AnalysisStatus::Unsupported
    );
    let reason = legacy
        .analysis
        .unsupported_reason
        .as_deref()
        .expect("an unsupported asset records why it is out of scope");
    assert!(reason.contains("FileVersionUE5"), "{reason}");
    assert!(
        index
            .reachability
            .unsupported_packages
            .contains("/Game/Legacy"),
        "an unsupported package is listed in reachability"
    );

    // A package that is not readable at all is still a strict failure.
    std::fs::write(root.join("Content/Broken.uasset"), b"not a package").unwrap();
    let error = scanner.scan(scan_options(ScanMode::Strict)).unwrap_err();
    assert_eq!(error.index().failures.len(), 1);
    assert_eq!(
        error.index().failures[0].stage,
        ScanFailureStage::Parse,
        "{:#?}",
        error.index().failures
    );
    assert_eq!(error.index().analysis.unsupported_assets, 1);

    std::fs::remove_dir_all(root).unwrap();
}

// A warm cache must replay the same classification it recorded: an out-of-scope
// package stays `unsupported` evidence instead of becoming a Parse failure.
#[test]
fn cached_out_of_scope_packages_replay_as_unsupported() {
    let root = temp_project("unsupported_cache");
    std::fs::write(root.join("Content/Legacy.uasset"), ue4_package()).unwrap();
    let cache_file = root.join("cache/index.sqlite");
    let scanner = ProjectScanner::new(ProjectLayout::discover(&root).unwrap());
    let options = ScanOptions {
        mode: ScanMode::Strict,
        cache: CachePathPolicy::CustomFile(cache_file),
    };

    let cold = scanner.scan(options.clone()).unwrap();
    assert_eq!(cold.stats.cache_misses, 1);
    assert_eq!(cold.analysis.unsupported_assets, 1);

    let warm = scanner.scan(options).unwrap();
    assert_eq!(warm.stats.cache_hits, 1);
    assert_eq!(warm.stats.cached_parse_failures, 0);
    assert!(warm.failures.is_empty(), "{:#?}", warm.failures);
    assert_eq!(warm.analysis.unsupported_assets, 1);
    assert_eq!(
        warm.asset("/Game/Legacy").unwrap().analysis,
        cold.asset("/Game/Legacy").unwrap().analysis,
        "a warm cache hit must reproduce the cold-run summary"
    );

    std::fs::remove_dir_all(root).unwrap();
}

fn try_symlink_file(target: &std::path::Path, link: &std::path::Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link)
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_file(target, link)
    }
}

#[test]
fn skipped_symlinks_are_counted_and_surfaced_as_diagnostics() {
    let root = temp_project("symlink");
    let real = root.join("Content/Real.uasset");
    std::fs::write(&real, minimal_package()).unwrap();
    // Symlink creation can require privileges (e.g. Windows); skip when it fails.
    if try_symlink_file(&real, &root.join("Content/Link.uasset")).is_err() {
        std::fs::remove_dir_all(&root).unwrap();
        return;
    }

    let scanner = ProjectScanner::new(ProjectLayout::discover(&root).unwrap());
    let index = scanner.scan(scan_options(ScanMode::Strict)).unwrap();
    std::fs::remove_dir_all(&root).unwrap();

    assert_eq!(index.stats.indexed, 1);
    assert_eq!(index.stats.skipped_symlinks, 1);
    assert!(
        index.diagnostics.iter().any(|diagnostic| {
            diagnostic.stage == ScanFailureStage::Discovery
                && diagnostic.severity == ScanDiagnosticSeverity::Warning
        }),
        "a skipped symlink should surface a Discovery warning"
    );
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
    let grouped_regions = index
        .assets
        .values()
        .flat_map(|asset| &asset.analysis.known_opaque.groups)
        .map(|group| group.regions)
        .sum::<usize>();
    assert_eq!(
        index.analysis.coverage.known_opaque_regions,
        grouped_regions
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
fn reachability_starts_from_configured_roots_and_includes_ownership_closure() {
    let root = temp_project("reachability");
    let content = root.join("Content");
    let map = content.join("Maps/World.umap");
    let actor = content.join("__ExternalActors__/Maps/World/0/AA/Actor.uasset");
    let isolated = content.join("Props/Unused.uasset");
    let config = root.join("Config/DefaultEngine.ini");
    std::fs::create_dir_all(map.parent().unwrap()).unwrap();
    std::fs::create_dir_all(actor.parent().unwrap()).unwrap();
    std::fs::create_dir_all(isolated.parent().unwrap()).unwrap();
    std::fs::create_dir_all(config.parent().unwrap()).unwrap();
    std::fs::write(&map, minimal_package()).unwrap();
    std::fs::write(&actor, minimal_package()).unwrap();
    std::fs::write(&isolated, minimal_package()).unwrap();
    std::fs::write(
        &config,
        "[/Script/EngineSettings.GameMapsSettings]\nGameDefaultMap=/Game/Maps/World.World\n",
    )
    .unwrap();

    let scanner = ProjectScanner::new(ProjectLayout::discover(&root).unwrap());
    let index = scanner.scan(scan_options(ScanMode::Strict)).unwrap();

    assert_eq!(index.reachability.configured_roots.len(), 1);
    assert_eq!(
        index.reachability.configured_roots[0]
            .resolved_package
            .as_deref(),
        Some("/Game/Maps/World")
    );
    assert!(
        index
            .reachability
            .reachable_runtime_packages
            .contains("/Game/Maps/World")
    );
    assert!(
        index
            .reachability
            .ownership_closure_members
            .contains("/Game/__ExternalActors__/Maps/World/0/AA/Actor")
    );
    assert!(
        index
            .reachability
            .reachable_runtime_packages
            .contains("/Game/__ExternalActors__/Maps/World/0/AA/Actor")
    );
    assert!(
        index
            .reachability
            .unreachable_project_assets
            .contains("/Game/Props/Unused")
    );
    assert!(
        index
            .reachability
            .isolated_project_assets
            .contains("/Game/Props/Unused")
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
            .flat_map(|asset| &asset.analysis.known_opaque.groups)
            .map(|group| group.regions)
            .sum::<usize>()
    );
    // Byte conservation (P0): every export byte is decoded or classified opaque.
    assert_eq!(
        index.analysis.coverage.unclassified_bytes, 0,
        "aggregate unclassified export bytes must be 0"
    );
    assert_eq!(
        index
            .assets
            .values()
            .map(|asset| asset.analysis.coverage.unclassified_bytes)
            .sum::<u64>(),
        0,
        "no asset may leave export bytes unclassified"
    );
    // Aggregated opaque_bytes equals the sum of per-asset totals (P1).
    assert_eq!(
        index.analysis.coverage.opaque_bytes,
        index
            .assets
            .values()
            .map(|asset| asset.analysis.coverage.opaque_bytes)
            .sum::<u64>()
    );
    // Adjacency carries no package self-loop (P2).
    let self_loops = index
        .forward
        .iter()
        .filter(|(package, references)| references.contains(package.as_str()))
        .count();
    assert_eq!(self_loops, 0, "adjacency must not contain self-loops");
    assert!(
        index.failures.is_empty(),
        "real project scan failures: {:#?}",
        index.failures
    );
}

#[test]
fn synthetic_project_scan_builds_adjacency_without_self_loops() {
    // CI-runnable analogue of scans_real_project_from_environment: hand-built packages
    // with cross-package soft references, so the full file-scan -> parse -> adjacency ->
    // aggregate path runs in CI without external assets. A references B and itself; the
    // self-reference must be dropped while the real edges remain.
    let root = temp_project("synthetic");
    std::fs::write(
        root.join("Content/A.uasset"),
        package_with_soft_refs(&["/Game/B", "/Game/A"]),
    )
    .unwrap();
    std::fs::write(
        root.join("Content/B.uasset"),
        package_with_soft_refs(&["/Game/A"]),
    )
    .unwrap();
    let scanner = ProjectScanner::new(ProjectLayout::discover(&root).unwrap());
    let index = scanner.scan(scan_options(ScanMode::AllowPartial)).unwrap();

    assert_eq!(index.stats.discovered, 2);
    assert_eq!(index.stats.indexed, 2);
    assert_scan_accounting(&index);
    assert_eq!(index.analysis.assets, 2);
    assert_eq!(index.analysis.coverage.unclassified_bytes, 0);

    // A -> B survives; the A -> A self-reference is excluded.
    let a_refs = index.forward_references("/Game/A").unwrap();
    assert!(a_refs.contains("/Game/B"));
    assert!(!a_refs.contains("/Game/A"));
    assert!(
        index
            .forward_references("/Game/B")
            .unwrap()
            .contains("/Game/A")
    );
    assert!(
        index
            .reverse_referencers("/Game/B")
            .unwrap()
            .contains("/Game/A")
    );

    let self_loops = index
        .forward
        .iter()
        .filter(|(package, references)| references.contains(package.as_str()))
        .count();
    assert_eq!(self_loops, 0, "adjacency must not contain self-loops");
    assert!(index.failures.is_empty(), "failures: {:#?}", index.failures);

    std::fs::remove_dir_all(root).unwrap();
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
fn fresh_positive_cache_reuses_cached_analysis_summary() {
    let root = temp_project("cache_analysis_summary");
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
    let mut cached_analysis = first.asset("/Game/Cached").unwrap().analysis.clone();
    cached_analysis.status = cc_uax_core::AnalysisStatus::Partial;
    cached_analysis.coverage.bytes_total = 12345;
    let connection = rusqlite::Connection::open(&cache_file).unwrap();
    connection
        .execute(
            "UPDATE package_refs SET analysis = ?1",
            [serde_json::to_string(&cached_analysis).unwrap()],
        )
        .unwrap();
    drop(connection);

    let cached = scanner.scan(options()).unwrap();
    let summary = &cached.asset("/Game/Cached").unwrap().analysis;
    assert_eq!(cached.stats.cache_hits, 1);
    assert_eq!(cached.stats.cache_misses, 0);
    assert_eq!(summary.status, cc_uax_core::AnalysisStatus::Partial);
    assert_eq!(summary.coverage.bytes_total, 12345);
    assert_eq!(cached.analysis.status, cc_uax_core::AnalysisStatus::Partial);

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
fn unavailable_custom_cache_is_recorded_when_partial_results_are_allowed() {
    let root = temp_project("custom_cache_error");
    std::fs::write(root.join("Content/A.uasset"), minimal_package()).unwrap();
    let cache_parent = root.join("cache-parent");
    std::fs::write(&cache_parent, b"not a directory").unwrap();
    let scanner = ProjectScanner::new(ProjectLayout::discover(&root).unwrap());

    let index = scanner
        .scan(ScanOptions {
            mode: ScanMode::AllowPartial,
            cache: CachePathPolicy::CustomFile(cache_parent.join("index.sqlite")),
        })
        .expect("AllowPartial must return the index when only the cache fails");

    assert_eq!(index.stats.indexed, 1);
    assert!(index.failures.iter().any(|failure| {
        failure.stage == ScanFailureStage::Cache
            && failure.message.contains("create cache directory")
    }));

    let strict = scanner
        .scan(ScanOptions {
            mode: ScanMode::Strict,
            cache: CachePathPolicy::CustomFile(cache_parent.join("index.sqlite")),
        })
        .unwrap_err();
    assert!(strict.index().failures.iter().any(|failure| {
        failure.stage == ScanFailureStage::Cache
            && failure.message.contains("create cache directory")
    }));

    std::fs::remove_dir_all(root).unwrap();
}
