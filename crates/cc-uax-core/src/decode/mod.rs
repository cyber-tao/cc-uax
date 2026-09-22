mod import_data;
mod member;
pub(crate) mod pins;
mod properties;
pub(crate) mod rigvm;
mod window;

pub(crate) use import_data::ImportSourceFile;

use crate::diagnostic::{ByteRangePreview, Diagnostic};
use crate::package::Package;
use crate::pin::{Pin, PinSerCtx, UserDefinedPin};
use crate::property::{ParseCtx, PropertyEntry, PropertyParseStatus};
use crate::reader::{Guid, Reader};
pub(crate) use crate::script::is_script_bytecode_class;

use crate::script::{DecodedScriptStruct, ScriptStructContext, decode_script_struct};
use crate::structured_value::{Value, json};
use crate::version::{SerializationPolicy, custom, ue5};
use std::collections::HashMap;

use import_data::{
    decode_import_data_prefix, is_asset_import_data_class, writes_import_data_prefix,
};
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
    /// `UAssetImportData`'s source-file list, decoded from the JSON `FString`
    /// the class writes before its tagged properties.
    pub(crate) source_files: Option<Vec<ImportSourceFile>>,
    /// End of a decoded class prefix (bytes before the tagged block that a
    /// class-specific decoder consumed). Only the remainder up to
    /// `property_start`, if any, is an undecoded pre-script region.
    pub(crate) decoded_prefix_end: Option<u64>,
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
    /// Where the tag loop itself stopped: the byte after the `None` terminator
    /// on a clean block, or after the last completed property otherwise. This is
    /// the only position that says whether the block closed; later decoders
    /// (pins, script) move the high-water mark past it and must not be read as
    /// evidence about the block.
    pub(crate) property_block_end: Option<u64>,
    /// The pin decoder ran on a graph node and could not decode its region. The
    /// bytes are then unattributed even though the property block closed.
    pub(crate) pins_failed: bool,
    /// Every byte range a decoder actually consumed, in claim order. Byte
    /// conservation is computed from the union of these, so a decoder that starts
    /// past where the previous one stopped leaves a visible gap rather than a
    /// silently "decoded" stretch.
    pub(crate) decoded_spans: Vec<(u64, u64)>,
    /// Bytes between two claimed spans that neither decoder consumed. Classified
    /// opaque and counted as unattributed: they always point at a decoder that
    /// stopped short or started late.
    pub(crate) decoded_gaps: Vec<ByteRangePreview>,
    /// End of the contiguous decoded region (high-water mark set by each
    /// decoder). `None` means no decoder ran and the whole payload is opaque.
    pub(crate) decoded_end: Option<u64>,
    /// `serial_size` of this export; the per-export byte-conservation total.
    pub(crate) serial_size: u64,
    /// Export payload bytes left neither decoded nor classified as opaque.
    pub(crate) unclassified_bytes: u64,
}

impl DecodedExport {
    /// Records `[start, end)` as consumed by a decoder and raises the decoded
    /// high-water mark, so the tail step opaque-classifies only what is left and
    /// can see any bytes between claims that nobody consumed.
    pub(crate) fn claim_span(&mut self, start: u64, end: u64) {
        if end > start {
            self.decoded_spans.push((start, end));
        }
        self.decoded_end = Some(self.decoded_end.map_or(end, |current| current.max(end)));
    }

    /// The export never wrote a tagged-property block, so its whole payload is
    /// the class's own serializer data (see `PropertyParseStatus::NativeOnly`).
    pub(crate) fn is_native_only_payload(&self) -> bool {
        self.property_status == Some(PropertyParseStatus::NativeOnly)
    }

    /// Whether the bytes left after every decoder ran are attributable to the
    /// class's own `Serialize`: either they follow a cleanly closed property
    /// block, or the class never wrote one. Anything else is unattributed and
    /// points at a decoding gap.
    pub(crate) fn tail_is_class_payload(&self) -> bool {
        (self.property_block_closed && !self.pins_failed) || self.is_native_only_payload()
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DecodedExportIdentity {
    /// The export's `FPackageIndex` (1-based), the same number `exports[].index`
    /// carries in the report.
    pub(crate) index: i32,
    pub(crate) name: String,
    pub(crate) class: String,
    pub(crate) is_asset: bool,
}

impl DecodedExportIdentity {
    /// The report path prefix for everything about this export. Diagnostics,
    /// opaque regions and `exports[].index` all key on the package index, so a
    /// consumer can join them; a second convention here (an array position)
    /// pointed the same byte range at two different exports.
    pub(crate) fn path(&self) -> String {
        format!("/exports/{}", self.index)
    }
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
        if let Some(err) = &self.package_metadata_error {
            diagnostics.push(Diagnostic::warning(
                "package_metadata_table_error",
                "/summary/metadata_offset",
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
            soft_object_paths_unavailable: self.soft_object_path_error.is_some(),
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
            nested_diagnostics: Default::default(),
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
                source_files: None,
                decoded_prefix_end: None,
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
                property_block_end: None,
                pins_failed: false,
                decoded_spans: Vec::new(),
                decoded_gaps: Vec::new(),
                decoded_end: None,
                serial_size: 0,
                unclassified_bytes: 0,
            };

            let export_path = export.identity.path();
            let mut serial_window = match export_serial_window(
                exp,
                has_script,
                file_len,
                is_native_only_payload_class(&class_full),
            ) {
                Ok(w) => w,
                Err(err) => {
                    diagnostics.push(
                        Diagnostic::error("serial_window_invalid", export_path.clone(), err)
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
                decode_rigvm_link_for_export(
                    &mut reader,
                    window,
                    &export_path,
                    diagnostics,
                    &mut export,
                );
            } else if let Some(window) = serial_window
                && !window.writes_tagged_block
            {
                // No tagged block means no property, pin, GUID or script decoder
                // has anything to read: every byte is the class's own serializer
                // data, classified as such by the tail step.
                export.property_status = Some(PropertyParseStatus::NativeOnly);
                account_export_tail(
                    &mut reader,
                    window,
                    &class_full,
                    &script_ctx,
                    &export_path,
                    diagnostics,
                    &mut export,
                );
                decoded.push(export);
                continue;
            }

            // UAssetImportData writes a JSON FString ahead of its tagged block.
            // Decoding it is what lets the tag loop start where the block really
            // starts on packages without a declared range, and turns the
            // provenance it holds into evidence instead of an opaque prefix.
            if let Some(window) = serial_window.as_mut()
                && window.writes_tagged_block
                && is_asset_import_data_class(&class_full)
                && writes_import_data_prefix(
                    self.summary.file_version_ue4,
                    self.summary.filter_editor_only(),
                )
                && let Some((source_files, prefix_end)) =
                    decode_import_data_prefix(&mut reader, *window)
            {
                export.source_files = Some(source_files);
                export.decoded_prefix_end = Some(prefix_end);
                if !window.has_declared_property_range {
                    window.property_start = prefix_end;
                }
            }

            // URigVMLink has no tagged block either; its two FStrings were read above.
            if !is_rigvm_link
                && (options.properties || is_node || capture_adapter_properties)
                && let Some(window) = serial_window
            {
                decode_properties_for_export(
                    &mut reader,
                    &ctx,
                    window,
                    &export_path,
                    &class_full,
                    options.properties || capture_adapter_properties,
                    diagnostics,
                    &mut export,
                );
            }

            // A `*Node` export owned by a `*Graph` export that no node rule
            // recognised would have its pin stream filed as class payload and the
            // graph reported complete without it. Say so instead.
            if options.pins
                && !is_node
                && !is_rigvm_link
                && serial_window.is_some()
                && looks_like_unrecognized_graph_node(self, exp, &class_full)
            {
                diagnostics.push(
                    Diagnostic::warning(
                        "graph_node_class_unrecognized",
                        format!("{export_path}/pins"),
                        format!(
                            "{class_full} is owned by a graph and named like a node but is not a known UEdGraphNode class; its pins were not decoded"
                        ),
                    )
                    .with_context(json!({ "class": class_full })),
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
                    &export_path,
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
                    &export_path,
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
    export_path: &str,
    diagnostics: &mut Vec<Diagnostic>,
    export: &mut DecodedExport,
) {
    let serial_size = window.serial_end.saturating_sub(window.serial_start);
    export.serial_size = serial_size;
    // A class prefix a decoder consumed is evidence, not an opaque region; only
    // what is left between it and the tagged block is undecoded.
    let prefix_end = export
        .decoded_prefix_end
        .unwrap_or(window.serial_start)
        .clamp(window.serial_start, window.property_start);
    let prefix_bytes = prefix_end - window.serial_start;
    if window.property_start > prefix_end {
        export.pre_script_region = Some(preview_range(reader, prefix_end, window.property_start));
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
    // Whether the block closed is judged from where the *tag loop* stopped, not
    // from the high-water mark: the pin decoder legitimately moves that mark past
    // `property_end`, and a failed pin decode leaves it exactly there, so reading
    // the mark inverted the two cases. `property_end` only marks the real end of
    // the block when UE declared one; otherwise it is just `serial_end`.
    let property_block_closed = matches!(
        export.property_status,
        Some(PropertyParseStatus::Complete | PropertyParseStatus::Empty)
    ) && (!window.has_declared_property_range
        || export.property_block_end == Some(window.property_end));
    export.property_block_closed = property_block_closed;
    if export.object_guid.is_none()
        && export.pins.is_none()
        && decoded_end < window.serial_end
        && property_block_closed
        && reader.seek(decoded_end).is_ok()
    {
        consume_object_guid_tail(reader, window.serial_end, export);
        let guid_end = reader.pos().clamp(decoded_end, window.serial_end);
        export.claim_span(decoded_end, guid_end);
        decoded_end = guid_end;
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
                let script_end = script_struct.end.clamp(decoded_end, window.serial_end);
                export.claim_span(decoded_end, script_end);
                decoded_end = script_end;
                if let Some(code) = &script_struct.bytecode {
                    if let Some(failure) = &code.failure {
                        diagnostics.push(Diagnostic::warning(
                            "script_bytecode_undecoded",
                            export_path.to_string(),
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
                            export_path.to_string(),
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
                    export_path.to_string(),
                    format!("{class_full} script serializer could not be decoded: {error:#}"),
                ));
            }
        }
    }
    if decoded_end < window.serial_end {
        export.post_property_tail = Some(preview_range(reader, decoded_end, window.serial_end));
    }

    // Byte conservation over the export: pre-script region + union of claimed
    // spans + gaps between claims + tail must equal serial_size. Gaps are bytes a
    // decoder skipped over (the next one started past where the previous one
    // stopped); they are classified opaque and unattributed so they show up
    // rather than being counted as decoded. Overlapping claims mean two decoders
    // both accounted for the same bytes, which is a bookkeeping defect and lands
    // in unclassified_bytes with a diagnostic.
    let mut spans: Vec<(u64, u64)> = export
        .decoded_spans
        .iter()
        .map(|&(start, end)| {
            (
                start.clamp(window.property_start, window.serial_end),
                end.clamp(window.property_start, window.serial_end),
            )
        })
        .filter(|(start, end)| end > start)
        .collect();
    spans.sort_unstable();
    let claimed_total: u64 = spans.iter().map(|(start, end)| end - start).sum();
    let mut covered = 0u64;
    let mut cursor = window.property_start;
    let mut gaps = Vec::new();
    for (start, end) in spans {
        if start > cursor {
            gaps.push((cursor, start));
        }
        let start = start.max(cursor);
        if end > start {
            covered += end - start;
        }
        cursor = cursor.max(end);
    }
    let overlap = claimed_total.saturating_sub(covered);
    let gap_total: u64 = gaps.iter().map(|(start, end)| end - start).sum();
    export.decoded_gaps = gaps
        .into_iter()
        .map(|(start, end)| preview_range(reader, start, end))
        .collect();
    if overlap > 0 {
        diagnostics.push(
            Diagnostic::warning(
                "export_bytes_double_claimed",
                export_path.to_string(),
                format!(
                    "{overlap} byte(s) were claimed by more than one decoder; the export's byte accounting is not trustworthy"
                ),
            )
            .with_offset(window.property_start),
        );
    }
    let pre = export.pre_script_region.as_ref().map_or(0, |p| p.size);
    let post = export.post_property_tail.as_ref().map_or(0, |p| p.size);
    export.unclassified_bytes = serial_size
        .saturating_sub(prefix_bytes)
        .saturating_sub(pre)
        .saturating_sub(post)
        .saturating_sub(covered)
        .saturating_sub(gap_total)
        .saturating_add(overlap);
}

/// Reads UObject's `PossiblySerializeObjectGuid` (a presence flag optionally
/// followed by an `FGuid`) at the reader's position, bounded by `end`.
///
/// The flag is written through `FStructuredArchive::TryEnterField`, which in a
/// binary archive emits `FArchive::SerializeBool` — exactly 0 or 1 as a uint32.
/// Any other value means the caller is not positioned at the end of the
/// tagged-property block.
///
/// One layout, two policies: the metadata decoder must stay aligned so it treats
/// a failure here as fatal, while the export tail walk is speculative and rolls
/// back. Keeping the layout in one place is what stops those policies from
/// drifting into two different readings of the same bytes.
pub(super) fn read_object_guid_field(
    reader: &mut Reader,
    end: u64,
) -> anyhow::Result<Option<Guid>> {
    match reader.read_i32_within(end, "object guid presence")? {
        0 => Ok(None),
        1 => Ok(Some(reader.read_guid_within(end, "object guid")?)),
        other => anyhow::bail!("unexpected object guid presence flag {other}"),
    }
}

fn consume_object_guid_tail(reader: &mut Reader, end: u64, export: &mut DecodedExport) {
    let start = reader.pos();
    match read_object_guid_field(reader, end) {
        // No annotation was written; the flag itself is still part of the field.
        Ok(None) => {}
        Ok(Some(guid)) if !guid.is_zero() => {
            export.object_guid = Some(guid.to_hex());
        }
        // A zero GUID or an unreadable field means these bytes are the class's own
        // payload, so leave them for the tail classifier instead of claiming them.
        Ok(Some(_)) | Err(_) => {
            let _ = reader.seek(start);
        }
    }
}

/// An export whose class name ends in `Node`, whose outer is an export of a class
/// named `*Graph`, and which no node rule claimed. `UEdGraph` owns its nodes
/// directly, so this is the shape a missing entry in the node-class list takes.
/// Sub-graphs (`*Graph` inside a graph) and members that are not nodes (MetaSound
/// graph members, comment metadata) do not end in `Node` and are not flagged.
fn looks_like_unrecognized_graph_node(
    package: &Package,
    exp: &crate::object::ObjectExport,
    class_full: &str,
) -> bool {
    let simple = class_full.rsplit(['.', '/']).next().unwrap_or(class_full);
    if !simple.ends_with("Node") {
        return false;
    }
    // PCG and RigVM model nodes live under a `*Graph` too, but they are model
    // objects read by their own adapters, not `UEdGraphNode`s with a pin stream.
    if is_pcg_model_object_class(class_full) || is_rigvm_model_object_class(class_full) {
        return false;
    }
    let outer = exp.outer_index.0;
    if outer <= 0 {
        return false;
    }
    let Some(outer_export) = package.exports.get((outer - 1) as usize) else {
        return false;
    };
    let outer_class = package.resolve_full_name(outer_export.class_index.0);
    outer_class
        .rsplit(['.', '/'])
        .next()
        .is_some_and(|name| name.ends_with("Graph"))
}

/// Classes whose `Serialize` never calls `Super::Serialize`, so their export
/// payload contains no tagged-property block at all.
///
/// From `FileVersionUE5` 1010 the export table says this directly (a zero script
/// range, see [`window::ExportSerialWindow::writes_tagged_block`]); this list is
/// what stands in for it on older packages. Each entry is verified against UE
/// source, 5.0 through 5.8:
/// - `URigHierarchy::Serialize` (`RigHierarchy.cpp`): `Save(Ar)`/`Load(Ar)` only.
/// - `URigVM::Serialize` (`RigVM.cpp`): calls `Super::Serialize` only for
///   reference collectors and memory counting, never for a linker archive.
pub(crate) fn is_native_only_payload_class(class: &str) -> bool {
    matches!(
        class,
        "/Script/ControlRig.RigHierarchy" | "/Script/RigVM.RigVM"
    )
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
