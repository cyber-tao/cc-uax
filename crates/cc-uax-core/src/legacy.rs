//! Reference tables of UE4-format packages.
//!
//! Real UE5 projects keep packages that were never resaved in UE5
//! (`FileVersionUE5` = 0). The analysis pipeline does not target them, but their
//! linker tables are read by the same code UE's asset registry uses, and without
//! those edges everything only they reference looks unreachable.

use crate::model::AssetReferences;
use crate::name::NameMap;
use crate::object::ObjectImport;
use crate::package::parse_soft_package_references;
use crate::reader::Reader;
use crate::references::asset_references;
use crate::rejection::PackageParseError;
use crate::summary::{PackageFileSummary, SummaryScope};

/// Linker reference tables of a UE4-format package (`FileVersionUE5` = 0), which
/// the analysis pipeline does not target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyPackageReferences {
    pub file_version_ue4: i32,
    pub references: AssetReferences,
}

/// Reads the name, import and soft-package-reference tables of a UE4-format
/// package. Exports, properties and graphs are not read.
///
/// A soft-reference table that cannot be read fails the whole call: a reference
/// list missing entries must not be reported as the package's complete tables.
pub fn read_legacy_package_references(
    data: &[u8],
) -> Result<LegacyPackageReferences, PackageParseError> {
    read_tables(data).map_err(PackageParseError::from)
}

fn read_tables(data: &[u8]) -> anyhow::Result<LegacyPackageReferences> {
    let mut r = Reader::new(data);
    let summary = PackageFileSummary::parse_scoped(&mut r, SummaryScope::LegacyReferences)?;
    let ue4 = summary.file_version_ue4;
    let names = NameMap::parse(&mut r, summary.name_offset, summary.name_count, ue4)?;
    let imports = ObjectImport::parse_table(
        &mut r,
        summary.import_offset,
        summary.import_count,
        ue4,
        summary.file_version_ue5,
        summary.filter_editor_only(),
        &summary.engine_version,
    )?;
    let (soft_package_references, soft_error) = parse_soft_package_references(
        &mut r,
        &names,
        summary.soft_package_references_offset,
        summary.soft_package_references_count,
        ue4,
    );
    if let Some(error) = soft_error {
        anyhow::bail!("{error}");
    }
    Ok(LegacyPackageReferences {
        file_version_ue4: ue4,
        references: asset_references(&names, &imports, &soft_package_references),
    })
}
