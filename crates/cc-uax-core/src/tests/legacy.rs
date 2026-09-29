use super::common::*;
use crate::reader::{Guid, Reader};
use crate::summary::{PackageFileSummary, SummaryScope};
use crate::{PackageView, read_legacy_package_references};

const PKG_FILTER_EDITOR_ONLY: u32 = 0x8000_0000;
const PKG_COOKED: u32 = 0x0000_0200;

/// One import row by name-table index: class package, class name, outer, object name.
type ImportRow = (i32, i32, i32, i32);

struct Ue4Package<'a> {
    legacy_file_version: i32,
    file_version_ue4: i32,
    package_flags: u32,
    names: &'a [&'a str],
    imports: &'a [ImportRow],
    /// Soft package references: FNames appended to the name table from 514, FStrings
    /// before it.
    soft: &'a [&'a str],
}

fn ue4_package(spec: &Ue4Package) -> Vec<u8> {
    let ue4 = spec.file_version_ue4;
    let mut all_names: Vec<&str> = spec.names.to_vec();
    let soft_as_names = ue4 >= crate::version::ue4::ADDED_SOFT_OBJECT_PATH;
    let soft_name_base = all_names.len();
    if soft_as_names {
        all_names.extend_from_slice(spec.soft);
    }
    let has_hashes = ue4 >= crate::version::ue4::NAME_HASHES_SERIALIZED;

    let mut tables = Vec::new();
    let names_at = |header_len: usize, tables: &Vec<u8>| (header_len + tables.len()) as i32;
    let header = |name_offset: i32, import_offset: i32, soft_offset: i32| {
        let mut d = Vec::new();
        push_u32(&mut d, 0x9E2A_83C1);
        push_i32(&mut d, spec.legacy_file_version);
        if spec.legacy_file_version != -4 {
            push_i32(&mut d, 0); // legacy ue3 version
        }
        push_i32(&mut d, ue4);
        push_i32(&mut d, 0); // licensee
        // Two custom versions in the layout the legacy version selects.
        push_i32(&mut d, 2);
        for (tag, version) in [(9u32, 2i32), (11, 5)] {
            match spec.legacy_file_version {
                -2 => {
                    push_u32(&mut d, tag);
                    push_i32(&mut d, version);
                }
                -5..=-3 => {
                    push_guid(&mut d, 1, 2, 3, tag);
                    push_i32(&mut d, version);
                    push_fstring(&mut d, "Dev-Version");
                }
                _ => {
                    push_guid(&mut d, 1, 2, 3, tag);
                    push_i32(&mut d, version);
                }
            }
        }
        push_i32(&mut d, 0); // total_header_size
        push_fstring(&mut d, "/Game/Legacy");
        push_u32(&mut d, spec.package_flags);
        push_i32(&mut d, all_names.len() as i32);
        push_i32(&mut d, name_offset);
        if ue4 >= crate::version::ue4::SERIALIZE_TEXT_IN_PACKAGES {
            push_i32(&mut d, 0);
            push_i32(&mut d, 0);
        }
        push_i32(&mut d, 0); // export_count
        push_i32(&mut d, 0); // export_offset
        push_i32(&mut d, spec.imports.len() as i32);
        push_i32(&mut d, import_offset);
        push_i32(&mut d, 0); // depends_offset
        if ue4 >= crate::version::ue4::ADD_STRING_ASSET_REFERENCES_MAP {
            push_i32(&mut d, spec.soft.len() as i32);
            push_i32(&mut d, soft_offset);
        }
        if ue4 >= crate::version::ue4::ADDED_SEARCHABLE_NAMES {
            push_i32(&mut d, 0);
        }
        push_i32(&mut d, 0); // thumbnail_table_offset
        push_guid(&mut d, 0, 0, 0, 0); // legacy guid
        push_i32(&mut d, 0); // generation_count
        if ue4 >= crate::version::ue4::ENGINE_VERSION_OBJECT {
            push_u16(&mut d, 4);
            push_u16(&mut d, 27);
            push_u16(&mut d, 0);
            push_u32(&mut d, 0);
            push_fstring(&mut d, "");
        } else {
            push_i32(&mut d, 0);
        }
        if ue4 >= crate::version::ue4::PACKAGE_SUMMARY_HAS_COMPATIBLE_ENGINE_VERSION {
            push_u16(&mut d, 4);
            push_u16(&mut d, 27);
            push_u16(&mut d, 0);
            push_u32(&mut d, 0);
            push_fstring(&mut d, "");
        }
        push_u32(&mut d, 0); // compression_flags
        push_i32(&mut d, 0); // compressed chunks
        push_u32(&mut d, 0); // package_source
        push_i32(&mut d, 0); // additional packages to cook
        if spec.legacy_file_version > -7 {
            push_i32(&mut d, 0); // num_texture_allocations
        }
        push_i32(&mut d, 0); // asset_registry_data_offset
        push_i64(&mut d, 0); // bulk_data_start_offset
        if ue4 >= crate::version::ue4::WORLD_LEVEL_INFO {
            push_i32(&mut d, 0);
        }
        if ue4 >= crate::version::ue4::CHANGED_CHUNKID_TO_BE_AN_ARRAY_OF_CHUNKIDS {
            push_i32(&mut d, 0); // chunk id count
        }
        if ue4 >= crate::version::ue4::PRELOAD_DEPENDENCIES_IN_COOKED_EXPORTS {
            push_i32(&mut d, 0);
            push_i32(&mut d, 0);
        }
        d
    };
    let header_len = header(0, 0, 0).len();

    let name_offset = names_at(header_len, &tables);
    for name in &all_names {
        push_fstring(&mut tables, name);
        if has_hashes {
            push_u32(&mut tables, 0);
        }
    }
    let import_offset = names_at(header_len, &tables);
    for &(class_package, class_name, outer, object_name) in spec.imports {
        push_raw_name(&mut tables, class_package);
        push_raw_name(&mut tables, class_name);
        push_i32(&mut tables, outer);
        push_raw_name(&mut tables, object_name);
    }
    let soft_offset = names_at(header_len, &tables);
    for (index, reference) in spec.soft.iter().enumerate() {
        if soft_as_names {
            push_raw_name(&mut tables, (soft_name_base + index) as i32);
        } else {
            push_fstring(&mut tables, reference);
        }
    }

    let mut data = header(name_offset, import_offset, soft_offset);
    data.extend_from_slice(&tables);
    data
}

/// `/Script/Engine` and `/Game/Foo` as `Package` imports with one object import
/// under the second, which is not a package reference.
const NAMES: [&str; 6] = [
    "/Script/CoreUObject", // 0
    "Package",             // 1
    "/Script/Engine",      // 2
    "/Game/Foo",           // 3
    "Texture2D",           // 4
    "Foo",                 // 5
];
const IMPORTS: [ImportRow; 3] = [(0, 1, 0, 2), (0, 1, 0, 3), (2, 4, -2, 5)];

fn spec<'a>(
    legacy_file_version: i32,
    file_version_ue4: i32,
    soft: &'a [&'a str],
) -> Ue4Package<'a> {
    Ue4Package {
        legacy_file_version,
        file_version_ue4,
        package_flags: PKG_FILTER_EDITOR_ONLY,
        names: &NAMES,
        imports: &IMPORTS,
        soft,
    }
}

#[test]
fn ue4_package_reference_tables_are_read_with_fname_soft_references() {
    let data = ue4_package(&spec(-7, 522, &["/Game/Soft/Bar"]));

    let legacy = read_legacy_package_references(&data).unwrap();

    assert_eq!(legacy.file_version_ue4, 522);
    assert_eq!(legacy.references.assets, ["/Game/Foo"]);
    assert_eq!(legacy.references.scripts, ["/Script/Engine"]);
    assert_eq!(legacy.references.soft, ["/Game/Soft/Bar"]);
}

#[test]
fn old_ue4_packages_use_guid_custom_versions_and_fstring_soft_references() {
    // 459: no name hashes (< 504), FString soft references (< 514), and object paths
    // reduced to package names (< 484).
    let data = ue4_package(&spec(
        -5,
        459,
        &["/Game/Soft/Bar.Bar", "Texture2D'/Game/Old/Tex.Tex'"],
    ));
    let legacy = read_legacy_package_references(&data).unwrap();
    assert_eq!(legacy.file_version_ue4, 459);
    assert_eq!(legacy.references.assets, ["/Game/Foo"]);
    assert_eq!(legacy.references.scripts, ["/Script/Engine"]);
    assert_eq!(legacy.references.soft, ["/Game/Old/Tex", "/Game/Soft/Bar"]);

    // From 484 the FString already holds only a package name.
    let data = ue4_package(&spec(-5, 490, &["/Game/Soft/Pkg"]));
    let legacy = read_legacy_package_references(&data).unwrap();
    assert_eq!(legacy.references.soft, ["/Game/Soft/Pkg"]);
}

#[test]
fn enum_custom_versions_parse_and_invent_their_guid_from_the_tag() {
    let data = ue4_package(&spec(-2, 522, &[]));
    assert!(read_legacy_package_references(&data).is_ok());

    let summary =
        PackageFileSummary::parse_scoped(&mut Reader::new(&data), SummaryScope::LegacyReferences)
            .unwrap();
    assert_eq!(summary.custom_version(Guid([0, 0, 0, 9])), Some(2));
    assert_eq!(summary.custom_version(Guid([0, 0, 0, 11])), Some(5));
}

#[test]
fn guid_custom_versions_skip_their_friendly_names() {
    let data = ue4_package(&spec(-4, 522, &[]));
    let summary =
        PackageFileSummary::parse_scoped(&mut Reader::new(&data), SummaryScope::LegacyReferences)
            .unwrap();
    assert_eq!(summary.custom_version(Guid([1, 2, 3, 11])), Some(5));
    assert!(read_legacy_package_references(&data).is_ok());
}

#[test]
fn the_legacy_reader_rejects_ue5_packages_and_analysis_parsing_is_unchanged() {
    let data = build_minimal_package();

    let error = read_legacy_package_references(&data).unwrap_err();
    assert!(error.is_out_of_scope(), "{error}");
    assert!(error.to_string().contains("not a UE4-format package"));

    assert!(PackageView::parse(&data).is_ok());
    let ue4 = ue4_package(&spec(-7, 522, &[]));
    assert!(
        PackageView::parse(&ue4)
            .err()
            .expect("a UE4 package is not analysed")
            .is_out_of_scope()
    );
}

#[test]
fn packages_older_than_the_oldest_loadable_version_are_out_of_scope() {
    let data = ue4_package(&spec(-7, 213, &[]));

    let error = read_legacy_package_references(&data).unwrap_err();

    assert!(error.is_out_of_scope(), "{error}");
}

#[test]
fn cooked_ue4_packages_are_out_of_scope_for_the_legacy_reader() {
    let mut cooked = spec(-7, 522, &[]);
    cooked.package_flags = PKG_FILTER_EDITOR_ONLY | PKG_COOKED;
    let data = ue4_package(&cooked);

    let error = read_legacy_package_references(&data).unwrap_err();

    assert!(error.is_out_of_scope(), "{error}");
    assert!(error.to_string().contains("PKG_Cooked"), "{error}");
}
