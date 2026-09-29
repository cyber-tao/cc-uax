//! Reading only the leading bytes of a package file that an analysis needs.
//!
//! The header tables and the export data come first; what a class serializer
//! appended after the last export (bulk data, thumbnails, the asset registry
//! section) is most of a large file and is never read. `cc_uax_core` says how far
//! to read (`package_read_extent`); this module does the reading.

use cc_uax_core::{PackageParseError, ReadExtent, legacy_package_read_extent, package_read_extent};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

/// Leading bytes read before the first extent query: enough for the summary of
/// nearly every package, and small enough that the many tiny assets of a project
/// cost one read each.
const INITIAL_READ_BYTES: u64 = 64 * 1024;

/// Extent queries before giving up and reading the whole file. The header size is
/// known after the first answer, so a well-formed package needs at most two.
const MAX_EXTENT_ROUNDS: usize = 4;

/// What the bytes are read for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadScope {
    /// A full analysis: the header tables and every export's data.
    Analysis,
    /// The linker reference tables of a UE4-format package: the header alone.
    LegacyReferences,
}

/// The leading bytes of a package file and the size of the whole file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageBytes {
    /// A prefix of the file: everything the scope needs, or the whole file when
    /// the extent could not be trusted.
    pub bytes: Vec<u8>,
    /// Size of the whole file, for `PackageView::parse_prefix`.
    pub file_len: u64,
}

/// Reads as much of `path` as `scope` needs and no more.
///
/// If the extent cannot be computed the whole file is read, so the normal parse
/// reports exactly what it always reported and a malformed summary can never hide
/// behind this shortcut. The one exception is an out-of-scope package: that is
/// decided by the summary alone, which the first read already holds.
pub fn read_package_file(path: &Path, scope: ReadScope) -> io::Result<PackageBytes> {
    let mut file = File::open(path)?;
    let file_len = file.metadata()?.len();
    let mut package = PackageBytes {
        bytes: Vec::new(),
        file_len,
    };
    fill(&mut file, &mut package, scope)?;
    Ok(package)
}

/// Widens bytes already read from `path` to what `scope` needs, without reading
/// any byte twice.
pub fn extend_package_bytes(
    path: &Path,
    package: &mut PackageBytes,
    scope: ReadScope,
) -> io::Result<()> {
    let mut file = File::open(path)?;
    fill(&mut file, package, scope)
}

fn fill(file: &mut File, package: &mut PackageBytes, scope: ReadScope) -> io::Result<()> {
    let file_len = package.file_len;
    extend_to(file, &mut package.bytes, INITIAL_READ_BYTES.min(file_len))?;
    for _ in 0..MAX_EXTENT_ROUNDS {
        match extent(scope, &package.bytes, file_len) {
            Ok(ReadExtent::NeedHeader(bytes)) => extend_to(file, &mut package.bytes, bytes)?,
            Ok(ReadExtent::Prefix(bytes)) => {
                return extend_to(file, &mut package.bytes, bytes);
            }
            Err(error) if error.is_out_of_scope() => return Ok(()),
            Err(_) => break,
        }
    }
    extend_to(file, &mut package.bytes, file_len)
}

fn extent(scope: ReadScope, head: &[u8], file_len: u64) -> Result<ReadExtent, PackageParseError> {
    match scope {
        ReadScope::Analysis => package_read_extent(head, file_len),
        ReadScope::LegacyReferences => legacy_package_read_extent(head, file_len),
    }
}

/// Reads from where `bytes` ends up to `target` bytes in total, never re-reading.
fn extend_to(file: &mut File, bytes: &mut Vec<u8>, target: u64) -> io::Result<()> {
    let have = bytes.len() as u64;
    if target <= have {
        return Ok(());
    }
    file.seek(SeekFrom::Start(have))?;
    file.by_ref().take(target - have).read_to_end(bytes)?;
    Ok(())
}
