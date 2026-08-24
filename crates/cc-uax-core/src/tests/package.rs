use super::common::*;
use crate::analysis::analyze_package;
use crate::name::NameMap;
use crate::reader::Reader;
use crate::{
    AnalysisDiagnostic, AnalysisStatus, AssetView, CapabilityKind, DiagnosticSeverity,
    KnownOpaqueKind, Package, PackageRejection, PackageView, PropertyDecodeStatus,
};

fn diagnostic_with_code<'a>(
    diagnostics: &'a [AnalysisDiagnostic],
    code: &str,
) -> &'a AnalysisDiagnostic {
    diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == code)
        .unwrap_or_else(|| panic!("missing diagnostic code {code}: {diagnostics:#?}"))
}

#[test]
fn package_rejects_pre_ue5_version() {
    let mut data = Vec::new();
    push_u32(&mut data, 0x9E2A_83C1);
    push_i32(&mut data, -8);
    push_i32(&mut data, 0);
    push_i32(&mut data, 522);
    push_i32(&mut data, 999);
    push_i32(&mut data, 0);

    let error = Package::parse(&data).err().unwrap().to_string();
    assert!(error.contains("FileVersionUE5=999"));
}

/// A header that stops right after the version fields, so `PackageView::parse`
/// rejects it on the version check rather than on a later table read.
fn version_only_header(legacy: i32, ue4: i32, ue5: i32, licensee: i32) -> Vec<u8> {
    let mut data = Vec::new();
    push_u32(&mut data, 0x9E2A_83C1);
    push_i32(&mut data, legacy);
    push_i32(&mut data, 0);
    push_i32(&mut data, ue4);
    if legacy <= -8 {
        push_i32(&mut data, ue5);
    }
    push_i32(&mut data, licensee);
    data
}

// A package this tool deliberately does not target is `OutOfScope`: callers that
// scan whole projects must be able to record it as `unsupported` evidence instead
// of a scan failure. Bytes that are not a readable package stay `Malformed`.
#[test]
fn out_of_scope_packages_are_classified_apart_from_malformed_ones() {
    let out_of_scope: [(&str, Vec<u8>); 5] = [
        // UE4 package: FileVersionUE5 is 0 but the UE4 version is set.
        ("ue4 package", version_only_header(-7, 522, 0, 0)),
        // Cooked/unversioned package: every version field is 0.
        ("unversioned", version_only_header(-8, 0, 0, 0)),
        // UE3 package: LegacyFileVersion is not negative.
        ("ue3 package", version_only_header(1, 0, 0, 0)),
        // FileVersionUE5 below the supported floor but non-zero.
        ("below floor", version_only_header(-8, 522, 999, 0)),
        // Above the highest known layout: UE refuses to read a package whose file
        // version is too new, because every later field may have changed.
        (
            "above ceiling",
            version_only_header(-8, 522, crate::version::ue5::HIGHEST + 1, 0),
        ),
    ];
    for (label, data) in out_of_scope {
        let error = PackageView::parse(&data)
            .err()
            .unwrap_or_else(|| panic!("{label} should be rejected"));
        assert_eq!(
            error.rejection(),
            PackageRejection::OutOfScope,
            "{label} should be out of scope, got: {error}"
        );
        assert!(error.is_out_of_scope(), "{label}");
    }

    // Wrong package magic is not a package at all.
    let mut wrong_magic = Vec::new();
    push_u32(&mut wrong_magic, 0x1234_5678);
    push_i32(&mut wrong_magic, -8);
    let error = PackageView::parse(&wrong_magic)
        .err()
        .expect("wrong magic should be rejected");
    assert_eq!(error.rejection(), PackageRejection::Malformed);
    assert!(!error.is_out_of_scope());

    // A truncated but otherwise in-scope header is malformed, not out of scope:
    // the version fields pass and the name table read runs off the end.
    let mut truncated = build_minimal_package();
    truncated.truncate(40);
    let error = PackageView::parse(&truncated)
        .err()
        .expect("truncated package should be rejected");
    assert_eq!(
        error.rejection(),
        PackageRejection::Malformed,
        "truncated package should be malformed, got: {error}"
    );
}

// Big-endian console packages and package-level compression are readable formats
// this tool does not target, so they must not look like corruption either.
#[test]
fn swapped_byte_order_is_out_of_scope() {
    let mut data = Vec::new();
    push_u32(&mut data, 0xC183_2A9E);
    let error = PackageView::parse(&data)
        .err()
        .expect("swapped tag should be rejected");
    assert_eq!(error.rejection(), PackageRejection::OutOfScope);
}

// `CompressedChunks` is a TArray, so a negative count is not an empty list.
// Treating it as empty would read PackageSource and every later summary field
// from the wrong offset, surfacing later as an unrelated malformed-table error.
// A positive count stays out of scope: the payload really is compressed.
#[test]
fn compressed_chunk_counts_are_classified_by_sign() {
    let negative = build_minimal_package_with_compressed_chunks(-1);
    let error = PackageView::parse(&negative)
        .err()
        .expect("negative chunk count should be rejected");
    assert!(
        error.to_string().contains("CompressedChunks count"),
        "unexpected error: {error}"
    );
    assert_eq!(error.rejection(), PackageRejection::Malformed);

    let positive = build_minimal_package_with_compressed_chunks(1);
    let error = PackageView::parse(&positive)
        .err()
        .expect("compressed package should be rejected");
    assert_eq!(error.rejection(), PackageRejection::OutOfScope);
    assert!(error.to_string().contains("package-level compression"));
}

#[test]
fn package_accepts_ue50_file_versions() {
    // FileVersionUE5 1000 is INITIAL_VERSION; 1007 is the last UE5.0 AUTOMATIC value.
    for (fv, major, minor) in [(1000, 5, 0), (1004, 5, 0), (1007, 5, 0)] {
        let data = build_minimal_package_with_version(fv, major, minor);
        let package = Package::parse(&data)
            .unwrap_or_else(|err| panic!("UE5.0 FileVersionUE5 {fv} failed to parse: {err:#}"));
        assert_eq!(package.summary.file_version_ue5, fv);
    }
}

// Every FileVersionUE5 in the supported range must parse, including the ones no
// real corpus asset happens to carry. The reference corpora cover 1004, 1007-1009
// and 1013-1018; without this the summary field order for 1000-1003, 1010-1012 is
// asserted by nothing at all, and 1010 (script offsets), 1011 (tag extensions) and
// 1012 (complete type names) are three of the most consequential gates there are.
#[test]
fn every_supported_file_version_parses() {
    use crate::version::{SUPPORTED_FILE_VERSION_FLOOR, ue5};

    for fv in SUPPORTED_FILE_VERSION_FLOOR..=ue5::HIGHEST {
        // Engine version only has to be plausible; the summary layout is gated on
        // FileVersionUE5 except for the import PackageName field, covered elsewhere.
        let filtered = build_minimal_package_with_version(fv, 5, 8);
        let package = Package::parse(&filtered)
            .unwrap_or_else(|err| panic!("FileVersionUE5 {fv} failed to parse: {err:#}"));
        assert_eq!(package.summary.file_version_ue5, fv);

        // Editor packages are not FilterEditorOnly, which adds the localization id
        // and PersistentGuid to the same header.
        let editor = build_minimal_editor_package_with_version(fv, 5, 8);
        let package = Package::parse(&editor).unwrap_or_else(|err| {
            panic!("unfiltered FileVersionUE5 {fv} failed to parse: {err:#}")
        });
        assert_eq!(package.summary.file_version_ue5, fv);
    }
}

// Threshold and threshold-1 for the gates that change the summary layout. A field
// read on the wrong side of one of these shifts every later offset, so each pair
// has to disagree about the header size in the expected direction.
#[test]
fn summary_layout_gates_change_the_header_at_their_threshold() {
    use crate::version::ue5;

    for (threshold, label) in [
        (ue5::NAMES_REFERENCED_FROM_EXPORT_DATA, "names referenced"),
        (ue5::PAYLOAD_TOC, "payload toc"),
        (ue5::DATA_RESOURCES, "data resources"),
        (ue5::ADD_SOFTOBJECTPATH_LIST, "soft object path list"),
        (ue5::METADATA_SERIALIZATION_OFFSET, "metadata offset"),
        (ue5::VERSE_CELLS, "verse cells"),
        (ue5::PACKAGE_SAVED_HASH, "saved hash"),
        (ue5::IMPORT_TYPE_HIERARCHIES, "import type hierarchies"),
    ] {
        let below = build_minimal_package_with_version(threshold - 1, 5, 8);
        let at = build_minimal_package_with_version(threshold, 5, 8);
        assert!(
            at.len() > below.len(),
            "{label} gate at {threshold} did not add header fields"
        );
        for (data, fv) in [(&below, threshold - 1), (&at, threshold)] {
            let package = Package::parse(data)
                .unwrap_or_else(|err| panic!("{label} at {fv} failed to parse: {err:#}"));
            assert_eq!(package.summary.file_version_ue5, fv);
        }
    }
}

#[test]
fn unfiltered_editor_package_parses_localization_and_persistent_guid() {
    for (fv, major, minor, legacy) in [(1008, 5, 1, -8), (1017, 5, 6, -9), (1018, 5, 8, -9)] {
        let data = build_minimal_editor_package_with_version(fv, major, minor);
        let package = Package::parse(&data).unwrap_or_else(|err| {
            panic!("unfiltered FileVersionUE5 {fv} failed to parse: {err:#}")
        });
        assert_eq!(package.summary.file_version_ue5, fv);
        assert_eq!(package.summary.legacy_file_version, legacy);
        assert!(!package.summary.filter_editor_only());
    }
}

#[test]
fn name_map_rejects_negative_count() {
    let data = [];
    let mut reader = Reader::new(&data);

    let error = NameMap::parse(&mut reader, 0, -1, 522)
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("name table count out of range"));
}

#[test]
fn package_view_analyzes_the_bound_minimal_package() {
    let data = build_minimal_package();
    let view = PackageView::parse(&data).expect("minimal package should parse");
    let analysis = view.analyze(AssetView::Full);

    assert_eq!(analysis.summary.file_version_ue4, 522);
    assert_eq!(analysis.summary.file_version_ue5, 1018);
    assert_eq!(analysis.summary.package_name, "TestPkg");
    assert_eq!(analysis.summary.export_count, 0);
    assert!(analysis.imports.is_empty());
    assert!(analysis.exports.is_empty());
}

#[test]
fn package_view_parses_ue56_file_version_1017() {
    let data = build_minimal_package_with_version(1017, 5, 6);
    let view = PackageView::parse(&data).expect("UE5.6 package should parse");
    let analysis = view.analyze(AssetView::Full);

    assert_eq!(analysis.summary.file_version_ue5, 1017);
    assert_eq!(analysis.summary.package_name, "TestPkg");
    assert_eq!(analysis.summary.export_count, 0);
    assert!(analysis.imports.is_empty());
    assert!(analysis.exports.is_empty());
}

#[test]
fn minimal_package_parses_across_supported_ue5_versions() {
    // The version-aware summary builder must emit a header the parser accepts for
    // every supported FileVersionUE5, from UE5.0 (1000) through UE5.8 (1018). Each
    // version gates a different set of summary fields, so a layout bug for any one
    // of them surfaces here as a parse failure.
    // 1010 (script offsets) and 1012 (complete tag type name) are the two gates
    // with the most branches behind them, and 1010/1011/1012 have no real-corpus
    // asset, so keep each of them in the matrix explicitly.
    for (fv, major, minor) in [
        (1000, 5, 0),
        (1004, 5, 0),
        (1007, 5, 0),
        (1008, 5, 1),
        (1009, 5, 2),
        (1010, 5, 3),
        (1011, 5, 3),
        (1012, 5, 4),
        (1013, 5, 5),
        (1014, 5, 5),
        (1015, 5, 5),
        (1016, 5, 6),
        (1017, 5, 6),
        (1018, 5, 7),
        (1018, 5, 8),
    ] {
        let data = build_minimal_package_with_version(fv, major, minor);
        let package = Package::parse(&data).unwrap_or_else(|err| {
            panic!("FileVersionUE5 {fv} (UE{major}.{minor}) failed to parse: {err:#}")
        });
        assert_eq!(package.summary.file_version_ue5, fv);
    }
}

// SCRIPT_SERIALIZATION_OFFSET (1010) adds the two int64 tagged-property offsets to
// every export map entry. No corpus asset sits on either side of this gate, and the
// offsets decide the export window shape, so pin the layout at threshold-1 and
// threshold and confirm a stray value is rejected rather than mis-sliced.
#[test]
fn export_map_script_offsets_appear_exactly_at_their_version_gate() {
    use crate::object::ObjectExport;

    let ue4 = crate::version::ue4::HIGHEST;
    let gate = crate::version::ue5::SCRIPT_SERIALIZATION_OFFSET;

    let entry = |ue5v: i32, script: Option<(i64, i64)>| {
        // parse_table requires a positive table offset, so pad the front.
        let mut table = vec![0u8; 4];
        let start = table.len();
        push_i32(&mut table, 0); // class_index
        push_i32(&mut table, 0); // super_index
        push_i32(&mut table, 0); // template_index
        push_i32(&mut table, 0); // outer_index
        push_raw_name(&mut table, 0); // object_name
        push_u32(&mut table, 0); // object_flags
        push_i64(&mut table, 64); // serial_size
        push_i64(&mut table, 1024); // serial_offset
        for _ in 0..3 {
            push_i32(&mut table, 0); // forced/not-for-client/not-for-server
        }
        push_i32(&mut table, 0); // bIsInheritedInstance (>= 1006)
        push_u32(&mut table, 0); // package_flags
        push_i32(&mut table, 0); // not_always_loaded_for_editor_game
        push_i32(&mut table, 0); // is_asset
        push_i32(&mut table, 0); // bGeneratePublicHash (>= 1003)
        for _ in 0..5 {
            push_i32(&mut table, 0); // preload dependency counts
        }
        if let Some((script_start, script_end)) = script {
            push_i64(&mut table, script_start);
            push_i64(&mut table, script_end);
        }
        let mut reader = Reader::new(&table);
        let parsed = ObjectExport::parse_table(&mut reader, start as i32, 1, ue4, ue5v);
        (parsed, table.len(), reader.pos())
    };

    // At threshold-1 the offsets are absent and the entry ends without them.
    let (parsed, len, pos) = entry(gate - 1, None);
    let exports = parsed.expect("a pre-gate entry parses without script offsets");
    assert_eq!(exports[0].script_serialization_start_offset, 0);
    assert_eq!(exports[0].script_serialization_end_offset, 0);
    assert_eq!(pos, len as u64, "pre-gate entry must consume exactly");

    // At the threshold both offsets are read, and a non-zero range is preserved so
    // the export window can bracket the tagged-property block.
    let (parsed, len, pos) = entry(gate, Some((8, 40)));
    let exports = parsed.expect("a gated entry parses with script offsets");
    assert_eq!(exports[0].script_serialization_start_offset, 8);
    assert_eq!(exports[0].script_serialization_end_offset, 40);
    assert_eq!(pos, len as u64, "gated entry must consume exactly");

    // A pre-gate byte layout read as if it were gated runs off the end rather than
    // silently reinterpreting neighbouring bytes as offsets.
    let (parsed, _, _) = entry(gate, None);
    assert!(
        parsed.is_err(),
        "reading absent script offsets must fail, not invent a window"
    );
}

// PROPERTY_TAG_COMPLETE_TYPE_NAME (1012) switches FPropertyTag from the legacy
// FName type plus type-specific extras to a TypeName tree with a flag byte. The
// same bytes must decode under exactly one layout, and no corpus asset sits on
// either side of this gate.
#[test]
fn property_tag_layout_switches_exactly_at_its_version_gate() {
    use crate::name::NameMap;
    use crate::pin::PinSerCtx;
    use crate::property::{ParseCtx, parse_properties_report};

    let names = NameMap {
        names: vec![
            "Count".to_string(),       // 0
            "IntProperty".to_string(), // 1
            "None".to_string(),        // 2
        ],
    };
    let gate = crate::version::ue5::PROPERTY_TAG_COMPLETE_TYPE_NAME;

    // Modern layout: name, TypeName tree (name + param count), size, flags, value.
    let mut modern = Vec::new();
    push_raw_name(&mut modern, 0);
    push_raw_name(&mut modern, 1);
    push_i32(&mut modern, 0); // inner type param count
    push_i32(&mut modern, 4); // size
    modern.push(0); // flags
    push_i32(&mut modern, 7);
    push_raw_name(&mut modern, 2);

    // Legacy layout: name, type FName, size, array index, HasPropertyGuid, then at
    // 1011 the property-extension flags, then the value.
    let mut legacy = Vec::new();
    push_raw_name(&mut legacy, 0);
    push_raw_name(&mut legacy, 1);
    push_i32(&mut legacy, 4); // size
    push_i32(&mut legacy, 0); // array index
    legacy.push(0); // HasPropertyGuid (uint8, not bool32)
    legacy.push(0); // EPropertyTagExtension flags (>= 1011)
    push_i32(&mut legacy, 7);
    push_raw_name(&mut legacy, 2);

    let decode = |bytes: &[u8], file_version_ue5: i32| {
        let ctx = ParseCtx {
            names: &names,
            resolve_object: &|_idx: i32| crate::DecodedValue::Null,
            pins: PinSerCtx::default(),
            soft_object_paths: &[],
            serialization: crate::version::SerializationPolicy::default(),
            file_version_ue4: crate::version::ue4::HIGHEST,
            file_version_ue5,
        };
        let mut reader = Reader::new(bytes);
        let parse = parse_properties_report(&mut reader, &ctx, bytes.len() as u64, "/properties");
        (parse, reader.pos())
    };

    for (label, bytes, version) in [
        ("modern at the gate", &modern, gate),
        ("legacy below the gate", &legacy, gate - 1),
    ] {
        let (parse, pos) = decode(bytes, version);
        assert_eq!(parse.entries.len(), 1, "{label}: {:#?}", parse.entries);
        assert_eq!(parse.entries[0].name, "Count", "{label}");
        assert_eq!(parse.entries[0].value.as_i64(), Some(7), "{label}");
        assert!(
            parse.diagnostics.is_empty(),
            "{label}: {:#?}",
            parse.diagnostics
        );
        assert_eq!(pos, bytes.len() as u64, "{label} must consume exactly");
    }

    // Each layout read under the other version must not silently produce a clean
    // decode: the tag fields do not line up.
    for (label, bytes, version) in [
        ("legacy bytes at the gate", &legacy, gate),
        ("modern bytes below the gate", &modern, gate - 1),
    ] {
        let (parse, _) = decode(bytes, version);
        let clean = parse.entries.len() == 1
            && parse.entries[0].name == "Count"
            && parse.entries[0].value.as_i64() == Some(7)
            && parse.diagnostics.is_empty();
        assert!(
            !clean,
            "{label} must not decode cleanly: {:#?}",
            parse.entries
        );
    }
}

#[test]
fn import_package_name_gate_follows_engine_version_for_filtered_packages() {
    // UE5.6/5.7 omit FObjectImport::PackageName for FilterEditorOnly packages, but UE5.8
    // always writes it, and both share FileVersionUE5 = 1018. The engine version is the
    // only signal separating the layouts, so a 5.8 filtered package must read PackageName
    // while a 5.7 filtered package must not.
    use crate::object::ObjectImport;
    use crate::summary::EngineVersion;

    let ue4 = crate::version::ue4::HIGHEST;
    let ue5 = crate::version::ue5::IMPORT_TYPE_HIERARCHIES; // 1018

    let entry = |with_package_name: bool| {
        // parse_table requires a positive table offset, so pad the front and point at it.
        let mut table = vec![0u8; 4];
        let start = table.len();
        push_raw_name(&mut table, 0); // class_package
        push_raw_name(&mut table, 1); // class_name
        push_i32(&mut table, 0); // outer_index
        push_raw_name(&mut table, 2); // object_name
        if with_package_name {
            push_raw_name(&mut table, 3); // package_name
        }
        push_i32(&mut table, 0); // is_optional (bool32, OPTIONAL_RESOURCES)
        (table, start as i32)
    };

    // UE5.8 filtered package: bytes carry PackageName, and it must be read.
    let (table, offset) = entry(true);
    let mut r = Reader::new(&table);
    let engine_58 = EngineVersion {
        major: 5,
        minor: 8,
        ..Default::default()
    };
    let imports = ObjectImport::parse_table(&mut r, offset, 1, ue4, ue5, true, &engine_58)
        .expect("5.8 filtered import table parses");
    assert_eq!(imports[0].package_name.as_ref().map(|n| n.index), Some(3));
    assert_eq!(
        r.pos(),
        table.len() as u64,
        "5.8 entry must consume PackageName"
    );

    // UE5.7 filtered package: bytes omit PackageName, and none is read.
    let (table, offset) = entry(false);
    let mut r = Reader::new(&table);
    let engine_57 = EngineVersion {
        major: 5,
        minor: 7,
        ..Default::default()
    };
    let imports = ObjectImport::parse_table(&mut r, offset, 1, ue4, ue5, true, &engine_57)
        .expect("5.7 filtered import table parses");
    assert!(imports[0].package_name.is_none());
    assert_eq!(
        r.pos(),
        table.len() as u64,
        "5.7 entry must not read PackageName"
    );
}

#[test]
fn soft_object_path_table_error_is_structured() {
    let mut data = build_minimal_package();
    put_i32(&mut data, 76, 1);
    put_i32(&mut data, 80, 999_999);

    let package = Package::parse(&data).unwrap();
    assert!(
        package
            .soft_object_path_error
            .as_deref()
            .unwrap()
            .contains("soft object path table seek failed")
    );

    let analysis = analyze_package(&package, &data, AssetView::References);
    let diagnostic = diagnostic_with_code(&analysis.diagnostics, "soft_object_path_table_error");
    assert_eq!(diagnostic.severity, DiagnosticSeverity::Warning);
    assert_eq!(diagnostic.path, "/summary/soft_object_paths");
    assert!(
        diagnostic
            .message
            .contains("soft object path table seek failed")
    );
}

// Every header table rejects a declared count that cannot fit in the remaining
// file before allocating for it. The soft-object-path table is no exception: a
// package must not be able to make the parser reserve for a count it cannot back.
#[test]
fn soft_object_path_count_beyond_the_file_is_rejected_before_reading() {
    // A FileVersionUE5 = 1018 entry is two FNames plus an FString length: 20 bytes.
    const ENTRY_BYTES: usize = 20;
    let base = build_minimal_package();
    // Point the table at the very end of the file so `remaining` is a known value.
    let table_offset = base.len();

    for (label, count, extra_bytes, should_fail) in [
        (
            "count needs one byte more than the file has",
            2,
            ENTRY_BYTES * 2 - 1,
            true,
        ),
        ("count fits exactly", 2, ENTRY_BYTES * 2, false),
    ] {
        let mut data = base.clone();
        data.resize(table_offset + extra_bytes, 0);
        put_i32(&mut data, 76, count);
        put_i32(&mut data, 80, table_offset as i32);

        let package = Package::parse(&data).unwrap();
        let error = package.soft_object_path_error.as_deref();
        if should_fail {
            let error = error.unwrap_or_else(|| panic!("{label}: expected a structured error"));
            assert!(
                error.contains("soft object path table count out of range"),
                "{label}: {error}"
            );
            assert!(
                package.soft_object_paths.is_empty(),
                "{label}: nothing should be decoded"
            );
        } else {
            assert!(error.is_none(), "{label}: {error:?}");
            assert_eq!(package.soft_object_paths.len(), count as usize, "{label}");
        }
    }
}

#[test]
fn invalid_script_window_is_structured() {
    let base = Package::parse(&build_minimal_package()).unwrap();
    let package = Package {
        summary: base.summary,
        names: NameMap {
            names: vec!["Obj".to_string()],
        },
        imports: Vec::new(),
        exports: vec![test_export(0, 4, 0, 8)],
        soft_object_paths: Vec::new(),
        soft_object_path_error: None,
        soft_package_references: Vec::new(),
        soft_package_reference_error: None,
    };

    let analysis = analyze_package(&package, &[0; 4], AssetView::Properties);
    let diagnostic = diagnostic_with_code(&analysis.diagnostics, "serial_window_invalid");
    assert_eq!(diagnostic.severity, DiagnosticSeverity::Error);
    assert_eq!(diagnostic.path, "/exports/0");
    assert!(diagnostic.message.contains("outside serial size"));
    assert!(analysis.exports[0].properties.is_empty());
}

#[test]
fn zero_script_window_uses_serial_range() {
    let base = Package::parse(&build_minimal_package()).unwrap();
    let mut data = Vec::new();
    data.push(0);
    push_raw_name(&mut data, 1);
    push_raw_name(&mut data, 2);
    push_i32(&mut data, 0);
    push_i32(&mut data, 4);
    data.push(0);
    push_i32(&mut data, 42);
    push_raw_name(&mut data, 3);

    let package = Package {
        summary: base.summary,
        names: NameMap {
            names: vec![
                "Obj".to_string(),
                "Value".to_string(),
                "IntProperty".to_string(),
                "None".to_string(),
            ],
        },
        imports: Vec::new(),
        exports: vec![test_export(0, data.len() as i64, 0, 0)],
        soft_object_paths: Vec::new(),
        soft_object_path_error: None,
        soft_package_references: Vec::new(),
        soft_package_reference_error: None,
    };

    let analysis = analyze_package(&package, &data, AssetView::Properties);
    let properties = &analysis.exports[0].properties;
    assert_eq!(properties.len(), 1);
    assert_eq!(properties[0].name, "Value");
    assert_eq!(properties[0].value.as_i64(), Some(42));
}

#[test]
fn pre_complete_typename_version_decodes_legacy_properties() {
    let mut base = Package::parse(&build_minimal_package()).unwrap();
    base.summary.file_version_ue5 = 1011;
    let mut data = Vec::new();
    data.push(0);
    push_legacy_tag_header(&mut data, 1, 2, 4);
    push_legacy_tag_tail(&mut data, 1011);
    push_i32(&mut data, 123);
    push_raw_name(&mut data, 3);

    let package = Package {
        summary: base.summary,
        names: NameMap {
            names: vec![
                "Obj".to_string(),
                "Value".to_string(),
                "IntProperty".to_string(),
                "None".to_string(),
            ],
        },
        imports: Vec::new(),
        exports: vec![test_export(0, data.len() as i64, 0, 0)],
        soft_object_paths: Vec::new(),
        soft_object_path_error: None,
        soft_package_references: Vec::new(),
        soft_package_reference_error: None,
    };

    let analysis = analyze_package(&package, &data, AssetView::Properties);
    let properties = &analysis.exports[0].properties;
    assert_eq!(properties.len(), 1);
    assert_eq!(properties[0].name, "Value");
    assert_eq!(properties[0].type_name, "IntProperty");
    assert_eq!(properties[0].value.as_i64(), Some(123));
    assert!(
        analysis
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != "properties_unsupported_version")
    );
}

#[test]
fn post_property_tail_is_classified_with_its_byte_range() {
    let base = Package::parse(&build_minimal_package()).unwrap();
    let mut data = Vec::new();
    data.push(0);
    push_raw_name(&mut data, 1);
    push_raw_name(&mut data, 2);
    push_i32(&mut data, 0);
    push_i32(&mut data, 4);
    data.push(0);
    push_i32(&mut data, 123);
    push_raw_name(&mut data, 3);
    data.extend_from_slice(&[1, 2, 3, 4]);

    let package = Package {
        summary: base.summary,
        names: NameMap {
            names: vec![
                "Obj".to_string(),
                "Value".to_string(),
                "IntProperty".to_string(),
                "None".to_string(),
            ],
        },
        imports: Vec::new(),
        exports: vec![test_export(0, data.len() as i64, 0, 0)],
        soft_object_paths: Vec::new(),
        soft_object_path_error: None,
        soft_package_references: Vec::new(),
        soft_package_reference_error: None,
    };

    let analysis = analyze_package(&package, &data, AssetView::Properties);
    let tail = analysis
        .known_opaque
        .iter()
        .find(|opaque| opaque.kind == KnownOpaqueKind::PostPropertyTail)
        .unwrap();
    let range = tail.byte_range.as_ref().unwrap();
    assert_eq!(range.size, 4);
    assert_eq!(range.preview, "01020304");
    assert!(
        analysis
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.code != "post_property_tail")
    );
}

// Bulk class data dwarfs everything else in a real project: on one corpus a
// single reason string covered 1.25 GB of skeletal-mesh render data alongside the
// handful of tails that actually point at a decoding gap. The two must be
// separable, and compiled bytecode must be named rather than anonymous.
#[test]
fn export_tails_separate_class_payloads_from_unattributed_bytes() {
    let single_export = |data: &[u8]| Package {
        summary: Package::parse(&build_minimal_package()).unwrap().summary,
        names: NameMap {
            names: vec!["Prop".to_string(), "None".to_string()],
        },
        imports: Vec::new(),
        exports: vec![test_export(0, data.len() as i64, 0, 0)],
        soft_object_paths: Vec::new(),
        soft_object_path_error: None,
        soft_package_references: Vec::new(),
        soft_package_reference_error: None,
    };
    let tail_of = |analysis: &crate::AssetAnalysis| {
        analysis
            .known_opaque
            .iter()
            .find(|opaque| opaque.kind == KnownOpaqueKind::PostPropertyTail)
            .expect("tail region")
            .reason
            .clone()
    };

    // A property block that closes cleanly, then a tail: those bytes are whatever
    // the class's own Serialize override wrote.
    let mut closed = Vec::new();
    closed.push(0); // EClassSerializationControlExtension: no extensions
    push_raw_name(&mut closed, 1); // None terminator, so the block is empty
    push_i32(&mut closed, 0); // PossiblySerializeObjectGuid: absent
    closed.extend_from_slice(&[1, 2, 3, 4]); // the class's own payload

    let analysis = analyze_package(&single_export(&closed), &closed, AssetView::Full);
    assert!(
        tail_of(&analysis).contains("class-owned serializer data"),
        "{}",
        tail_of(&analysis)
    );
    assert_eq!(analysis.coverage.class_payload_bytes, 4);
    assert_eq!(analysis.coverage.unattributed_tail_bytes, 0);

    // A block that never reached a terminator leaves the same bytes unattributed:
    // the decoder cannot say what they are, which is the case worth watching.
    let mut unresolved = Vec::new();
    unresolved.push(0);
    push_raw_name(&mut unresolved, 0); // "Prop": a tag, not the terminator
    unresolved.extend_from_slice(&[1, 2, 3, 4]); // truncated tag header

    let analysis = analyze_package(&single_export(&unresolved), &unresolved, AssetView::Full);
    assert!(
        tail_of(&analysis).contains("did not close cleanly"),
        "{}",
        tail_of(&analysis)
    );
    assert_eq!(analysis.coverage.class_payload_bytes, 0);
    assert_eq!(
        analysis.coverage.unattributed_tail_bytes,
        unresolved.len() as u64
    );
}

#[test]
fn non_tagged_property_payload_is_reported_as_status() {
    let base = Package::parse(&build_minimal_package()).unwrap();
    let mut data = vec![0];
    data.extend_from_slice(&[1, 2, 3, 4]);
    let package = Package {
        summary: base.summary,
        names: NameMap {
            names: vec!["Obj".to_string()],
        },
        imports: Vec::new(),
        exports: vec![test_export(0, data.len() as i64, 0, 0)],
        soft_object_paths: Vec::new(),
        soft_object_path_error: None,
        soft_package_references: Vec::new(),
        soft_package_reference_error: None,
    };

    let analysis = analyze_package(&package, &data, AssetView::Properties);
    assert_eq!(
        analysis.exports[0].property_status,
        Some(PropertyDecodeStatus::NonTaggedPayload)
    );
    assert!(analysis.exports[0].properties.is_empty());

    // This outcome counts against tagged-property coverage, so it has to say so.
    // A report that is `partial` with an empty `diagnostics` array tells a
    // consumer nothing about which export is missing or why.
    assert_eq!(analysis.status, AnalysisStatus::Partial);
    let diagnostic = diagnostic_with_code(&analysis.diagnostics, "export_payload_not_tagged");
    assert_eq!(diagnostic.path, "/exports/0/properties");
    assert_eq!(diagnostic.offset, Some(0));
    assert_eq!(analysis.coverage.property_exports_total, 1);
    assert_eq!(analysis.coverage.property_exports_complete, 0);
    assert_eq!(analysis.coverage.property_exports_not_tagged, 1);
    assert_eq!(analysis.coverage.property_exports_failed, 0);

    // The capability names the gap; that string is the only place a consumer can
    // read what is missing, so it must survive into the report.
    let capability = analysis
        .capabilities
        .iter()
        .find(|capability| capability.kind == CapabilityKind::TaggedProperties)
        .expect("tagged property capability");
    assert_eq!(capability.status, AnalysisStatus::Partial);
    assert!(
        capability
            .detail
            .as_deref()
            .is_some_and(|detail| detail.contains("not a tagged payload")),
        "{capability:#?}"
    );
}

#[test]
fn soft_package_references_are_parsed_and_filtered() {
    let mut data = build_minimal_package();
    let name_offset = data.len() as i32;
    push_fstring(&mut data, "/Game/Foo/SoftDep");
    push_u32(&mut data, 0);
    push_fstring(&mut data, "None");
    push_u32(&mut data, 0);
    let soft_offset = data.len() as i32;
    push_raw_name(&mut data, 0);
    push_raw_name(&mut data, 1);
    put_i32(&mut data, 68, 2);
    put_i32(&mut data, 72, name_offset);
    put_i32(&mut data, 132, 2);
    put_i32(&mut data, 136, soft_offset);

    let package = Package::parse(&data).expect("package with soft refs should parse");
    assert!(package.soft_package_reference_error.is_none());
    assert_eq!(
        package.soft_package_references,
        vec!["/Game/Foo/SoftDep", "None"]
    );

    let analysis = analyze_package(&package, &data, AssetView::References);
    assert_eq!(analysis.references.soft, vec!["/Game/Foo/SoftDep"]);
}

#[test]
fn soft_package_table_failure_is_not_silenced() {
    let mut data = build_minimal_package();
    put_i32(&mut data, 132, 1);
    put_i32(&mut data, 136, 999_999);

    let package = Package::parse(&data).expect("broken optional table keeps package inspectable");
    assert!(package.soft_package_reference_error.is_some());
    let analysis = analyze_package(&package, &data, AssetView::References);
    let diagnostic =
        diagnostic_with_code(&analysis.diagnostics, "soft_package_reference_table_error");
    assert_eq!(diagnostic.severity, DiagnosticSeverity::Warning);
    assert!(
        diagnostic
            .message
            .contains("soft package reference table seek failed")
    );
}
