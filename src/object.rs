use crate::reader::{RawName, Reader};
use crate::version::{ue4, ue5};
use anyhow::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackageIndex(pub i32);

impl PackageIndex {
    pub fn is_null(&self) -> bool {
        self.0 == 0
    }
    pub fn is_export(&self) -> bool {
        self.0 > 0
    }
    pub fn is_import(&self) -> bool {
        self.0 < 0
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
    pub is_optional: bool,
}

impl ObjectImport {
    pub fn parse_table(
        r: &mut Reader,
        offset: i32,
        count: i32,
        ue4v: i32,
        ue5v: i32,
        filter_editor_only: bool,
    ) -> Result<Vec<ObjectImport>> {
        let mut out = Vec::with_capacity(count.max(0) as usize);
        if offset <= 0 || count <= 0 {
            return Ok(out);
        }
        r.seek(offset as u64)?;
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
            let is_optional = if ue5v >= ue5::OPTIONAL_RESOURCES {
                r.read_bool32()?
            } else {
                false
            };
            out.push(ObjectImport {
                class_package,
                class_name,
                outer_index,
                object_name,
                package_name,
                is_optional,
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
    pub forced_export: bool,
    pub not_for_client: bool,
    pub not_for_server: bool,
    pub is_inherited_instance: bool,
    pub package_flags: u32,
    pub not_always_loaded_for_editor_game: bool,
    pub is_asset: bool,
    pub generate_public_hash: bool,
    pub first_export_dependency: i32,
    pub serialization_before_serialization_deps: i32,
    pub create_before_serialization_deps: i32,
    pub serialization_before_create_deps: i32,
    pub create_before_create_deps: i32,
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
        let mut out = Vec::with_capacity(count.max(0) as usize);
        if offset <= 0 || count <= 0 {
            return Ok(out);
        }
        r.seek(offset as u64)?;
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
            let forced_export = r.read_bool32()?;
            let not_for_client = r.read_bool32()?;
            let not_for_server = r.read_bool32()?;
            if ue5v < ue5::REMOVE_OBJECT_EXPORT_PACKAGE_GUID {
                let _package_guid = r.read_guid()?;
            }
            let is_inherited_instance = if ue5v >= ue5::TRACK_OBJECT_EXPORT_IS_INHERITED {
                r.read_bool32()?
            } else {
                false
            };
            let package_flags = r.read_u32()?;
            let not_always_loaded_for_editor_game = if ue4v >= ue4::LOAD_FOR_EDITOR_GAME {
                r.read_bool32()?
            } else {
                false
            };
            let is_asset = if ue4v >= ue4::COOKED_ASSETS_IN_EDITOR_SUPPORT {
                r.read_bool32()?
            } else {
                false
            };
            let generate_public_hash = if ue5v >= ue5::OPTIONAL_RESOURCES {
                r.read_bool32()?
            } else {
                false
            };
            let (mut fed, mut bss, mut cbs, mut sbc, mut cbc) = (-1, -1, -1, -1, -1);
            if ue4v >= ue4::PRELOAD_DEPENDENCIES_IN_COOKED_EXPORTS {
                fed = r.read_i32()?;
                bss = r.read_i32()?;
                cbs = r.read_i32()?;
                sbc = r.read_i32()?;
                cbc = r.read_i32()?;
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
                forced_export,
                not_for_client,
                not_for_server,
                is_inherited_instance,
                package_flags,
                not_always_loaded_for_editor_game,
                is_asset,
                generate_public_hash,
                first_export_dependency: fed,
                serialization_before_serialization_deps: bss,
                create_before_serialization_deps: cbs,
                serialization_before_create_deps: sbc,
                create_before_create_deps: cbc,
                script_serialization_start_offset: ss_start,
                script_serialization_end_offset: ss_end,
            });
        }
        Ok(out)
    }
}
