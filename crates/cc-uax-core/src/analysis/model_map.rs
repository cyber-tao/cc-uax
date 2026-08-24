//! Conversion from the decoded, borrow-bound representation into the owned report
//! model. Nothing here decides anything: it only reshapes evidence the decoders
//! and adapters already produced.

use crate::decode::DecodedExport;
use crate::diagnostic::{Diagnostic, Severity};
use crate::model::*;
use crate::package::Package;
use crate::property::{PropertyEntry, PropertyParseStatus};
use crate::references::collect_package_references;
use crate::version::ue5;
use std::collections::BTreeSet;

pub(super) fn summary_to_model(package: &Package) -> AssetSummary {
    let summary = &package.summary;
    AssetSummary {
        package_name: summary.package_name.clone(),
        tag: summary.tag,
        legacy_file_version: summary.legacy_file_version,
        file_version_ue4: summary.file_version_ue4,
        file_version_ue5: summary.file_version_ue5,
        file_version_licensee: summary.file_version_licensee_ue,
        package_flags: summary.package_flags,
        filter_editor_only: summary.filter_editor_only(),
        total_header_size: summary.total_header_size,
        bulk_data_start_offset: summary.bulk_data_start_offset,
        name_count: summary.name_count,
        import_count: summary.import_count,
        export_count: summary.export_count,
        saved_by_engine_version: summary.engine_version.display(),
        compatible_engine_version: summary.compatible_engine_version.display(),
        custom_versions: summary
            .custom_versions
            .iter()
            .map(|version| CustomVersionInfo {
                guid: version.key.to_hex(),
                version: version.version,
            })
            .collect(),
    }
}

pub(super) fn references_to_model(package: &Package) -> AssetReferences {
    let (assets, scripts) = collect_package_references(package.import_class_object_names());
    let soft = package
        .soft_package_references
        .iter()
        .filter(|reference| !reference.is_empty() && reference.as_str() != "None")
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    AssetReferences {
        assets,
        scripts,
        soft,
    }
}

pub(super) fn imports_to_model(package: &Package) -> Vec<AssetImport> {
    package
        .imports
        .iter()
        .enumerate()
        .map(|(index, import)| {
            let package_index = -((index as i32) + 1);
            AssetImport {
                index: package_index,
                class_package: package.names.resolve_raw(import.class_package),
                class: package.names.resolve_raw(import.class_name),
                name: package.names.resolve_raw(import.object_name),
                outer_index: import.outer_index.0,
                outer_name: package.resolve_full_name(import.outer_index.0),
                package_name: import
                    .package_name
                    .map(|name| package.names.resolve_raw(name)),
                full_name: package.resolve_full_name(package_index),
            }
        })
        .collect()
}

pub(super) fn export_to_model(
    package: &Package,
    export: &DecodedExport,
    include_serialization: bool,
) -> AssetExport {
    let raw = package
        .exports
        .get((export.identity.index - 1).max(0) as usize);
    let outer_index = raw.map_or(0, |raw| raw.outer_index.0);
    AssetExport {
        index: export.identity.index,
        name: export.identity.name.clone(),
        class: export.identity.class.clone(),
        super_name: raw.map_or_else(String::new, |raw| {
            package.resolve_full_name(raw.super_index.0)
        }),
        template_name: raw.map_or_else(String::new, |raw| {
            package.resolve_full_name(raw.template_index.0)
        }),
        outer_index,
        outer_name: package.resolve_full_name(outer_index),
        full_name: package.resolve_full_name(export.identity.index),
        is_asset: export.identity.is_asset,
        serialization: include_serialization.then(|| ExportSerialization {
            object_flags: raw.map_or(0, |raw| raw.object_flags),
            serial_offset: raw.map_or(0, |raw| raw.serial_offset),
            serial_size: raw.map_or(0, |raw| raw.serial_size),
            script_serialization_start: raw
                .filter(|_| package.summary.file_version_ue5 >= ue5::SCRIPT_SERIALIZATION_OFFSET)
                .map(|raw| raw.script_serialization_start_offset),
            script_serialization_end: raw
                .filter(|_| package.summary.file_version_ue5 >= ue5::SCRIPT_SERIALIZATION_OFFSET)
                .map(|raw| raw.script_serialization_end_offset),
        }),
        object_guid: export.object_guid.clone(),
        property_status: export.property_status.map(property_status_to_model),
        properties: export
            .properties
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(property_to_model)
            .collect(),
        metadata: export.metadata.clone(),
        member: export.member.as_ref().map(|member| MemberReference {
            name: member.name.clone(),
            parent: member.parent.clone(),
        }),
    }
}

pub(super) fn property_status_to_model(status: PropertyParseStatus) -> PropertyDecodeStatus {
    match status {
        PropertyParseStatus::Complete => PropertyDecodeStatus::Complete,
        PropertyParseStatus::Empty => PropertyDecodeStatus::Empty,
        PropertyParseStatus::NonTaggedPayload => PropertyDecodeStatus::NonTaggedPayload,
        PropertyParseStatus::FailedAfterEntries => PropertyDecodeStatus::FailedAfterEntries,
    }
}

pub(super) fn property_to_model(property: &PropertyEntry) -> AssetProperty {
    AssetProperty {
        name: property.name.clone(),
        type_name: property.type_str.clone(),
        array_index: property.array_index,
        value: property.value.clone(),
        guid: property.guid.clone(),
    }
}

pub(super) fn diagnostic_to_model(diagnostic: &Diagnostic) -> AnalysisDiagnostic {
    AnalysisDiagnostic {
        severity: match diagnostic.severity {
            Severity::Error => DiagnosticSeverity::Error,
            Severity::Warning => DiagnosticSeverity::Warning,
            Severity::Info => DiagnosticSeverity::Info,
        },
        code: diagnostic.code.clone(),
        path: diagnostic.path.clone(),
        message: diagnostic.message.clone(),
        offset: diagnostic.offset,
        details: diagnostic.context.as_deref().cloned(),
    }
}
