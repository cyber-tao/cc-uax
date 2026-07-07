use crate::name::NameMap;
use crate::object::{ObjectExport, ObjectImport};
use crate::pin::{
    Pin, PinRef, PinSerCtx, PinTerminalType, container_type_label, direction_label, parse_node_pins,
};
use crate::property::{
    ParseCtx, PropertyEntry, entries_to_json, parse_object_properties, read_soft_object_path,
};
use crate::reader::{Guid, Reader};
use crate::summary::PackageFileSummary;
use crate::version::{custom, ue5};
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};

const PACKAGE_CLASS_NAME: &str = "Package";
const SCRIPT_PATH_PREFIX: &str = "/Script/";

#[derive(Debug, Clone, Copy)]
struct ExportSerialWindow {
    property_start: u64,
    property_end: u64,
    serial_end: u64,
}

#[derive(Debug, Clone)]
pub struct OutputSections {
    pub summary: bool,
    pub imports: bool,
    pub names: bool,
    pub references: bool,
    pub exports: bool,
    pub pins: bool,
    pub properties: bool,
    pub layout: bool,
}

impl Default for OutputSections {
    fn default() -> Self {
        Self::full()
    }
}

impl OutputSections {
    pub fn none() -> Self {
        Self {
            summary: false,
            imports: false,
            names: false,
            references: false,
            exports: false,
            pins: false,
            properties: false,
            layout: false,
        }
    }

    pub fn full() -> Self {
        Self {
            summary: true,
            imports: true,
            names: false,
            references: false,
            exports: true,
            pins: true,
            properties: true,
            layout: true,
        }
    }

    pub fn parse(spec: &str) -> Result<Self> {
        let mut s = Self::none();
        let mut seen = false;
        for raw in spec.split(',') {
            let tok = raw.trim();
            if tok.is_empty() {
                continue;
            }
            seen = true;
            match tok.to_ascii_lowercase().as_str() {
                "full" | "all" => s.merge(&Self::full()),
                "logic" | "graph" => {
                    s.summary = true;
                    s.exports = true;
                    s.pins = true;
                }
                "debug" => {
                    s.summary = true;
                    s.imports = true;
                    s.exports = true;
                    s.properties = true;
                    s.layout = true;
                }
                "summary" => s.summary = true,
                "imports" => s.imports = true,
                "names" => s.names = true,
                "references" | "refs" => s.references = true,
                "exports" | "identity" => s.exports = true,
                "pins" => {
                    s.exports = true;
                    s.pins = true;
                }
                "properties" | "props" => {
                    s.exports = true;
                    s.properties = true;
                }
                "layout" => {
                    s.exports = true;
                    s.layout = true;
                }
                other => bail!(
                    "unknown section '{other}'; valid: summary, imports, exports, pins, properties, layout, names, references; presets: logic, debug, full"
                ),
            }
        }
        if !seen {
            bail!("no sections specified");
        }
        Ok(s)
    }

    fn merge(&mut self, other: &Self) {
        self.summary |= other.summary;
        self.imports |= other.imports;
        self.names |= other.names;
        self.references |= other.references;
        self.exports |= other.exports;
        self.pins |= other.pins;
        self.properties |= other.properties;
        self.layout |= other.layout;
    }
}

pub struct Package {
    pub summary: PackageFileSummary,
    pub names: NameMap,
    pub imports: Vec<ObjectImport>,
    pub exports: Vec<ObjectExport>,
    pub soft_object_paths: Vec<Value>,
    pub soft_object_path_error: Option<String>,
    pub soft_package_references: Vec<String>,
    pub soft_package_reference_error: Option<String>,
}

impl Package {
    pub fn parse(data: &[u8]) -> Result<Package> {
        let mut r = Reader::new(data);
        let summary = PackageFileSummary::parse(&mut r)?;

        let ue4 = summary.file_version_ue4;
        let ue5 = summary.file_version_ue5;
        let filter_editor = summary.filter_editor_only();

        let names = NameMap::parse(&mut r, summary.name_offset, summary.name_count, ue4)?;
        let imports = ObjectImport::parse_table(
            &mut r,
            summary.import_offset,
            summary.import_count,
            ue4,
            ue5,
            filter_editor,
        )?;
        let exports = ObjectExport::parse_table(
            &mut r,
            summary.export_offset,
            summary.export_count,
            ue4,
            ue5,
        )?;

        let (soft_object_paths, soft_object_path_error) = parse_soft_object_path_table(
            &mut r,
            &names,
            summary.soft_object_paths_offset,
            summary.soft_object_paths_count,
        );

        let (soft_package_references, soft_package_reference_error) = parse_soft_package_references(
            &mut r,
            &names,
            summary.soft_package_references_offset,
            summary.soft_package_references_count,
        );

        Ok(Package {
            summary,
            names,
            imports,
            exports,
            soft_object_paths,
            soft_object_path_error,
            soft_package_references,
            soft_package_reference_error,
        })
    }

    pub fn resolve_full_name(&self, index: i32, depth: u32) -> String {
        if index == 0 || depth > 64 {
            return String::new();
        }
        if index < 0 {
            let i = (-index - 1) as usize;
            match self.imports.get(i) {
                Some(imp) => {
                    let name = self.names.resolve_raw(imp.object_name);
                    let outer = self.resolve_full_name(imp.outer_index.0, depth + 1);
                    if outer.is_empty() {
                        name
                    } else {
                        format!("{outer}.{name}")
                    }
                }
                None => format!("<invalid_import#{i}>"),
            }
        } else {
            let i = (index - 1) as usize;
            match self.exports.get(i) {
                Some(exp) => {
                    let name = self.names.resolve_raw(exp.object_name);
                    let outer = self.resolve_full_name(exp.outer_index.0, depth + 1);
                    if outer.is_empty() {
                        name
                    } else {
                        format!("{outer}.{name}")
                    }
                }
                None => format!("<invalid_export#{i}>"),
            }
        }
    }

    pub fn resolve_object_ref(&self, index: i32) -> Value {
        if index == 0 {
            return Value::Null;
        }
        let full = self.resolve_full_name(index, 0);
        json!({ "ref": full, "index": index })
    }

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

    fn import_class_object_names(&self) -> impl Iterator<Item = (String, String)> + '_ {
        self.imports.iter().map(|imp| {
            (
                self.names.resolve_raw(imp.class_name),
                self.names.resolve_raw(imp.object_name),
            )
        })
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

    pub fn referenced_packages(&self) -> Vec<String> {
        sorted_referenced_packages(
            self.import_class_object_names(),
            &self.soft_package_references,
        )
    }

    pub fn references_package(&self, package_path: &str) -> bool {
        self.referenced_packages()
            .iter()
            .any(|p| p.eq_ignore_ascii_case(package_path))
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
                            diagnostics.push(diagnostic(
                                "warning",
                                "properties_unconsumed_bytes",
                                format!("/exports/{i}/properties"),
                                format!(
                                    "property parser left {} byte(s) unconsumed",
                                    range - consumed
                                ),
                                Some(json!({
                                    "unconsumed_bytes": range - consumed,
                                    "property_start": start,
                                    "property_end": end,
                                    "parser_position": reader.pos(),
                                })),
                            ));
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

fn parse_soft_object_path_table(
    r: &mut Reader,
    names: &NameMap,
    offset: i32,
    count: i32,
) -> (Vec<Value>, Option<String>) {
    let mut out = Vec::new();
    if count < 0 {
        return (
            out,
            Some(format!("soft object path count out of range: {count}")),
        );
    }
    if count == 0 {
        return (out, None);
    }
    if offset <= 0 {
        return (
            out,
            Some(format!(
                "soft object path table offset must be positive when count is {count}"
            )),
        );
    }
    if let Err(err) = r.seek(offset as u64) {
        return (
            out,
            Some(format!("soft object path table seek failed: {err:#}")),
        );
    }
    for i in 0..count {
        match read_soft_object_path(r, names) {
            Ok(v) => out.push(v),
            Err(err) => {
                return (
                    out,
                    Some(format!(
                        "soft object path table entry {}/{} failed at offset {}: {err:#}",
                        i + 1,
                        count,
                        r.pos()
                    )),
                );
            }
        }
    }
    (out, None)
}

/// The SoftPackageReferences header table: one FName package name per entry
/// (written by SavePackage from FLinkerSave::SoftPackageReferenceList).
fn parse_soft_package_references(
    r: &mut Reader,
    names: &NameMap,
    offset: i32,
    count: i32,
) -> (Vec<String>, Option<String>) {
    let mut out = Vec::new();
    if count < 0 {
        return (
            out,
            Some(format!(
                "soft package reference count out of range: {count}"
            )),
        );
    }
    if count == 0 {
        return (out, None);
    }
    if offset <= 0 {
        return (
            out,
            Some(format!(
                "soft package reference table offset must be positive when count is {count}"
            )),
        );
    }
    if let Err(err) = r.seek(offset as u64) {
        return (
            out,
            Some(format!("soft package reference table seek failed: {err:#}")),
        );
    }
    if (count as u64).saturating_mul(8) > r.remaining() {
        return (
            out,
            Some(format!(
                "soft package reference count out of range: {count}"
            )),
        );
    }
    for i in 0..count {
        match r.read_raw_name() {
            Ok(raw) => out.push(names.resolve_raw(raw)),
            Err(err) => {
                return (
                    out,
                    Some(format!(
                        "soft package reference entry {}/{} failed at offset {}: {err:#}",
                        i + 1,
                        count,
                        r.pos()
                    )),
                );
            }
        }
    }
    (out, None)
}

fn is_valid_package_name(name: &str) -> bool {
    !name.is_empty() && name != "None"
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

pub fn collect_package_references<I, S>(imports: I) -> (Vec<String>, Vec<String>)
where
    I: IntoIterator<Item = (S, S)>,
    S: AsRef<str>,
{
    let mut assets = BTreeSet::new();
    let mut scripts = BTreeSet::new();
    for (class, name) in imports {
        if class.as_ref() != PACKAGE_CLASS_NAME {
            continue;
        }
        let name = name.as_ref();
        if name.is_empty() {
            continue;
        }
        if name.starts_with(SCRIPT_PATH_PREFIX) {
            scripts.insert(name.to_owned());
        } else {
            assets.insert(name.to_owned());
        }
    }
    (assets.into_iter().collect(), scripts.into_iter().collect())
}

pub fn package_path_from_relative(rel: &str, mount: &str) -> String {
    let mount = format!("/{}", mount.trim_matches('/'));
    let normalized = rel.replace('\\', "/");
    let trimmed = normalized.trim_start_matches('/');
    let lower = trimmed.to_ascii_lowercase();
    let without_ext = if lower.ends_with(".uasset") {
        &trimmed[..trimmed.len() - 7]
    } else if lower.ends_with(".umap") {
        &trimmed[..trimmed.len() - 5]
    } else {
        trimmed
    };
    format!("{mount}/{without_ext}")
}

fn sorted_referenced_packages<I, S>(imports: I, soft: &[String]) -> Vec<String>
where
    I: IntoIterator<Item = (S, S)>,
    S: AsRef<str>,
{
    let (assets, scripts) = collect_package_references(imports);
    let mut refs: BTreeSet<String> = assets.into_iter().collect();
    refs.extend(scripts);
    refs.extend(soft.iter().filter(|s| is_valid_package_name(s)).cloned());
    refs.into_iter().collect()
}

/// Extract a package's forward references by parsing only the header, name table,
/// import table and soft-package-reference table — skipping the export and
/// soft-object-path tables. This is the hot path for `--scan-dir` reverse scans.
pub fn referenced_packages_from_bytes(data: &[u8]) -> Result<Vec<String>> {
    let mut r = Reader::new(data);
    let summary = PackageFileSummary::parse(&mut r)?;
    let ue4 = summary.file_version_ue4;
    let ue5 = summary.file_version_ue5;
    let filter_editor = summary.filter_editor_only();
    let names = NameMap::parse(&mut r, summary.name_offset, summary.name_count, ue4)?;
    let imports = ObjectImport::parse_table(
        &mut r,
        summary.import_offset,
        summary.import_count,
        ue4,
        ue5,
        filter_editor,
    )?;
    let (soft, _soft_err) = parse_soft_package_references(
        &mut r,
        &names,
        summary.soft_package_references_offset,
        summary.soft_package_references_count,
    );
    Ok(sorted_referenced_packages(
        imports.iter().map(|imp| {
            (
                names.resolve_raw(imp.class_name),
                names.resolve_raw(imp.object_name),
            )
        }),
        &soft,
    ))
}
