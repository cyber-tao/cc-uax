mod member;
pub(crate) mod pins;
mod properties;
pub(crate) mod rigvm;
mod window;

use crate::diagnostic::{ByteRangePreview, Diagnostic};
use crate::package::Package;
use crate::pin::{Pin, PinSerCtx, UserDefinedPin};
use crate::property::{ParseCtx, PropertyEntry, PropertyParseStatus};
use crate::reader::Reader;
pub(crate) use crate::script::is_script_bytecode_class;

use crate::script::{DecodedScriptStruct, ScriptStructContext, decode_script_struct};
use crate::structured_value::{Value, json};
use crate::version::{SerializationPolicy, custom, ue5};
use std::collections::HashMap;

use pins::{decode_pins_for_export, is_graph_node_class};
use properties::decode_properties_for_export;
use rigvm::{
    DecodedRigVmLink, decode_rigvm_link_for_export, is_rigvm_link_class,
    is_rigvm_model_object_class,
};
use window::{ExportSerialWindow, export_serial_window, preview_range};

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct DecodeOptions {
    pub(crate) exports: bool,
    pub(crate) pins: bool,
    pub(crate) properties: bool,
}

impl DecodeOptions {
    pub(crate) const fn none() -> Self {
        Self {
            exports: false,
            pins: false,
            properties: false,
        }
    }

    pub(crate) const fn full() -> Self {
        Self {
            exports: true,
            pins: true,
            properties: true,
        }
    }
}

pub(crate) struct DecodeReport<'a> {
    pub(crate) package: &'a Package,
    pub(crate) exports: Vec<DecodedExport>,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone)]
pub(crate) struct DecodedExport {
    pub(crate) identity: DecodedExportIdentity,
    pub(crate) properties: Option<Vec<PropertyEntry>>,
    pub(crate) property_status: Option<PropertyParseStatus>,
    pub(crate) pre_script_region: Option<ByteRangePreview>,
    pub(crate) post_property_tail: Option<ByteRangePreview>,
    pub(crate) object_guid: Option<String>,
    pub(crate) metadata: Option<Value>,
    pub(crate) pins: Option<Vec<Pin>>,
    pub(crate) user_defined_pins: Option<Vec<UserDefinedPin>>,
    pub(crate) member: Option<MemberRef>,
    pub(crate) rigvm_link: Option<DecodedRigVmLink>,
    /// The `UStruct`/`UFunction` serializer block, for the classes that write
    /// compiled script. Its absence on such a class means the block could not be
    /// decoded and the whole remainder stays opaque.
    pub(crate) script_struct: Option<DecodedScriptStruct>,
    /// Whether the tagged-property block ended where it was supposed to. When it
    /// did, a remaining tail is data the class's own `Serialize` override wrote
    /// (mesh render data, lightmaps, script bytecode); when it did not, the tail
    /// is unattributed and the decoder cannot say what those bytes are. Those are
    /// very different pieces of evidence and must not share one reason string.
    pub(crate) property_block_closed: bool,
    /// End of the contiguous decoded region (high-water mark set by each
    /// decoder). `None` means no decoder ran and the whole payload is opaque.
    pub(crate) decoded_end: Option<u64>,
    /// `serial_size` of this export; the per-export byte-conservation total.
    pub(crate) serial_size: u64,
    /// Export payload bytes left neither decoded nor classified as opaque.
    pub(crate) unclassified_bytes: u64,
}

impl DecodedExport {
    /// Raises the decoded high-water mark; decoders call this after consuming
    /// their region so the tail step opaque-classifies only what is left.
    pub(crate) fn advance_decoded_end(&mut self, pos: u64) {
        self.decoded_end = Some(self.decoded_end.map_or(pos, |current| current.max(pos)));
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DecodedExportIdentity {
    pub(crate) index: i32,
    pub(crate) name: String,
    pub(crate) class: String,
    pub(crate) is_asset: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct MemberRef {
    pub(crate) name: String,
    pub(crate) parent: Option<Value>,
}

impl Package {
    pub(crate) fn decode<'a>(&'a self, data: &[u8], options: &DecodeOptions) -> DecodeReport<'a> {
        let mut diagnostics = self.table_diagnostics();
        let exports = if options.exports {
            self.decode_exports(data, options, &mut diagnostics)
        } else {
            Vec::new()
        };
        DecodeReport {
            package: self,
            exports,
            diagnostics,
        }
    }

    fn table_diagnostics(&self) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        if let Some(err) = &self.soft_object_path_error {
            diagnostics.push(Diagnostic::warning(
                "soft_object_path_table_error",
                "/summary/soft_object_paths",
                err.clone(),
            ));
        }
        if let Some(err) = &self.soft_package_reference_error {
            diagnostics.push(Diagnostic::warning(
                "soft_package_reference_table_error",
                "/summary/soft_package_references",
                err.clone(),
            ));
        }
        diagnostics
    }

    fn decode_exports(
        &self,
        data: &[u8],
        options: &DecodeOptions,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Vec<DecodedExport> {
        let object_ref_memo = std::cell::RefCell::new(HashMap::<i32, Value>::new());
        let resolve = |idx: i32| {
            if idx == 0 {
                return Value::Null;
            }
            object_ref_memo
                .borrow_mut()
                .entry(idx)
                .or_insert_with(|| self.resolve_object_ref(idx))
                .clone()
        };
        let pin_ctx = PinSerCtx::from_summary(&self.summary);
        let ctx = ParseCtx {
            names: &self.names,
            resolve_object: &resolve,
            pins: pin_ctx,
            soft_object_paths: &self.soft_object_paths,
            serialization: SerializationPolicy {
                niagara_version: self
                    .summary
                    .custom_version(custom::NIAGARA_OBJECT_VERSION)
                    .unwrap_or(-1),
                fortnite_main_version: self
                    .summary
                    .custom_version(custom::FORTNITE_MAIN_OBJECT_VERSION)
                    .unwrap_or(-1),
                instanced_struct_version: self
                    .summary
                    .custom_version(custom::INSTANCED_STRUCT_VERSION)
                    .unwrap_or(-1),
                state_tree_instance_storage_version: self
                    .summary
                    .custom_version(custom::STATE_TREE_INSTANCE_STORAGE_VERSION)
                    .unwrap_or(-1),
                fortnite_release_version: self
                    .summary
                    .custom_version(custom::FORTNITE_RELEASE_BRANCH_OBJECT_VERSION)
                    .unwrap_or(-1),
                property_bag_version: self
                    .summary
                    .custom_version(custom::PROPERTY_BAG_VERSION)
                    .unwrap_or(-1),
                ue5_release_stream_version: self
                    .summary
                    .custom_version(custom::UE5_RELEASE_STREAM_OBJECT_VERSION)
                    .unwrap_or(-1),
                editor_version: self
                    .summary
                    .custom_version(custom::EDITOR_OBJECT_VERSION)
                    .unwrap_or(-1),
            },
            file_version_ue4: self.summary.file_version_ue4,
            file_version_ue5: self.summary.file_version_ue5,
        };
        let script_ctx = ScriptStructContext::new(self);
        let mut reader = Reader::new(data);
        let file_len = reader.len();
        let has_script = self.summary.file_version_ue5 >= ue5::SCRIPT_SERIALIZATION_OFFSET;
        let mut decoded = Vec::with_capacity(self.exports.len());

        for (i, exp) in self.exports.iter().enumerate() {
            let pkg_index = (i as i32) + 1;
            let class_full = self.resolve_full_name(exp.class_index.0);
            let is_node = is_graph_node_class(&class_full);
            let is_rigvm_link = is_rigvm_link_class(&class_full);
            let capture_adapter_properties = options.pins
                && ((is_rigvm_model_object_class(&class_full) && !is_rigvm_link)
                    || is_pcg_model_object_class(&class_full)
                    || is_state_tree_model_object_class(&class_full));
            let mut export = DecodedExport {
                identity: DecodedExportIdentity {
                    index: pkg_index,
                    name: self.names.resolve_raw(exp.object_name),
                    class: class_full.clone(),
                    is_asset: exp.is_asset,
                },
                properties: None,
                property_status: None,
                pre_script_region: None,
                post_property_tail: None,
                object_guid: None,
                metadata: None,
                pins: None,
                user_defined_pins: None,
                member: None,
                rigvm_link: None,
                script_struct: None,
                property_block_closed: false,
                decoded_end: None,
                serial_size: 0,
                unclassified_bytes: 0,
            };

            let serial_window = match export_serial_window(exp, has_script, file_len) {
                Ok(w) => w,
                Err(err) => {
                    diagnostics.push(
                        Diagnostic::error("serial_window_invalid", format!("/exports/{i}"), err)
                            .with_context(json!({
                                "export_index": pkg_index,
                                "serial_offset": exp.serial_offset,
                                "serial_size": exp.serial_size,
                            })),
                    );
                    // No valid window: the payload cannot be accounted for, so
                    // every declared byte is unclassified.
                    let size = exp.serial_size.max(0) as u64;
                    export.serial_size = size;
                    export.unclassified_bytes = size;
                    decoded.push(export);
                    continue;
                }
            };

            if is_rigvm_link
                && (options.properties || options.pins)
                && let Some(window) = serial_window
            {
                decode_rigvm_link_for_export(&mut reader, window, i, diagnostics, &mut export);
            } else if (options.properties || is_node || capture_adapter_properties)
                && let Some(window) = serial_window
            {
                decode_properties_for_export(
                    &mut reader,
                    &ctx,
                    window,
                    i,
                    &class_full,
                    options.properties || capture_adapter_properties,
                    diagnostics,
                    &mut export,
                );
            }

            if options.pins
                && let Some(window) = serial_window
            {
                decode_pins_for_export(
                    self,
                    &mut reader,
                    &ctx,
                    &pin_ctx,
                    has_script,
                    window,
                    i,
                    &class_full,
                    diagnostics,
                    &mut export,
                );
            }

            if let Some(window) = serial_window {
                account_export_tail(
                    &mut reader,
                    window,
                    &class_full,
                    &script_ctx,
                    i,
                    diagnostics,
                    &mut export,
                );
            }

            decoded.push(export);
        }
        decoded
    }
}

/// Registers every export byte that no decoder claimed as classified opaque so
/// that `serial_size == decoded + opaque` holds and `unclassified_bytes` is 0.
/// The pre-script region and the post-decoder tail are the only two gaps a
/// bounded export window can leave once each decoder reports its high-water mark.
fn account_export_tail(
    reader: &mut Reader,
    window: ExportSerialWindow,
    class_full: &str,
    script_ctx: &ScriptStructContext<'_>,
    export_index: usize,
    diagnostics: &mut Vec<Diagnostic>,
    export: &mut DecodedExport,
) {
    let serial_size = window.serial_end.saturating_sub(window.serial_start);
    export.serial_size = serial_size;
    if window.property_start > window.serial_start {
        export.pre_script_region = Some(preview_range(
            reader,
            window.serial_start,
            window.property_start,
        ));
    }
    let mut decoded_end = export
        .decoded_end
        .unwrap_or(window.property_start)
        .clamp(window.property_start, window.serial_end);
    // UObject::Serialize writes PossiblySerializeObjectGuid immediately after
    // SerializeScriptProperties returns (Obj.cpp), so the flag follows the tagged
    // properties even when a subclass Serialize override appends more data. Graph
    // nodes read it inside the pin decoder; every other export reads it here so
    // the GUID becomes evidence instead of opaque tail.
    //
    // `property_end` only marks the real end of the property block when UE
    // declared one; otherwise it is just `serial_end` and comparing against it
    // would make this branch unreachable for every non-script export.
    let property_block_closed = matches!(
        export.property_status,
        Some(PropertyParseStatus::Complete | PropertyParseStatus::Empty)
    ) && (!window.has_declared_property_range
        || decoded_end == window.property_end);
    export.property_block_closed = property_block_closed;
    if export.object_guid.is_none()
        && export.pins.is_none()
        && decoded_end < window.serial_end
        && property_block_closed
        && reader.seek(decoded_end).is_ok()
    {
        consume_object_guid_tail(reader, window.serial_end, export);
        decoded_end = reader.pos().clamp(decoded_end, window.serial_end);
    }
    // `UStruct::Serialize` resumes exactly here: everything before it belongs to
    // `UObject`, and the compiled script sits a few fixed fields further on.
    if property_block_closed
        && decoded_end < window.serial_end
        && is_script_bytecode_class(class_full)
        && reader.seek(decoded_end).is_ok()
    {
        match decode_script_struct(reader, window.serial_end, class_full, script_ctx) {
            Ok(script_struct) => {
                decoded_end = script_struct.end.clamp(decoded_end, window.serial_end);
                if let Some(code) = &script_struct.bytecode {
                    if let Some(failure) = &code.failure {
                        diagnostics.push(Diagnostic::warning(
                            "script_bytecode_undecoded",
                            format!("/exports/{export_index}"),
                            format!(
                                "compiled script bytecode could not be disassembled: {failure}"
                            ),
                        ));
                    } else if !code.sizes_agree() {
                        // The disk length is enforced by the bounded read, so a
                        // mismatch here means an expression's in-memory width is
                        // wrong even though it consumed the right file bytes.
                        diagnostics.push(Diagnostic::warning(
                            "script_bytecode_size_mismatch",
                            format!("/exports/{export_index}"),
                            format!(
                                "disassembly accounted for {} in-memory byte(s) but the struct declares {}",
                                code.summary.as_ref().map_or(0, |summary| summary.icode),
                                code.buffer_size
                            ),
                        ));
                    }
                }
                export.script_struct = Some(script_struct);
            }
            Err(error) => {
                diagnostics.push(Diagnostic::warning(
                    "script_struct_undecoded",
                    format!("/exports/{export_index}"),
                    format!("{class_full} script serializer could not be decoded: {error:#}"),
                ));
            }
        }
    }
    if decoded_end < window.serial_end {
        export.post_property_tail = Some(preview_range(reader, decoded_end, window.serial_end));
    }
    let pre = export.pre_script_region.as_ref().map_or(0, |p| p.size);
    let post = export.post_property_tail.as_ref().map_or(0, |p| p.size);
    let decoded = decoded_end.saturating_sub(window.property_start);
    export.unclassified_bytes = serial_size
        .saturating_sub(pre)
        .saturating_sub(post)
        .saturating_sub(decoded);
}

/// Reads UObject's `PossiblySerializeObjectGuid` (a presence flag optionally
/// followed by an `FGuid`) at the reader's position, bounded by `end`.
///
/// The flag is written through `FStructuredArchive::TryEnterField`, which in a
/// binary archive emits `FArchive::SerializeBool` — exactly 0 or 1 as a uint32.
/// Any other value means the property block did not end where the caller thinks
/// it did, so the bytes stay classified opaque rather than being consumed.
fn consume_object_guid_tail(reader: &mut Reader, end: u64, export: &mut DecodedExport) {
    let start = reader.pos();
    if end.saturating_sub(start) < 4 {
        return;
    }
    let Ok(present) = reader.read_u32() else {
        let _ = reader.seek(start);
        return;
    };
    match present {
        0 => {}
        1 => {
            if end.saturating_sub(reader.pos()) >= 16
                && let Ok(guid) = reader.read_guid()
                && !guid.is_zero()
            {
                export.object_guid = Some(guid.to_hex());
            } else {
                let _ = reader.seek(start);
            }
        }
        _ => {
            let _ = reader.seek(start);
        }
    }
}

fn is_pcg_model_object_class(class: &str) -> bool {
    class.starts_with("/Script/PCG.") || class.starts_with("/Script/PCGEditor.")
}

fn is_state_tree_model_object_class(class: &str) -> bool {
    class.starts_with("/Script/StateTree")
}

/// A Niagara object whose payload includes a compiled VM or GPU representation
/// (`FNiagaraVMExecutableData`, simulation-stage and shader data) rather than
/// source-level graph evidence.
pub(crate) fn is_niagara_compiled_class(class: &str) -> bool {
    let Some(simple) = class.rsplit(['.', '/']).next() else {
        return false;
    };
    class.starts_with("/Script/Niagara.")
        && matches!(
            simple,
            "NiagaraSystem" | "NiagaraScript" | "NiagaraEmitter" | "NiagaraSimulationStageBase"
        )
}
