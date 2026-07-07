//! Forward/reverse package-reference analysis: partitioning imports into asset
//! vs script references, mapping disk paths to package paths, and the header-only
//! fast path used by `--scan-dir` reverse scans.

use crate::name::NameMap;
use crate::object::ObjectImport;
use crate::package::{Package, parse_soft_package_references};
use crate::reader::Reader;
use crate::summary::PackageFileSummary;
use anyhow::Result;
use std::collections::BTreeSet;

const PACKAGE_CLASS_NAME: &str = "Package";
const SCRIPT_PATH_PREFIX: &str = "/Script/";

impl Package {
    pub(crate) fn import_class_object_names(&self) -> impl Iterator<Item = (String, String)> + '_ {
        self.imports.iter().map(|imp| {
            (
                self.names.resolve_raw(imp.class_name),
                self.names.resolve_raw(imp.object_name),
            )
        })
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
}

pub(crate) fn is_valid_package_name(name: &str) -> bool {
    !name.is_empty() && name != "None"
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
