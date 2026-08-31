use super::common::temp_project;
use crate::{CachePathPolicy, ProjectLayout, ProjectScanner, ScanFailureStage, ScanOptions};

fn scan(root: &std::path::Path) -> crate::ProjectIndex {
    ProjectScanner::new(ProjectLayout::discover(root).unwrap())
        .scan(ScanOptions {
            cache: CachePathPolicy::Disabled,
            ..ScanOptions::default()
        })
        .unwrap()
}

/// A Content tree shared by several platform `.uproject` files must scan, with
/// the unresolved descriptor reported rather than aborting the run.
#[test]
fn a_content_tree_shared_by_several_uprojects_still_scans() {
    let root = temp_project("entry_points_ambiguous");
    for name in ["Game.uproject", "Game_Steam.uproject", "Game_Pico.uproject"] {
        std::fs::write(root.join(name), b"{}").unwrap();
    }
    std::fs::create_dir_all(root.join("Config")).unwrap();
    std::fs::write(
        root.join("Config/DefaultEngine.ini"),
        "[/Script/EngineSettings.GameMapsSettings]\nGameDefaultMap=/Game/Maps/Boot.Boot\n",
    )
    .unwrap();

    let index = scan(&root);

    // The shared Config is still read, so entry points are not lost outright.
    assert_eq!(
        index
            .entry_points
            .reference("GameDefaultMap")
            .unwrap()
            .package_path,
        "/Game/Maps/Boot"
    );
    let diagnostic = index
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.stage == ScanFailureStage::Config)
        .unwrap_or_else(|| panic!("expected a config diagnostic: {:#?}", index.diagnostics));
    assert!(
        diagnostic.message.contains("Game_Steam.uproject"),
        "the diagnostic must name the candidates: {}",
        diagnostic.message
    );

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn parses_crlf_section_last_wins_and_generated_class_suffixes() {
    let root = temp_project("entry_points_crlf");
    std::fs::create_dir_all(root.join("Config")).unwrap();
    std::fs::write(
        root.join("Config/DefaultEngine.ini"),
        "[Wrong.Section]\r\n\
         GameDefaultMap=/Game/Wrong.Wrong\r\n\
         [/Script/EngineSettings.GameMapsSettings]\r\n\
         GameDefaultMap=/Game/Maps/First.First\r\n\
         gamedefaultmap=/Game/Maps/Last.Last\r\n\
         GameInstanceClass=Class'/Game/Framework/GI.GI_C'\r\n\
         GlobalDefaultGameMode=/Game/Framework/GM.GM_C\r\n",
    )
    .unwrap();

    let index = scan(&root);
    let game_map = index.entry_points.reference("GameDefaultMap").unwrap();
    assert_eq!(game_map.key, "GameDefaultMap");
    assert_eq!(game_map.source, "Config/DefaultEngine.ini");
    assert_eq!(game_map.object_path, "/Game/Maps/Last.Last");
    assert_eq!(game_map.package_path, "/Game/Maps/Last");
    assert_eq!(
        index
            .entry_points
            .reference("GameInstanceClass")
            .unwrap()
            .package_path,
        "/Game/Framework/GI"
    );
    assert_eq!(
        index
            .entry_points
            .reference("GlobalDefaultGameMode")
            .unwrap()
            .package_path,
        "/Game/Framework/GM"
    );
    assert!(index.diagnostics.is_empty());

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn later_default_files_and_platform_files_override_without_cross_platform_leakage() {
    let root = temp_project("entry_points_override");
    std::fs::create_dir_all(root.join("Config/Windows")).unwrap();
    std::fs::create_dir_all(root.join("Config/Linux")).unwrap();
    std::fs::write(
        root.join("Config/DefaultEngine.ini"),
        "[/Script/EngineSettings.GameMapsSettings]\n\
         EditorStartupMap=/Game/Maps/EngineDefault.EngineDefault\n",
    )
    .unwrap();
    std::fs::write(
        root.join("Config/DefaultGame.ini"),
        "[/Script/EngineSettings.GameMapsSettings]\n\
         EditorStartupMap=/Game/Maps/GameDefault.GameDefault\n",
    )
    .unwrap();
    std::fs::write(
        root.join("Config/Windows/WindowsEngine.ini"),
        "[/Script/EngineSettings.GameMapsSettings]\n\
         EditorStartupMap=/Game/Maps/Windows.Windows\n",
    )
    .unwrap();
    std::fs::write(
        root.join("Config/Windows/WindowsGame.ini"),
        "[/Script/EngineSettings.GameMapsSettings]\n\
         ServerDefaultMap=/Game/Maps/Server.Server\n",
    )
    .unwrap();

    let index = scan(&root);
    let defaults = index.entry_points.reference("EditorStartupMap").unwrap();
    assert_eq!(defaults.package_path, "/Game/Maps/GameDefault");
    assert_eq!(defaults.source, "Config/DefaultGame.ini");
    let windows = index
        .entry_points
        .reference_for_platform("windows", "EditorStartupMap")
        .unwrap();
    assert_eq!(windows.package_path, "/Game/Maps/Windows");
    assert_eq!(windows.source, "Config/Windows/WindowsEngine.ini");
    assert_eq!(
        index
            .entry_points
            .reference_for_platform("Windows", "ServerDefaultMap")
            .unwrap()
            .package_path,
        "/Game/Maps/Server"
    );
    assert_eq!(
        index
            .entry_points
            .reference_for_platform("Linux", "EditorStartupMap")
            .unwrap()
            .package_path,
        "/Game/Maps/GameDefault"
    );
    assert!(!index.entry_points.platforms.contains_key("Linux"));

    std::fs::remove_dir_all(root).unwrap();
}

// A `Config/<Platform>/` directory usually holds packaging or SDK settings only.
// Treating its existence as a platform override reported overrides that do not
// exist and listed every default root once per such directory.
#[test]
fn a_platform_config_without_entry_points_is_not_an_override() {
    let root = temp_project("entry_points_platform_noise");
    std::fs::create_dir_all(root.join("Config/Android")).unwrap();
    std::fs::write(
        root.join("Config/DefaultEngine.ini"),
        "[/Script/EngineSettings.GameMapsSettings]\n\
         GameDefaultMap=/Game/Maps/Start.Start\n",
    )
    .unwrap();
    std::fs::write(
        root.join("Config/Android/AndroidGame.ini"),
        "[/Script/AndroidRuntimeSettings.AndroidRuntimeSettings]\n\
         PackageName=com.example.game\n",
    )
    .unwrap();

    let index = scan(&root);
    assert!(
        index.entry_points.platforms.is_empty(),
        "{:#?}",
        index.entry_points.platforms
    );
    assert_eq!(index.reachability.configured_roots.len(), 1);

    std::fs::remove_dir_all(root).unwrap();
}

// The cook list is what a build actually ships. GameDefaultMap is frequently a
// developer map, so without these the real shipped maps looked unreachable.
#[test]
fn packaging_cook_roots_become_configured_roots() {
    let root = temp_project("entry_points_cook");
    std::fs::create_dir_all(root.join("Config")).unwrap();
    std::fs::write(
        root.join("Content/Dev.uasset"),
        super::common::minimal_package(),
    )
    .unwrap();
    std::fs::write(
        root.join("Content/Shipped.uasset"),
        super::common::minimal_package(),
    )
    .unwrap();
    std::fs::create_dir_all(root.join("Content/Extra")).unwrap();
    std::fs::write(
        root.join("Content/Extra/Always.uasset"),
        super::common::minimal_package(),
    )
    .unwrap();
    std::fs::write(
        root.join("Config/DefaultEngine.ini"),
        "[/Script/EngineSettings.GameMapsSettings]\n\
         GameDefaultMap=/Game/Dev.Dev\n",
    )
    .unwrap();
    std::fs::write(
        root.join("Config/DefaultGame.ini"),
        "[/Script/UnrealEd.ProjectPackagingSettings]\n\
         +MapsToCook=(FilePath=\"/Game/Shipped\")\n\
         +MapsToCook=(FilePath=\"/Game/Removed\")\n\
         -MapsToCook=(FilePath=\"/Game/Removed\")\n\
         +DirectoriesToAlwaysCook=(Path=\"/Game/Extra\")\n",
    )
    .unwrap();

    let index = scan(&root);
    let roots = &index.reachability.configured_roots;
    let resolved = |package: &str| {
        roots
            .iter()
            .find(|root| root.package_path == package)
            .unwrap_or_else(|| panic!("missing root {package}: {roots:#?}"))
    };
    assert_eq!(resolved("/Game/Shipped").key, "MapsToCook");
    assert_eq!(
        resolved("/Game/Shipped").resolution,
        crate::RootResolution::Indexed
    );
    // A `-MapsToCook` line removes the entry rather than adding a second one.
    assert!(
        roots
            .iter()
            .all(|root| root.package_path != "/Game/Removed")
    );
    // A cook directory expands to the indexed packages beneath it.
    assert_eq!(
        resolved("/Game/Extra/Always").key,
        "DirectoriesToAlwaysCook"
    );
    assert!(
        index
            .reachability
            .reachable_runtime_packages
            .contains("/Game/Extra/Always")
    );
    assert!(index.reachability.unreachable_project_assets.is_empty());

    std::fs::remove_dir_all(root).unwrap();
}

// `resolved_package` alone never proved a root exists in the scan: the canonical
// map is seeded from reference targets too, so an unmounted /Engine or /Script
// path resolved to a name and looked as real as an indexed asset.
#[test]
fn root_resolution_separates_indexed_assets_from_names_only() {
    let root = temp_project("entry_points_resolution");
    std::fs::create_dir_all(root.join("Config")).unwrap();
    std::fs::write(
        root.join("Content/Start.uasset"),
        super::common::package_with_soft_refs(&["/Engine/Maps/Entry"]),
    )
    .unwrap();
    std::fs::write(
        root.join("Config/DefaultEngine.ini"),
        "[/Script/EngineSettings.GameMapsSettings]\n\
         GameDefaultMap=/Game/Start.Start\n\
         ServerDefaultMap=/Engine/Maps/Entry.Entry\n\
         GameInstanceClass=/Script/Engine.GameInstance\n",
    )
    .unwrap();

    let index = scan(&root);
    let root_for = |key: &str| {
        index
            .reachability
            .configured_roots
            .iter()
            .find(|root| root.key == key)
            .unwrap_or_else(|| panic!("missing {key}"))
    };
    assert_eq!(
        root_for("GameDefaultMap").resolution,
        crate::RootResolution::Indexed
    );
    // Referenced by a scanned asset but outside every mount, so never parsed.
    assert_eq!(
        root_for("ServerDefaultMap").resolution,
        crate::RootResolution::ReferencedOnly
    );
    // Nothing in the scan knows this name at all.
    assert_eq!(
        root_for("GameInstanceClass").resolution,
        crate::RootResolution::Unresolved
    );
    assert_eq!(root_for("GameInstanceClass").resolved_package, None);

    std::fs::remove_dir_all(root).unwrap();
}

// `TransitionMap=` and `GlobalDefaultServerGameMode=None` are how stock UE
// project configs spell "no object". They are not invalid paths, so they must not
// produce a diagnostic: any non-cache diagnostic forces the project report to
// `partial`, which would make a healthy project look broken.
#[test]
fn empty_and_none_entry_point_values_are_an_explicit_unset_not_a_diagnostic() {
    let root = temp_project("entry_points_unset");
    std::fs::create_dir_all(root.join("Config")).unwrap();
    std::fs::write(
        root.join("Config/DefaultEngine.ini"),
        "[/Script/EngineSettings.GameMapsSettings]\n\
         EditorStartupMap=/Game/Maps/Start.Start\n\
         LocalMapOptions=\n\
         TransitionMap=\n\
         GameInstanceClass=none\n\
         GlobalDefaultServerGameMode=None\n\
         ServerDefaultMap=   \n\
         GlobalDefaultGameMode=\"\"\n",
    )
    .unwrap();

    let index = scan(&root);
    assert!(
        index.diagnostics.is_empty(),
        "an explicit unset is not a diagnostic: {:#?}",
        index.diagnostics
    );
    for key in [
        "TransitionMap",
        "GameInstanceClass",
        "GlobalDefaultServerGameMode",
        "ServerDefaultMap",
        "GlobalDefaultGameMode",
    ] {
        assert!(
            index.entry_points.reference(key).is_none(),
            "{key} is unset and must not become a reachability root"
        );
    }
    assert_eq!(
        index
            .entry_points
            .reference("EditorStartupMap")
            .unwrap()
            .package_path,
        "/Game/Maps/Start"
    );

    std::fs::remove_dir_all(root).unwrap();
}

// UE does not clear an already-configured property because a later line failed to
// parse. Dropping the earlier value would silently shrink the set of reachability
// roots and inflate `unreachable_project_assets`.
#[test]
fn an_invalid_value_keeps_the_earlier_value_while_an_unset_clears_it() {
    let root = temp_project("entry_points_keep");
    std::fs::create_dir_all(root.join("Config")).unwrap();
    std::fs::write(
        root.join("Config/DefaultEngine.ini"),
        "[/Script/EngineSettings.GameMapsSettings]\n\
         GameDefaultMap=/Game/Maps/Keep.Keep\n\
         GameDefaultMap=not-a-path\n\
         EditorStartupMap=/Game/Maps/Drop.Drop\n\
         EditorStartupMap=None\n",
    )
    .unwrap();

    let index = scan(&root);
    assert_eq!(
        index
            .entry_points
            .reference("GameDefaultMap")
            .unwrap()
            .package_path,
        "/Game/Maps/Keep",
        "an invalid later value must not erase a valid earlier one"
    );
    assert!(
        index.entry_points.reference("EditorStartupMap").is_none(),
        "an explicit unset still wins over an earlier value"
    );
    assert_eq!(index.diagnostics.len(), 1, "{:#?}", index.diagnostics);
    assert_eq!(index.diagnostics[0].stage, ScanFailureStage::Config);
    assert!(
        index.diagnostics[0].message.contains("GameDefaultMap"),
        "{}",
        index.diagnostics[0].message
    );

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_paths_warn_without_exposing_values_or_absolute_source_paths() {
    let root = temp_project("entry_points_redaction");
    std::fs::create_dir_all(root.join("Config")).unwrap();
    std::fs::write(
        root.join("Config/DefaultEngine.ini"),
        "[/Script/EngineSettings.GameMapsSettings]\n\
         TransitionMap=C:\\Private\\Secret.Secret\n\
         SecretToken=do-not-report-this-value\n",
    )
    .unwrap();

    let index = scan(&root);
    assert!(index.entry_points.reference("TransitionMap").is_none());
    assert_eq!(index.diagnostics.len(), 1);
    let diagnostic = &index.diagnostics[0];
    assert_eq!(diagnostic.stage, ScanFailureStage::Config);
    assert_eq!(
        diagnostic.path,
        std::path::Path::new("Config/DefaultEngine.ini")
    );
    assert!(diagnostic.path.is_relative());
    assert!(!diagnostic.message.contains("C:"));
    assert!(!diagnostic.message.contains("Private"));
    assert!(!diagnostic.message.contains("do-not-report"));
    assert!(
        !diagnostic
            .message
            .contains(&root.to_string_lossy().to_string())
    );

    std::fs::remove_dir_all(root).unwrap();
}
