use crate::name::NameMap;
use crate::object::{ObjectExport, ObjectImport};
use crate::property::{entries_to_json, parse_object_properties, ParseCtx};
use crate::reader::Reader;
use crate::summary::PackageFileSummary;
use crate::version::ue5;
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::BTreeSet;

const PACKAGE_CLASS_NAME: &str = "Package";
const SCRIPT_PATH_PREFIX: &str = "/Script/";

#[derive(Debug, Clone, Default)]
pub struct JsonOptions {
    pub include_names: bool,
    pub summary_only: bool,
    pub no_properties: bool,
    pub references_only: bool,
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
        root.insert("summary".into(), self.summary_json());
        if opts.include_names {
            root.insert("names".into(), json!(self.names.names));
        }
        if opts.references_only {
            root.insert("references".into(), self.references_json());
        } else if !opts.summary_only {
            root.insert("imports".into(), self.imports_json());
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

        let mut arr = Vec::with_capacity(self.exports.len());
        for (i, exp) in self.exports.iter().enumerate() {
            let pkg_index = (i as i32) + 1;
            let mut obj = serde_json::Map::new();
            obj.insert("index".into(), json!(pkg_index));
            obj.insert(
                "name".into(),
                json!(self.names.resolve_raw(exp.object_name)),
            );
            obj.insert(
                "class".into(),
                name_or_null(self.resolve_full_name(exp.class_index.0, 0)),
            );
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
            if exp.is_asset {
                obj.insert("is_asset".into(), json!(true));
            }

            let has_script = !self.summary.is_unversioned()
                && self.summary.file_version_ue5 >= ue5::SCRIPT_SERIALIZATION_OFFSET;
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

            if !opts.no_properties && exp.serial_size > 0 && exp.serial_offset >= 0 {
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
                    obj.insert("properties".into(), entries_to_json(&props));
                    let consumed = reader.pos().saturating_sub(start as u64);
                    let range = (end - start) as u64;
                    if consumed < range {
                        obj.insert(
                            "properties_unconsumed_bytes".into(),
                            json!(range - consumed),
                        );
                    }
                } else if has_script && end == start {
                    obj.insert("properties".into(), Value::Array(Vec::new()));
                }
            }

            arr.push(Value::Object(obj));
        }
        Value::Array(arr)
    }
}

fn name_or_null(s: String) -> Value {
    if s.is_empty() {
        Value::Null
    } else {
        json!(s)
    }
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
