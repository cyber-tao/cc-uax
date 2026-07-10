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
        resolve_object: &|_idx: i32| serde_json::Value::Null,
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
        resolve_object: &|idx: i32| serde_json::json!({ "index": idx }),
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
        serde_json::json!({ "asset_path": "/Game/A.A" }),
        serde_json::json!({ "asset_path": "/Game/B.B" }),
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
        resolve_object: &|_idx: i32| serde_json::Value::Null,
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
        resolve_object: &|_idx: i32| serde_json::Value::Null,
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
        resolve_object: &|_idx: i32| serde_json::Value::Null,
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
        resolve_object: &|_idx: i32| serde_json::Value::Null,
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
