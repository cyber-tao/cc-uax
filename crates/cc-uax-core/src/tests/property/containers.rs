use super::super::common::*;
use crate::model::OpaqueReason;
use crate::name::NameMap;
use crate::pin::PinSerCtx;
use crate::property::{
    BlockOwner, ContainerStructNames, ContainerStructs, ParseCtx, parse_object_properties_report,
    parse_properties, parse_properties_report, parse_struct_properties_report,
};
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
        soft_object_paths_unavailable: false,
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
        nested_diagnostics: Default::default(),
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
// property type, never the UScriptStruct behind `StructProperty`. `TArray` still
// carries an inner tag in its payload, but `FSetProperty::SerializeItem` and
// `FMapProperty::SerializeItem` write none, so a set element or map key/value
// struct really is undecodable by design and must be classified as its own
// limitation rather than as a decoder failure.
#[test]
fn a_legacy_map_without_an_inner_struct_name_is_its_own_limitation() {
    let names = NameMap {
        names: vec![
            "SomeProjectStructKeyedMap".to_string(), // 0: not a declaration we know
            "MapProperty".to_string(),               // 1
            "StructProperty".to_string(),            // 2
            "NameProperty".to_string(),              // 3
            "None".to_string(),                      // 4
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
        soft_object_paths_unavailable: false,
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        // Below PROPERTY_TAG_EXTENSION_AND_OVERRIDABLE_SERIALIZATION so the tag
        // carries neither the object control byte nor the extension flags, and
        // therefore below PROPERTY_TAG_COMPLETE_TYPE_NAME as well.
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_EXTENSION_AND_OVERRIDABLE_SERIALIZATION
            - 1,
        nested_diagnostics: Default::default(),
    };
    let mut r = Reader::new(&d);
    let parse =
        crate::property::parse_properties_report(&mut r, &ctx, d.len() as u64, "/properties");

    assert_eq!(parse.entries.len(), 1, "{:#?}", parse.entries);
    let value = &parse.entries[0].value;
    let opaque = value.as_opaque().expect("the value stays opaque");
    assert_eq!(opaque.reason, OpaqueReason::MissingInnerStructName);
    assert!(
        opaque
            .reason
            .description()
            .contains("does not record a set/map element struct name"),
        "{}",
        opaque.reason.description()
    );
    assert_eq!(
        opaque.byte_range.size,
        opaque.byte_range.end - opaque.byte_range.start
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

fn legacy_ctx<'a>(names: &'a NameMap) -> ParseCtx<'a> {
    ParseCtx {
        names,
        resolve_object: &|_idx: i32| crate::DecodedValue::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        soft_object_paths_unavailable: false,
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_EXTENSION_AND_OVERRIDABLE_SERIALIZATION
            - 1,
        nested_diagnostics: Default::default(),
    }
}

// The declarations behind a handful of set/map properties are known from UE
// source, and on the reference corpus they were what kept every UE5.0–5.3
// Blueprint (`UBlueprintGeneratedClass::PropertyGuids`, `TMap<FName, FGuid>`) and
// Niagara asset from ever being `complete`. Naming the element struct from that
// table lets the payload decode; the tag's `Size` still bounds it.
#[test]
fn a_legacy_map_whose_declaration_is_known_decodes_its_struct_elements() {
    let names = NameMap {
        names: vec![
            "PropertyGuids".to_string(),  // 0
            "MapProperty".to_string(),    // 1
            "NameProperty".to_string(),   // 2
            "StructProperty".to_string(), // 3
            "None".to_string(),           // 4
            "MyVariable".to_string(),     // 5
        ],
    };
    let mut payload = Vec::new();
    push_i32(&mut payload, 0); // NumToRemove
    push_i32(&mut payload, 1); // Num
    push_raw_name(&mut payload, 5); // key: FName "MyVariable"
    push_guid(
        &mut payload,
        0x1111_1111,
        0x2222_2222,
        0x3333_3333,
        0x4444_4444,
    );
    let mut d = Vec::new();
    push_raw_name(&mut d, 0);
    push_raw_name(&mut d, 1);
    push_i32(&mut d, payload.len() as i32);
    push_i32(&mut d, 0);
    push_raw_name(&mut d, 2); // key type NameProperty
    push_raw_name(&mut d, 3); // value type StructProperty, no struct name
    d.push(0); // HasPropertyGuid
    d.extend_from_slice(&payload);
    push_raw_name(&mut d, 4);

    let ctx = legacy_ctx(&names);
    let mut r = Reader::new(&d);
    let parse =
        crate::property::parse_properties_report(&mut r, &ctx, d.len() as u64, "/properties");

    assert!(parse.diagnostics.is_empty(), "{:#?}", parse.diagnostics);
    let entry = &parse.entries[0];
    assert_eq!(
        entry.type_str,
        "MapProperty(NameProperty,StructProperty(Guid))"
    );
    assert_eq!(entry.value[0]["key"].as_str(), Some("MyVariable"));
    assert_eq!(
        entry.value[0]["value"].as_str(),
        Some("11111111222222223333333344444444")
    );
}

// The same property name can be declared on several types, so a declaration that
// UE source ties to one type only names the elements when that type is the one
// whose block is being read.
#[test]
fn a_scoped_declaration_resolves_only_under_its_owner() {
    let names = NameMap {
        names: vec![
            "BindingIdToReferences".to_string(), // 0
            "MapProperty".to_string(),           // 1
            "StructProperty".to_string(),        // 2
            "None".to_string(),                  // 3
        ],
    };
    let mut payload = Vec::new();
    push_i32(&mut payload, 0); // NumToRemove
    push_i32(&mut payload, 1); // Num
    push_guid(&mut payload, 1, 2, 3, 4); // key: FGuid
    push_raw_name(&mut payload, 3); // value: a tagged block holding only None
    let mut d = Vec::new();
    push_raw_name(&mut d, 0);
    push_raw_name(&mut d, 1);
    push_i32(&mut d, payload.len() as i32);
    push_i32(&mut d, 0);
    push_raw_name(&mut d, 2); // key type StructProperty, no struct name
    push_raw_name(&mut d, 2); // value type StructProperty, no struct name
    d.push(0); // HasPropertyGuid
    d.extend_from_slice(&payload);
    push_raw_name(&mut d, 3);
    let ctx = legacy_ctx(&names);
    let end = d.len() as u64;

    let mut r = Reader::new(&d);
    let named = parse_struct_properties_report(
        &mut r,
        &ctx,
        end,
        "/properties",
        "LevelSequenceBindingReferences",
    );
    assert!(named.diagnostics.is_empty(), "{:#?}", named.diagnostics);
    assert_eq!(
        named.entries[0].type_str,
        "MapProperty(StructProperty(Guid),StructProperty(LevelSequenceBindingReferenceArray))"
    );
    assert_eq!(
        named.entries[0].value[0]["value"]["@struct"].as_str(),
        Some("LevelSequenceBindingReferenceArray")
    );

    let mut r = Reader::new(&d);
    let unknown = parse_properties_report(&mut r, &ctx, end, "/properties");
    let mut r = Reader::new(&d);
    let other = parse_struct_properties_report(&mut r, &ctx, end, "/properties", "SomethingElse");
    for parse in [unknown, other] {
        let opaque = parse.entries[0]
            .value
            .as_opaque()
            .expect("another owner does not get the name");
        assert_eq!(opaque.reason, OpaqueReason::MissingInnerStructName);
        assert!(
            parse
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "property_tag_missing_inner_struct_name"),
            "{:#?}",
            parse.diagnostics
        );
    }
}

// A declaration with no owner list matches by property name alone, whatever the
// block belongs to: it is declared on a type whose subclasses appear as the export
// class, or on several structs, so no single owner could be named.
#[test]
fn an_unscoped_declaration_still_matches_by_name_alone() {
    let names = NameMap {
        names: vec![
            "PropertyGuids".to_string(),  // 0
            "MapProperty".to_string(),    // 1
            "NameProperty".to_string(),   // 2
            "StructProperty".to_string(), // 3
            "None".to_string(),           // 4
            "MyVariable".to_string(),     // 5
        ],
    };
    let mut payload = Vec::new();
    push_i32(&mut payload, 0);
    push_i32(&mut payload, 1);
    push_raw_name(&mut payload, 5);
    push_guid(&mut payload, 1, 2, 3, 4);
    let mut d = Vec::new();
    push_raw_name(&mut d, 0);
    push_raw_name(&mut d, 1);
    push_i32(&mut d, payload.len() as i32);
    push_i32(&mut d, 0);
    push_raw_name(&mut d, 2);
    push_raw_name(&mut d, 3);
    d.push(0);
    d.extend_from_slice(&payload);
    push_raw_name(&mut d, 4);
    let ctx = legacy_ctx(&names);
    let end = d.len() as u64;

    for owner in [
        BlockOwner::Unknown,
        BlockOwner::Struct("SomethingElse"),
        BlockOwner::Class {
            name: "/Game/BP.BP_C",
            reflected: None,
        },
    ] {
        let mut r = Reader::new(&d);
        let parse = parse_object_properties_report(&mut r, &ctx, end, "/properties", owner);
        assert!(parse.diagnostics.is_empty(), "{:#?}", parse.diagnostics);
        assert_eq!(
            parse.entries[0].type_str,
            "MapProperty(NameProperty,StructProperty(Guid))"
        );
    }
}

/// A stand-in for the package's own class declarations: names one struct for the
/// values of `VectorParameterValues`, for one container kind only.
struct FakeDeclarations {
    container: &'static str,
    value_struct: &'static str,
}

impl ContainerStructNames for FakeDeclarations {
    fn container_struct_names(&self, property: &str, container: &str) -> Option<ContainerStructs> {
        (property == "VectorParameterValues" && container == self.container).then(|| {
            ContainerStructs {
                key: None,
                value: Some(self.value_struct.to_string()),
            }
        })
    }
}

/// `VectorParameterValues: TMap<FName, ?>` with one entry whose value is 16 bytes,
/// the width of both an `FGuid` and an `FLinearColor`.
fn vector_parameter_values_map() -> (NameMap, Vec<u8>) {
    let names = NameMap {
        names: vec![
            "VectorParameterValues".to_string(), // 0
            "MapProperty".to_string(),           // 1
            "NameProperty".to_string(),          // 2
            "StructProperty".to_string(),        // 3
            "None".to_string(),                  // 4
            "Color".to_string(),                 // 5
        ],
    };
    let mut payload = Vec::new();
    push_i32(&mut payload, 0);
    push_i32(&mut payload, 1);
    push_raw_name(&mut payload, 5);
    for channel in [1.0f32, 0.5, 0.25, 1.0] {
        push_f32(&mut payload, channel);
    }
    let mut d = Vec::new();
    push_raw_name(&mut d, 0);
    push_raw_name(&mut d, 1);
    push_i32(&mut d, payload.len() as i32);
    push_i32(&mut d, 0);
    push_raw_name(&mut d, 2);
    push_raw_name(&mut d, 3);
    d.push(0);
    d.extend_from_slice(&payload);
    push_raw_name(&mut d, 4);
    (names, d)
}

// The package's own declaration is authoritative: a Blueprint variable can share
// a name with an engine property and still be declared as something else. The two
// structs are the same width, so the result shows which one won rather than which
// one fits.
#[test]
fn a_reflected_declaration_beats_the_table() {
    let (names, d) = vector_parameter_values_map();
    let ctx = legacy_ctx(&names);
    let end = d.len() as u64;
    let class = "/Script/DatasmithContent.DatasmithMaterialInstanceTemplate";
    let fake = FakeDeclarations {
        container: "MapProperty",
        value_struct: "Guid",
    };

    let mut r = Reader::new(&d);
    let reflected = parse_object_properties_report(
        &mut r,
        &ctx,
        end,
        "/properties",
        BlockOwner::Class {
            name: class,
            reflected: Some(&fake),
        },
    );
    assert!(
        reflected.diagnostics.is_empty(),
        "{:#?}",
        reflected.diagnostics
    );
    assert_eq!(
        reflected.entries[0].type_str,
        "MapProperty(NameProperty,StructProperty(Guid))"
    );

    let mut r = Reader::new(&d);
    let table = parse_object_properties_report(
        &mut r,
        &ctx,
        end,
        "/properties",
        BlockOwner::Class {
            name: class,
            reflected: None,
        },
    );
    assert!(table.diagnostics.is_empty(), "{:#?}", table.diagnostics);
    assert_eq!(
        table.entries[0].type_str,
        "MapProperty(NameProperty,StructProperty(LinearColor))"
    );
}

#[test]
fn a_reflected_declaration_of_another_container_kind_is_ignored() {
    let (names, d) = vector_parameter_values_map();
    let ctx = legacy_ctx(&names);
    let fake = FakeDeclarations {
        container: "SetProperty",
        value_struct: "Guid",
    };

    let mut r = Reader::new(&d);
    let parse = parse_object_properties_report(
        &mut r,
        &ctx,
        d.len() as u64,
        "/properties",
        BlockOwner::Class {
            name: "/Script/DatasmithContent.DatasmithMaterialInstanceTemplate",
            reflected: Some(&fake),
        },
    );

    assert_eq!(
        parse.entries[0].type_str,
        "MapProperty(NameProperty,StructProperty(LinearColor))"
    );
}

// A legacy container tag records `ByteProperty` for a `TEnumAsByte<E>` element
// too, but `FByteProperty::SerializeItem` writes an enum as an 8-byte FName.
// The count and the tag's `Size` say which it is; reading names as bytes gave
// name indices as values and a short read.
#[test]
fn legacy_byte_array_elements_are_names_when_the_size_says_so() {
    let names = NameMap {
        names: vec![
            "Modes".to_string(),         // 0
            "ArrayProperty".to_string(), // 1
            "ByteProperty".to_string(),  // 2
            "None".to_string(),          // 3
            "EMode::Fast".to_string(),   // 4
            "EMode::Slow".to_string(),   // 5
        ],
    };
    // `payload` is the whole value: the element count followed by the elements.
    let build = |payload: &[u8]| {
        let mut d = Vec::new();
        push_raw_name(&mut d, 0);
        push_raw_name(&mut d, 1);
        push_i32(&mut d, payload.len() as i32);
        push_i32(&mut d, 0);
        push_raw_name(&mut d, 2); // inner: ByteProperty, no enum name
        d.push(0); // HasPropertyGuid
        d.extend_from_slice(payload);
        push_raw_name(&mut d, 3);
        d
    };
    let ctx = legacy_ctx(&names);

    // Two enum names: 4 (count) + 2 * 8 bytes.
    let mut as_names = Vec::new();
    push_i32(&mut as_names, 2);
    push_raw_name(&mut as_names, 4);
    push_raw_name(&mut as_names, 5);
    let d = build(&as_names);
    let mut r = Reader::new(&d);
    let parse =
        crate::property::parse_properties_report(&mut r, &ctx, d.len() as u64, "/properties");
    assert!(parse.diagnostics.is_empty(), "{:#?}", parse.diagnostics);
    assert_eq!(parse.entries[0].value[0].as_str(), Some("EMode::Fast"));
    assert_eq!(parse.entries[0].value[1].as_str(), Some("EMode::Slow"));

    // Two plain bytes: 4 (count) + 2 bytes. The same tag, read as bytes.
    let mut as_bytes = Vec::new();
    push_i32(&mut as_bytes, 2);
    as_bytes.extend_from_slice(&[7, 9]);
    let d = build(&as_bytes);
    let mut r = Reader::new(&d);
    let parse =
        crate::property::parse_properties_report(&mut r, &ctx, d.len() as u64, "/properties");
    assert!(parse.diagnostics.is_empty(), "{:#?}", parse.diagnostics);
    assert_eq!(parse.entries[0].value[0].as_i64(), Some(7));
    assert_eq!(parse.entries[0].value[1].as_i64(), Some(9));
}

/// Name table shared by the legacy struct-array tests below.
fn legacy_struct_array_names() -> NameMap {
    NameMap {
        names: vec![
            "Points".to_string(),         // 0
            "ArrayProperty".to_string(),  // 1
            "StructProperty".to_string(), // 2
            "None".to_string(),           // 3
            "Weight".to_string(),         // 4
            "IntProperty".to_string(),    // 5
            "MyPoint".to_string(),        // 6
            "Guid".to_string(),           // 7
        ],
    }
}

/// The inner `FPropertyTag` that `FArrayProperty::SerializeItem` writes between a
/// struct array's element count and its elements. It is a complete legacy tag, so
/// it repeats the array's own name and carries the element struct name plus a
/// struct GUID (`ue4 >= STRUCT_GUID_IN_PROPERTY_TAG`).
fn push_inner_array_struct_tag(v: &mut Vec<u8>, struct_name_idx: i32, size: i32, ue5: i32) {
    push_legacy_tag_header(v, 0, 2, size); // name "Points", type "StructProperty"
    push_raw_name(v, struct_name_idx);
    push_guid(v, 0, 0, 0, 0); // StructGuid
    push_legacy_tag_tail(v, ue5);
}

fn legacy_struct_array_ctx(names: &NameMap, file_version_ue5: i32) -> ParseCtx<'_> {
    ParseCtx {
        names,
        resolve_object: &|_idx: i32| crate::DecodedValue::Null,
        pins: PinSerCtx::default(),
        soft_object_paths: &[],
        soft_object_paths_unavailable: false,
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5,
        nested_diagnostics: Default::default(),
    }
}

/// Wraps `payload` as one legacy `ArrayProperty(StructProperty)` tag plus a None
/// terminator.
fn legacy_struct_array_property(payload: &[u8], ue5: i32) -> Vec<u8> {
    let mut d = Vec::new();
    push_legacy_tag_header(&mut d, 0, 1, payload.len() as i32); // Points, ArrayProperty
    push_raw_name(&mut d, 2); // inner type: "StructProperty", no struct name
    push_legacy_tag_tail(&mut d, ue5);
    d.extend_from_slice(payload);
    push_raw_name(&mut d, 3); // None terminator
    d
}

// The element struct name is on disk after all: below
// PROPERTY_TAG_COMPLETE_TYPE_NAME, `FArrayProperty::SerializeItem` writes a full
// inner FPropertyTag after the element count (PropertyArray.cpp). Reading it is
// what makes a legacy array of structs decodable, so a UE5.0-5.3 array must
// surface the struct name rather than becoming an opaque region.
#[test]
fn a_legacy_struct_array_recovers_its_element_struct_name_from_the_inner_tag() {
    let names = legacy_struct_array_names();
    let ue5 = crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME - 1;

    // One element: a struct written as tagged properties (Weight=7) plus its None
    // terminator, which is how UE serializes a struct with no custom serializer.
    let mut element = Vec::new();
    push_legacy_tag_header(&mut element, 4, 5, 4); // Weight, IntProperty
    push_legacy_tag_tail(&mut element, ue5);
    push_i32(&mut element, 7);
    push_raw_name(&mut element, 3); // None (ends the struct)

    let mut payload = Vec::new();
    push_i32(&mut payload, 1); // element count
    push_inner_array_struct_tag(&mut payload, 6, element.len() as i32, ue5);
    payload.extend_from_slice(&element);
    let d = legacy_struct_array_property(&payload, ue5);

    let ctx = legacy_struct_array_ctx(&names, ue5);
    let mut r = Reader::new(&d);
    let parse =
        crate::property::parse_properties_report(&mut r, &ctx, d.len() as u64, "/properties");

    assert_eq!(parse.entries.len(), 1, "{:#?}", parse.entries);
    let decoded = &parse.entries[0].value[0];
    assert_eq!(decoded["@struct"].as_str(), Some("MyPoint"));
    assert_eq!(decoded["properties"][0]["name"].as_str(), Some("Weight"));
    assert_eq!(decoded["properties"][0]["value"].as_i64(), Some(7));
    assert!(parse.diagnostics.is_empty(), "{:#?}", parse.diagnostics);
    assert_eq!(parse.decoded_end, Some(d.len() as u64));
}

// The recovered struct name also selects a native decoder, so a legacy array of
// structs with a custom serializer decodes structurally instead of staying opaque.
#[test]
fn a_legacy_struct_array_decodes_native_elements_via_the_inner_tag() {
    let names = legacy_struct_array_names();
    let ue5 = crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME - 1;

    let mut element = Vec::new();
    push_guid(
        &mut element,
        0x1111_1111,
        0x2222_2222,
        0x3333_3333,
        0x4444_4444,
    );

    let mut payload = Vec::new();
    push_i32(&mut payload, 1);
    push_inner_array_struct_tag(&mut payload, 7, element.len() as i32, ue5); // "Guid"
    payload.extend_from_slice(&element);
    let d = legacy_struct_array_property(&payload, ue5);

    let ctx = legacy_struct_array_ctx(&names, ue5);
    let mut r = Reader::new(&d);
    let parse =
        crate::property::parse_properties_report(&mut r, &ctx, d.len() as u64, "/properties");

    assert_eq!(parse.entries.len(), 1, "{:#?}", parse.entries);
    assert_eq!(
        parse.entries[0].value[0].as_str(),
        Some("11111111222222223333333344444444")
    );
    assert!(parse.diagnostics.is_empty(), "{:#?}", parse.diagnostics);
}

// At PROPERTY_TAG_COMPLETE_TYPE_NAME the struct name moves into the container
// tag's type tree and UE stops writing the inner tag, so reading one would
// consume element bytes. Threshold and threshold-1 must disagree about the layout.
#[test]
fn a_complete_type_name_struct_array_has_no_inner_tag() {
    let names = legacy_struct_array_names();
    let ue5 = crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME;

    let mut payload = Vec::new();
    push_i32(&mut payload, 1); // element count
    push_guid(&mut payload, 0xAAAA_AAAA, 0, 0, 0); // element, no inner tag

    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // Points
    push_raw_name(&mut d, 1); // ArrayProperty
    push_i32(&mut d, 1); // one type parameter
    push_raw_name(&mut d, 2); // StructProperty
    push_i32(&mut d, 1); // one type parameter
    push_raw_name(&mut d, 7); // Guid
    push_i32(&mut d, 0);
    push_i32(&mut d, payload.len() as i32);
    // Guid has native serialization, so UE sets HasBinaryOrNativeSerialize on the
    // array tag (a container's flag is the OR of its elements').
    d.push(0x08);
    d.extend_from_slice(&payload);
    push_raw_name(&mut d, 3); // None

    let ctx = legacy_struct_array_ctx(&names, ue5);
    let mut r = Reader::new(&d);
    let parse =
        crate::property::parse_properties_report(&mut r, &ctx, d.len() as u64, "/properties");

    assert_eq!(parse.entries.len(), 1, "{:#?}", parse.entries);
    assert_eq!(
        parse.entries[0].value[0].as_str(),
        Some("AAAAAAAA000000000000000000000000")
    );
    assert!(parse.diagnostics.is_empty(), "{:#?}", parse.diagnostics);
}

// A truncated inner tag must fail the property rather than guess a struct name,
// and the loop must still resynchronise on the tag's declared size so the
// following property decodes.
#[test]
fn a_truncated_inner_array_tag_falls_back_without_desyncing() {
    let names = legacy_struct_array_names();
    let ue5 = crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME - 1;

    let mut payload = Vec::new();
    push_i32(&mut payload, 1); // element count
    push_raw_name(&mut payload, 0); // inner tag name, then nothing else

    let mut d = legacy_struct_array_property(&payload, ue5);
    // Replace the trailing None with a second property, then the terminator.
    d.truncate(d.len() - 8);
    push_legacy_tag_header(&mut d, 4, 5, 4); // Weight, IntProperty
    push_legacy_tag_tail(&mut d, ue5);
    push_i32(&mut d, 42);
    push_raw_name(&mut d, 3); // None

    let ctx = legacy_struct_array_ctx(&names, ue5);
    let mut r = Reader::new(&d);
    let parse =
        crate::property::parse_properties_report(&mut r, &ctx, d.len() as u64, "/properties");

    assert_eq!(parse.entries.len(), 2, "{:#?}", parse.entries);
    assert!(parse.entries[0].value.is_opaque());
    assert!(
        parse
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "property_value_fallback"),
        "{:#?}",
        parse.diagnostics
    );
    // Resynchronised: the next property still decodes at the right offset.
    assert_eq!(parse.entries[1].name, "Weight");
    assert_eq!(parse.entries[1].value.as_i64(), Some(42));
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
        soft_object_paths_unavailable: false,
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
        nested_diagnostics: Default::default(),
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
        soft_object_paths_unavailable: false,
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
        nested_diagnostics: Default::default(),
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].value["asset_path"].as_str(), Some("/Game/B.B"));
}

/// A declared-but-unreadable soft-object-path table must not silently fall back
/// to inline decoding.
///
/// On disk the value is a 4-byte index into a list the parser could not build.
/// Reading it as an inline path (two FNames plus an FString) would misread every
/// soft reference in the package, so it stays undecoded with a stated reason.
#[test]
fn unavailable_soft_object_path_table_does_not_fall_back_to_inline() {
    let names = NameMap {
        names: vec![
            "Ref".to_string(),
            "SoftObjectProperty".to_string(),
            "None".to_string(),
        ],
    };
    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // Ref
    push_raw_name(&mut d, 1); // SoftObjectProperty
    push_i32(&mut d, 0); // type name inner param count
    push_i32(&mut d, 4); // size = the int32 index
    d.push(0); // flags
    push_i32(&mut d, 1); // the index itself
    push_raw_name(&mut d, 2); // None

    let ctx = ParseCtx {
        names: &names,
        resolve_object: &|_idx: i32| crate::DecodedValue::Null,
        pins: PinSerCtx::default(),
        // The table failed to parse, so nothing was decoded from it.
        soft_object_paths: &[],
        soft_object_paths_unavailable: true,
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
        nested_diagnostics: Default::default(),
    };
    let mut r = Reader::new(&d);
    let parse =
        crate::property::parse_properties_report(&mut r, &ctx, d.len() as u64, "/properties");

    assert_eq!(parse.entries.len(), 1);
    assert!(
        parse.entries[0].value.get("asset_path").is_none(),
        "an unresolvable index must not be reported as a path: {:?}",
        parse.entries[0].value
    );
    let diagnostic = parse
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == "property_value_fallback")
        .unwrap_or_else(|| panic!("expected a fallback diagnostic: {:#?}", parse.diagnostics));
    assert!(
        diagnostic.message.contains("soft object path list"),
        "the reason must name the unreadable list: {}",
        diagnostic.message
    );
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
        data.len() as u64,
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
        data.len() as u64,
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
        data.len() as u64,
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
        soft_object_paths_unavailable: false,
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
        nested_diagnostics: Default::default(),
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
        soft_object_paths_unavailable: false,
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
        nested_diagnostics: Default::default(),
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
        soft_object_paths_unavailable: false,
        serialization: crate::version::SerializationPolicy::default(),
        file_version_ue4: crate::version::ue4::HIGHEST,
        file_version_ue5: crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
        nested_diagnostics: Default::default(),
    };
    let mut r = Reader::new(&d);
    let entries = parse_properties(&mut r, &ctx, d.len() as u64);

    assert_eq!(entries.len(), 1);
    let elems = entries[0].value.as_array().unwrap();
    assert_eq!(elems.len(), 2);
    assert_eq!(elems[0].as_i64(), Some(7));
    assert_eq!(elems[1].as_i64(), Some(8));
}

/// Name table for the legacy `ByteProperty` container tests below.
fn legacy_byte_container_names() -> NameMap {
    NameMap {
        names: vec![
            "Prop".to_string(),           // 0
            "MapProperty".to_string(),    // 1
            "ByteProperty".to_string(),   // 2
            "DoubleProperty".to_string(), // 3
            "None".to_string(),           // 4
            "EMode::Fast".to_string(),    // 5
            "EMode::Slow".to_string(),    // 6
            "IntProperty".to_string(),    // 7
            "NameProperty".to_string(),   // 8
            "SetProperty".to_string(),    // 9
            "StructProperty".to_string(), // 10
            "X".to_string(),              // 11
            "ArrayProperty".to_string(),  // 12
        ],
    }
}

/// A legacy container tag named `Prop` of the given property type whose type
/// arguments are `params` (name indices), followed by `payload` and `None`.
fn legacy_container_property(type_idx: i32, params: &[i32], payload: &[u8]) -> Vec<u8> {
    let mut d = Vec::new();
    push_legacy_tag_header(&mut d, 0, type_idx, payload.len() as i32);
    for &param in params {
        push_raw_name(&mut d, param);
    }
    d.push(0); // HasPropertyGuid
    d.extend_from_slice(payload);
    push_raw_name(&mut d, 4);
    d
}

fn parse_legacy_container(d: &[u8], ctx: &ParseCtx) -> crate::property::PropertyParse {
    let mut r = Reader::new(d);
    crate::property::parse_properties_report(&mut r, ctx, d.len() as u64, "/properties")
}

#[test]
fn legacy_byte_keyed_map_reads_enum_name_keys_by_exact_fit() {
    let names = legacy_byte_container_names();
    let mut payload = Vec::new();
    push_i32(&mut payload, 0); // NumToRemove
    push_i32(&mut payload, 2);
    push_raw_name(&mut payload, 5);
    push_f64(&mut payload, 1.5);
    push_raw_name(&mut payload, 6);
    push_f64(&mut payload, 2.25);
    let d = legacy_container_property(1, &[2, 3], &payload);

    let parse = parse_legacy_container(&d, &legacy_ctx(&names));

    assert!(parse.diagnostics.is_empty(), "{:#?}", parse.diagnostics);
    let value = &parse.entries[0].value;
    assert_eq!(value[0]["key"].as_str(), Some("EMode::Fast"));
    assert_eq!(value[0]["value"].as_f64(), Some(1.5));
    assert_eq!(value[1]["key"].as_str(), Some("EMode::Slow"));
    assert_eq!(value[1]["value"].as_f64(), Some(2.25));
}

#[test]
fn legacy_byte_keyed_map_reads_raw_byte_keys_when_that_is_the_fit() {
    let names = legacy_byte_container_names();
    let mut payload = Vec::new();
    push_i32(&mut payload, 0);
    push_i32(&mut payload, 2);
    payload.push(7);
    push_i32(&mut payload, 100);
    payload.push(9);
    push_i32(&mut payload, 200);
    let d = legacy_container_property(1, &[2, 7], &payload);

    let parse = parse_legacy_container(&d, &legacy_ctx(&names));

    assert!(parse.diagnostics.is_empty(), "{:#?}", parse.diagnostics);
    let value = &parse.entries[0].value;
    assert_eq!(value[0]["key"].as_i64(), Some(7));
    assert_eq!(value[0]["value"].as_i64(), Some(100));
    assert_eq!(value[1]["key"].as_i64(), Some(9));
    assert_eq!(value[1]["value"].as_i64(), Some(200));
}

#[test]
fn legacy_map_with_byte_values_reads_enum_name_values() {
    let names = legacy_byte_container_names();
    let mut payload = Vec::new();
    push_i32(&mut payload, 0);
    push_i32(&mut payload, 1);
    push_raw_name(&mut payload, 11); // key: FName "X"
    push_raw_name(&mut payload, 5); // value: enum name
    let d = legacy_container_property(1, &[8, 2], &payload);

    let parse = parse_legacy_container(&d, &legacy_ctx(&names));

    assert!(parse.diagnostics.is_empty(), "{:#?}", parse.diagnostics);
    let value = &parse.entries[0].value;
    assert_eq!(value[0]["key"].as_str(), Some("X"));
    assert_eq!(value[0]["value"].as_str(), Some("EMode::Fast"));
}

#[test]
fn legacy_byte_set_with_a_removed_entry_reads_enum_names_in_both_lists() {
    let names = legacy_byte_container_names();
    let mut payload = Vec::new();
    push_i32(&mut payload, 1); // NumToRemove
    push_raw_name(&mut payload, 5);
    push_i32(&mut payload, 1); // Num
    push_raw_name(&mut payload, 6);
    let d = legacy_container_property(9, &[2], &payload);

    let parse = parse_legacy_container(&d, &legacy_ctx(&names));

    assert!(parse.diagnostics.is_empty(), "{:#?}", parse.diagnostics);
    let value = &parse.entries[0].value;
    assert_eq!(value.as_array().unwrap().len(), 1);
    assert_eq!(value[0].as_str(), Some("EMode::Slow"));
}

#[test]
fn legacy_byte_map_prefers_the_layout_whose_names_are_valid() {
    let names = legacy_byte_container_names();
    // Nine bytes fill either (byte key, enum value) or (enum key, byte value).
    // Read as an enum key they begin with index 3 + (6 << 8), which the table
    // does not contain, so only the first layout is real.
    let mut payload = Vec::new();
    push_i32(&mut payload, 0);
    push_i32(&mut payload, 1);
    payload.push(3);
    push_raw_name(&mut payload, 6);
    let d = legacy_container_property(1, &[2, 2], &payload);

    let parse = parse_legacy_container(&d, &legacy_ctx(&names));

    assert!(parse.diagnostics.is_empty(), "{:#?}", parse.diagnostics);
    let value = &parse.entries[0].value;
    assert_eq!(value[0]["key"].as_i64(), Some(3));
    assert_eq!(value[0]["value"].as_str(), Some("EMode::Slow"));
}

#[test]
fn legacy_byte_map_that_two_layouts_fill_differently_is_ambiguous() {
    let names = legacy_byte_container_names();
    // All-zero entry bytes are a valid `Prop` name and a zero byte in either
    // order, so (byte, enum) and (enum, byte) both fit with different values.
    let mut payload = Vec::new();
    push_i32(&mut payload, 0);
    push_i32(&mut payload, 1);
    payload.extend_from_slice(&[0; 9]);
    let d = legacy_container_property(1, &[2, 2], &payload);
    let value_start = (d.len() - 8 - payload.len()) as u64;

    let parse = parse_legacy_container(&d, &legacy_ctx(&names));

    assert!(parse.diagnostics.is_empty(), "{:#?}", parse.diagnostics);
    let opaque = parse.entries[0].value.as_opaque().expect("ambiguous value");
    assert_eq!(opaque.reason, OpaqueReason::AmbiguousLegacyByteWidth);
    assert_eq!(opaque.byte_range.start, value_start);
    assert_eq!(opaque.byte_range.end, value_start + payload.len() as u64);
    assert_eq!(opaque.byte_range.size, payload.len() as u64);
    assert_eq!(
        opaque.type_name.as_deref(),
        Some("MapProperty(ByteProperty,ByteProperty)")
    );
    assert!(opaque.message.contains("key=bytes"), "{}", opaque.message);
    assert!(
        opaque.message.contains("value=enum names"),
        "{}",
        opaque.message
    );
}

#[test]
fn legacy_byte_map_that_neither_width_fills_falls_back_naming_the_container() {
    let names = legacy_byte_container_names();
    let mut payload = Vec::new();
    push_i32(&mut payload, 0);
    push_i32(&mut payload, 1);
    payload.extend_from_slice(&[1, 2, 3, 4, 5]);
    let d = legacy_container_property(1, &[2, 3], &payload);

    let parse = parse_legacy_container(&d, &legacy_ctx(&names));

    let opaque = parse.entries[0]
        .value
        .as_opaque()
        .expect("value falls back");
    assert_eq!(opaque.reason, OpaqueReason::UndecodedValue);
    assert!(
        opaque
            .message
            .contains("MapProperty(ByteProperty,DoubleProperty)")
            && opaque.message.contains("13-byte value"),
        "{}",
        opaque.message
    );
    assert!(
        parse
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "property_value_fallback"),
        "{:#?}",
        parse.diagnostics
    );
}

#[test]
fn complete_type_name_byte_array_is_read_as_bytes_without_probing() {
    let names = legacy_byte_container_names();
    let mut d = Vec::new();
    push_raw_name(&mut d, 0); // Prop
    push_raw_name(&mut d, 12); // ArrayProperty
    push_i32(&mut d, 1); // one type parameter
    push_raw_name(&mut d, 2); // ByteProperty, no enum
    push_i32(&mut d, 0);
    // One element followed by bytes that would be an enum name at the legacy
    // widths; with a complete type name the element is a plain byte.
    let mut payload = Vec::new();
    push_i32(&mut payload, 1);
    push_raw_name(&mut payload, 5);
    push_i32(&mut d, payload.len() as i32);
    d.push(0); // flags
    d.extend_from_slice(&payload);
    push_raw_name(&mut d, 4);
    let mut ctx = legacy_ctx(&names);
    ctx.file_version_ue5 = crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME;

    let parse = parse_legacy_container(&d, &ctx);

    // Read as plain bytes (one element), the window is left 7 bytes short; a
    // container of fixed-width elements must fill it, so the value is opaque
    // rather than a byte array with a trailing warning.
    let opaque = parse.entries[0].value.as_opaque().expect("opaque value");
    assert_eq!(opaque.reason, OpaqueReason::ValueUnderconsumed);
    assert_eq!(opaque.byte_range.size, 12);
    assert!(
        parse
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "property_value_fallback"),
        "{:#?}",
        parse.diagnostics
    );
}
#[test]
fn a_rejected_byte_width_leaves_no_nested_diagnostics() {
    let names = legacy_byte_container_names();
    let mut payload = Vec::new();
    push_i32(&mut payload, 0);
    push_i32(&mut payload, 1);
    payload.push(3); // key: a plain byte
    // Value: an unnamed struct whose tagged block is `X: IntProperty = 1`.
    push_legacy_tag_header(&mut payload, 0, 7, 4);
    push_legacy_tag_tail(&mut payload, 1010);
    push_i32(&mut payload, 1);
    push_raw_name(&mut payload, 4);
    let d = legacy_container_property(1, &[2, 10], &payload);

    let parse = parse_legacy_container(&d, &legacy_ctx(&names));

    assert!(parse.diagnostics.is_empty(), "{:#?}", parse.diagnostics);
    let value = &parse.entries[0].value;
    assert_eq!(value[0]["key"].as_i64(), Some(3));
    assert_eq!(
        value[0]["value"]["properties"][0]["name"].as_str(),
        Some("Prop")
    );
    assert_eq!(
        value[0]["value"]["properties"][0]["value"].as_i64(),
        Some(1)
    );
}
