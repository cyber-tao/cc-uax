use crate::reader::{Guid, Reader};
use crate::version::{PACKAGE_FILE_TAG, PACKAGE_FILE_TAG_SWAPPED, ue4, ue5};
use anyhow::{Result, bail};

const PKG_FILTER_EDITOR_ONLY: u32 = 0x8000_0000;
/// FCustomVersion entry on disk: 16-byte GUID + 4-byte version.
const CUSTOM_VERSION_ENTRY_BYTES: u64 = 20;

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
    pub saved_hash: Option<[u8; 20]>,
    pub total_header_size: i32,
    pub package_name: String,
    pub package_flags: u32,
    pub name_count: i32,
    pub name_offset: i32,
    pub soft_object_paths_count: i32,
    pub soft_object_paths_offset: i32,
    pub localization_id: String,
    pub gatherable_text_data_count: i32,
    pub gatherable_text_data_offset: i32,
    pub export_count: i32,
    pub export_offset: i32,
    pub import_count: i32,
    pub import_offset: i32,
    pub cell_export_count: i32,
    pub cell_export_offset: i32,
    pub cell_import_count: i32,
    pub cell_import_offset: i32,
    pub metadata_offset: i32,
    pub depends_offset: i32,
    pub soft_package_references_count: i32,
    pub soft_package_references_offset: i32,
    pub searchable_names_offset: i32,
    pub thumbnail_table_offset: i32,
    pub import_type_hierarchies_count: i32,
    pub import_type_hierarchies_offset: i32,
    pub engine_version: EngineVersion,
    pub compatible_engine_version: EngineVersion,
    pub compression_flags: u32,
    pub package_source: u32,
    pub asset_registry_data_offset: i32,
    pub bulk_data_start_offset: i64,
    pub world_tile_info_data_offset: i32,
    pub preload_dependency_count: i32,
    pub preload_dependency_offset: i32,
    pub names_referenced_from_export_data_count: i32,
    pub payload_toc_offset: i64,
    pub data_resource_offset: i32,
}

impl PackageFileSummary {
    pub fn is_unversioned(&self) -> bool {
        self.file_version_ue4 == 0
            && self.file_version_ue5 == 0
            && self.file_version_licensee_ue == 0
    }

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
        let tag = r.read_u32()?;
        if tag == PACKAGE_FILE_TAG_SWAPPED {
            bail!(
                "package uses swapped (big-endian) byte order, possibly a cooked console package; unsupported"
            );
        }
        if tag != PACKAGE_FILE_TAG {
            bail!(
                "invalid package magic: 0x{tag:08X} (expected 0x{PACKAGE_FILE_TAG:08X}); not a valid .uasset file"
            );
        }

        let legacy_file_version = r.read_i32()?;
        if legacy_file_version >= 0 {
            bail!(
                "looks like a legacy UE3 package (LegacyFileVersion={legacy_file_version}); unsupported"
            );
        }
        if legacy_file_version < -9 {
            bail!(
                "package format version too new (LegacyFileVersion={legacy_file_version}); out of known range"
            );
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
            bail!(
                "package is unversioned (no version info, typically a cooked package); this tool targets versioned editor assets"
            );
        }
        if file_version_ue5 < ue5::INITIAL_VERSION {
            bail!(
                "unsupported package FileVersionUE5={file_version_ue5}; this tool targets UE5 versioned editor assets (FileVersionUE5 >= {})",
                ue5::INITIAL_VERSION
            );
        }

        let ue4v = file_version_ue4;
        let ue5v = file_version_ue5;

        let mut saved_hash = None;
        let mut total_header_size = 0i32;
        if ue5v >= ue5::PACKAGE_SAVED_HASH {
            saved_hash = Some(r.read_io_hash()?);
            total_header_size = r.read_i32()?;
        }

        let mut custom_versions = Vec::new();
        if legacy_file_version <= -2 {
            let count = r.read_i32()?;
            if count < 0
                || (count as u64).saturating_mul(CUSTOM_VERSION_ENTRY_BYTES) > r.remaining()
            {
                bail!("custom version count out of range: {count}");
            }
            for _ in 0..count {
                let key = r.read_guid()?;
                let version = r.read_i32()?;
                custom_versions.push(CustomVersion { key, version });
            }
        }

        if ue5v < ue5::PACKAGE_SAVED_HASH {
            total_header_size = r.read_i32()?;
        }

        let package_name = r.read_fstring()?;
        let package_flags = r.read_u32()?;
        let filter_editor_only = package_flags & PKG_FILTER_EDITOR_ONLY != 0;

        let name_count = r.read_i32()?;
        let name_offset = r.read_i32()?;

        let (mut soft_object_paths_count, mut soft_object_paths_offset) = (0, 0);
        if ue5v >= ue5::ADD_SOFTOBJECTPATH_LIST {
            soft_object_paths_count = r.read_i32()?;
            soft_object_paths_offset = r.read_i32()?;
        }

        let mut localization_id = String::new();
        if !filter_editor_only && ue4v >= ue4::ADDED_PACKAGE_SUMMARY_LOCALIZATION_ID {
            localization_id = r.read_fstring()?;
        }

        let (mut gatherable_text_data_count, mut gatherable_text_data_offset) = (0, 0);
        if ue4v >= ue4::SERIALIZE_TEXT_IN_PACKAGES {
            gatherable_text_data_count = r.read_i32()?;
            gatherable_text_data_offset = r.read_i32()?;
        }

        let export_count = r.read_i32()?;
        let export_offset = r.read_i32()?;
        let import_count = r.read_i32()?;
        let import_offset = r.read_i32()?;

        let (mut cell_export_count, mut cell_export_offset) = (0, 0);
        let (mut cell_import_count, mut cell_import_offset) = (0, 0);
        if ue5v >= ue5::VERSE_CELLS {
            cell_export_count = r.read_i32()?;
            cell_export_offset = r.read_i32()?;
            cell_import_count = r.read_i32()?;
            cell_import_offset = r.read_i32()?;
        }

        let mut metadata_offset = 0;
        if ue5v >= ue5::METADATA_SERIALIZATION_OFFSET {
            metadata_offset = r.read_i32()?;
        }

        let depends_offset = r.read_i32()?;

        let (mut soft_package_references_count, mut soft_package_references_offset) = (0, 0);
        if ue4v >= ue4::ADD_STRING_ASSET_REFERENCES_MAP {
            soft_package_references_count = r.read_i32()?;
            soft_package_references_offset = r.read_i32()?;
        }

        let mut searchable_names_offset = 0;
        if ue4v >= ue4::ADDED_SEARCHABLE_NAMES {
            searchable_names_offset = r.read_i32()?;
        }

        let thumbnail_table_offset = r.read_i32()?;

        let (mut import_type_hierarchies_count, mut import_type_hierarchies_offset) = (0, 0);
        if ue5v >= ue5::IMPORT_TYPE_HIERARCHIES {
            import_type_hierarchies_count = r.read_i32()?;
            import_type_hierarchies_offset = r.read_i32()?;
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
        if generation_count < 0 || (generation_count as u64).saturating_mul(8) > r.remaining() {
            bail!("generation count out of range: {generation_count}");
        }
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

        let compression_flags = r.read_u32()?;

        let compressed_chunks_count = r.read_i32()?;
        if compressed_chunks_count != 0 {
            bail!(
                "package uses package-level compression (CompressedChunks={compressed_chunks_count}); cannot parse"
            );
        }

        let package_source = r.read_u32()?;

        let additional_count = r.read_i32()?;
        if additional_count < 0 || additional_count as u64 > r.remaining() {
            bail!("AdditionalPackagesToCook count out of range: {additional_count}");
        }
        for _ in 0..additional_count {
            let _ = r.read_fstring()?;
        }

        if legacy_file_version > -7 {
            let _num_texture_allocations = r.read_i32()?;
        }

        let asset_registry_data_offset = r.read_i32()?;
        let bulk_data_start_offset = r.read_i64()?;

        let mut world_tile_info_data_offset = 0;
        if ue4v >= ue4::WORLD_LEVEL_INFO {
            world_tile_info_data_offset = r.read_i32()?;
        }

        if ue4v >= ue4::CHANGED_CHUNKID_TO_BE_AN_ARRAY_OF_CHUNKIDS {
            let chunk_count = r.read_i32()?;
            if chunk_count < 0 || (chunk_count as u64).saturating_mul(4) > r.remaining() {
                bail!("ChunkIDs count out of range: {chunk_count}");
            }
            for _ in 0..chunk_count {
                let _ = r.read_i32()?;
            }
        } else if ue4v >= ue4::ADDED_CHUNKID_TO_ASSETDATA_AND_UPACKAGE {
            let _chunk_id = r.read_i32()?;
        }

        let (mut preload_dependency_count, mut preload_dependency_offset) = (-1, 0);
        if ue4v >= ue4::PRELOAD_DEPENDENCIES_IN_COOKED_EXPORTS {
            preload_dependency_count = r.read_i32()?;
            preload_dependency_offset = r.read_i32()?;
        }

        let mut names_referenced_from_export_data_count = name_count;
        if ue5v >= ue5::NAMES_REFERENCED_FROM_EXPORT_DATA {
            names_referenced_from_export_data_count = r.read_i32()?;
        }

        let mut payload_toc_offset = -1i64;
        if ue5v >= ue5::PAYLOAD_TOC {
            payload_toc_offset = r.read_i64()?;
        }

        let mut data_resource_offset = -1i32;
        if ue5v >= ue5::DATA_RESOURCES {
            data_resource_offset = r.read_i32()?;
        }

        Ok(PackageFileSummary {
            tag,
            legacy_file_version,
            file_version_ue4,
            file_version_ue5,
            file_version_licensee_ue,
            custom_versions,
            saved_hash,
            total_header_size,
            package_name,
            package_flags,
            name_count,
            name_offset,
            soft_object_paths_count,
            soft_object_paths_offset,
            localization_id,
            gatherable_text_data_count,
            gatherable_text_data_offset,
            export_count,
            export_offset,
            import_count,
            import_offset,
            cell_export_count,
            cell_export_offset,
            cell_import_count,
            cell_import_offset,
            metadata_offset,
            depends_offset,
            soft_package_references_count,
            soft_package_references_offset,
            searchable_names_offset,
            thumbnail_table_offset,
            import_type_hierarchies_count,
            import_type_hierarchies_offset,
            engine_version,
            compatible_engine_version,
            compression_flags,
            package_source,
            asset_registry_data_offset,
            bulk_data_start_offset,
            world_tile_info_data_offset,
            preload_dependency_count,
            preload_dependency_offset,
            names_referenced_from_export_data_count,
            payload_toc_offset,
            data_resource_offset,
        })
    }
}
