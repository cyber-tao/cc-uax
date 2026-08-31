//! Parsed representation of a UE5 package and the top-level parse orchestration:
//! header → name/import/export tables → soft-object-path and soft-package-reference
//! tables, plus the name/object index resolution shared by the output and reference
//! layers.

use crate::name::NameMap;
use crate::object::{ObjectExport, ObjectImport};
use crate::property::read_soft_object_path;
use crate::reader::{FSTRING_LENGTH_BYTES, RAW_NAME_BYTES, Reader, seek_to_table};
use crate::structured_value::{Value, json};
use crate::summary::PackageFileSummary;
use crate::version::ue5;
use anyhow::Result;

/// Maximum outer-chain depth when resolving a full object name; guards against
/// cyclic outer references in malformed packages.
const MAX_RESOLVE_DEPTH: u32 = 64;

pub struct Package {
    pub(crate) summary: PackageFileSummary,
    pub(crate) names: NameMap,
    pub(crate) imports: Vec<ObjectImport>,
    pub(crate) exports: Vec<ObjectExport>,
    pub(crate) soft_object_paths: Vec<Value>,
    pub(crate) soft_object_path_error: Option<String>,
    pub(crate) soft_package_references: Vec<String>,
    pub(crate) soft_package_reference_error: Option<String>,
}

impl Package {
    pub(crate) fn parse(data: &[u8]) -> Result<Package> {
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
            &summary.engine_version,
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
            ue5,
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

    pub fn resolve_full_name(&self, index: i32) -> String {
        self.resolve_full_name_at(index, 0)
    }

    /// Walk the outer chain to build a dotted full name. `depth` guards against
    /// cyclic outer references in malformed packages; see `MAX_RESOLVE_DEPTH`.
    fn resolve_full_name_at(&self, index: i32, depth: u32) -> String {
        if index == 0 || depth > MAX_RESOLVE_DEPTH {
            return String::new();
        }
        if index < 0 {
            let Some(i) = index
                .checked_neg()
                .and_then(|value| value.checked_sub(1))
                .and_then(|value| usize::try_from(value).ok())
            else {
                return format!("<invalid_package_index#{index}>");
            };
            match self.imports.get(i) {
                Some(imp) => {
                    let name = self.names.resolve_raw(imp.object_name);
                    let outer = self.resolve_full_name_at(imp.outer_index.0, depth + 1);
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
                    let outer = self.resolve_full_name_at(exp.outer_index.0, depth + 1);
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
        let full = self.resolve_full_name(index);
        json!({ "ref": full, "index": index })
    }
}

fn parse_soft_object_path_table(
    r: &mut Reader,
    names: &NameMap,
    offset: i32,
    count: i32,
    file_version_ue5: i32,
) -> (Vec<Value>, Option<String>) {
    let mut out = Vec::new();
    // Same guard as every other header table, but a failure here is reported to
    // the caller rather than aborting the parse: a package with an unreadable
    // soft-path table still yields usable name/import/export evidence.
    match seek_to_table(
        r,
        "soft object path table",
        offset,
        count,
        soft_object_path_min_bytes(file_version_ue5),
    ) {
        Ok(true) => {}
        Ok(false) => return (out, None),
        Err(err) => return (out, Some(format!("{err:#}"))),
    }
    out.reserve(count as usize);
    // The header declares no end offset for this table, so the file end is the
    // only bound the entries have.
    let table_end = r.len();
    for i in 0..count {
        match read_soft_object_path(r, names, file_version_ue5, table_end) {
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

/// Smallest on-disk `FSoftObjectPath` entry: an `FTopLevelAssetPath` (two FNames)
/// from `FSOFTOBJECTPATH_REMOVE_ASSET_PATH_FNAMES` on, one FName before it, plus
/// the 4-byte length of an empty sub-path FString.
fn soft_object_path_min_bytes(file_version_ue5: i32) -> u64 {
    let asset_path = if file_version_ue5 >= ue5::FSOFTOBJECTPATH_REMOVE_ASSET_PATH_FNAMES {
        2 * RAW_NAME_BYTES
    } else {
        RAW_NAME_BYTES
    };
    asset_path + FSTRING_LENGTH_BYTES
}

/// The SoftPackageReferences header table: one FName package name per entry
/// (written by SavePackage from FLinkerSave::SoftPackageReferenceList).
pub(crate) fn parse_soft_package_references(
    r: &mut Reader,
    names: &NameMap,
    offset: i32,
    count: i32,
) -> (Vec<String>, Option<String>) {
    let mut out = Vec::new();
    match seek_to_table(
        r,
        "soft package reference table",
        offset,
        count,
        RAW_NAME_BYTES,
    ) {
        Ok(true) => {}
        Ok(false) => return (out, None),
        Err(err) => return (out, Some(format!("{err:#}"))),
    }
    out.reserve(count as usize);
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
