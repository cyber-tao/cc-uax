//! How many leading bytes of a package file an analysis actually reads.
//!
//! A package is header tables, then every export's data, then whatever the class
//! serializers appended after the last export (bulk data, thumbnails, the asset
//! registry section). Nothing in this crate reads that tail, and on a real
//! project it is most of the file, so callers that read a file from disk ask
//! this module how much of it to read.

use crate::object::ObjectExport;
use crate::reader::Reader;
use crate::rejection::PackageParseError;
use crate::summary::{PackageFileSummary, SummaryScope};

/// How much of a package file an analysis reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadExtent {
    /// `head` is shorter than the header tables; read this many leading bytes and
    /// ask again.
    NeedHeader(u64),
    /// Read exactly this many leading bytes: the header tables and every export's
    /// serial range.
    Prefix(u64),
}

/// The extent of a package the analysis pipeline targets. `head` is the leading
/// bytes read so far and `file_len` the whole file's length.
///
/// Every error means the extent cannot be trusted, so a caller falls back to
/// reading the whole file and lets the normal parse report exactly what it
/// always reported. An out-of-scope error stays classified as out of scope.
pub fn package_read_extent(head: &[u8], file_len: u64) -> Result<ReadExtent, PackageParseError> {
    extent(head, file_len, SummaryScope::Analysis).map_err(PackageParseError::from)
}

/// The extent of a UE4-format package whose reference tables alone are read: the
/// header, and nothing after it.
pub fn legacy_package_read_extent(
    head: &[u8],
    file_len: u64,
) -> Result<ReadExtent, PackageParseError> {
    extent(head, file_len, SummaryScope::LegacyReferences).map_err(PackageParseError::from)
}

fn extent(head: &[u8], file_len: u64, scope: SummaryScope) -> anyhow::Result<ReadExtent> {
    let mut r = Reader::new(head);
    let summary = PackageFileSummary::parse_scoped(&mut r, scope)?;
    let header = u64::try_from(summary.total_header_size)
        .ok()
        .filter(|&size| size > 0)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "TotalHeaderSize {} is not a usable header length",
                summary.total_header_size
            )
        })?
        .min(file_len);
    if (head.len() as u64) < header {
        return Ok(ReadExtent::NeedHeader(header));
    }

    // The extent trusts `TotalHeaderSize` to end every table this crate reads.
    // A table that starts past it breaks that, so refuse the shortcut.
    let tables: &[(&str, i32, i32)] = &[
        ("name", summary.name_offset, summary.name_count),
        (
            "soft package reference",
            summary.soft_package_references_offset,
            summary.soft_package_references_count,
        ),
        ("import", summary.import_offset, summary.import_count),
        ("export", summary.export_offset, summary.export_count),
        (
            "soft object path",
            summary.soft_object_paths_offset,
            summary.soft_object_paths_count,
        ),
        ("metadata", summary.metadata_offset, 1),
    ];
    for &(label, offset, count) in tables {
        let is_read = count > 0
            && offset > 0
            && (scope == SummaryScope::Analysis
                || matches!(label, "name" | "import" | "soft package reference"));
        if is_read && offset as u64 >= header {
            anyhow::bail!("{label} table at {offset} starts past the header end {header}");
        }
    }
    if scope == SummaryScope::LegacyReferences {
        return Ok(ReadExtent::Prefix(header));
    }

    let exports = ObjectExport::parse_table(
        &mut r,
        summary.export_offset,
        summary.export_count,
        summary.file_version_ue4,
        summary.file_version_ue5,
    )?;
    let mut end = header;
    for export in &exports {
        // An empty export never reads its offset.
        if export.serial_size == 0 {
            continue;
        }
        let (Ok(start), Ok(size)) = (
            u64::try_from(export.serial_offset),
            u64::try_from(export.serial_size),
        ) else {
            anyhow::bail!(
                "export serial range (offset {}, size {}) cannot be expressed",
                export.serial_offset,
                export.serial_size
            );
        };
        let export_end = start.checked_add(size).ok_or_else(|| {
            anyhow::anyhow!("export serial range at {start} overflows with size {size}")
        })?;
        end = end.max(export_end);
    }
    Ok(ReadExtent::Prefix(end.min(file_len)))
}
