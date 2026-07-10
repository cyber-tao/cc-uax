use crate::collect_package_references;
use crate::name::NameMap;
use crate::object::PackageIndex;
use crate::property::TypeName;
use crate::reader::Reader;
use crate::{ByteRangePreview, Diagnostic, Severity};

#[test]
fn package_index_semantics() {
    assert!(PackageIndex(0).is_null());

    assert_eq!(PackageIndex(1).export_index(), Some(0));

    assert_eq!(PackageIndex(5).export_index(), Some(4));

    assert!(PackageIndex(5).import_index().is_none());

    assert_eq!(PackageIndex(-1).import_index(), Some(0));

    assert_eq!(PackageIndex(-3).import_index(), Some(2));

    assert_eq!(PackageIndex(i32::MIN).import_index(), None);
}

#[test]
fn typename_rejects_excessive_nesting() {
    let names = NameMap {
        names: vec!["Nested".to_string()],
    };
    let mut bytes = Vec::new();
    for _ in 0..=64 {
        bytes.extend_from_slice(&0_i32.to_le_bytes());
        bytes.extend_from_slice(&0_i32.to_le_bytes());
        bytes.extend_from_slice(&1_i32.to_le_bytes());
    }
    bytes.extend_from_slice(&0_i32.to_le_bytes());
    bytes.extend_from_slice(&0_i32.to_le_bytes());
    bytes.extend_from_slice(&0_i32.to_le_bytes());

    let err = TypeName::parse(&mut Reader::new(&bytes), &names).unwrap_err();
    assert!(err.to_string().contains("nesting exceeds"));
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
    .with_context(crate::structured_value::json!({
        "preview": ByteRangePreview {
            start: 128,
            end: 132,
            size: 4,
            preview: "01020304".to_string(),
        }
    }));

    let json = serde_json_crate::to_value(&diagnostic).unwrap();
    assert_eq!(json["severity"], "warning");
    assert_eq!(json["code"], "property_value_fallback");
    assert_eq!(json["path"], "/exports/2/properties/Health");
    assert_eq!(json["offset"], 128);
    assert_eq!(json["context"]["preview"]["size"], 4);

    let info = Diagnostic::new(Severity::Info, "note", "/", "message");
    let info_json = serde_json_crate::to_value(&info).unwrap();
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
