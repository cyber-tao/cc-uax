//! Classification of the export bytes no decoder claimed.
//!
//! Every unclaimed byte has to end up in known_opaque with a cause, because
//! unclassified_bytes staying zero is what makes coverage an account rather
//! than a counter.

use crate::decode::{
    DecodeReport, DecodedExport, is_niagara_compiled_class, is_script_bytecode_class,
};
use crate::model::{KnownOpaque, KnownOpaqueKind, OpaqueByteRange};
use crate::structured_value::{Map, Value};
use std::collections::BTreeSet;

/// Why an export has bytes left after every decoder ran.
///
/// A tail after a cleanly closed property block is data the class's own
/// `Serialize` override wrote — mesh render data, lightmaps, compiled bytecode —
/// and is expected. A tail after an unresolved property block is unattributed:
/// the decoder does not know what those bytes are, and it is the only one of the
/// two that points at a decoding gap. On a real project the first accounts for
/// gigabytes of bulk asset data, so sharing one reason with the second made the
/// opaque byte total unreadable.
pub(super) fn tail_reason(export: &DecodedExport) -> &'static str {
    if !export.property_block_closed {
        return "bytes follow a tagged-property block that did not close cleanly, so they cannot be attributed";
    }
    if is_script_bytecode_class(&export.identity.class) {
        // `UStruct::Serialize`, including the compiled script, is decoded; what
        // can be left is the concrete class's own block, which for a generated
        // class is `UClass::Serialize`.
        return if export.script_struct.is_some() {
            "class serializer data written after the decoded UStruct block (UClass::Serialize)"
        } else {
            "compiled script struct written after the tagged properties (UStruct::Serialize)"
        };
    }
    if is_niagara_compiled_class(&export.identity.class) {
        return "compiled Niagara VM/GPU payload written after the tagged properties";
    }
    "class-owned serializer data written after the tagged properties"
}

pub(super) fn collect_known_opaque(
    report: &DecodeReport<'_>,
    include_property_values: bool,
) -> Vec<KnownOpaque> {
    let mut opaque = Vec::new();
    for export in &report.exports {
        let export_path = format!("/exports/{}", export.identity.index);
        if let Some(pre) = &export.pre_script_region
            && pre.size > 0
        {
            opaque.push(KnownOpaque {
                path: format!("{export_path}/pre_script_region"),
                kind: KnownOpaqueKind::PreScriptRegion,
                type_name: Some(export.identity.class.clone()),
                reason: "bytes precede the tagged-property block and are not decoded".into(),
                byte_range: Some(OpaqueByteRange {
                    start: pre.start,
                    end: pre.end,
                    size: pre.size,
                    preview: pre.preview.clone(),
                }),
            });
        }
        if let Some(tail) = &export.post_property_tail
            && tail.size > 0
        {
            opaque.push(KnownOpaque {
                path: format!("{export_path}/post_property_tail"),
                kind: KnownOpaqueKind::PostPropertyTail,
                type_name: Some(export.identity.class.clone()),
                reason: tail_reason(export).into(),
                byte_range: Some(OpaqueByteRange {
                    start: tail.start,
                    end: tail.end,
                    size: tail.size,
                    preview: tail.preview.clone(),
                }),
            });
        }
        if include_property_values {
            if let Some(properties) = &export.properties {
                for property in properties {
                    collect_opaque_value(
                        &property.value,
                        &format!("{export_path}/properties/{}", property.name),
                        Some(&property.type_str),
                        KnownOpaqueKind::PropertyValue,
                        &mut opaque,
                    );
                }
            }
            if let Some(metadata) = &export.metadata {
                collect_opaque_value(
                    metadata,
                    &format!("{export_path}/metadata"),
                    Some("PackageMetaData"),
                    KnownOpaqueKind::Metadata,
                    &mut opaque,
                );
            }
        }
    }
    opaque
}

pub(super) fn collect_opaque_value(
    value: &Value,
    path: &str,
    type_name: Option<&str>,
    kind: KnownOpaqueKind,
    output: &mut Vec<KnownOpaque>,
) {
    match value {
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                collect_opaque_value(value, &format!("{path}/{index}"), type_name, kind, output);
            }
        }
        Value::Object(object) => {
            if let Some(properties) = object.get("properties").and_then(Value::as_array) {
                for property in properties {
                    let Some(entry) = property.as_object() else {
                        continue;
                    };
                    let (Some(name), Some(value)) = (
                        entry.get("name").and_then(Value::as_str),
                        entry.get("value"),
                    ) else {
                        continue;
                    };
                    collect_opaque_value(
                        value,
                        &format!("{path}/{name}"),
                        entry.get("type").and_then(Value::as_str).or(type_name),
                        kind,
                        output,
                    );
                }
            }
            let reason = if object.contains_key("@unparsed") {
                Some("property decoder emitted an unparsed byte preview".to_string())
            } else if object.get("status").and_then(Value::as_str) == Some("opaque") {
                Some(
                    object
                        .get("reason")
                        .and_then(Value::as_str)
                        .unwrap_or("decoder marked the value opaque")
                        .to_string(),
                )
            } else if object.contains_key("@struct") && object.contains_key("payload") {
                Some("custom struct payload is retained without semantic decoding".to_string())
            } else if object.get("size").is_some_and(Value::is_number)
                && object.get("preview").is_some_and(Value::is_string)
            {
                Some("byte payload is represented only by a bounded preview".to_string())
            } else {
                None
            };
            if let Some(reason) = reason {
                let path = path.strip_suffix("/serialized_data").unwrap_or(path);
                output.push(KnownOpaque {
                    path: path.to_string(),
                    kind,
                    type_name: type_name.map(normalize_opaque_type_name),
                    reason,
                    byte_range: opaque_byte_range(object).or_else(|| {
                        object
                            .get("payload")
                            .and_then(Value::as_object)
                            .and_then(opaque_byte_range)
                    }),
                });
                return;
            }
            for (key, value) in object {
                if key == "properties" {
                    continue;
                }
                collect_opaque_value(value, &format!("{path}/{key}"), type_name, kind, output);
            }
        }
        _ => {}
    }
}

pub(super) fn normalize_opaque_type_name(type_name: &str) -> String {
    let Some(offset) = type_name.find("StructProperty(") else {
        return type_name.to_string();
    };
    let rest = &type_name[offset + "StructProperty(".len()..];
    rest.split(['(', ')']).next().unwrap_or(rest).to_string()
}

pub(super) fn dedupe_known_opaque(values: &mut Vec<KnownOpaque>) {
    let mut seen = BTreeSet::new();
    values.retain(|value| {
        seen.insert((
            opaque_kind_rank(value.kind),
            value.path.clone(),
            value.type_name.clone(),
        ))
    });
}

pub(super) fn opaque_kind_rank(kind: KnownOpaqueKind) -> u8 {
    match kind {
        KnownOpaqueKind::PropertyValue => 0,
        KnownOpaqueKind::PreScriptRegion => 1,
        KnownOpaqueKind::PostPropertyTail => 2,
        KnownOpaqueKind::Metadata => 3,
        KnownOpaqueKind::Capability => 4,
    }
}

pub(super) fn opaque_byte_range(object: &Map) -> Option<OpaqueByteRange> {
    let start = object.get("start")?.as_u64()?;
    let end = object.get("end")?.as_u64()?;
    let size = object.get("size")?.as_u64()?;
    if end.checked_sub(start)? != size {
        return None;
    }
    Some(OpaqueByteRange {
        start,
        end,
        size,
        preview: object
            .get("preview")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
    })
}
