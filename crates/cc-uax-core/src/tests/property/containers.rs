use super::super::common::*;
use crate::name::NameMap;
use crate::pin::PinSerCtx;
use crate::property::{ParseCtx, parse_properties};
use crate::reader::Reader;

#[test]
fn optional_property_decodes_set_and_unset() {
    let names = NameMap {
        names: vec![
            "OptSet".to_string(),           // 0
            "OptionalProperty".to_string(), // 1
            "BoolProperty".to_string(),     // 2
            "OptUnset".to_string(),         // 3
            "None".to_string(),             // 4
        ],
    };
    let mut d = Vec::new();
    // Set optional bool = true: presence(bool32)=1 + inner bool byte=1.
    push_raw_name(&mut d, 0); // OptSet
    push_raw_name(&mut d, 1); // OptionalProperty
    push_i32(&mut d, 1); // one inner type param
    push_raw_name(&mut d, 2); // BoolProperty
    push_i32(&mut d, 0); // inner param count
    push_i32(&mut d, 5); // size
    d.push(0); // flags
    push_i32(&mut d, 1); // presence = set
    d.push(1); // inner bool value
    // Unset optional bool: presence(bool32)=0 only.
    push_raw_name(&mut d, 3); // OptUnset
    push_raw_name(&mut d, 1); // OptionalProperty
    push_i32(&mut d, 1);
    push_raw_name(&mut d, 2); // BoolProperty
    push_i32(&mut d, 0);
    push_i32(&mut d, 4); // size
    d.push(0); // flags
    push_i32(&mut d, 0); // presence = unset
    push_raw_name(&mut d, 4); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| crate::DecodedValue::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "OptSet");
    assert_eq!(entries[0].value.as_bool(), Some(true));
    assert_eq!(entries[1].name, "OptUnset");
    assert!(entries[1].value.is_null());
}

/// Builds one tagged `MapProperty` whose key is a `StructProperty`, using the
/// legacy (`FileVersionUE5` < `PROPERTY_TAG_COMPLETE_TYPE_NAME`) tag layout that
/// records only the key/value *property* type names.
fn legacy_struct_keyed_map(names: &NameMap, payload: &[u8]) -> Vec<u8> {
    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // property name
    push_raw_name(&mut d, 1); // "MapProperty"
    push_i32(&mut d, payload.len() as i32); // size
    push_i32(&mut d, 0); // array index
    push_raw_name(&mut d, 2); // key type: "StructProperty" -- no struct name follows
    push_raw_name(&mut d, 3); // value type: "NameProperty"
    d.push(0); // HasPropertyGuid
    d.extend_from_slice(payload);
    push_raw_name(&mut d, 4); // None terminator
    let _ = names;
    d
}

// Below PROPERTY_TAG_COMPLETE_TYPE_NAME a container tag records only the element's
// property type, never the UScriptStruct behind `StructProperty`, which UE
// resolves from its reflection registry. That payload is undecodable by design, so
// it must be classified as its own limitation rather than as a decoder failure.
#[test]
fn a_legacy_container_without_an_inner_struct_name_is_its_own_limitation() {
    let names = NameMap {
        names: vec![
            "VariableToScriptVariable".to_string(), // 0
            "MapProperty".to_string(),              // 1
            "StructProperty".to_string(),           // 2
            "NameProperty".to_string(),             // 3
            "None".to_string(),                     // 4
        ],
    };
    // NumToRemove = 0, Num = 1, then an undecodable struct key payload.
    let mut payload = Vec::new();
    push_i32(&mut payload, 0);
    push_i32(&mut payload, 1);
    payload.extend_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]);
    let d = legacy_struct_keyed_map(&names, &payload);

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| crate::DecodedValue::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        // Below PROPERTY_TAG_EXTENSION_AND_OVERRIDABLE_SERIALIZATION so the tag
        // carries neither the object control byte nor the extension flags, and
        // therefore below PROPERTY_TAG_COMPLETE_TYPE_NAME as well.
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_EXTENSION_AND_OVERRIDABLE_SERIALIZATION
            - 1,
    };
    let mut r = Reader::new(&d);
    let parse =
        crate::property::parse_properties_report(&mut r, &ctx, d.len() as u64, "/properties");

    assert_eq!(parse.entries.len(), 1, "{:#?}", parse.entries);
    let value = &parse.entries[0].value;
    assert_eq!(value["status"].as_str(), Some("opaque"));
    let reason = value["reason"].as_str().unwrap();
    assert!(
        reason.contains("does not record the inner struct name"),
        "{reason}"
    );
    // The generic fallback code would read as a decoder defect; this one does not.
    assert!(
        parse
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "property_tag_missing_inner_struct_name"),
        "{:#?}",
        parse.diagnostics
    );
    assert!(
        !parse
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "property_value_fallback"),
        "{:#?}",
        parse.diagnostics
    );
}

// The missing inner struct name only blocks structs with a custom serializer. A
// struct that serializes as tagged properties is self-describing on disk, so a
// legacy array of them must still decode rather than becoming opaque.
#[test]
fn a_legacy_array_of_tagged_structs_still_decodes_without_an_inner_struct_name() {
    let names = NameMap {
        names: vec![
            "Points".to_string(),         // 0
            "ArrayProperty".to_string(),  // 1
            "StructProperty".to_string(), // 2
            "None".to_string(),           // 3
            "Weight".to_string(),         // 4
            "IntProperty".to_string(),    // 5
        ],
    };
    // One element: a struct written as tagged properties (Weight=7) plus its None
    // terminator, which is exactly how UE serializes a struct with no serializer.
    let mut payload = Vec::new();
    push_i32(&mut payload, 1); // element count
    push_raw_name(&mut payload, 4); // Weight
    push_raw_name(&mut payload, 5); // IntProperty
    push_i32(&mut payload, 4); // size
    push_i32(&mut payload, 0); // array index
    payload.push(0); // HasPropertyGuid
    push_i32(&mut payload, 7); // value
    push_raw_name(&mut payload, 3); // None (ends the struct)

    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // Points
    push_raw_name(&mut d, 1); // ArrayProperty
    push_i32(&mut d, payload.len() as i32);
    push_i32(&mut d, 0); // array index
    push_raw_name(&mut d, 2); // inner type: "StructProperty", no struct name
    d.push(0); // HasPropertyGuid
    d.extend_from_slice(&payload);
    push_raw_name(&mut d, 3); // None terminator

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| crate::DecodedValue::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_EXTENSION_AND_OVERRIDABLE_SERIALIZATION
            - 1,
    };
    let mut r = Reader::new(&d);
    let parse =
        crate::property::parse_properties_report(&mut r, &ctx, d.len() as u64, "/properties");

    assert_eq!(parse.entries.len(), 1, "{:#?}", parse.entries);
    let element = &parse.entries[0].value[0];
    assert_eq!(element["properties"][0]["name"].as_str(), Some("Weight"));
    assert_eq!(element["properties"][0]["value"].as_i64(), Some(7));
    assert!(parse.diagnostics.is_empty(), "{:#?}", parse.diagnostics);
}

#[test]
fn multicast_inline_delegate_decodes() {
    let names = NameMap {
        names: vec![
            "OnFire".to_string(),
            "MulticastInlineDelegateProperty".to_string(),
            "HandleFire".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    push_i32(&mut value, 1); // invocation count
    push_i32(&mut value, -3); // object index
    push_raw_name(&mut value, 2); // function name
    assert_eq!(value.len(), 16);

    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // OnFire
    push_raw_name(&mut d, 1); // MulticastInlineDelegateProperty
    push_i32(&mut d, 0);
    push_i32(&mut d, value.len() as i32);
    d.push(0);
    d.extend_from_slice(&value);
    push_raw_name(&mut d, 3); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|idx: i32| crate::structured_value::json!({ "index": idx }),
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let arr = entries[0].value.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["function"].as_str(), Some("HandleFire"));
    assert_eq!(arr[0]["object"]["index"].as_i64(), Some(-3));
}

#[test]
fn soft_object_property_resolves_list_index() {
    let names = NameMap {
        names: vec![
            "Ref".to_string(),
            "SoftObjectProperty".to_string(),
            "None".to_string(),
        ],
    };
    let table = vec![
        crate::structured_value::json!({ "asset_path": "/Game/A.A" }),
        crate::structured_value::json!({ "asset_path": "/Game/B.B" }),
    ];
    let mut value = Vec::new();
    push_i32(&mut value, 1); // index into the soft object path list

    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // Ref
    push_raw_name(&mut d, 1); // SoftObjectProperty
    push_i32(&mut d, 0); // type name inner param count
    push_i32(&mut d, value.len() as i32); // size = 4
    d.push(0); // flags
    d.extend_from_slice(&value);
    push_raw_name(&mut d, 2); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| crate::DecodedValue::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &table,
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].value["asset_path"].as_str(), Some("/Game/B.B"));
}

// FSoftObjectPath::SerializePathWithoutFixup: below FSOFTOBJECTPATH_REMOVE_ASSET_PATH_FNAMES
// an inline soft path is a single FName AssetPathName, not an FTopLevelAssetPath pair.
// Test read_soft_object_path directly so the soft-path layout is isolated from the
// property-tag format (both are version-gated on the same FileVersionUE5).
#[test]
fn read_soft_object_path_pre_1007_is_single_fname() {
    let names = NameMap {
        names: vec!["None".to_string(), "/Game/Curves/Foo.Foo".to_string()],
    };
    let mut data = Vec::new();
    push_raw_name(&mut data, 1); // AssetPathName (single FName)
    push_i32(&mut data, 0); // empty SubPathString
    assert_eq!(data.len(), 12);

    let mut r = Reader::new(&data);
    let value = crate::property::read_soft_object_path(
        &mut r,
        &names,
        crate::version::ue5::LARGE_WORLD_COORDINATES, // 1004 < 1007
    )
    .unwrap();
    assert_eq!(value["asset_path"].as_str(), Some("/Game/Curves/Foo.Foo"));
    assert_eq!(r.pos(), data.len() as u64);
}

// At and above the threshold the inline soft path is an FTopLevelAssetPath pair.
#[test]
fn read_soft_object_path_from_1007_is_top_level_asset_path_pair() {
    let names = NameMap {
        names: vec![
            "None".to_string(),
            "/Game/Curves".to_string(), // PackageName
            "Foo".to_string(),          // AssetName
        ],
    };
    let mut data = Vec::new();
    push_raw_name(&mut data, 1); // PackageName
    push_raw_name(&mut data, 2); // AssetName
    push_i32(&mut data, 0); // empty SubPathString

    let mut r = Reader::new(&data);
    let value = crate::property::read_soft_object_path(
        &mut r,
        &names,
        crate::version::ue5::FSOFTOBJECTPATH_REMOVE_ASSET_PATH_FNAMES,
    )
    .unwrap();
    assert_eq!(value["asset_path"].as_str(), Some("/Game/Curves.Foo"));
    assert_eq!(r.pos(), data.len() as u64);
}

#[test]
fn read_soft_object_path_utf8_subpath_without_nul_is_consumed() {
    // FortniteMain 192 writes SubPath as FUtf8String: positive length, no trailing NUL.
    // read_fstring's positive-length branch already matches that layout.
    let names = NameMap {
        names: vec![
            "None".to_string(),
            "/Game/Curves".to_string(),
            "Foo".to_string(),
        ],
    };
    let mut data = Vec::new();
    push_raw_name(&mut data, 1);
    push_raw_name(&mut data, 2);
    let sub = b"Socket";
    push_i32(&mut data, sub.len() as i32);
    data.extend_from_slice(sub);

    let mut r = Reader::new(&data);
    let value = crate::property::read_soft_object_path(
        &mut r,
        &names,
        crate::version::ue5::FSOFTOBJECTPATH_REMOVE_ASSET_PATH_FNAMES,
    )
    .unwrap();
    assert_eq!(value["asset_path"].as_str(), Some("/Game/Curves.Foo"));
    assert_eq!(value["sub_path"].as_str(), Some("Socket"));
    assert_eq!(r.pos(), data.len() as u64);
}

#[test]
fn lazy_object_property_decodes_guid() {
    // FLinkerSave writes a LazyObjectProperty value as the 16-byte FUniqueObjectGuid,
    // not a package index.
    let names = NameMap {
        names: vec![
            "Lazy".to_string(),
            "LazyObjectProperty".to_string(),
            "None".to_string(),
        ],
    };
    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // Lazy
    push_raw_name(&mut d, 1); // LazyObjectProperty
    push_i32(&mut d, 0); // type name inner param count
    push_i32(&mut d, 16); // size
    d.push(0); // flags
    for x in [0x1122_3344u32, 0x5566_7788, 0x99AA_BBCC, 0xDDEE_FF00] {
        push_u32(&mut d, x);
    }
    push_raw_name(&mut d, 2); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| crate::DecodedValue::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].value["lazy_object_guid"].as_str(),
        Some("112233445566778899AABBCCDDEEFF00")
    );
}

#[test]
fn map_removed_keys_are_discarded() {
    // A delta-saved TMap serializes NumKeysToRemove key payloads before the live
    // pairs; the parser must consume them to stay aligned.
    let names = NameMap {
        names: vec![
            "Weights".to_string(),
            "MapProperty".to_string(),
            "IntProperty".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    push_i32(&mut value, 1); // NumKeysToRemove
    push_i32(&mut value, 777); // removed key payload
    push_i32(&mut value, 1); // pair count
    push_i32(&mut value, 5); // key
    push_i32(&mut value, 50); // value

    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // Weights
    push_raw_name(&mut d, 1); // MapProperty
    push_i32(&mut d, 2); // two type parameters
    push_raw_name(&mut d, 2); // IntProperty (key)
    push_i32(&mut d, 0);
    push_raw_name(&mut d, 2); // IntProperty (value)
    push_i32(&mut d, 0);
    push_i32(&mut d, value.len() as i32);
    d.push(0); // flags
    d.extend_from_slice(&value);
    push_raw_name(&mut d, 3); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| crate::DecodedValue::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let pairs = entries[0].value.as_array().unwrap();
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0]["key"].as_i64(), Some(5));
    assert_eq!(pairs[0]["value"].as_i64(), Some(50));
}

#[test]
fn set_removed_elements_are_discarded() {
    let names = NameMap {
        names: vec![
            "Ids".to_string(),
            "SetProperty".to_string(),
            "IntProperty".to_string(),
            "None".to_string(),
        ],
    };
    let mut value = Vec::new();
    push_i32(&mut value, 1); // NumElementsToRemove
    push_i32(&mut value, 999); // removed element payload
    push_i32(&mut value, 2); // element count
    push_i32(&mut value, 7);
    push_i32(&mut value, 8);

    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // Ids
    push_raw_name(&mut d, 1); // SetProperty
    push_i32(&mut d, 1); // one type parameter
    push_raw_name(&mut d, 2); // IntProperty
    push_i32(&mut d, 0);
    push_i32(&mut d, value.len() as i32);
    d.push(0); // flags
    d.extend_from_slice(&value);
    push_raw_name(&mut d, 3); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| crate::DecodedValue::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let elems = entries[0].value.as_array().unwrap();
    assert_eq!(elems.len(), 2);
    assert_eq!(elems[0].as_i64(), Some(7));
    assert_eq!(elems[1].as_i64(), Some(8));
}
