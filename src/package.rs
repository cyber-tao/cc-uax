use crate::name::NameMap;
use crate::object::{ObjectExport, ObjectImport};
use crate::pin::{Pin, PinRef, PinSerCtx, direction_label, parse_node_pins};
use crate::property::{ParseCtx, PropertyEntry, entries_to_json, parse_object_properties};
use crate::reader::{Guid, Reader};
use crate::summary::PackageFileSummary;
use crate::version::ue5;
use anyhow::{Result, bail};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};

const PACKAGE_CLASS_NAME: &str = "Package";
const SCRIPT_PATH_PREFIX: &str = "/Script/";

#[derive(Debug, Clone)]
pub struct JsonOptions {
    pub summary: bool,
    pub imports: bool,
    pub names: bool,
    pub references: bool,
    pub exports: bool,
    pub pins: bool,
    pub properties: bool,
    pub layout: bool,
}

impl Default for JsonOptions {
    fn default() -> Self {
        Self::full()
    }
}

impl JsonOptions {
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
                "refs" => s.references = true,
                "min" => s.summary = true,
                "summary" => s.summary = true,
                "imports" => s.imports = true,
                "names" => s.names = true,
                "references" => s.references = true,
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
                    "unknown section '{other}'; valid sections: summary, imports, exports, identity, pins, properties, layout, names, references; presets: logic, debug, full, refs, min"
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
}

impl Package {
    pub fn parse(data: &[u8]) -> Result<Package> {
        let mut r = Reader::new(data);
        let summary = PackageFileSummary::parse(&mut r)?;

        let ue4 = summary.file_version_ue4;
        let ue5 = summary.file_version_ue5;
        let unversioned = summary.is_unversioned();
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
            unversioned,
        )?;

        Ok(Package {
            summary,
            names,
            imports,
            exports,
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

    pub fn to_json(&self, data: &[u8], opts: &JsonOptions) -> Value {
        let mut root = serde_json::Map::new();
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
            root.insert("exports".into(), self.exports_json(data, opts));
        }
        Value::Object(root)
    }

    fn summary_json(&self) -> Value {
        let s = &self.summary;
        let custom: Vec<Value> = s
            .custom_versions
            .iter()
            .map(|c| json!({ "key": c.key.to_hex(), "version": c.version }))
            .collect();
        json!({
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
        })
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
        let (assets, scripts) = collect_package_references(self.imports.iter().map(|imp| {
            (
                self.names.resolve_raw(imp.class_name),
                self.names.resolve_raw(imp.object_name),
            )
        }));
        json!({ "assets": assets, "scripts": scripts })
    }

    pub fn referenced_packages(&self) -> Vec<String> {
        let (mut refs, scripts) = collect_package_references(self.imports.iter().map(|imp| {
            (
                self.names.resolve_raw(imp.class_name),
                self.names.resolve_raw(imp.object_name),
            )
        }));
        refs.extend(scripts);
        refs.sort();
        refs
    }

    pub fn references_package(&self, package_path: &str) -> bool {
        self.referenced_packages()
            .iter()
            .any(|p| p.eq_ignore_ascii_case(package_path))
    }

    fn exports_json(&self, data: &[u8], opts: &JsonOptions) -> Value {
        let resolve = |idx: i32| self.resolve_object_ref(idx);
        let ctx = ParseCtx {
            names: &self.names,
            resolve_object: &resolve,
        };
        let mut reader = Reader::new(data);
        let file_len = reader.len();
        let pin_ctx = PinSerCtx::from_summary(&self.summary);
        let has_script = !self.summary.is_unversioned()
            && self.summary.file_version_ue5 >= ue5::SCRIPT_SERIALIZATION_OFFSET;

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

            let in_bounds = exp.serial_size > 0 && exp.serial_offset >= 0;
            if (opts.properties || is_node) && in_bounds {
                let (start, end) = if has_script {
                    (
                        exp.serial_offset + exp.script_serialization_start_offset,
                        exp.serial_offset + exp.script_serialization_end_offset,
                    )
                } else {
                    (exp.serial_offset, exp.serial_offset + exp.serial_size)
                };

                if end > start
                    && start >= 0
                    && (end as u64) <= file_len
                    && reader.seek(start as u64).is_ok()
                {
                    let props = parse_object_properties(
                        &mut reader,
                        &ctx,
                        end as u64,
                        self.summary.file_version_ue5,
                    );
                    if let Some((member, from)) = distill_member(&props) {
                        obj.insert("member".into(), json!(member));
                        if let Some(from) = from {
                            obj.insert("member_from".into(), from);
                        }
                    }
                    if opts.properties {
                        obj.insert("properties".into(), entries_to_json(&props));
                        let consumed = reader.pos().saturating_sub(start as u64);
                        let range = (end - start) as u64;
                        if consumed < range {
                            obj.insert(
                                "properties_unconsumed_bytes".into(),
                                json!(range - consumed),
                            );
                        }
                    }
                } else if opts.properties && has_script && end == start {
                    obj.insert("properties".into(), Value::Array(Vec::new()));
                }
            }

            let mut pins = None;
            if opts.pins && in_bounds {
                pins = self.try_parse_pins(
                    &mut reader,
                    exp,
                    &class_full,
                    has_script,
                    file_len,
                    &ctx,
                    &pin_ctx,
                );
                if pins.is_none() {
                    let pin_start = exp.serial_offset + exp.script_serialization_end_offset;
                    let pin_end = exp.serial_offset + exp.serial_size;
                    if has_script && is_node && pin_end > pin_start {
                        obj.insert("pins_unparsed_bytes".into(), json!(pin_end - pin_start));
                    }
                }
            }

            objs.push(obj);
            export_pins.push(pins);
        }

        if !opts.pins {
            return Value::Array(objs.into_iter().map(Value::Object).collect());
        }

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
                    obj.insert("pins".into(), self.pins_to_json(pins, &pin_name_by_id));
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
        exp: &ObjectExport,
        class_full: &str,
        has_script: bool,
        file_len: u64,
        ctx: &ParseCtx,
        pin_ctx: &PinSerCtx,
    ) -> Option<Vec<Pin>> {
        if !has_script || !is_graph_node_class(class_full) {
            return None;
        }
        let pin_start = exp.serial_offset + exp.script_serialization_end_offset;
        let pin_end = exp.serial_offset + exp.serial_size;
        if pin_start < 0 || pin_end <= pin_start || (pin_end as u64) > file_len {
            return None;
        }
        reader.seek(pin_start as u64).ok()?;
        parse_node_pins(reader, pin_end as u64, ctx, pin_ctx)
    }

    fn pins_to_json(&self, pins: &[Pin], names: &HashMap<(i32, Guid), String>) -> Value {
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
                        .map(|r| self.link_to_json(r, names))
                        .collect();
                    o.insert("linked_to".into(), Value::Array(links));
                }
                if !p.sub_pins.is_empty() {
                    let links: Vec<Value> = p
                        .sub_pins
                        .iter()
                        .map(|r| self.link_to_json(r, names))
                        .collect();
                    o.insert("sub_pins".into(), Value::Array(links));
                }
                if let Some(parent) = &p.parent_pin {
                    o.insert("parent_pin".into(), self.link_to_json(parent, names));
                }
                Value::Object(o)
            })
            .collect();
        Value::Array(arr)
    }

    fn link_to_json(&self, r: &PinRef, names: &HashMap<(i32, Guid), String>) -> Value {
        let mut o = serde_json::Map::new();
        o.insert(
            "node".into(),
            name_or_null(self.resolve_full_name(r.node_index, 0)),
        );
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

fn is_graph_node_class(class_full: &str) -> bool {
    let simple = class_full.rsplit(['.', '/']).next().unwrap_or(class_full);
    simple.starts_with("K2Node")
        || simple.starts_with("EdGraphNode")
        || simple.contains("GraphNode")
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
    let without_ext = trimmed
        .strip_suffix(".uasset")
        .or_else(|| trimmed.strip_suffix(".umap"))
        .unwrap_or(trimmed);
    format!("{mount}/{without_ext}")
}
