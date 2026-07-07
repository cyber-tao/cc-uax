use crate::diagnostic::{ByteRangePreview, Diagnostic};
use crate::object::ObjectExport;
use crate::output::sections::OutputSections;
use crate::package::Package;
use crate::pin::{Pin, PinSerCtx, parse_node_pins_report};
use crate::property::{
    ParseCtx, PropertyEntry, PropertyParse, parse_object_properties_report, read_soft_object_path,
    to_hex,
};
use crate::reader::Reader;
use crate::version::{custom, ue5};
use serde_json::{Value, json};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct DecodeOptions {
    pub sections: OutputSections,
}

impl DecodeOptions {
    pub fn new(sections: OutputSections) -> Self {
        Self { sections }
    }
}

impl Default for DecodeOptions {
    fn default() -> Self {
        Self {
            sections: OutputSections::default(),
        }
    }
}

impl From<OutputSections> for DecodeOptions {
    fn from(sections: OutputSections) -> Self {
        Self { sections }
    }
}

impl From<&OutputSections> for DecodeOptions {
    fn from(sections: &OutputSections) -> Self {
        Self {
            sections: sections.clone(),
        }
    }
}

pub struct DecodeReport<'a> {
    pub package: &'a Package,
    pub sections: OutputSections,
    pub exports: Vec<DecodedExport>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone)]
pub struct DecodedExport {
    pub identity: DecodedExportIdentity,
    pub layout: Option<DecodedExportLayout>,
    pub properties: Option<Vec<PropertyEntry>>,
    pub post_property_tail: Option<ByteRangePreview>,
    pub object_guid: Option<String>,
    pub metadata: Option<Value>,
    pub pins: Option<Vec<Pin>>,
    pub member: Option<MemberRef>,
}

#[derive(Debug, Clone)]
pub struct DecodedExportIdentity {
    pub index: i32,
    pub name: String,
    pub class: String,
    pub is_asset: bool,
}

#[derive(Debug, Clone)]
pub struct DecodedExportLayout {
    pub super_name: String,
    pub template_name: String,
    pub outer_name: String,
    pub full_name: String,
    pub object_flags: u32,
    pub serial_offset: i64,
    pub serial_size: i64,
    pub script_serialization_start: Option<i64>,
    pub script_serialization_end: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct MemberRef {
    pub name: String,
    pub parent: Option<Value>,
}

#[derive(Debug, Clone, Copy)]
struct ExportSerialWindow {
    property_start: u64,
    property_end: u64,
    serial_end: u64,
}

impl Package {
    pub fn decode<'a>(&'a self, data: &[u8], options: &DecodeOptions) -> DecodeReport<'a> {
        let mut diagnostics = self.table_diagnostics();
        let exports = if options.sections.exports {
            self.decode_exports(data, &options.sections, &mut diagnostics)
        } else {
            Vec::new()
        };
        DecodeReport {
            package: self,
            sections: options.sections.clone(),
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
        sections: &OutputSections,
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
            niagara_version: self
                .summary
                .custom_version(custom::NIAGARA_OBJECT_VERSION)
                .unwrap_or(-1),
            fortnite_main_version: self
                .summary
                .custom_version(custom::FORTNITE_MAIN_OBJECT_VERSION)
                .unwrap_or(-1),
            file_version_ue4: self.summary.file_version_ue4,
            file_version_ue5: self.summary.file_version_ue5,
        };
        let mut reader = Reader::new(data);
        let file_len = reader.len();
        let has_script = self.summary.file_version_ue5 >= ue5::SCRIPT_SERIALIZATION_OFFSET;
        let mut decoded = Vec::with_capacity(self.exports.len());

        for (i, exp) in self.exports.iter().enumerate() {
            let pkg_index = (i as i32) + 1;
            let class_full = self.resolve_full_name(exp.class_index.0);
            let is_node = is_graph_node_class(&class_full);
            let mut export = DecodedExport {
                identity: DecodedExportIdentity {
                    index: pkg_index,
                    name: self.names.resolve_raw(exp.object_name),
                    class: class_full.clone(),
                    is_asset: exp.is_asset,
                },
                layout: sections.layout.then(|| DecodedExportLayout {
                    super_name: self.resolve_full_name(exp.super_index.0),
                    template_name: self.resolve_full_name(exp.template_index.0),
                    outer_name: self.resolve_full_name(exp.outer_index.0),
                    full_name: self.resolve_full_name(pkg_index),
                    object_flags: exp.object_flags,
                    serial_offset: exp.serial_offset,
                    serial_size: exp.serial_size,
                    script_serialization_start: has_script
                        .then_some(exp.script_serialization_start_offset),
                    script_serialization_end: has_script
                        .then_some(exp.script_serialization_end_offset),
                }),
                properties: None,
                post_property_tail: None,
                object_guid: None,
                metadata: None,
                pins: None,
                member: None,
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
                    decoded.push(export);
                    continue;
                }
            };

            if (sections.properties || is_node)
                && let Some(window) = serial_window
            {
                decode_properties_for_export(
                    &mut reader,
                    &ctx,
                    has_script,
                    window,
                    i,
                    &class_full,
                    sections,
                    diagnostics,
                    &mut export,
                );
            }

            if sections.pins
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

            decoded.push(export);
        }
        decoded
    }
}

#[allow(clippy::too_many_arguments)]
fn decode_properties_for_export(
    reader: &mut Reader,
    ctx: &ParseCtx,
    has_script: bool,
    window: ExportSerialWindow,
    export_i: usize,
    class_full: &str,
    sections: &OutputSections,
    diagnostics: &mut Vec<Diagnostic>,
    export: &mut DecodedExport,
) {
    let start = window.property_start;
    let end = window.property_end;
    if end == start {
        if sections.properties {
            export.properties = Some(Vec::new());
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
    } = parsed;
    diagnostics.extend(prop_diags);

    if let Some(member) = distill_member(&entries) {
        export.member = Some(member);
    }
    if sections.properties {
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
    export_i: usize,
    class_full: &str,
    diagnostics: &mut Vec<Diagnostic>,
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
                diagnostics.push(
                    Diagnostic::warning(
                        "package_metadata_tail_unparsed",
                        format!("/exports/{export_i}/metadata"),
                        format!("failed to parse PackageMetaData payload: {err:#}"),
                    )
                    .with_offset(metadata_start),
                );
                let _ = reader.seek(metadata_start);
            }
        }
    }
    if reader.pos() < window.property_end {
        let tail = preview_range(reader, reader.pos(), window.property_end);
        diagnostics.push(
            Diagnostic::warning(
                "post_property_tail",
                format!("/exports/{export_i}/post_property_tail"),
                format!("{} byte(s) remain after property decoding", tail.size),
            )
            .with_offset(tail.start)
            .with_context(json!({ "tail": tail })),
        );
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
    let object_count = reader.read_i32()?;
    validate_metadata_count(object_count, reader, end, "object metadata")?;
    let root_count = reader.read_i32()?;
    validate_metadata_count(root_count, reader, end, "root metadata")?;

    let mut object_metadata = Vec::with_capacity(object_count as usize);
    for _ in 0..object_count {
        let object = read_soft_object_path(reader, ctx.names)?;
        let values = parse_metadata_name_string_map(reader, ctx, end)?;
        object_metadata.push(json!({ "object": object, "values": values }));
    }

    let mut root_metadata = serde_json::Map::new();
    for _ in 0..root_count {
        let key = ctx.names.resolve_raw(reader.read_raw_name()?);
        let value = reader.read_fstring()?;
        root_metadata.insert(key, json!(value));
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
    let mut map = serde_json::Map::new();
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

#[allow(clippy::too_many_arguments)]
fn decode_pins_for_export(
    package: &Package,
    reader: &mut Reader,
    ctx: &ParseCtx,
    pin_ctx: &PinSerCtx,
    has_script: bool,
    window: ExportSerialWindow,
    export_i: usize,
    class_full: &str,
    diagnostics: &mut Vec<Diagnostic>,
    export: &mut DecodedExport,
) {
    if !has_script || !is_graph_node_class(class_full) {
        return;
    }
    let pin_start = window.property_end;
    let pin_end = window.serial_end;
    if pin_end <= pin_start || reader.seek(pin_start).is_err() {
        return;
    }

    let candidates = pin_parse_contexts(package, *pin_ctx);
    let mut best = None;
    let mut best_pos = pin_start;
    let mut failures = Vec::new();
    for candidate in candidates {
        if reader.seek(pin_start).is_err() {
            continue;
        }
        let path = format!("/exports/{export_i}/pins");
        match parse_node_pins_report(reader, pin_end, ctx, &candidate, &path) {
            Ok(parsed) => {
                let consumed_pos = reader.pos();
                if consumed_pos >= best_pos {
                    best_pos = consumed_pos;
                    best = Some(parsed.pins);
                }
            }
            Err(diag) => failures.push(diag.with_context(json!({
                "has_source_index": candidate.has_source_index,
                "has_uobject_wrapper": candidate.has_uobject_wrapper,
                "has_single_precision_float": candidate.has_single_precision_float,
            }))),
        }
    }
    if let Some(pins) = best {
        let _ = reader.seek(best_pos);
        export.pins = Some(pins);
        return;
    }

    diagnostics.extend(failures);
    let pin_bytes = pin_end.saturating_sub(pin_start);
    if pin_bytes > 0 {
        diagnostics.push(
            Diagnostic::warning(
                "pins_unparsed_bytes",
                format!("/exports/{export_i}/pins"),
                format!("pin parser could not decode {pin_bytes} byte(s)"),
            )
            .with_context(json!({
                "unparsed_bytes": pin_bytes,
                "property_end": pin_start,
                "serial_end": pin_end,
            })),
        );
    }
}

fn pin_parse_contexts(package: &Package, primary: PinSerCtx) -> Vec<PinSerCtx> {
    let source_known = package
        .summary
        .custom_version(custom::UE5_MAIN_STREAM_OBJECT_VERSION)
        .is_some();
    let wrapper_known = package
        .summary
        .custom_version(custom::RELEASE_OBJECT_VERSION)
        .is_some();
    let single_precision_known = package
        .summary
        .custom_version(custom::UE5_RELEASE_STREAM_OBJECT_VERSION)
        .is_some();

    let source_options = if source_known {
        vec![primary.has_source_index]
    } else {
        vec![primary.has_source_index, !primary.has_source_index]
    };
    let wrapper_options = if wrapper_known {
        vec![primary.has_uobject_wrapper]
    } else {
        vec![primary.has_uobject_wrapper, !primary.has_uobject_wrapper]
    };
    let single_precision_options = if single_precision_known {
        vec![primary.has_single_precision_float]
    } else {
        vec![
            primary.has_single_precision_float,
            !primary.has_single_precision_float,
        ]
    };

    let mut out: Vec<PinSerCtx> = Vec::new();
    for has_source_index in source_options {
        for &has_uobject_wrapper in &wrapper_options {
            for &has_single_precision_float in &single_precision_options {
                let ctx = PinSerCtx {
                    filter_editor_only: primary.filter_editor_only,
                    has_source_index,
                    has_uobject_wrapper,
                    has_single_precision_float,
                };
                if !out.iter().any(|existing| {
                    existing.filter_editor_only == ctx.filter_editor_only
                        && existing.has_source_index == ctx.has_source_index
                        && existing.has_uobject_wrapper == ctx.has_uobject_wrapper
                        && existing.has_single_precision_float == ctx.has_single_precision_float
                }) {
                    out.push(ctx);
                }
            }
        }
    }
    out
}

fn preview_range(reader: &mut Reader, start: u64, end: u64) -> ByteRangePreview {
    let size = end.saturating_sub(start);
    let preview_len = size.min(64) as usize;
    let _ = reader.seek(start);
    let preview = reader.read_bytes(preview_len).unwrap_or_default();
    let _ = reader.seek(end);
    ByteRangePreview {
        start,
        end,
        size,
        preview: to_hex(&preview),
    }
}

fn export_serial_window(
    exp: &ObjectExport,
    has_script: bool,
    file_len: u64,
) -> std::result::Result<Option<ExportSerialWindow>, String> {
    if exp.serial_size <= 0 {
        return Ok(None);
    }
    if exp.serial_offset < 0 {
        return Err(format!(
            "negative serial offset {} for non-empty export",
            exp.serial_offset
        ));
    }

    let serial_start = exp.serial_offset as u64;
    let serial_size = exp.serial_size as u64;
    let serial_end = serial_start
        .checked_add(serial_size)
        .ok_or_else(|| "serial range overflows u64".to_string())?;
    if serial_end > file_len {
        return Err(format!(
            "serial range [{serial_start}, {serial_end}) exceeds file length {file_len}"
        ));
    }

    if !has_script {
        return Ok(Some(ExportSerialWindow {
            property_start: serial_start,
            property_end: serial_end,
            serial_end,
        }));
    }

    let script_start = exp.script_serialization_start_offset;
    let script_end = exp.script_serialization_end_offset;
    if script_start == 0 && script_end == 0 {
        return Ok(Some(ExportSerialWindow {
            property_start: serial_start,
            property_end: serial_end,
            serial_end,
        }));
    }
    if script_start < 0 || script_end < script_start || script_end > exp.serial_size {
        return Err(format!(
            "script serialization range [{script_start}, {script_end}) is outside serial size {}",
            exp.serial_size
        ));
    }

    Ok(Some(ExportSerialWindow {
        property_start: serial_start
            .checked_add(script_start as u64)
            .ok_or_else(|| "script serialization start overflows u64".to_string())?,
        property_end: serial_start
            .checked_add(script_end as u64)
            .ok_or_else(|| "script serialization end overflows u64".to_string())?,
        serial_end,
    }))
}

fn is_graph_node_class(class_full: &str) -> bool {
    let simple = class_full.rsplit(['.', '/']).next().unwrap_or(class_full);
    if simple.starts_with("K2Node") || simple.starts_with("EdGraphNode") {
        return true;
    }
    if simple.starts_with("NiagaraNode") || simple == "NiagaraOverviewNode" {
        return true;
    }
    if simple.contains("Binding") {
        return false;
    }
    simple.contains("GraphNode")
}

fn distill_member(props: &[PropertyEntry]) -> Option<MemberRef> {
    const REF_PROPS: [&str; 4] = [
        "FunctionReference",
        "EventReference",
        "VariableReference",
        "DelegateReference",
    ];
    for e in props {
        if !REF_PROPS.contains(&e.name.as_str()) {
            continue;
        }
        let inner = match e.value.get("properties").and_then(Value::as_array) {
            Some(a) => a,
            None => continue,
        };
        let mut name = None;
        let mut parent = None;
        for p in inner {
            match p.get("name").and_then(Value::as_str) {
                Some("MemberName") => {
                    name = p.get("value").and_then(Value::as_str).map(str::to_owned);
                }
                Some("MemberParent") => {
                    parent = p.get("value").cloned();
                }
                _ => {}
            }
        }
        if let Some(name) = name {
            return Some(MemberRef { name, parent });
        }
    }
    None
}
