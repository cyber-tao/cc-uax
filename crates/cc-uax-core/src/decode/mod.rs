mod member;
pub(crate) mod pins;
mod properties;
pub(crate) mod rigvm;
mod window;

use crate::diagnostic::{ByteRangePreview, Diagnostic};
use crate::package::Package;
use crate::pin::{Pin, PinSerCtx, UserDefinedPin};
use crate::property::{ParseCtx, PropertyEntry, PropertyParseStatus};
use crate::reader::Reader;
use crate::structured_value::{Value, json};
use crate::version::{SerializationPolicy, custom, ue5};
use std::collections::HashMap;

use pins::{decode_pins_for_export, is_graph_node_class};
use properties::decode_properties_for_export;
use rigvm::{
    DecodedRigVmLink, decode_rigvm_link_for_export, is_rigvm_link_class,
    is_rigvm_model_object_class,
};
use window::export_serial_window;

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct DecodeOptions {
    pub(crate) exports: bool,
    pub(crate) pins: bool,
    pub(crate) properties: bool,
    pub(crate) layout: bool,
}

impl DecodeOptions {
    pub(crate) const fn none() -> Self {
        Self {
            exports: false,
            pins: false,
            properties: false,
            layout: false,
        }
    }

    pub(crate) const fn full() -> Self {
        Self {
            exports: true,
            pins: true,
            properties: true,
            layout: true,
        }
    }
}

#[allow(dead_code)]
pub(crate) struct DecodeReport<'a> {
    pub(crate) package: &'a Package,
    pub(crate) exports: Vec<DecodedExport>,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct DecodedExport {
    pub(crate) identity: DecodedExportIdentity,
    pub(crate) layout: Option<DecodedExportLayout>,
    pub(crate) properties: Option<Vec<PropertyEntry>>,
    pub(crate) property_status: Option<PropertyParseStatus>,
    pub(crate) post_property_tail: Option<ByteRangePreview>,
    pub(crate) object_guid: Option<String>,
    pub(crate) metadata: Option<Value>,
    pub(crate) pins: Option<Vec<Pin>>,
    pub(crate) user_defined_pins: Option<Vec<UserDefinedPin>>,
    pub(crate) member: Option<MemberRef>,
    pub(crate) rigvm_link: Option<DecodedRigVmLink>,
}

#[derive(Debug, Clone)]
pub(crate) struct DecodedExportIdentity {
    pub(crate) index: i32,
    pub(crate) name: String,
    pub(crate) class: String,
    pub(crate) is_asset: bool,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct DecodedExportLayout {
    pub(crate) super_name: String,
    pub(crate) template_name: String,
    pub(crate) outer_name: String,
    pub(crate) full_name: String,
    pub(crate) object_flags: u32,
    pub(crate) serial_offset: i64,
    pub(crate) serial_size: i64,
    pub(crate) script_serialization_start: Option<i64>,
    pub(crate) script_serialization_end: Option<i64>,
}

#[derive(Debug, Clone)]
pub(crate) struct MemberRef {
    pub(crate) name: String,
    pub(crate) parent: Option<Value>,
}

impl Package {
    pub(crate) fn decode<'a>(&'a self, data: &[u8], options: &DecodeOptions) -> DecodeReport<'a> {
        let mut diagnostics = self.table_diagnostics();
        let exports = if options.exports {
            self.decode_exports(data, options, &mut diagnostics)
        } else {
            Vec::new()
        };
        DecodeReport {
            package: self,
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
        options: &DecodeOptions,
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
            serialization: SerializationPolicy {
                niagara_version: self
                    .summary
                    .custom_version(custom::NIAGARA_OBJECT_VERSION)
                    .unwrap_or(-1),
                fortnite_main_version: self
                    .summary
                    .custom_version(custom::FORTNITE_MAIN_OBJECT_VERSION)
                    .unwrap_or(-1),
                instanced_struct_version: self
                    .summary
                    .custom_version(custom::INSTANCED_STRUCT_VERSION)
                    .unwrap_or(-1),
                state_tree_instance_storage_version: self
                    .summary
                    .custom_version(custom::STATE_TREE_INSTANCE_STORAGE_VERSION)
                    .unwrap_or(-1),
                fortnite_release_version: self
                    .summary
                    .custom_version(custom::FORTNITE_RELEASE_BRANCH_OBJECT_VERSION)
                    .unwrap_or(-1),
            },
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
            let is_rigvm_link = is_rigvm_link_class(&class_full);
            let capture_adapter_properties = options.pins
                && ((is_rigvm_model_object_class(&class_full) && !is_rigvm_link)
                    || is_pcg_model_object_class(&class_full)
                    || is_state_tree_model_object_class(&class_full));
            let mut export = DecodedExport {
                identity: DecodedExportIdentity {
                    index: pkg_index,
                    name: self.names.resolve_raw(exp.object_name),
                    class: class_full.clone(),
                    is_asset: exp.is_asset,
                },
                layout: options.layout.then(|| DecodedExportLayout {
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
                property_status: None,
                post_property_tail: None,
                object_guid: None,
                metadata: None,
                pins: None,
                user_defined_pins: None,
                member: None,
                rigvm_link: None,
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

            if is_rigvm_link
                && (options.properties || options.pins)
                && let Some(window) = serial_window
            {
                decode_rigvm_link_for_export(&mut reader, window, i, diagnostics, &mut export);
            } else if (options.properties || is_node || capture_adapter_properties)
                && let Some(window) = serial_window
            {
                decode_properties_for_export(
                    &mut reader,
                    &ctx,
                    has_script,
                    window,
                    i,
                    &class_full,
                    options.properties || capture_adapter_properties,
                    diagnostics,
                    &mut export,
                );
            }

            if options.pins
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

fn is_pcg_model_object_class(class: &str) -> bool {
    class.starts_with("/Script/PCG.") || class.starts_with("/Script/PCGEditor.")
}

fn is_state_tree_model_object_class(class: &str) -> bool {
    class.starts_with("/Script/StateTree")
}
