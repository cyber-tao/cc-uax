//! JSON serialization for a parsed `Package`: the section dispatch in `to_json`
//! and the per-section builders (summary/imports/references/exports/pins), plus
//! the export serial-window computation and pin-graph rendering helpers.

use crate::object::ObjectExport;
use crate::output::sections::OutputSections;
use crate::package::Package;
use crate::pin::{
    Pin, PinRef, PinSerCtx, PinTerminalType, container_type_label, direction_label, parse_node_pins,
};
use crate::property::{ParseCtx, PropertyEntry, entries_to_json, parse_object_properties};
use crate::reader::{Guid, Reader};
use crate::references::{collect_package_references, is_valid_package_name};
use crate::version::{custom, ue5};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};

#[derive(Debug, Clone, Copy)]
struct ExportSerialWindow {
    property_start: u64,
    property_end: u64,
    serial_end: u64,
}

impl Package {
    pub fn to_json(&self, data: &[u8], opts: &OutputSections) -> Value {
        let mut root = serde_json::Map::new();
        let mut diagnostics = Vec::new();
        self.collect_table_diagnostics(&mut diagnostics);
        if opts.summary {
            root.insert("summary".into(), self.summary_json());
        }
        if opts.names {
            root.insert("names".into(), json!(self.names.names));
        }
        if opts.references {
            root.insert("references".into(), self.references_json());
        }
        if opts.imports {
            root.insert("imports".into(), self.imports_json());
        }
        if opts.exports {
            root.insert(
                "exports".into(),
                self.exports_json(data, opts, &mut diagnostics),
            );
        }
        root.insert("diagnostics".into(), Value::Array(diagnostics));
        Value::Object(root)
    }

    fn summary_json(&self) -> Value {
        let s = &self.summary;
        let custom: Vec<Value> = s
            .custom_versions
            .iter()
            .map(|c| json!({ "key": c.key.to_hex(), "version": c.version }))
            .collect();
        let summary = json!({
            "package_name": s.package_name,
            "tag": format!("0x{:08X}", s.tag),
            "legacy_file_version": s.legacy_file_version,
            "file_version_ue4": s.file_version_ue4,
            "file_version_ue5": s.file_version_ue5,
            "file_version_licensee": s.file_version_licensee_ue,
            "saved_by_engine_version": s.engine_version.display(),
            "compatible_engine_version": s.compatible_engine_version.display(),
            "package_flags": format!("0x{:08X}", s.package_flags),
            "total_header_size": s.total_header_size,
            "name_count": s.name_count,
            "import_count": s.import_count,
            "export_count": s.export_count,
            "bulk_data_start_offset": s.bulk_data_start_offset,
            "custom_versions": custom,
        });
        summary
    }

    fn collect_table_diagnostics(&self, diagnostics: &mut Vec<Value>) {
        if let Some(err) = &self.soft_object_path_error {
            diagnostics.push(diagnostic(
                "warning",
                "soft_object_path_table_error",
                "/summary/soft_object_paths".to_string(),
                err.clone(),
                None,
            ));
        }
        if let Some(err) = &self.soft_package_reference_error {
            diagnostics.push(diagnostic(
                "warning",
                "soft_package_reference_table_error",
                "/summary/soft_package_references".to_string(),
                err.clone(),
                None,
            ));
        }
    }

    fn imports_json(&self) -> Value {
        let arr: Vec<Value> = self
            .imports
            .iter()
            .enumerate()
            .map(|(i, imp)| {
                let pkg_index = -((i as i32) + 1);
                json!({
                    "index": pkg_index,
                    "class_package": self.names.resolve_raw(imp.class_package),
                    "class": self.names.resolve_raw(imp.class_name),
                    "name": self.names.resolve_raw(imp.object_name),
                    "outer": name_or_null(self.resolve_full_name(imp.outer_index.0, 0)),
                    "package_name": imp.package_name.map(|p| self.names.resolve_raw(p)),
                    "full_name": self.resolve_full_name(pkg_index, 0),
                })
            })
            .collect();
        Value::Array(arr)
    }

    fn references_json(&self) -> Value {
        let (assets, scripts) = collect_package_references(self.import_class_object_names());
        let soft: Vec<String> = self
            .soft_package_references
            .iter()
            .filter(|s| is_valid_package_name(s))
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        json!({ "assets": assets, "scripts": scripts, "soft": soft })
    }

    fn exports_json(
        &self,
        data: &[u8],
        opts: &OutputSections,
        diagnostics: &mut Vec<Value>,
    ) -> Value {
        // resolve_object_ref walks the outer chain and allocates; the same index recurs
        // often across a large property/pin tree, so memoize the resolved JSON by index.
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

        let mut objs: Vec<serde_json::Map<String, Value>> = Vec::with_capacity(self.exports.len());
        let mut export_pins: Vec<Option<Vec<Pin>>> = Vec::with_capacity(self.exports.len());

        for (i, exp) in self.exports.iter().enumerate() {
            let pkg_index = (i as i32) + 1;
            let class_full = self.resolve_full_name(exp.class_index.0, 0);
            let is_node = is_graph_node_class(&class_full);
            let mut obj = serde_json::Map::new();
            obj.insert("index".into(), json!(pkg_index));
            obj.insert(
                "name".into(),
                json!(self.names.resolve_raw(exp.object_name)),
            );
            obj.insert("class".into(), name_or_null(class_full.clone()));
            if exp.is_asset {
                obj.insert("is_asset".into(), json!(true));
            }
            if opts.layout {
                obj.insert(
                    "super".into(),
                    name_or_null(self.resolve_full_name(exp.super_index.0, 0)),
                );
                obj.insert(
                    "template".into(),
                    name_or_null(self.resolve_full_name(exp.template_index.0, 0)),
                );
                obj.insert(
                    "outer".into(),
                    name_or_null(self.resolve_full_name(exp.outer_index.0, 0)),
                );
                obj.insert(
                    "full_name".into(),
                    json!(self.resolve_full_name(pkg_index, 0)),
                );
                obj.insert(
                    "object_flags".into(),
                    json!(format!("0x{:08X}", exp.object_flags)),
                );
                obj.insert("serial_offset".into(), json!(exp.serial_offset));
                obj.insert("serial_size".into(), json!(exp.serial_size));
                if has_script {
                    obj.insert(
                        "script_serialization_start".into(),
                        json!(exp.script_serialization_start_offset),
                    );
                    obj.insert(
                        "script_serialization_end".into(),
                        json!(exp.script_serialization_end_offset),
                    );
                }
            }

            let serial_window = match export_serial_window(exp, has_script, file_len) {
                Ok(w) => w,
                Err(err) => {
                    diagnostics.push(diagnostic(
                        "error",
                        "serial_window_invalid",
                        format!("/exports/{i}"),
                        err,
                        Some(json!({
                            "export_index": pkg_index,
                            "serial_offset": exp.serial_offset,
                            "serial_size": exp.serial_size,
                        })),
                    ));
                    None
                }
            };
            if (opts.properties || is_node)
                && let Some(window) = serial_window
            {
                let start = window.property_start;
                let end = window.property_end;

                if end > start && reader.seek(start).is_ok() {
                    let props = parse_object_properties(&mut reader, &ctx, end);
                    if let Some((member, from)) = distill_member(&props) {
                        obj.insert("member".into(), json!(member));
                        if let Some(from) = from {
                            obj.insert("member_from".into(), from);
                        }
                    }
                    if opts.properties {
                        obj.insert("properties".into(), entries_to_json(&props));
                        let consumed = reader.pos().saturating_sub(start);
                        let range = end - start;
                        if consumed < range {
                            let tail_start = reader.pos();
                            let tail_size = range - consumed;
                            let preview_len = tail_size.min(64) as usize;
                            let preview = reader.read_bytes(preview_len).unwrap_or_default();
                            obj.insert(
                                "post_property_tail".into(),
                                json!({
                                    "size": tail_size,
                                    "start": tail_start,
                                    "end": end,
                                    "preview": hex_preview(&preview),
                                }),
                            );
                        }
                    }
                } else if opts.properties && end == start {
                    obj.insert("properties".into(), Value::Array(Vec::new()));
                }
            }

            let mut pins = None;
            if opts.pins
                && let Some(window) = serial_window
            {
                pins = self.try_parse_pins(
                    &mut reader,
                    &class_full,
                    has_script,
                    &ctx,
                    &pin_ctx,
                    window,
                );
                if pins.is_none() {
                    let pin_bytes = window.serial_end.saturating_sub(window.property_end);
                    if has_script && is_node && pin_bytes > 0 {
                        diagnostics.push(diagnostic(
                            "warning",
                            "pins_unparsed_bytes",
                            format!("/exports/{i}/pins"),
                            format!("pin parser could not decode {pin_bytes} byte(s)"),
                            Some(json!({
                                "unparsed_bytes": pin_bytes,
                                "property_end": window.property_end,
                                "serial_end": window.serial_end,
                            })),
                        ));
                    }
                }
            }

            objs.push(obj);
            export_pins.push(pins);
        }

        if !opts.pins {
            return Value::Array(objs.into_iter().map(Value::Object).collect());
        }

        let export_full_names: Vec<String> = (0..self.exports.len())
            .map(|i| self.resolve_full_name((i as i32) + 1, 0))
            .collect();

        let mut pin_name_by_id: HashMap<(i32, Guid), String> = HashMap::new();
        for (i, pins) in export_pins.iter().enumerate() {
            if let Some(pins) = pins {
                let node_index = (i as i32) + 1;
                for p in pins {
                    pin_name_by_id.insert((node_index, p.pin_id), p.name.clone());
                }
            }
        }

        let arr: Vec<Value> = objs
            .into_iter()
            .enumerate()
            .map(|(i, mut obj)| {
                if let Some(pins) = &export_pins[i] {
                    obj.insert(
                        "pins".into(),
                        self.pins_to_json(pins, &pin_name_by_id, &export_full_names),
                    );
                }
                Value::Object(obj)
            })
            .collect();
        Value::Array(arr)
    }

    #[allow(clippy::too_many_arguments)]
    fn try_parse_pins(
        &self,
        reader: &mut Reader,
        class_full: &str,
        has_script: bool,
        ctx: &ParseCtx,
        pin_ctx: &PinSerCtx,
        window: ExportSerialWindow,
    ) -> Option<Vec<Pin>> {
        if !has_script || !is_graph_node_class(class_full) {
            return None;
        }
        let pin_start = window.property_end;
        let pin_end = window.serial_end;
        if pin_end <= pin_start {
            return None;
        }
        reader.seek(pin_start).ok()?;
        let candidates = self.pin_parse_contexts(*pin_ctx);
        let mut best = None;
        let mut best_pos = pin_start;
        for candidate in candidates {
            reader.seek(pin_start).ok()?;
            if let Some(parsed) = parse_node_pins(reader, pin_end, ctx, &candidate) {
                let consumed_pos = reader.pos();
                if consumed_pos >= best_pos {
                    best_pos = consumed_pos;
                    best = Some(parsed);
                }
            }
        }
        if best.is_some() {
            let _ = reader.seek(best_pos);
        }
        best
    }

    fn pin_parse_contexts(&self, primary: PinSerCtx) -> Vec<PinSerCtx> {
        let source_known = self
            .summary
            .custom_version(custom::UE5_MAIN_STREAM_OBJECT_VERSION)
            .is_some();
        let wrapper_known = self
            .summary
            .custom_version(custom::RELEASE_OBJECT_VERSION)
            .is_some();
        let single_precision_known = self
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

    fn pins_to_json(
        &self,
        pins: &[Pin],
        names: &HashMap<(i32, Guid), String>,
        export_full_names: &[String],
    ) -> Value {
        let arr: Vec<Value> = pins
            .iter()
            .map(|p| {
                let mut o = serde_json::Map::new();
                o.insert("name".into(), json!(p.name));
                o.insert("direction".into(), json!(direction_label(p.direction)));
                if !p.category.is_empty() {
                    o.insert("category".into(), json!(p.category));
                }
                if !p.sub_category.is_empty() {
                    o.insert("sub_category".into(), json!(p.sub_category));
                }
                if p.sub_category_object != 0 {
                    o.insert(
                        "sub_category_object".into(),
                        self.resolve_object_ref(p.sub_category_object),
                    );
                }
                o.insert(
                    "container_type".into(),
                    json!(container_type_label(p.container_type)),
                );
                if let Some(value_type) = &p.value_type {
                    o.insert(
                        "value_type".into(),
                        terminal_type_to_json(value_type, |idx| self.resolve_object_ref(idx)),
                    );
                }
                o.insert("is_reference".into(), json!(p.is_reference));
                o.insert("is_weak_pointer".into(), json!(p.is_weak_pointer));
                o.insert("is_const".into(), json!(p.is_const));
                o.insert("is_uobject_wrapper".into(), json!(p.is_uobject_wrapper));
                o.insert(
                    "serialize_as_single_precision_float".into(),
                    json!(p.serialize_as_single_precision_float),
                );
                if p.member_parent != 0 || !p.member_name.is_empty() || !p.member_guid.is_zero() {
                    let mut member = serde_json::Map::new();
                    if p.member_parent != 0 {
                        member.insert("parent".into(), self.resolve_object_ref(p.member_parent));
                    }
                    if !p.member_name.is_empty() {
                        member.insert("name".into(), json!(p.member_name));
                    }
                    if !p.member_guid.is_zero() {
                        member.insert("guid".into(), json!(p.member_guid.to_hex()));
                    }
                    o.insert("member_reference".into(), Value::Object(member));
                }
                if !p.default_value.is_empty() {
                    o.insert("default_value".into(), json!(p.default_value));
                }
                if p.default_object != 0 {
                    o.insert(
                        "default_object".into(),
                        self.resolve_object_ref(p.default_object),
                    );
                }
                o.insert("pin_id".into(), json!(p.pin_id.to_hex()));
                if !p.linked_to.is_empty() {
                    let links: Vec<Value> = p
                        .linked_to
                        .iter()
                        .map(|r| self.link_to_json(r, names, export_full_names))
                        .collect();
                    o.insert("linked_to".into(), Value::Array(links));
                }
                if !p.sub_pins.is_empty() {
                    let links: Vec<Value> = p
                        .sub_pins
                        .iter()
                        .map(|r| self.link_to_json(r, names, export_full_names))
                        .collect();
                    o.insert("sub_pins".into(), Value::Array(links));
                }
                if let Some(parent) = &p.parent_pin {
                    o.insert(
                        "parent_pin".into(),
                        self.link_to_json(parent, names, export_full_names),
                    );
                }
                if let Some(pass_through) = &p.reference_pass_through {
                    o.insert(
                        "reference_pass_through".into(),
                        self.link_to_json(pass_through, names, export_full_names),
                    );
                }
                if let Some(guid) = p.persistent_guid
                    && !guid.is_zero()
                {
                    o.insert("persistent_guid".into(), json!(guid.to_hex()));
                }
                if let Some(flags) = &p.editor_flags {
                    o.insert(
                        "editor_flags".into(),
                        json!({
                            "hidden": flags.hidden,
                            "not_connectable": flags.not_connectable,
                            "default_value_read_only": flags.default_value_read_only,
                            "default_value_ignored": flags.default_value_ignored,
                            "advanced_view": flags.advanced_view,
                            "orphaned_pin": flags.orphaned_pin,
                        }),
                    );
                }
                Value::Object(o)
            })
            .collect();
        Value::Array(arr)
    }

    fn link_to_json(
        &self,
        r: &PinRef,
        names: &HashMap<(i32, Guid), String>,
        export_full_names: &[String],
    ) -> Value {
        let mut o = serde_json::Map::new();
        let node = if r.node_index > 0 {
            export_full_names
                .get((r.node_index - 1) as usize)
                .cloned()
                .unwrap_or_else(|| self.resolve_full_name(r.node_index, 0))
        } else {
            self.resolve_full_name(r.node_index, 0)
        };
        o.insert("node".into(), name_or_null(node));
        o.insert("node_index".into(), json!(r.node_index));
        match names.get(&(r.node_index, r.pin_id)) {
            Some(name) => {
                o.insert("pin".into(), json!(name));
            }
            None => {
                o.insert("pin_id".into(), json!(r.pin_id.to_hex()));
            }
        }
        Value::Object(o)
    }
}

fn name_or_null(s: String) -> Value {
    if s.is_empty() { Value::Null } else { json!(s) }
}

fn terminal_type_to_json<F>(ty: &PinTerminalType, resolve: F) -> Value
where
    F: Fn(i32) -> Value,
{
    let mut o = serde_json::Map::new();
    o.insert("category".into(), json!(ty.category));
    if !ty.sub_category.is_empty() {
        o.insert("sub_category".into(), json!(ty.sub_category));
    }
    if ty.sub_category_object != 0 {
        o.insert(
            "sub_category_object".into(),
            resolve(ty.sub_category_object),
        );
    }
    o.insert("is_const".into(), json!(ty.is_const));
    o.insert("is_weak_pointer".into(), json!(ty.is_weak_pointer));
    o.insert("is_uobject_wrapper".into(), json!(ty.is_uobject_wrapper));
    Value::Object(o)
}

fn diagnostic(
    severity: &str,
    code: &str,
    path: String,
    message: String,
    details: Option<Value>,
) -> Value {
    let mut o = serde_json::Map::new();
    o.insert("severity".into(), json!(severity));
    o.insert("code".into(), json!(code));
    o.insert("path".into(), json!(path));
    o.insert("message".into(), json!(message));
    if let Some(details) = details {
        o.insert("details".into(), details);
    }
    Value::Object(o)
}

fn hex_preview(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0f) as usize] as char);
    }
    s
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
    // UNiagaraNode* and UNiagaraOverviewNode are UEdGraphNode subclasses whose
    // class names do not contain "GraphNode".
    if simple.starts_with("NiagaraNode") || simple == "NiagaraOverviewNode" {
        return true;
    }
    // Binding helper objects (e.g. AnimGraphNodeBinding_Base) contain "GraphNode"
    // but are not UEdGraphNode pin-bearing nodes; the exclusion only applies to the
    // fuzzy contains() match, not the exact prefixes above.
    if simple.contains("Binding") {
        return false;
    }
    simple.contains("GraphNode")
}

fn distill_member(props: &[PropertyEntry]) -> Option<(String, Option<Value>)> {
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
        let mut member = None;
        let mut from = None;
        for p in inner {
            match p.get("name").and_then(Value::as_str) {
                Some("MemberName") => {
                    member = p.get("value").and_then(Value::as_str).map(str::to_owned);
                }
                Some("MemberParent") => {
                    from = p.get("value").cloned();
                }
                _ => {}
            }
        }
        if let Some(m) = member {
            return Some((m, from));
        }
    }
    None
}
