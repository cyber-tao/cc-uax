use super::DecodedExport;
use super::member::distill_member;
use super::window::{ExportSerialWindow, preview_range};
use crate::diagnostic::Diagnostic;
use crate::property::{
    ParseCtx, PropertyParse, PropertyParseStatus, parse_object_properties_report,
};
use crate::reader::Reader;
use crate::structured_value::{Map, Value, json};

#[allow(clippy::too_many_arguments)]
pub(super) fn decode_properties_for_export(
    reader: &mut Reader,
    ctx: &ParseCtx,
    has_script: bool,
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
    } = parsed;
    export.property_status = Some(status);
    diagnostics.extend(prop_diags);

    if let Some(member) = distill_member(&entries) {
        export.member = Some(member);
    }
    if capture_properties {
        export.properties = Some(entries);
        consume_known_post_property_data(
            reader,
            ctx,
            has_script,
            window,
            export_i,
            class_full,
            diagnostics,
            export,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn consume_known_post_property_data(
    reader: &mut Reader,
    ctx: &ParseCtx,
    has_script: bool,
    window: ExportSerialWindow,
    _export_i: usize,
    class_full: &str,
    _diagnostics: &mut Vec<Diagnostic>,
    export: &mut DecodedExport,
) {
    if !has_script {
        consume_object_guid_tail(reader, window.property_end, export);
    }
    if class_full == "/Script/CoreUObject.MetaData" && reader.pos() < window.property_end {
        let metadata_start = reader.pos();
        match parse_package_metadata_tail(reader, ctx, window.property_end) {
            Ok(metadata) => {
                export.metadata = Some(metadata);
            }
            Err(err) => {
                let payload = preview_range(reader, metadata_start, window.property_end);
                export.metadata = Some(json!({
                    "status": "opaque",
                    "reason": format!("failed to parse PackageMetaData payload: {err:#}"),
                    "payload": payload
                }));
                let _ = reader.seek(window.property_end);
            }
        }
    }
    if reader.pos() < window.property_end {
        let tail = preview_range(reader, reader.pos(), window.property_end);
        export.post_property_tail = Some(tail);
    }
}

fn consume_object_guid_tail(reader: &mut Reader, end: u64, export: &mut DecodedExport) {
    if end.saturating_sub(reader.pos()) < 4 {
        return;
    }
    let start = reader.pos();
    match reader.read_bool32() {
        Ok(true) if end.saturating_sub(reader.pos()) >= 16 => {
            if let Ok(guid) = reader.read_guid()
                && !guid.is_zero()
            {
                export.object_guid = Some(guid.to_hex());
            }
        }
        Ok(true) => {
            let _ = reader.seek(start);
        }
        Ok(false) => {}
        Err(_) => {
            let _ = reader.seek(start);
        }
    }
}

fn parse_package_metadata_tail(
    reader: &mut Reader,
    ctx: &ParseCtx,
    end: u64,
) -> anyhow::Result<Value> {
    // UDEPRECATED_MetaData::Serialize (UE5.7):
    //   ObjectMetaDataMap: TMap<FWeakObjectPtr, TMap<FName, FString>>
    //   RootMetaDataMap:   TMap<FName, FString>  (gated by FEditorObjectVersion::RootMetaDataSupport)
    // A serialized FWeakObjectPtr is `Ar << UObject*`, i.e. an FPackageIndex (i32)
    // in a linker-saved package — not a soft object path. The root map follows the
    // full object map; there is no second count read up front.
    let object_count = reader.read_i32()?;
    validate_metadata_count(object_count, reader, end, "object metadata")?;

    let mut object_metadata = Vec::with_capacity(object_count as usize);
    for _ in 0..object_count {
        let object = (ctx.resolve_object)(reader.read_i32()?);
        let values = parse_metadata_name_string_map(reader, ctx, end)?;
        object_metadata.push(json!({ "object": object, "values": values }));
    }

    // RootMetaDataMap is present in every editor package new enough to carry
    // FEditorObjectVersion::RootMetaDataSupport; read it when payload remains.
    let mut root_metadata = Map::new();
    if reader.pos() < end {
        let root_count = reader.read_i32()?;
        validate_metadata_count(root_count, reader, end, "root metadata")?;
        for _ in 0..root_count {
            let key = ctx.names.resolve_raw(reader.read_raw_name()?);
            let value = reader.read_fstring()?;
            root_metadata.insert(key, json!(value));
        }
    }

    Ok(json!({
        "object_metadata": object_metadata,
        "root_metadata": root_metadata,
    }))
}

fn parse_metadata_name_string_map(
    reader: &mut Reader,
    ctx: &ParseCtx,
    end: u64,
) -> anyhow::Result<Value> {
    let count = reader.read_i32()?;
    validate_metadata_count(count, reader, end, "metadata value")?;
    let mut map = Map::new();
    for _ in 0..count {
        let key = ctx.names.resolve_raw(reader.read_raw_name()?);
        let value = reader.read_fstring()?;
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
    if count < 0 || (count as u64).saturating_mul(8) > end.saturating_sub(reader.pos()) {
        anyhow::bail!("{label} count out of range: {count}");
    }
    Ok(())
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

    // UDEPRECATED_MetaData::Serialize writes the whole object map (keyed by an
    // FWeakObjectPtr = FPackageIndex i32), then the root map. The regression this
    // guards: reading the root count up front, or reading the object key as a
    // soft object path, both misalign the stream.
    #[test]
    fn package_metadata_reads_object_map_then_root_map() {
        let names = NameMap {
            names: vec![
                "BlueprintType".to_string(),
                "PackageLocalizationNamespace".to_string(),
            ],
        };
        let mut data = Vec::new();
        push_i32(&mut data, 1); // object_count
        push_i32(&mut data, 2); // FWeakObjectPtr key = FPackageIndex(2)
        push_i32(&mut data, 1); // value map count
        push_name(&mut data, 0); // FName "BlueprintType"
        push_fstring(&mut data, "true");
        push_i32(&mut data, 1); // root_count, after the object map
        push_name(&mut data, 1); // FName "PackageLocalizationNamespace"
        push_fstring(&mut data, "NS");

        let ctx = ParseCtx {
            names: &names,
            resolve_object: &|index: i32| json!({ "index": index }),
            pins: PinSerCtx::default(),
            soft_object_paths: &[],
            serialization: SerializationPolicy::default(),
            file_version_ue4: ue4::HIGHEST,
            file_version_ue5: ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME,
        };
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
}
