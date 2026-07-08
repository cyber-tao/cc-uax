use super::DecodedExport;
use super::window::ExportSerialWindow;
use crate::diagnostic::Diagnostic;
use crate::package::Package;
use crate::pin::{PinSerCtx, parse_node_pins_report};
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
    if !has_script || !is_graph_node_class(class_full) {
        return;
    }
    let pin_start = window.property_end;
    let pin_end = window.serial_end;
    if pin_end <= pin_start || reader.seek(pin_start).is_err() {
        return;
    }

    let candidates = pin_parse_contexts(package, *pin_ctx);
    let mut best = None;
    let mut best_pos = pin_start;
    let mut failures = Vec::new();
    for candidate in candidates {
        if reader.seek(pin_start).is_err() {
            continue;
        }
        let path = format!("/exports/{export_i}/pins");
        match parse_node_pins_report(reader, pin_end, ctx, &candidate, &path) {
            Ok(parsed) => {
                let consumed_pos = reader.pos();
                if consumed_pos >= best_pos {
                    best_pos = consumed_pos;
                    best = Some(parsed.pins);
                }
            }
            Err(diag) => failures.push(diag.with_context(json!({
                "has_source_index": candidate.has_source_index,
                "has_uobject_wrapper": candidate.has_uobject_wrapper,
                "has_single_precision_float": candidate.has_single_precision_float,
            }))),
        }
    }
    if let Some(pins) = best {
        let _ = reader.seek(best_pos);
        export.pins = Some(pins);
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
