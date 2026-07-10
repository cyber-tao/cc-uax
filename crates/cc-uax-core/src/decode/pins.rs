use super::DecodedExport;
use super::window::{ExportSerialWindow, preview_range};
use crate::diagnostic::Diagnostic;
use crate::package::Package;
use crate::pin::{
    PinSerCtx, UserDefinedPin, framework_pin_version, is_supported_framework_pin_version,
    locate_legacy_pin_start, parse_node_pins_report, parse_user_defined_pins_report,
};
use crate::property::ParseCtx;
use crate::reader::Reader;
use crate::version::custom;
use serde_json::json;

#[allow(clippy::too_many_arguments)]
pub(super) fn decode_pins_for_export(
    package: &Package,
    reader: &mut Reader,
    ctx: &ParseCtx,
    pin_ctx: &PinSerCtx,
    has_script: bool,
    window: ExportSerialWindow,
    export_i: usize,
    class_full: &str,
    diagnostics: &mut Vec<Diagnostic>,
    export: &mut DecodedExport,
) {
    if !is_graph_node_class(class_full) {
        return;
    }
    let path = format!("/exports/{export_i}/pins");
    let pin_start = if has_script {
        window.property_end
    } else {
        if reader.seek(window.property_start).is_err() {
            return;
        }
        match locate_legacy_pin_start(reader, window.serial_end, ctx, &path) {
            Ok(start) => start,
            Err(diagnostic) => {
                diagnostics.push(diagnostic);
                return;
            }
        }
    };
    let pin_end = window.serial_end;
    if pin_end <= pin_start || reader.seek(pin_start).is_err() {
        return;
    }

    let editable_pin_class = is_editable_pin_class(class_full);
    let framework_version = framework_pin_version(&package.summary);
    let candidates = pin_parse_contexts(package, *pin_ctx);
    let mut best = None;
    let mut best_pos = pin_start;
    let mut failures = Vec::new();
    for candidate in candidates {
        if reader.seek(pin_start).is_err() {
            continue;
        }
        match parse_node_pins_report(reader, pin_end, ctx, &candidate, &path) {
            Ok(parsed) => {
                let mut user_defined_pins: Option<Vec<UserDefinedPin>> = None;
                let mut selected_diagnostics = Vec::new();
                if editable_pin_class {
                    let version = framework_version.unwrap_or(-1);
                    if is_supported_framework_pin_version(version) {
                        match parse_user_defined_pins_report(
                            reader,
                            pin_end,
                            ctx,
                            &candidate,
                            version,
                            &format!("{path}/user_defined"),
                        ) {
                            Ok(pins) => user_defined_pins = Some(pins),
                            Err(diagnostic) => {
                                failures.push(diagnostic.with_context(json!({
                                    "framework_version": framework_version,
                                    "has_source_index": candidate.has_source_index,
                                    "has_uobject_wrapper": candidate.has_uobject_wrapper,
                                    "has_single_precision_float": candidate.has_single_precision_float,
                                })));
                                continue;
                            }
                        }
                        if framework_version.is_none() {
                            selected_diagnostics.push(
                                Diagnostic::warning(
                                    "framework_pin_version_missing",
                                    format!("{path}/user_defined"),
                                    "Dev-Framework custom version is absent; parsed FUserPinInfo with the legacy FString name layout",
                                )
                                .with_offset(reader.pos()),
                            );
                        }
                    } else {
                        selected_diagnostics.push(
                            Diagnostic::warning(
                                "framework_pin_version_unsupported",
                                format!("{path}/user_defined"),
                                format!(
                                    "Dev-Framework custom version {version} is newer than the supported UE5.7 layout"
                                ),
                            )
                            .with_offset(reader.pos())
                            .with_context(json!({ "framework_version": version })),
                        );
                    }
                }
                let consumed_pos = reader.pos();
                if consumed_pos < pin_end {
                    selected_diagnostics.push(
                        Diagnostic::warning(
                            if editable_pin_class {
                                "user_defined_pins_trailing_bytes"
                            } else {
                                "pin_region_trailing_bytes"
                            },
                            &path,
                            format!(
                                "{} byte(s) remain after the known graph-node serialization",
                                pin_end - consumed_pos
                            ),
                        )
                        .with_offset(consumed_pos)
                        .with_context(json!({
                            "class": class_full,
                            "tail_start": consumed_pos,
                            "serial_end": pin_end,
                            "tail_size": pin_end - consumed_pos,
                        })),
                    );
                }
                if best.is_none() || consumed_pos > best_pos {
                    best_pos = consumed_pos;
                    best = Some((parsed.pins, user_defined_pins, selected_diagnostics));
                }
            }
            Err(diag) => failures.push(diag.with_context(json!({
                "has_source_index": candidate.has_source_index,
                "has_uobject_wrapper": candidate.has_uobject_wrapper,
                "has_single_precision_float": candidate.has_single_precision_float,
            }))),
        }
    }
    if let Some((pins, user_defined_pins, selected_diagnostics)) = best {
        export.post_property_tail =
            (best_pos < pin_end).then(|| preview_range(reader, best_pos, pin_end));
        let _ = reader.seek(best_pos);
        export.pins = Some(pins);
        export.user_defined_pins = user_defined_pins;
        diagnostics.extend(selected_diagnostics);
        return;
    }

    diagnostics.extend(failures);
    let pin_bytes = pin_end.saturating_sub(pin_start);
    if pin_bytes > 0 {
        diagnostics.push(
            Diagnostic::warning(
                "pins_unparsed_bytes",
                format!("/exports/{export_i}/pins"),
                format!("pin parser could not decode {pin_bytes} byte(s)"),
            )
            .with_context(json!({
                "unparsed_bytes": pin_bytes,
                "property_end": pin_start,
                "serial_end": pin_end,
            })),
        );
    }
}

fn pin_parse_contexts(package: &Package, primary: PinSerCtx) -> Vec<PinSerCtx> {
    let source_known = package
        .summary
        .custom_version(custom::UE5_MAIN_STREAM_OBJECT_VERSION)
        .is_some();
    let wrapper_known = package
        .summary
        .custom_version(custom::RELEASE_OBJECT_VERSION)
        .is_some();
    let single_precision_known = package
        .summary
        .custom_version(custom::UE5_RELEASE_STREAM_OBJECT_VERSION)
        .is_some();

    let source_options = if source_known {
        vec![primary.has_source_index]
    } else {
        vec![primary.has_source_index, !primary.has_source_index]
    };
    let wrapper_options = if wrapper_known {
        vec![primary.has_uobject_wrapper]
    } else {
        vec![primary.has_uobject_wrapper, !primary.has_uobject_wrapper]
    };
    let single_precision_options = if single_precision_known {
        vec![primary.has_single_precision_float]
    } else {
        vec![
            primary.has_single_precision_float,
            !primary.has_single_precision_float,
        ]
    };

    let mut out: Vec<PinSerCtx> = Vec::new();
    for has_source_index in source_options {
        for &has_uobject_wrapper in &wrapper_options {
            for &has_single_precision_float in &single_precision_options {
                let ctx = PinSerCtx {
                    filter_editor_only: primary.filter_editor_only,
                    has_source_index,
                    has_uobject_wrapper,
                    has_single_precision_float,
                };
                if !out.iter().any(|existing| {
                    existing.filter_editor_only == ctx.filter_editor_only
                        && existing.has_source_index == ctx.has_source_index
                        && existing.has_uobject_wrapper == ctx.has_uobject_wrapper
                        && existing.has_single_precision_float == ctx.has_single_precision_float
                }) {
                    out.push(ctx);
                }
            }
        }
    }
    out
}

pub(super) fn is_graph_node_class(class_full: &str) -> bool {
    let simple = class_full.rsplit(['.', '/']).next().unwrap_or(class_full);
    if simple.starts_with("K2Node") || simple.starts_with("EdGraphNode") {
        return true;
    }
    if simple.starts_with("NiagaraNode") || simple == "NiagaraOverviewNode" {
        return true;
    }
    if simple.contains("Binding") {
        return false;
    }
    simple.contains("GraphNode")
}

fn is_editable_pin_class(class_full: &str) -> bool {
    let simple = class_full.rsplit(['.', '/']).next().unwrap_or(class_full);
    matches!(
        simple,
        "K2Node_EditablePinBase"
            | "K2Node_FunctionTerminator"
            | "K2Node_FunctionEntry"
            | "K2Node_FunctionResult"
            | "K2Node_Event"
            | "K2Node_CustomEvent"
            | "K2Node_Tunnel"
    )
}
