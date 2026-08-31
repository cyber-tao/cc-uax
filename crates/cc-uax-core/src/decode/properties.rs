use super::DecodedExport;
use super::member::distill_member;
use super::window::{ExportSerialWindow, preview_range};
use crate::diagnostic::Diagnostic;
use crate::property::{
    ParseCtx, PropertyParse, PropertyParseStatus, parse_object_properties_report,
};
use crate::reader::{RAW_NAME_BYTES, Reader};
use crate::structured_value::{Map, Value, json};
use crate::version::custom;

#[allow(clippy::too_many_arguments)]
pub(super) fn decode_properties_for_export(
    reader: &mut Reader,
    ctx: &ParseCtx,
    window: ExportSerialWindow,
    export_i: usize,
    class_full: &str,
    capture_properties: bool,
    diagnostics: &mut Vec<Diagnostic>,
    export: &mut DecodedExport,
) {
    let start = window.property_start;
    let end = window.property_end;
    if end == start {
        if capture_properties {
            export.properties = Some(Vec::new());
            export.property_status = Some(PropertyParseStatus::Empty);
        }
        return;
    }
    if end < start || reader.seek(start).is_err() {
        return;
    }

    let prop_path = format!("/exports/{export_i}/properties");
    let parsed = parse_object_properties_report(reader, ctx, end, &prop_path);
    let PropertyParse {
        entries,
        diagnostics: prop_diags,
        status,
        decoded_end,
    } = parsed;
    export.property_status = Some(status);
    diagnostics.extend(prop_diags);
    // A non-tagged payload is the one incomplete outcome the tag loop reaches
    // without saying anything: it decodes nothing and leaves the whole window
    // opaque, yet it still counts against tagged-property coverage. Without this
    // the report can be `partial` with an empty `diagnostics` array, which tells a
    // consumer nothing about which export is missing or why.
    if status == PropertyParseStatus::NonTaggedPayload {
        diagnostics.push(
            Diagnostic::warning(
                "export_payload_not_tagged",
                &prop_path,
                format!(
                    "export payload does not start with a tagged property block; the declared range [{start}, {end}) is retained as opaque evidence"
                ),
            )
            .with_offset(start)
            .with_context(json!({
                "class": class_full,
                "declared_property_range": window.has_declared_property_range,
                "range_start": start,
                "range_end": end,
            })),
        );
    }

    if let Some(member) = distill_member(&entries) {
        export.member = Some(member);
    }
    // The tag loop reports the last offset it consumed as evidence. A non-tagged
    // payload decodes nothing, and a failed parse stops at its last completed
    // property, so neither can claim bytes it did not decode; whatever is left
    // over becomes classified opaque tail instead.
    let mut end_of_decoded = if matches!(status, PropertyParseStatus::NonTaggedPayload) {
        start
    } else {
        decoded_end.unwrap_or(start).clamp(start, end)
    };
    if capture_properties {
        export.properties = Some(entries);
        // Known post-property serializers continue from where the tag loop
        // stopped, so anything they consume extends the decoded range.
        if reader.seek(end_of_decoded).is_ok()
            && consume_known_post_property_data(reader, ctx, window, class_full, export)
        {
            end_of_decoded = reader.pos().clamp(end_of_decoded, end);
        }
    }
    export.advance_decoded_end(end_of_decoded);
}

/// Runs the serializers that follow an export's tagged properties. Returns
/// whether the reader's position is now decoded evidence; a failed decode leaves
/// the cursor where it was so the region stays classified opaque rather than
/// being counted as consumed.
fn consume_known_post_property_data(
    reader: &mut Reader,
    ctx: &ParseCtx,
    window: ExportSerialWindow,
    class_full: &str,
    export: &mut DecodedExport,
) -> bool {
    if class_full != "/Script/CoreUObject.MetaData" || reader.pos() >= window.property_end {
        return false;
    }
    let metadata_start = reader.pos();
    match parse_package_metadata_tail(reader, ctx, window.property_end) {
        Ok(metadata) => {
            export.metadata = Some(metadata);
            true
        }
        Err(err) => {
            let payload = preview_range(reader, metadata_start, window.property_end);
            export.metadata = Some(json!({
                "status": "opaque",
                "reason": format!("failed to parse PackageMetaData payload: {err:#}"),
                "payload": payload
            }));
            let _ = reader.seek(metadata_start);
            false
        }
    }
}

fn parse_package_metadata_tail(
    reader: &mut Reader,
    ctx: &ParseCtx,
    end: u64,
) -> anyhow::Result<Value> {
    // UDEPRECATED_MetaData::Serialize (UE5.3–5.8) runs `Super::Serialize` first,
    // so the maps do not start at the property terminator: UObject::Serialize
    // appends PossiblySerializeObjectGuid after the tagged properties (Obj.cpp),
    // and only then does UMetaData write
    //   ObjectMetaDataMap: TMap<FWeakObjectPtr, TMap<FName, FString>>
    //   RootMetaDataMap:   TMap<FName, FString>
    // A serialized FWeakObjectPtr is `Ar << UObject*`, i.e. an FPackageIndex (i32)
    // in a linker-saved package — not a soft object path.
    skip_object_guid_field(reader, end)?;

    let object_count = reader.read_i32_within(end, "object metadata count")?;
    validate_metadata_count(object_count, reader, end, "object metadata")?;

    let mut object_metadata = Vec::with_capacity(object_count as usize);
    for _ in 0..object_count {
        let object = (ctx.resolve_object)(reader.read_i32_within(end, "metadata object")?);
        let values = parse_metadata_name_string_map(reader, ctx, end)?;
        object_metadata.push(json!({ "object": object, "values": values }));
    }

    // The root map exists only from FEditorObjectVersion::RootMetaDataSupport on;
    // a package without that custom version writes nothing here.
    let mut root_metadata = Map::new();
    if ctx.serialization.editor_version >= custom::EDITOR_ROOT_META_DATA_SUPPORT {
        let root_count = reader.read_i32_within(end, "root metadata count")?;
        validate_metadata_count(root_count, reader, end, "root metadata")?;
        for _ in 0..root_count {
            let key = ctx
                .names
                .resolve_raw(reader.read_raw_name_within(end, "root metadata key")?);
            let value = reader.read_fstring_within(end, "root metadata value")?;
            root_metadata.insert(key, json!(value));
        }
    }

    Ok(json!({
        "object_metadata": object_metadata,
        "root_metadata": root_metadata,
    }))
}

/// Consumes `UObject::Serialize`'s trailing `PossiblySerializeObjectGuid` so the
/// metadata maps that follow are read from the right offset. Unlike the export
/// tail walk this cannot guess: a malformed field means the whole metadata
/// payload is opaque.
fn skip_object_guid_field(reader: &mut Reader, end: u64) -> anyhow::Result<()> {
    super::read_object_guid_field(reader, end).map(|_| ())
}

fn parse_metadata_name_string_map(
    reader: &mut Reader,
    ctx: &ParseCtx,
    end: u64,
) -> anyhow::Result<Value> {
    let count = reader.read_i32_within(end, "metadata value count")?;
    validate_metadata_count(count, reader, end, "metadata value")?;
    let mut map = Map::new();
    for _ in 0..count {
        let key = ctx
            .names
            .resolve_raw(reader.read_raw_name_within(end, "metadata key")?);
        let value = reader.read_fstring_within(end, "metadata value")?;
        map.insert(key, json!(value));
    }
    Ok(Value::Object(map))
}

fn validate_metadata_count(
    count: i32,
    reader: &Reader,
    end: u64,
    label: &str,
) -> anyhow::Result<()> {
    crate::reader::validate_dynamic_count(
        count,
        end.saturating_sub(reader.pos()),
        RAW_NAME_BYTES,
        label,
    )
}

#[cfg(test)]
mod metadata_tests {
    use super::*;
    use crate::name::NameMap;
    use crate::pin::PinSerCtx;
    use crate::version::{SerializationPolicy, ue4, ue5};

    fn push_i32(v: &mut Vec<u8>, x: i32) {
        v.extend_from_slice(&x.to_le_bytes());
    }

    fn push_name(v: &mut Vec<u8>, index: i32) {
        push_i32(v, index);
        push_i32(v, 0);
    }

    fn push_fstring(v: &mut Vec<u8>, s: &str) {
        push_i32(v, (s.len() + 1) as i32);
        v.extend_from_slice(s.as_bytes());
        v.push(0);
    }

    fn metadata_ctx<'a>(names: &'a NameMap, editor_version: i32) -> ParseCtx<'a> {
        ParseCtx {
            names,
            resolve_object: &|index: i32| json!({ "index": index }),
            pins: PinSerCtx::default(),
            soft_object_paths: &[],
            soft_object_paths_unavailable: false,
            serialization: SerializationPolicy {
                editor_version,
                ..SerializationPolicy::default()
            },
            file_version_ue4: ue4::HIGHEST,
            file_version_ue5: ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
        }
    }

    // UDEPRECATED_MetaData::Serialize runs Super::Serialize first, so the maps
    // start after UObject's PossiblySerializeObjectGuid, not at the property
    // terminator. The regressions this guards: skipping that presence flag,
    // reading the root count up front, and reading the object key as a soft
    // object path — each of which misaligns the stream.
    #[test]
    fn package_metadata_reads_object_guid_then_object_map_then_root_map() {
        let names = NameMap {
            names: vec![
                "BlueprintType".to_string(),
                "PackageLocalizationNamespace".to_string(),
            ],
        };
        let mut data = Vec::new();
        push_i32(&mut data, 0); // PossiblySerializeObjectGuid: not present
        push_i32(&mut data, 1); // object_count
        push_i32(&mut data, 2); // FWeakObjectPtr key = FPackageIndex(2)
        push_i32(&mut data, 1); // value map count
        push_name(&mut data, 0); // FName "BlueprintType"
        push_fstring(&mut data, "true");
        push_i32(&mut data, 1); // root_count, after the object map
        push_name(&mut data, 1); // FName "PackageLocalizationNamespace"
        push_fstring(&mut data, "NS");

        let ctx = metadata_ctx(&names, custom::EDITOR_ROOT_META_DATA_SUPPORT);
        let mut reader = Reader::new(&data);
        let value = parse_package_metadata_tail(&mut reader, &ctx, data.len() as u64).unwrap();

        let objects = value["object_metadata"].as_array().unwrap();
        assert_eq!(objects.len(), 1);
        assert_eq!(objects[0]["object"]["index"].as_i64(), Some(2));
        assert_eq!(objects[0]["values"]["BlueprintType"].as_str(), Some("true"));
        assert_eq!(
            value["root_metadata"]["PackageLocalizationNamespace"].as_str(),
            Some("NS")
        );
        // The full window is consumed with no misalignment.
        assert_eq!(reader.pos(), data.len() as u64);
    }

    // A present object GUID adds 16 bytes between the properties and the maps.
    #[test]
    fn package_metadata_consumes_a_present_object_guid() {
        let names = NameMap {
            names: vec!["ToolTip".to_string()],
        };
        let mut data = Vec::new();
        push_i32(&mut data, 1); // PossiblySerializeObjectGuid: present
        for _ in 0..4 {
            push_i32(&mut data, 0x1234_5678);
        }
        push_i32(&mut data, 1); // object_count
        push_i32(&mut data, -3); // FWeakObjectPtr key = import index
        push_i32(&mut data, 1); // value map count
        push_name(&mut data, 0);
        push_fstring(&mut data, "hint");

        let ctx = metadata_ctx(&names, -1);
        let mut reader = Reader::new(&data);
        let value = parse_package_metadata_tail(&mut reader, &ctx, data.len() as u64).unwrap();

        let objects = value["object_metadata"].as_array().unwrap();
        assert_eq!(objects[0]["object"]["index"].as_i64(), Some(-3));
        assert_eq!(objects[0]["values"]["ToolTip"].as_str(), Some("hint"));
        assert_eq!(reader.pos(), data.len() as u64);
    }

    // Below FEditorObjectVersion::RootMetaDataSupport there is no root map, so
    // reading one would consume bytes belonging to the next serializer.
    #[test]
    fn package_metadata_skips_the_root_map_before_its_custom_version() {
        let names = NameMap {
            names: vec!["ToolTip".to_string()],
        };
        let mut data = Vec::new();
        push_i32(&mut data, 0); // no object guid
        push_i32(&mut data, 0); // object_count
        let consumed = data.len() as u64;
        push_i32(&mut data, 0xDEAD); // belongs to a later serializer

        let ctx = metadata_ctx(&names, custom::EDITOR_ROOT_META_DATA_SUPPORT - 1);
        let mut reader = Reader::new(&data);
        let value = parse_package_metadata_tail(&mut reader, &ctx, data.len() as u64).unwrap();

        assert!(value["object_metadata"].as_array().unwrap().is_empty());
        assert!(value["root_metadata"].as_object().unwrap().is_empty());
        assert_eq!(reader.pos(), consumed);
    }

    // An FString whose payload runs past the window must fail rather than read
    // into the following export's bytes.
    #[test]
    fn package_metadata_value_cannot_read_past_the_window() {
        let names = NameMap {
            names: vec!["ToolTip".to_string()],
        };
        let mut data = Vec::new();
        push_i32(&mut data, 0); // no object guid
        push_i32(&mut data, 1); // object_count
        push_i32(&mut data, 1); // object key
        push_i32(&mut data, 1); // value map count
        push_name(&mut data, 0);
        push_i32(&mut data, 64); // FString length beyond the window
        data.extend_from_slice(b"short");

        let ctx = metadata_ctx(&names, -1);
        let mut reader = Reader::new(&data);
        let err = parse_package_metadata_tail(&mut reader, &ctx, data.len() as u64).unwrap_err();
        assert!(format!("{err:#}").contains("metadata value"), "{err:#}");
    }
}
