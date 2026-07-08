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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountMap {
    entries: Vec<MountEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MountEntry {
    mount: String,
    disk_prefix: String,
}

impl MountMap {
    pub fn parse(spec: &str) -> std::result::Result<Self, String> {
        let mut entries = Vec::new();
        for raw in spec.split(',') {
            let token = raw.trim();
            if token.is_empty() {
                continue;
            }
            let (mount, disk_prefix) = match token.split_once('=') {
                Some((mount, disk_prefix)) => {
                    (normalize_mount(mount)?, normalize_disk_prefix(disk_prefix)?)
                }
                None => (normalize_mount(token)?, String::new()),
            };
            entries.push(MountEntry { mount, disk_prefix });
        }
        if entries.is_empty() {
            return Err(
                "mount mapping must not be empty (e.g. /Game or /Game=Content)".to_string(),
            );
        }
        entries.sort_by(|a, b| b.disk_prefix.len().cmp(&a.disk_prefix.len()));
        Ok(Self { entries })
    }

    pub fn single(mount: &str) -> std::result::Result<Self, String> {
        Self::parse(mount)
    }

    pub fn map_relative(&self, rel: &str) -> String {
        let normalized = normalize_relative_path(rel);
        let entry = self
            .entries
            .iter()
            .find(|entry| relative_matches_prefix(&normalized, &entry.disk_prefix))
            .unwrap_or_else(|| {
                self.entries
                    .iter()
                    .find(|entry| entry.disk_prefix.is_empty())
                    .unwrap_or(&self.entries[0])
            });
        let mapped = strip_disk_prefix(&normalized, &entry.disk_prefix);
        join_mount_path(&entry.mount, mapped)
    }
}

pub fn package_path_from_relative(rel: &str, mount: &str) -> String {
    let mounts = MountMap::single(mount).unwrap_or_else(|_| MountMap {
        entries: vec![MountEntry {
            mount: "/Game".to_string(),
            disk_prefix: String::new(),
        }],
    });
    package_path_from_relative_with_mounts(rel, &mounts)
}

pub fn package_path_from_relative_with_mounts(rel: &str, mounts: &MountMap) -> String {
    mounts.map_relative(rel)
}

fn normalize_mount(raw: &str) -> std::result::Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.trim_matches('/').is_empty() {
        return Err("mount prefix must not be empty (e.g. /Game)".to_string());
    }
    if trimmed.contains([':', '\\']) || trimmed.contains(char::is_whitespace) {
        return Err(format!(
            "mount prefix '{raw}' looks like a filesystem path; expected a UE mount root like /Game"
        ));
    }
    Ok(format!("/{}", trimmed.trim_matches('/')))
}

fn normalize_disk_prefix(raw: &str) -> std::result::Result<String, String> {
    let trimmed = raw.trim();
    if trimmed.contains([':', '\\']) || trimmed.contains(char::is_whitespace) {
        return Err(format!(
            "mount disk prefix '{raw}' must be a relative path under --scan-dir"
        ));
    }
    if trimmed.is_empty() || trimmed == "." {
        return Ok(String::new());
    }
    Ok(normalize_relative_path(trimmed))
}

fn normalize_relative_path(path: &str) -> String {
    path.replace('\\', "/").trim_start_matches('/').to_string()
}

fn relative_matches_prefix(rel: &str, prefix: &str) -> bool {
    prefix.is_empty()
        || rel.eq_ignore_ascii_case(prefix)
        || rel.get(prefix.len()..).is_some_and(|tail| {
            tail.starts_with('/') && rel[..prefix.len()].eq_ignore_ascii_case(prefix)
        })
}

fn strip_disk_prefix<'a>(rel: &'a str, prefix: &str) -> &'a str {
    if prefix.is_empty() {
        return rel;
    }
    if rel.eq_ignore_ascii_case(prefix) {
        return "";
    }
    rel.get(prefix.len() + 1..).unwrap_or(rel)
}

fn join_mount_path(mount: &str, rel: &str) -> String {
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
    if without_ext.is_empty() {
        mount.to_string()
    } else {
        format!("{mount}/{without_ext}")
    }
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
    let (soft, soft_err) = parse_soft_package_references(
        &mut r,
        &names,
        summary.soft_package_references_offset,
        summary.soft_package_references_count,
    );
    if let Some(err) = soft_err {
        anyhow::bail!("soft package reference table failed: {err}");
    }
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
