use crate::decode::DecodeReport;
use crate::package::Package;
use crate::references::{collect_package_references, is_valid_package_name};
use serde_json::{Value, json};
use std::collections::BTreeSet;

use super::export_json::exports_to_json;
use super::property_json::name_or_null;

impl DecodeReport<'_> {
    pub fn to_json(&self) -> Value {
        let mut root = serde_json::Map::new();
        if self.sections.summary {
            root.insert("summary".into(), summary_json(self.package));
        }
        if self.sections.names {
            root.insert("names".into(), json!(self.package.names.names));
        }
        if self.sections.references {
            root.insert("references".into(), references_json(self.package));
        }
        if self.sections.imports {
            root.insert("imports".into(), imports_json(self.package));
        }
        if self.sections.exports {
            root.insert("exports".into(), exports_to_json(self));
        }
        root.insert("diagnostics".into(), json!(self.diagnostics));
        Value::Object(root)
    }
}

fn summary_json(package: &Package) -> Value {
    let s = &package.summary;
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

fn imports_json(package: &Package) -> Value {
    let arr: Vec<Value> = package
        .imports
        .iter()
        .enumerate()
        .map(|(i, imp)| {
            let pkg_index = -((i as i32) + 1);
            json!({
                "index": pkg_index,
                "class_package": package.names.resolve_raw(imp.class_package),
                "class": package.names.resolve_raw(imp.class_name),
                "name": package.names.resolve_raw(imp.object_name),
                "outer": name_or_null(package.resolve_full_name(imp.outer_index.0)),
                "package_name": imp.package_name.map(|p| package.names.resolve_raw(p)),
                "full_name": package.resolve_full_name(pkg_index),
            })
        })
        .collect();
    Value::Array(arr)
}

fn references_json(package: &Package) -> Value {
    let (assets, scripts) = collect_package_references(package.import_class_object_names());
    let soft: Vec<String> = package
        .soft_package_references
        .iter()
        .filter(|s| is_valid_package_name(s))
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    json!({ "assets": assets, "scripts": scripts, "soft": soft })
}
