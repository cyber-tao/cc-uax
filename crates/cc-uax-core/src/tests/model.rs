use crate::collect_package_references;
use crate::name::NameMap;
use crate::object::PackageIndex;
use crate::property::TypeName;
use crate::{
    ByteRangePreview, Diagnostic, MountMap, OutputSections, Severity, package_path_from_relative,
    package_path_from_relative_with_mounts,
};

#[test]
fn package_index_semantics() {
    assert!(PackageIndex(0).is_null());

    assert_eq!(PackageIndex(1).export_index(), Some(0));

    assert_eq!(PackageIndex(5).export_index(), Some(4));

    assert!(PackageIndex(5).import_index().is_none());

    assert_eq!(PackageIndex(-1).import_index(), Some(0));

    assert_eq!(PackageIndex(-3).import_index(), Some(2));
}

#[test]
fn name_resolution_with_number() {
    let names = NameMap {
        names: vec!["Foo".to_string(), "Bar".to_string()],
    };

    assert_eq!(names.resolve(0, 0), "Foo");

    assert_eq!(names.resolve(1, 3), "Bar_2");

    assert_eq!(names.resolve(99, 0), "<invalid_name#99>");
}

#[test]
fn typename_display_nested() {
    let t = TypeName {
        name: "MapProperty".to_string(),

        params: vec![
            TypeName {
                name: "NameProperty".to_string(),

                params: vec![],
            },
            TypeName {
                name: "IntProperty".to_string(),

                params: vec![],
            },
        ],
    };

    assert_eq!(t.display(), "MapProperty(NameProperty,IntProperty)");

    let simple = TypeName {
        name: "IntProperty".to_string(),

        params: vec![],
    };

    assert_eq!(simple.display(), "IntProperty");
}

#[test]
fn diagnostic_json_shape_is_stable() {
    let diagnostic = Diagnostic::warning(
        "property_value_fallback",
        "/exports/2/properties/Health",
        "decoded property value using fallback bytes",
    )
    .with_offset(128)
    .with_context(serde_json::json!({
        "preview": ByteRangePreview {
            start: 128,
            end: 132,
            size: 4,
            preview: "01020304".to_string(),
        }
    }));

    let json = serde_json::to_value(&diagnostic).unwrap();
    assert_eq!(json["severity"], "warning");
    assert_eq!(json["code"], "property_value_fallback");
    assert_eq!(json["path"], "/exports/2/properties/Health");
    assert_eq!(json["offset"], 128);
    assert_eq!(json["context"]["preview"]["size"], 4);

    let info = Diagnostic::new(Severity::Info, "note", "/", "message");
    let info_json = serde_json::to_value(&info).unwrap();
    assert_eq!(info_json["severity"], "info");
    assert!(info_json.get("offset").is_none());
    assert!(info_json.get("context").is_none());
}

#[test]
fn package_references_partition() {
    let imports = vec![
        ("Package", "/Game/Foo/BP_Bar"),
        ("Package", "/Script/Engine"),
        ("Class", "Actor"),
        ("Package", "/Game/Foo/BP_Bar"),
        ("Package", "/Script/CoreUObject"),
        ("Package", "/Game/Audio/SC_Test"),
        ("Package", ""),
    ];

    let (assets, scripts) = collect_package_references(imports);

    assert_eq!(assets, vec!["/Game/Audio/SC_Test", "/Game/Foo/BP_Bar"]);

    assert_eq!(scripts, vec!["/Script/CoreUObject", "/Script/Engine"]);
}

#[test]
fn package_path_mapping() {
    assert_eq!(
        package_path_from_relative("Foo/BP_Bar.uasset", "/Game"),
        "/Game/Foo/BP_Bar"
    );

    assert_eq!(
        package_path_from_relative("Sub\\Dir\\A.uasset", "/Game"),
        "/Game/Sub/Dir/A"
    );

    assert_eq!(
        package_path_from_relative("Maps/Main.umap", "Game/"),
        "/Game/Maps/Main"
    );

    assert_eq!(
        package_path_from_relative("/Widgets/W_HUD.uasset", "/MyPlugin"),
        "/MyPlugin/Widgets/W_HUD"
    );

    assert_eq!(
        package_path_from_relative("Foo/BP_Upper.UASSET", "/Game"),
        "/Game/Foo/BP_Upper"
    );

    assert_eq!(
        package_path_from_relative("Maps/Main.UMAP", "/Game"),
        "/Game/Maps/Main"
    );
}

#[test]
fn mount_map_maps_game_plugin_and_engine_roots() {
    let mounts =
        MountMap::parse("/Game=Content,/MyPlugin=Plugins/MyPlugin/Content,/Engine=Engine/Content")
            .unwrap();

    assert_eq!(
        package_path_from_relative_with_mounts("Content/Project/Maps/Lobby.umap", &mounts),
        Some("/Game/Project/Maps/Lobby".to_string())
    );

    assert_eq!(
        package_path_from_relative_with_mounts(
            "Plugins/MyPlugin/Content/Widgets/W_HUD.uasset",
            &mounts
        ),
        Some("/MyPlugin/Widgets/W_HUD".to_string())
    );

    assert_eq!(
        package_path_from_relative_with_mounts(
            "Engine/Content/EngineMaterials/M_Default.uasset",
            &mounts
        ),
        Some("/Engine/EngineMaterials/M_Default".to_string())
    );

    assert_eq!(
        package_path_from_relative_with_mounts("Unmapped/Thing.uasset", &mounts),
        None
    );
}

#[test]
fn output_sections_parse_presets_and_aliases() {
    let logic = OutputSections::parse("logic,refs").unwrap();

    assert!(logic.summary);

    assert!(logic.exports);

    assert!(logic.pins);

    assert!(logic.references);

    assert!(!logic.properties);

    let debug = OutputSections::parse("debug").unwrap();

    assert!(debug.summary);

    assert!(debug.imports);

    assert!(debug.exports);

    assert!(debug.properties);

    assert!(debug.layout);

    assert!(!debug.pins);

    assert!(OutputSections::parse(" ").is_err());

    assert!(OutputSections::parse("summary,unknown").is_err());
}

#[test]
fn output_sections_edge_cases() {
    // pins / properties / layout each imply exports.
    let pins = OutputSections::parse("pins").unwrap();
    assert!(pins.pins && pins.exports);
    let props = OutputSections::parse("properties").unwrap();
    assert!(props.properties && props.exports);
    let layout = OutputSections::parse("layout").unwrap();
    assert!(layout.layout && layout.exports);

    // Aliases resolve to their canonical section.
    let aliased = OutputSections::parse("refs,props,identity").unwrap();
    assert!(aliased.references && aliased.properties && aliased.exports);

    let dump = OutputSections::parse("dump").unwrap();
    assert!(
        dump.summary && dump.imports && dump.exports && dump.pins && dump.properties && dump.layout
    );
    assert!(!dump.names && !dump.references);

    let all = OutputSections::parse("all").unwrap();
    assert!(all.names && all.references);

    // Parsing is case-insensitive and tolerates duplicate tokens and whitespace.
    let dup = OutputSections::parse("SUMMARY, summary , Dump").unwrap();
    assert!(dup.summary && dup.imports && dup.exports && dup.pins && dup.properties && dup.layout);

    // A single unknown token anywhere is rejected; so is an empty spec.
    assert!(OutputSections::parse("summary,bogus").is_err());
    assert!(OutputSections::parse("full").is_err());
    assert!(OutputSections::parse("").is_err());
}
