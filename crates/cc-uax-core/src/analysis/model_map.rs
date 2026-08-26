//! Conversion from the decoded, borrow-bound representation into the owned report
//! model. Nothing here decides anything: it only reshapes evidence the decoders
//! and adapters already produced.

use crate::decode::DecodedExport;
use crate::diagnostic::{Diagnostic, Severity};
use crate::model::*;
use crate::package::Package;
use crate::property::{PropertyEntry, PropertyParseStatus};
use crate::references::collect_package_references;
use crate::script::field::DecodedField;
use crate::script::{DecodedBytecode, DecodedScriptStruct, function_flag_names};
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
        script: export.script_struct.as_ref().map(script_struct_to_model),
    }
}

fn script_struct_to_model(script: &DecodedScriptStruct) -> ScriptStructInfo {
    ScriptStructInfo {
        super_struct: script.super_struct.clone(),
        children: script.children.clone(),
        properties: script
            .properties
            .iter()
            .map(script_field_to_model)
            .collect(),
        function: script.function.as_ref().map(|function| ScriptFunctionInfo {
            flags: function.flags,
            flag_names: function_flag_names(function.flags)
                .into_iter()
                .map(str::to_owned)
                .collect(),
            event_graph_function: function.event_graph_function.clone(),
            event_graph_call_offset: function.event_graph_call_offset,
        }),
        class: script.class.as_ref().map(|class| ScriptClassInfo {
            flags: class.class_flags,
            functions: class.functions.iter().cloned().collect(),
            class_within: class.class_within.clone(),
            config_name: class.class_config_name.clone(),
            generated_by: class.class_generated_by.clone(),
            interfaces: class.interfaces.clone(),
            default_object: class.default_object.clone(),
        }),
        bytecode: script.bytecode.as_ref().map(|code| {
            let summary = code.summary.as_ref();
            ScriptBytecodeInfo {
                buffer_size: code.buffer_size,
                serialized_size: code.serialized_size,
                undecoded_reason: bytecode_undecoded_reason(code),
                expressions: summary.map_or(0, |summary| summary.expressions),
                opcodes: summary
                    .map(|summary| {
                        summary
                            .opcodes
                            .iter()
                            .map(|(name, count)| ((*name).to_owned(), *count))
                            .collect()
                    })
                    .unwrap_or_default(),
                references: summary
                    .map(|summary| {
                        summary
                            .references
                            .iter()
                            .map(|(kind, target)| ScriptBytecodeReference {
                                kind: kind.as_str().to_owned(),
                                target: target.clone(),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            }
        }),
    }
}

/// Why a stream is not fully decoded evidence: it failed to walk, or it walked
/// the right number of file bytes while disagreeing with the in-memory size the
/// struct declared.
fn bytecode_undecoded_reason(code: &DecodedBytecode) -> Option<String> {
    if let Some(failure) = &code.failure {
        return Some(failure.clone());
    }
    (!code.sizes_agree()).then(|| {
        format!(
            "disassembly accounted for {} in-memory byte(s) against a declared {}",
            code.summary.as_ref().map_or(0, |summary| summary.icode),
            code.buffer_size
        )
    })
}

fn script_field_to_model(field: &DecodedField) -> ScriptProperty {
    ScriptProperty {
        name: field.name.clone(),
        type_name: field.type_name.clone(),
        type_object: field.type_object.clone(),
        flags: field.flags,
        array_dim: field.array_dim,
        rep_notify_func: field.rep_notify_func.clone(),
        inner: field.inner.iter().map(script_field_to_model).collect(),
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
