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
