use cc_uax::OutputSections;
use cc_uax::name::NameMap;
use cc_uax::object::PackageIndex;
use cc_uax::references::{collect_package_references, package_path_from_relative};
use cc_uax::property::TypeName;

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
