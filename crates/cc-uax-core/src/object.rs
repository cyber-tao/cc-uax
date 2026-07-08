use crate::reader::{RawName, Reader};
use crate::version::{ue4, ue5};
use anyhow::{Result, bail};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackageIndex(pub i32);

#[cfg(test)]
impl PackageIndex {
    pub fn is_null(&self) -> bool {
        self.0 == 0
    }
    pub fn export_index(&self) -> Option<usize> {
        if self.0 > 0 {
            Some((self.0 - 1) as usize)
        } else {
            None
        }
    }
    pub fn import_index(&self) -> Option<usize> {
        if self.0 < 0 {
            Some((-self.0 - 1) as usize)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone)]
pub struct ObjectImport {
    pub class_package: RawName,
    pub class_name: RawName,
    pub outer_index: PackageIndex,
    pub object_name: RawName,
    pub package_name: Option<RawName>,
}

/// Minimum bytes for one import-table entry before version-gated extras:
/// class_package(8) + class_name(8) + outer_index(4) + object_name(8).
const IMPORT_ENTRY_MIN_BYTES: u64 = 28;

impl ObjectImport {
    pub fn parse_table(
        r: &mut Reader,
        offset: i32,
        count: i32,
        ue4v: i32,
        ue5v: i32,
        filter_editor_only: bool,
    ) -> Result<Vec<ObjectImport>> {
        if count < 0 {
            bail!("import count out of range: {count}");
        }
        if count == 0 {
            return Ok(Vec::new());
        }
        if offset <= 0 {
            bail!("import table offset must be positive when import count is {count}");
        }
        r.seek(offset as u64)?;
        let min_entry_bytes = IMPORT_ENTRY_MIN_BYTES
            + if ue4v >= ue4::NON_OUTER_PACKAGE_IMPORT && !filter_editor_only {
                8
            } else {
                0
            }
            + if ue5v >= ue5::OPTIONAL_RESOURCES {
                4
            } else {
                0
            };
        if (count as u64).saturating_mul(min_entry_bytes) > r.remaining() {
            bail!("import table count out of range: {count}");
        }
        let mut out = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let class_package = r.read_raw_name()?;
            let class_name = r.read_raw_name()?;
            let outer_index = PackageIndex(r.read_i32()?);
            let object_name = r.read_raw_name()?;
            let package_name = if ue4v >= ue4::NON_OUTER_PACKAGE_IMPORT && !filter_editor_only {
                Some(r.read_raw_name()?)
            } else {
                None
            };
            if ue5v >= ue5::OPTIONAL_RESOURCES {
                let _is_optional = r.read_bool32()?;
            }
            out.push(ObjectImport {
                class_package,
                class_name,
                outer_index,
                object_name,
                package_name,
            });
        }
        Ok(out)
    }
}

#[derive(Debug, Clone)]
pub struct ObjectExport {
    pub class_index: PackageIndex,
    pub super_index: PackageIndex,
    pub template_index: PackageIndex,
    pub outer_index: PackageIndex,
    pub object_name: RawName,
    pub object_flags: u32,
    pub serial_size: i64,
    pub serial_offset: i64,
    pub is_asset: bool,
    pub script_serialization_start_offset: i64,
    pub script_serialization_end_offset: i64,
}

impl ObjectExport {
    pub fn parse_table(
        r: &mut Reader,
        offset: i32,
        count: i32,
        ue4v: i32,
        ue5v: i32,
    ) -> Result<Vec<ObjectExport>> {
        if count < 0 {
            bail!("export count out of range: {count}");
        }
        if count == 0 {
            return Ok(Vec::new());
        }
        if offset <= 0 {
            bail!("export table offset must be positive when export count is {count}");
        }
        r.seek(offset as u64)?;
        let min_entry_bytes = export_entry_min_bytes(ue4v, ue5v);
        if (count as u64).saturating_mul(min_entry_bytes) > r.remaining() {
            bail!("export table count out of range: {count}");
        }
        let mut out = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let class_index = PackageIndex(r.read_i32()?);
            let super_index = PackageIndex(r.read_i32()?);
            let template_index = if ue4v >= ue4::TEMPLATEINDEX_IN_COOKED_EXPORTS {
                PackageIndex(r.read_i32()?)
            } else {
                PackageIndex(0)
            };
            let outer_index = PackageIndex(r.read_i32()?);
            let object_name = r.read_raw_name()?;
            let object_flags = r.read_u32()?;
            let (serial_size, serial_offset) = if ue4v >= ue4::SERIALSIZE_64BIT_EXPORTMAP {
                (r.read_i64()?, r.read_i64()?)
            } else {
                (r.read_i32()? as i64, r.read_i32()? as i64)
            };
            let _forced_export = r.read_bool32()?;
            let _not_for_client = r.read_bool32()?;
            let _not_for_server = r.read_bool32()?;
            if ue5v < ue5::REMOVE_OBJECT_EXPORT_PACKAGE_GUID {
                let _package_guid = r.read_guid()?;
            }
            if ue5v >= ue5::TRACK_OBJECT_EXPORT_IS_INHERITED {
                let _is_inherited_instance = r.read_bool32()?;
            }
            let _package_flags = r.read_u32()?;
            if ue4v >= ue4::LOAD_FOR_EDITOR_GAME {
                let _not_always_loaded_for_editor_game = r.read_bool32()?;
            }
            let is_asset = if ue4v >= ue4::COOKED_ASSETS_IN_EDITOR_SUPPORT {
                r.read_bool32()?
            } else {
                false
            };
            if ue5v >= ue5::OPTIONAL_RESOURCES {
                let _generate_public_hash = r.read_bool32()?;
            }
            if ue4v >= ue4::PRELOAD_DEPENDENCIES_IN_COOKED_EXPORTS {
                let _first_export_dependency = r.read_i32()?;
                let _serialization_before_serialization_deps = r.read_i32()?;
                let _create_before_serialization_deps = r.read_i32()?;
                let _serialization_before_create_deps = r.read_i32()?;
                let _create_before_create_deps = r.read_i32()?;
            }
            let (mut ss_start, mut ss_end) = (0i64, 0i64);
            if ue5v >= ue5::SCRIPT_SERIALIZATION_OFFSET {
                ss_start = r.read_i64()?;
                ss_end = r.read_i64()?;
            }
            out.push(ObjectExport {
                class_index,
                super_index,
                template_index,
                outer_index,
                object_name,
                object_flags,
                serial_size,
                serial_offset,
                is_asset,
                script_serialization_start_offset: ss_start,
                script_serialization_end_offset: ss_end,
            });
        }
        Ok(out)
    }
}

fn export_entry_min_bytes(ue4v: i32, ue5v: i32) -> u64 {
    let mut n = 4 + 4 + 4 + 8 + 4 + 12 + 4;
    if ue4v >= ue4::TEMPLATEINDEX_IN_COOKED_EXPORTS {
        n += 4;
    }
    n += if ue4v >= ue4::SERIALSIZE_64BIT_EXPORTMAP {
        16
    } else {
        8
    };
    if ue5v < ue5::REMOVE_OBJECT_EXPORT_PACKAGE_GUID {
        n += 16;
    }
    if ue5v >= ue5::TRACK_OBJECT_EXPORT_IS_INHERITED {
        n += 4;
    }
    if ue4v >= ue4::LOAD_FOR_EDITOR_GAME {
        n += 4;
    }
    if ue4v >= ue4::COOKED_ASSETS_IN_EDITOR_SUPPORT {
        n += 4;
    }
    if ue5v >= ue5::OPTIONAL_RESOURCES {
        n += 4;
    }
    if ue4v >= ue4::PRELOAD_DEPENDENCIES_IN_COOKED_EXPORTS {
        n += 20;
    }
    if ue5v >= ue5::SCRIPT_SERIALIZATION_OFFSET {
        n += 16;
    }
    n
}
