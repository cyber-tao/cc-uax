use crate::reader::{FSTRING_LENGTH_BYTES, Guid, Reader, ensure_count_fits};
use crate::rejection::out_of_scope;
use crate::version::{PACKAGE_FILE_TAG, PACKAGE_FILE_TAG_SWAPPED, ue4, ue5};
use anyhow::{Result, bail};

const PKG_FILTER_EDITOR_ONLY: u32 = 0x8000_0000;
/// `PKG_Cooked` (CoreUObject `ObjectMacros.h`, `EPackageFlags`): the package was cooked.
const PKG_COOKED: u32 = 0x0000_0200;
/// `PKG_UnversionedProperties` (CoreUObject `ObjectMacros.h`, `EPackageFlags`): tagged
/// properties are replaced by unversioned property serialization.
const PKG_UNVERSIONED_PROPERTIES: u32 = 0x0000_2000;
/// FCustomVersion entry on disk (`Optimized` format): 16-byte GUID + 4-byte version.
const CUSTOM_VERSION_ENTRY_BYTES: u64 = 20;
/// `FEnumCustomVersion_DEPRECATED` entry: `uint32` tag + `int32` version.
const ENUM_CUSTOM_VERSION_ENTRY_BYTES: u64 = 8;
/// Smallest `FGuidCustomVersion_DEPRECATED` entry: GUID + version + the length
/// of an empty friendly-name `FString`.
const GUID_CUSTOM_VERSION_MIN_ENTRY_BYTES: u64 = 16 + 4 + FSTRING_LENGTH_BYTES;

/// How much of a package the caller wants to read.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SummaryScope {
    /// The versioned UE5 editor package the analysis pipeline targets.
    Analysis,
    /// A UE4-format package (`FileVersionUE5` = 0) whose linker reference tables
    /// alone are read.
    LegacyReferences,
}

#[derive(Debug, Clone)]
pub struct CustomVersion {
    pub key: Guid,
    pub version: i32,
}

#[derive(Debug, Clone, Default)]
pub struct EngineVersion {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
    pub changelist: u32,
    pub branch: String,
}

impl EngineVersion {
    fn parse(r: &mut Reader) -> Result<Self> {
        Ok(EngineVersion {
            major: r.read_u16()?,
            minor: r.read_u16()?,
            patch: r.read_u16()?,
            changelist: r.read_u32()?,
            branch: r.read_fstring()?,
        })
    }

    pub fn display(&self) -> String {
        format!(
            "{}.{}.{}-{}+{}",
            self.major, self.minor, self.patch, self.changelist, self.branch
        )
    }
}

#[derive(Debug, Clone)]
pub struct PackageFileSummary {
    pub tag: u32,
    pub legacy_file_version: i32,
    pub file_version_ue4: i32,
    pub file_version_ue5: i32,
    pub file_version_licensee_ue: i32,
    pub custom_versions: Vec<CustomVersion>,
    pub total_header_size: i32,
    pub package_name: String,
    pub package_flags: u32,
    pub name_count: i32,
    pub name_offset: i32,
    pub soft_object_paths_count: i32,
    pub soft_object_paths_offset: i32,
    pub export_count: i32,
    pub export_offset: i32,
    pub import_count: i32,
    pub import_offset: i32,
    pub soft_package_references_count: i32,
    pub soft_package_references_offset: i32,
    /// `MetaDataOffset` (`METADATA_SERIALIZATION_OFFSET`, UE5.6+): where the
    /// package-level metadata maps live now that `UMetaData` is no longer an
    /// export. `0` when the package predates the field or wrote no table.
    pub metadata_offset: i32,
    pub engine_version: EngineVersion,
    pub compatible_engine_version: EngineVersion,
    pub bulk_data_start_offset: i64,
}

impl PackageFileSummary {
    pub fn filter_editor_only(&self) -> bool {
        self.package_flags & PKG_FILTER_EDITOR_ONLY != 0
    }

    pub fn custom_version(&self, key: Guid) -> Option<i32> {
        self.custom_versions
            .iter()
            .find(|c| c.key == key)
            .map(|c| c.version)
    }

    pub fn parse(r: &mut Reader) -> Result<Self> {
        Self::parse_scoped(r, SummaryScope::Analysis)
    }

    pub(crate) fn parse_scoped(r: &mut Reader, scope: SummaryScope) -> Result<Self> {
        let tag = r.read_u32()?;
        if tag == PACKAGE_FILE_TAG_SWAPPED {
            return Err(out_of_scope(
                "package uses swapped (big-endian) byte order, possibly a cooked console package; unsupported",
            ));
        }
        if tag != PACKAGE_FILE_TAG {
            bail!(
                "invalid package magic: 0x{tag:08X} (expected 0x{PACKAGE_FILE_TAG:08X}); not a valid .uasset file"
            );
        }

        let legacy_file_version = r.read_i32()?;
        if legacy_file_version >= 0 {
            return Err(out_of_scope(format!(
                "looks like a legacy UE3 package (LegacyFileVersion={legacy_file_version}); unsupported"
            )));
        }
        if legacy_file_version < -9 {
            return Err(out_of_scope(format!(
                "package format version too new (LegacyFileVersion={legacy_file_version}); out of known range"
            )));
        }

        if legacy_file_version != -4 {
            let _legacy_ue3 = r.read_i32()?;
        }

        let file_version_ue4 = r.read_i32()?;
        let file_version_ue5 = if legacy_file_version <= -8 {
            r.read_i32()?
        } else {
            0
        };
        let file_version_licensee_ue = r.read_i32()?;

        let unversioned =
            file_version_ue4 == 0 && file_version_ue5 == 0 && file_version_licensee_ue == 0;
        if unversioned {
            return Err(out_of_scope(
                "package is unversioned (no version info, typically a cooked package); this tool targets versioned editor assets",
            ));
        }
        if scope == SummaryScope::LegacyReferences {
            if file_version_ue5 != 0 || !(-7..=-2).contains(&legacy_file_version) {
                return Err(out_of_scope(format!(
                    "not a UE4-format package (LegacyFileVersion={legacy_file_version}, FileVersionUE5={file_version_ue5})"
                )));
            }
            if file_version_ue4 < ue4::OLDEST_LOADABLE_PACKAGE {
                return Err(out_of_scope(format!(
                    "FileVersionUE4={file_version_ue4} is older than the oldest loadable package version ({})",
                    ue4::OLDEST_LOADABLE_PACKAGE
                )));
            }
        } else {
            if file_version_ue5 < crate::version::SUPPORTED_FILE_VERSION_FLOOR {
                return Err(out_of_scope(format!(
                    "unsupported package FileVersionUE5={file_version_ue5}; this tool targets UE5.0–5.8 versioned editor assets (FileVersionUE5 >= {})",
                    crate::version::SUPPORTED_FILE_VERSION_FLOOR
                )));
            }
            // UE stops reading a package whose file version it does not know
            // (PackageFileSummary.cpp bails right after FileVersionLicensee when
            // IsFileVersionTooNew), because every field after this point may have
            // changed. Assuming the 5.8 layout would parse the tables from the wrong
            // offsets and still produce a report.
            if file_version_ue5 > ue5::HIGHEST {
                return Err(out_of_scope(format!(
                    "package FileVersionUE5={file_version_ue5} is newer than this parser understands (highest known is {})",
                    ue5::HIGHEST
                )));
            }
        }

        let ue4v = file_version_ue4;
        let ue5v = file_version_ue5;

        let mut total_header_size = 0i32;
        if ue5v >= ue5::PACKAGE_SAVED_HASH {
            let _saved_hash = r.read_io_hash()?;
            total_header_size = r.read_i32()?;
        }

        let mut custom_versions = Vec::new();
        if legacy_file_version <= -2 {
            // `GetCustomVersionFormatForArchive` (PackageFileSummary.cpp): the
            // legacy version, not the engine, selects the entry layout. UE5
            // packages are -8/-9 and always take the optimized one.
            let count = r.read_i32()?;
            let entry_bytes = match legacy_file_version {
                -2 => ENUM_CUSTOM_VERSION_ENTRY_BYTES,
                -5..=-3 => GUID_CUSTOM_VERSION_MIN_ENTRY_BYTES,
                _ => CUSTOM_VERSION_ENTRY_BYTES,
            };
            ensure_count_fits(count, r.remaining(), entry_bytes, "custom version")?;
            for _ in 0..count {
                match legacy_file_version {
                    // FEnumCustomVersion_DEPRECATED: the GUID is invented from
                    // three zeroes and the tag (CustomVersion.cpp).
                    -2 => {
                        let tag = r.read_u32()?;
                        let version = r.read_i32()?;
                        custom_versions.push(CustomVersion {
                            key: Guid([0, 0, 0, tag]),
                            version,
                        });
                    }
                    // FGuidCustomVersion_DEPRECATED: the friendly name is not kept.
                    -5..=-3 => {
                        let key = r.read_guid()?;
                        let version = r.read_i32()?;
                        let _friendly_name = r.read_fstring()?;
                        custom_versions.push(CustomVersion { key, version });
                    }
                    _ => {
                        let key = r.read_guid()?;
                        let version = r.read_i32()?;
                        custom_versions.push(CustomVersion { key, version });
                    }
                }
            }
        }

        if ue5v < ue5::PACKAGE_SAVED_HASH {
            total_header_size = r.read_i32()?;
        }

        let package_name = r.read_fstring()?;
        let package_flags = r.read_u32()?;
        if package_flags & PKG_COOKED != 0 {
            return Err(out_of_scope(
                "package is cooked (PKG_Cooked); this tool targets uncooked editor packages",
            ));
        }
        if package_flags & PKG_UNVERSIONED_PROPERTIES != 0 {
            return Err(out_of_scope(
                "package uses unversioned property serialization (PKG_UnversionedProperties), which only cooked packages write",
            ));
        }
        let filter_editor_only = package_flags & PKG_FILTER_EDITOR_ONLY != 0;

        let name_count = r.read_i32()?;
        let name_offset = r.read_i32()?;

        let (mut soft_object_paths_count, mut soft_object_paths_offset) = (0, 0);
        if ue5v >= ue5::ADD_SOFTOBJECTPATH_LIST {
            soft_object_paths_count = r.read_i32()?;
            soft_object_paths_offset = r.read_i32()?;
        }

        if !filter_editor_only && ue4v >= ue4::ADDED_PACKAGE_SUMMARY_LOCALIZATION_ID {
            let _localization_id = r.read_fstring()?;
        }

        if ue4v >= ue4::SERIALIZE_TEXT_IN_PACKAGES {
            let _gatherable_text_data_count = r.read_i32()?;
            let _gatherable_text_data_offset = r.read_i32()?;
        }

        let export_count = r.read_i32()?;
        let export_offset = r.read_i32()?;
        let import_count = r.read_i32()?;
        let import_offset = r.read_i32()?;

        if ue5v >= ue5::VERSE_CELLS {
            let _cell_export_count = r.read_i32()?;
            let _cell_export_offset = r.read_i32()?;
            let _cell_import_count = r.read_i32()?;
            let _cell_import_offset = r.read_i32()?;
        }

        let mut metadata_offset = 0;
        if ue5v >= ue5::METADATA_SERIALIZATION_OFFSET {
            metadata_offset = r.read_i32()?;
        }

        let _depends_offset = r.read_i32()?;

        let (mut soft_package_references_count, mut soft_package_references_offset) = (0, 0);
        if ue4v >= ue4::ADD_STRING_ASSET_REFERENCES_MAP {
            soft_package_references_count = r.read_i32()?;
            soft_package_references_offset = r.read_i32()?;
        }

        if ue4v >= ue4::ADDED_SEARCHABLE_NAMES {
            let _searchable_names_offset = r.read_i32()?;
        }

        let _thumbnail_table_offset = r.read_i32()?;

        if ue5v >= ue5::IMPORT_TYPE_HIERARCHIES {
            let _import_type_hierarchies_count = r.read_i32()?;
            let _import_type_hierarchies_offset = r.read_i32()?;
        }

        if ue5v < ue5::PACKAGE_SAVED_HASH {
            let _legacy_guid = r.read_guid()?;
        }

        if !filter_editor_only && ue4v >= ue4::ADDED_PACKAGE_OWNER {
            let _persistent_guid = r.read_guid()?;
        }
        if !filter_editor_only
            && (ue4::ADDED_PACKAGE_OWNER..ue4::NON_OUTER_PACKAGE_IMPORT).contains(&ue4v)
        {
            let _owner_persistent_guid = r.read_guid()?;
        }

        let generation_count = r.read_i32()?;
        ensure_count_fits(generation_count, r.remaining(), 8, "generation")?;
        for _ in 0..generation_count {
            let _gen_export_count = r.read_i32()?;
            let _gen_name_count = r.read_i32()?;
        }

        let engine_version = if ue4v >= ue4::ENGINE_VERSION_OBJECT {
            EngineVersion::parse(r)?
        } else {
            let _changelist = r.read_i32()?;
            EngineVersion::default()
        };

        let compatible_engine_version =
            if ue4v >= ue4::PACKAGE_SUMMARY_HAS_COMPATIBLE_ENGINE_VERSION {
                EngineVersion::parse(r)?
            } else {
                engine_version.clone()
            };

        let _compression_flags = r.read_u32()?;

        let compressed_chunks_count = r.read_i32()?;
        if compressed_chunks_count > 0 {
            return Err(out_of_scope(format!(
                "package uses package-level compression (CompressedChunks={compressed_chunks_count}); cannot parse"
            )));
        }
        // A negative count is not an empty TArray: continuing here would read
        // PackageSource and every later field from the wrong offset.
        ensure_count_fits(compressed_chunks_count, 0, 0, "CompressedChunks")?;

        let _package_source = r.read_u32()?;

        let additional_count = r.read_i32()?;
        ensure_count_fits(
            additional_count,
            r.remaining(),
            FSTRING_LENGTH_BYTES,
            "AdditionalPackagesToCook",
        )?;
        for _ in 0..additional_count {
            let _ = r.read_fstring()?;
        }

        if legacy_file_version > -7 {
            let _num_texture_allocations = r.read_i32()?;
        }

        let _asset_registry_data_offset = r.read_i32()?;
        let bulk_data_start_offset = r.read_i64()?;

        if ue4v >= ue4::WORLD_LEVEL_INFO {
            let _world_tile_info_data_offset = r.read_i32()?;
        }

        if ue4v >= ue4::CHANGED_CHUNKID_TO_BE_AN_ARRAY_OF_CHUNKIDS {
            let chunk_count = r.read_i32()?;
            ensure_count_fits(chunk_count, r.remaining(), 4, "ChunkIDs")?;
            for _ in 0..chunk_count {
                let _ = r.read_i32()?;
            }
        } else if ue4v >= ue4::ADDED_CHUNKID_TO_ASSETDATA_AND_UPACKAGE {
            let _chunk_id = r.read_i32()?;
        }

        if ue4v >= ue4::PRELOAD_DEPENDENCIES_IN_COOKED_EXPORTS {
            let _preload_dependency_count = r.read_i32()?;
            let _preload_dependency_offset = r.read_i32()?;
        }

        if ue5v >= ue5::NAMES_REFERENCED_FROM_EXPORT_DATA {
            let _names_referenced_from_export_data_count = r.read_i32()?;
        }

        if ue5v >= ue5::PAYLOAD_TOC {
            let _payload_toc_offset = r.read_i64()?;
        }

        if ue5v >= ue5::DATA_RESOURCES {
            let _data_resource_offset = r.read_i32()?;
        }

        Ok(PackageFileSummary {
            tag,
            legacy_file_version,
            file_version_ue4,
            file_version_ue5,
            file_version_licensee_ue,
            custom_versions,
            total_header_size,
            package_name,
            package_flags,
            name_count,
            name_offset,
            soft_object_paths_count,
            soft_object_paths_offset,
            export_count,
            export_offset,
            import_count,
            import_offset,
            soft_package_references_count,
            soft_package_references_offset,
            metadata_offset,
            engine_version,
            compatible_engine_version,
            bulk_data_start_offset,
        })
    }
}
