use super::DecodedExport;
use super::window::{ExportSerialWindow, preview_range};
use crate::diagnostic::Diagnostic;
use crate::reader::Reader;
use serde_json::json;

const RIGVM_DEVELOPER_PREFIX: &str = "/Script/RigVMDeveloper.";
const RIGVM_GRAPH_CLASS: &str = "/Script/RigVMDeveloper.RigVMGraph";
const RIGVM_FUNCTION_LIBRARY_CLASS: &str = "/Script/RigVMDeveloper.RigVMFunctionLibrary";
const RIGVM_PIN_CLASS: &str = "/Script/RigVMDeveloper.RigVMPin";
const RIGVM_LINK_CLASS: &str = "/Script/RigVMDeveloper.RigVMLink";
const RIGVM_INJECTION_INFO_CLASS: &str = "/Script/RigVMDeveloper.RigVMInjectionInfo";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DecodedRigVmLink {
    pub(crate) source_pin_path: String,
    pub(crate) target_pin_path: String,
}

pub(crate) fn is_rigvm_graph_class(class_full: &str) -> bool {
    matches!(class_full, RIGVM_GRAPH_CLASS | RIGVM_FUNCTION_LIBRARY_CLASS)
}

pub(crate) fn is_rigvm_link_class(class_full: &str) -> bool {
    class_full == RIGVM_LINK_CLASS
}

pub(crate) fn is_rigvm_node_class(class_full: &str) -> bool {
    class_full
        .strip_prefix(RIGVM_DEVELOPER_PREFIX)
        .is_some_and(|simple| simple.starts_with("RigVM") && simple.ends_with("Node"))
}

pub(crate) fn is_rigvm_model_object_class(class_full: &str) -> bool {
    if is_rigvm_graph_class(class_full)
        || is_rigvm_link_class(class_full)
        || class_full == RIGVM_PIN_CLASS
        || class_full == RIGVM_INJECTION_INFO_CLASS
    {
        return true;
    }
    is_rigvm_node_class(class_full)
}

/// `URigVMLink::Serialize` intentionally does not call `Super::Serialize`.
/// Its complete package payload is exactly two consecutive `FString` values.
pub(super) fn decode_rigvm_link_for_export(
    reader: &mut Reader<'_>,
    window: ExportSerialWindow,
    export_i: usize,
    diagnostics: &mut Vec<Diagnostic>,
    export: &mut DecodedExport,
) {
    let start = window.property_start;
    let end = window.serial_end;
    let path = format!("/exports/{export_i}/rigvm_link");
    if end < start {
        diagnostics.push(
            Diagnostic::error(
                "rigvm_link_window_invalid",
                path,
                format!("RigVM link window [{start}, {end}) is invalid"),
            )
            .with_offset(start),
        );
        return;
    }

    let payload_size = match usize::try_from(end - start) {
        Ok(size) => size,
        Err(_) => {
            diagnostics.push(
                Diagnostic::error(
                    "rigvm_link_window_too_large",
                    path,
                    format!("RigVM link payload size {} does not fit usize", end - start),
                )
                .with_offset(start),
            );
            return;
        }
    };
    if reader.seek(start).is_err() {
        return;
    }
    let payload = match reader.read_bytes(payload_size) {
        Ok(payload) => payload,
        Err(err) => {
            diagnostics.push(
                Diagnostic::error(
                    "rigvm_link_payload_read_failed",
                    path,
                    format!("failed to read bounded RigVM link payload: {err:#}"),
                )
                .with_offset(start),
            );
            return;
        }
    };

    let mut payload_reader = Reader::new(&payload);
    let source_pin_path = match payload_reader.read_fstring() {
        Ok(value) => value,
        Err(err) => {
            record_link_decode_failure(
                reader,
                start,
                end,
                export_i,
                format!("failed to decode source pin path: {err:#}"),
                diagnostics,
                export,
            );
            return;
        }
    };
    let target_pin_path = match payload_reader.read_fstring() {
        Ok(value) => value,
        Err(err) => {
            record_link_decode_failure(
                reader,
                start,
                end,
                export_i,
                format!("failed to decode target pin path: {err:#}"),
                diagnostics,
                export,
            );
            return;
        }
    };

    export.rigvm_link = Some(DecodedRigVmLink {
        source_pin_path,
        target_pin_path,
    });
    if payload_reader.remaining() > 0 {
        let tail_start = start + payload_reader.pos();
        export.post_property_tail = Some(preview_range(reader, tail_start, end));
        diagnostics.push(
            Diagnostic::warning(
                "rigvm_link_trailing_bytes",
                path,
                format!(
                    "{} byte(s) remain after the two URigVMLink FString fields",
                    payload_reader.remaining()
                ),
            )
            .with_offset(tail_start)
            .with_context(json!({
                "payload_start": start,
                "payload_end": end,
            })),
        );
    }
    let _ = reader.seek(end);
}

fn record_link_decode_failure(
    reader: &mut Reader<'_>,
    start: u64,
    end: u64,
    export_i: usize,
    message: String,
    diagnostics: &mut Vec<Diagnostic>,
    export: &mut DecodedExport,
) {
    export.post_property_tail = Some(preview_range(reader, start, end));
    diagnostics.push(
        Diagnostic::error(
            "rigvm_link_decode_failed",
            format!("/exports/{export_i}/rigvm_link"),
            message,
        )
        .with_offset(start),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn push_fstring(out: &mut Vec<u8>, value: &str) {
        out.extend_from_slice(&((value.len() + 1) as i32).to_le_bytes());
        out.extend_from_slice(value.as_bytes());
        out.push(0);
    }

    #[test]
    fn rigvm_model_classification_is_narrow() {
        assert!(is_rigvm_graph_class(RIGVM_GRAPH_CLASS));
        assert!(is_rigvm_model_object_class(
            "/Script/RigVMDeveloper.RigVMUnitNode"
        ));
        assert!(is_rigvm_model_object_class(RIGVM_PIN_CLASS));
        assert!(!is_rigvm_model_object_class(
            "/Script/ControlRigDeveloper.ControlRigGraphNode"
        ));
    }

    #[test]
    fn two_fstrings_fill_the_exact_link_payload() {
        let mut payload = Vec::new();
        push_fstring(&mut payload, "Source.ExecuteContext");
        push_fstring(&mut payload, "Target.ExecuteContext");
        let mut reader = Reader::new(&payload);
        let mut diagnostics = Vec::new();
        let mut export = empty_link_export();
        decode_rigvm_link_for_export(
            &mut reader,
            ExportSerialWindow {
                property_start: 0,
                property_end: payload.len() as u64,
                serial_end: payload.len() as u64,
            },
            0,
            &mut diagnostics,
            &mut export,
        );

        assert!(diagnostics.is_empty());
        assert!(export.post_property_tail.is_none());
        assert_eq!(
            export.rigvm_link,
            Some(DecodedRigVmLink {
                source_pin_path: "Source.ExecuteContext".into(),
                target_pin_path: "Target.ExecuteContext".into(),
            })
        );
    }

    #[test]
    fn bounded_reader_rejects_truncated_target_path() {
        let mut payload = Vec::new();
        push_fstring(&mut payload, "Source.Value");
        payload.extend_from_slice(&10_i32.to_le_bytes());
        payload.extend_from_slice(b"short");
        let mut reader = Reader::new(&payload);
        let mut diagnostics = Vec::new();
        let mut export = empty_link_export();
        decode_rigvm_link_for_export(
            &mut reader,
            ExportSerialWindow {
                property_start: 0,
                property_end: payload.len() as u64,
                serial_end: payload.len() as u64,
            },
            0,
            &mut diagnostics,
            &mut export,
        );

        assert!(export.rigvm_link.is_none());
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "rigvm_link_decode_failed");
        assert_eq!(
            export.post_property_tail.unwrap().size,
            payload.len() as u64
        );
    }

    fn empty_link_export() -> DecodedExport {
        DecodedExport {
            identity: crate::decode::DecodedExportIdentity {
                index: 1,
                name: "RigVMLink_0".into(),
                class: RIGVM_LINK_CLASS.into(),
                is_asset: false,
            },
            layout: None,
            properties: None,
            property_status: None,
            post_property_tail: None,
            object_guid: None,
            metadata: None,
            pins: None,
            user_defined_pins: None,
            member: None,
            rigvm_link: None,
        }
    }
}
