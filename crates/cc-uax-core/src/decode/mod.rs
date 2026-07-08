mod member;
mod pins;
mod properties;
mod window;

use crate::diagnostic::{ByteRangePreview, Diagnostic};
use crate::output::sections::OutputSections;
use crate::package::Package;
use crate::pin::{Pin, PinSerCtx};
use crate::property::{ParseCtx, PropertyEntry};
use crate::reader::Reader;
use crate::version::{custom, ue5};
use serde_json::{Value, json};
use std::collections::HashMap;

use pins::{decode_pins_for_export, is_graph_node_class};
use properties::decode_properties_for_export;
use window::export_serial_window;

#[derive(Debug, Clone, Default)]
pub struct DecodeOptions {
    pub sections: OutputSections,
}

impl DecodeOptions {
    pub fn new(sections: OutputSections) -> Self {
        Self { sections }
    }
}

impl From<OutputSections> for DecodeOptions {
    fn from(sections: OutputSections) -> Self {
        Self { sections }
    }
}

impl From<&OutputSections> for DecodeOptions {
    fn from(sections: &OutputSections) -> Self {
        Self {
            sections: sections.clone(),
        }
    }
}

pub struct DecodeReport<'a> {
    pub package: &'a Package,
    pub sections: OutputSections,
    pub exports: Vec<DecodedExport>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone)]
pub struct DecodedExport {
    pub identity: DecodedExportIdentity,
    pub layout: Option<DecodedExportLayout>,
    pub properties: Option<Vec<PropertyEntry>>,
    pub post_property_tail: Option<ByteRangePreview>,
    pub object_guid: Option<String>,
    pub metadata: Option<Value>,
    pub pins: Option<Vec<Pin>>,
    pub member: Option<MemberRef>,
}

#[derive(Debug, Clone)]
pub struct DecodedExportIdentity {
    pub index: i32,
    pub name: String,
    pub class: String,
    pub is_asset: bool,
}

#[derive(Debug, Clone)]
pub struct DecodedExportLayout {
    pub super_name: String,
    pub template_name: String,
    pub outer_name: String,
    pub full_name: String,
    pub object_flags: u32,
    pub serial_offset: i64,
    pub serial_size: i64,
    pub script_serialization_start: Option<i64>,
    pub script_serialization_end: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct MemberRef {
    pub name: String,
    pub parent: Option<Value>,
}

impl Package {
    pub fn decode<'a>(&'a self, data: &[u8], options: &DecodeOptions) -> DecodeReport<'a> {
        let mut diagnostics = self.table_diagnostics();
        let exports = if options.sections.exports {
            self.decode_exports(data, &options.sections, &mut diagnostics)
        } else {
            Vec::new()
        };
        DecodeReport {
            package: self,
            sections: options.sections.clone(),
            exports,
            diagnostics,
        }
    }

    fn table_diagnostics(&self) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        if let Some(err) = &self.soft_object_path_error {
            diagnostics.push(Diagnostic::warning(
                "soft_object_path_table_error",
                "/summary/soft_object_paths",
                err.clone(),
            ));
        }
        if let Some(err) = &self.soft_package_reference_error {
            diagnostics.push(Diagnostic::warning(
                "soft_package_reference_table_error",
                "/summary/soft_package_references",
                err.clone(),
            ));
        }
        diagnostics
    }

    fn decode_exports(
        &self,
        data: &[u8],
        sections: &OutputSections,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Vec<DecodedExport> {
        let object_ref_memo = std::cell::RefCell::new(HashMap::<i32, Value>::new());
        let resolve = |idx: i32| {
            if idx == 0 {
                return Value::Null;
            }
            object_ref_memo
                .borrow_mut()
                .entry(idx)
                .or_insert_with(|| self.resolve_object_ref(idx))
                .clone()
        };
        let pin_ctx = PinSerCtx::from_summary(&self.summary);
        let ctx = ParseCtx {
            names: &self.names,
            resolve_object: &resolve,
            pins: pin_ctx,
            soft_object_paths: &self.soft_object_paths,
            niagara_version: self
                .summary
                .custom_version(custom::NIAGARA_OBJECT_VERSION)
                .unwrap_or(-1),
            fortnite_main_version: self
                .summary
                .custom_version(custom::FORTNITE_MAIN_OBJECT_VERSION)
                .unwrap_or(-1),
            file_version_ue4: self.summary.file_version_ue4,
            file_version_ue5: self.summary.file_version_ue5,
        };
        let mut reader = Reader::new(data);
        let file_len = reader.len();
        let has_script = self.summary.file_version_ue5 >= ue5::SCRIPT_SERIALIZATION_OFFSET;
        let mut decoded = Vec::with_capacity(self.exports.len());

        for (i, exp) in self.exports.iter().enumerate() {
            let pkg_index = (i as i32) + 1;
            let class_full = self.resolve_full_name(exp.class_index.0);
            let is_node = is_graph_node_class(&class_full);
            let mut export = DecodedExport {
                identity: DecodedExportIdentity {
                    index: pkg_index,
                    name: self.names.resolve_raw(exp.object_name),
                    class: class_full.clone(),
                    is_asset: exp.is_asset,
                },
                layout: sections.layout.then(|| DecodedExportLayout {
                    super_name: self.resolve_full_name(exp.super_index.0),
                    template_name: self.resolve_full_name(exp.template_index.0),
                    outer_name: self.resolve_full_name(exp.outer_index.0),
                    full_name: self.resolve_full_name(pkg_index),
                    object_flags: exp.object_flags,
                    serial_offset: exp.serial_offset,
                    serial_size: exp.serial_size,
                    script_serialization_start: has_script
                        .then_some(exp.script_serialization_start_offset),
                    script_serialization_end: has_script
                        .then_some(exp.script_serialization_end_offset),
                }),
                properties: None,
                post_property_tail: None,
                object_guid: None,
                metadata: None,
                pins: None,
                member: None,
            };

            let serial_window = match export_serial_window(exp, has_script, file_len) {
                Ok(w) => w,
                Err(err) => {
                    diagnostics.push(
                        Diagnostic::error("serial_window_invalid", format!("/exports/{i}"), err)
                            .with_context(json!({
                                "export_index": pkg_index,
                                "serial_offset": exp.serial_offset,
                                "serial_size": exp.serial_size,
                            })),
                    );
                    decoded.push(export);
                    continue;
                }
            };

            if (sections.properties || is_node)
                && let Some(window) = serial_window
            {
                decode_properties_for_export(
                    &mut reader,
                    &ctx,
                    has_script,
                    window,
                    i,
                    &class_full,
                    sections,
                    diagnostics,
                    &mut export,
                );
            }

            if sections.pins
                && let Some(window) = serial_window
            {
                decode_pins_for_export(
                    self,
                    &mut reader,
                    &ctx,
                    &pin_ctx,
                    has_script,
                    window,
                    i,
                    &class_full,
                    diagnostics,
                    &mut export,
                );
            }

            decoded.push(export);
        }
        decoded
    }
}
