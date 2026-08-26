use crate::graph_models::{LogicGraph, PcgGraph, RigVmGraph, StateTreeGraph};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ops::AddAssign;

/// Bumped to 8 when exports gained the decoded `script` block: the `UStruct`,
/// `UFunction` and `UClass` serializers are read as structured fields and the
/// compiled Kismet bytecode is disassembled, so `blueprint_bytecode` reports what
/// it recovered instead of naming a gap.
pub const ASSET_ANALYSIS_SCHEMA_VERSION: u32 = 8;

/// serde `skip_serializing_if` helper: drop `false` booleans from the rendered
/// report so only set flags are emitted.
pub(crate) fn is_false(value: &bool) -> bool {
    !*value
}

/// serde `skip_serializing_if` helper: drop an FName-derived string when it is
/// empty or the canonical UE null name `None`.
pub(crate) fn is_absent_name(value: &str) -> bool {
    value.is_empty() || value == "None"
}

/// serde `skip_serializing_if` helper: drop a zero `i32` (default array index).
pub(crate) fn is_zero_i32(value: &i32) -> bool {
    *value == 0
}

/// serde `skip_serializing_if` helper: drop the default static-array length of 1.
pub(crate) fn is_one_i32(value: &i32) -> bool {
    *value == 1
}

/// serde `skip_serializing_if` helper: drop a zero `usize` (default unresolved count).
pub(crate) fn is_zero_usize(value: &usize) -> bool {
    *value == 0
}

/// serde `skip_serializing_if` helper: drop a zero `u64` byte total.
pub(crate) fn is_zero_u64(value: &u64) -> bool {
    *value == 0
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnalysisStatus {
    Complete,
    Partial,
    Unsupported,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetView {
    Summary,
    Logic,
    Properties,
    References,
    Full,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetAnalysis {
    pub schema_version: u32,
    pub view: AssetView,
    pub status: AnalysisStatus,
    pub summary: AssetSummary,
    #[serde(default, skip_serializing_if = "AssetReferences::is_empty")]
    pub references: AssetReferences,
    /// Present only when the view decoded both the reference tables and the values
    /// to check against them, which today is `full`. Absent means the cross-check
    /// was not run, which is not the same as finding nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_evidence: Option<ReferenceEvidence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub imports: Vec<AssetImport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exports: Vec<AssetExport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graphs: Vec<LogicGraph>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rigvm_graphs: Vec<RigVmGraph>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pcg_graphs: Vec<PcgGraph>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub state_tree_graphs: Vec<StateTreeGraph>,
    pub coverage: ParseCoverage,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<AnalysisDiagnostic>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities: Vec<AnalysisCapability>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub known_opaque: Vec<KnownOpaque>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct ParseCoverage {
    pub bytes_total: u64,
    /// Sum of every analyzed export's `serial_size`; the denominator for byte
    /// conservation over export payloads.
    #[serde(skip_serializing_if = "is_zero_u64")]
    pub export_bytes_total: u64,
    pub exports_total: usize,
    pub exports_analyzed: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub property_exports_total: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub property_exports_complete: usize,
    /// Exports whose payload is not a tagged-property block at all, so nothing was
    /// decoded and the whole declared range stays opaque. Distinct from
    /// [`Self::property_exports_failed`]: this is a payload shape the decoder does
    /// not model, not a tagged block that broke.
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub property_exports_not_tagged: usize,
    /// Exports whose tagged-property block started decoding and then failed, so
    /// the properties before the failure are evidence and the rest is opaque.
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub property_exports_failed: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub properties_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub graph_nodes_total: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub graph_nodes_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub pins_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub graph_edges_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub rigvm_graphs_total: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub rigvm_graphs_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub rigvm_nodes_total: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub rigvm_nodes_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub rigvm_pins_total: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub rigvm_pins_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub rigvm_links_total: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub rigvm_links_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub pcg_graphs_total: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub pcg_graphs_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub pcg_nodes_total: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub pcg_nodes_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub pcg_pins_total: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub pcg_pins_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub pcg_edges_total: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub pcg_edges_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub state_tree_graphs_total: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub state_tree_graphs_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub state_tree_states_total: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub state_tree_states_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub state_tree_tasks_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub state_tree_conditions_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub state_tree_transitions_decoded: usize,
    /// Exports whose class writes a `UStruct` serializer block (Blueprint
    /// functions, delegate signatures, generated classes).
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub script_structs_total: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub script_structs_decoded: usize,
    /// Reflected `FProperty` declarations recovered from those blocks.
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub script_properties_decoded: usize,
    /// Serialized bytes of compiled script the disassembler consumed.
    #[serde(skip_serializing_if = "is_zero_u64")]
    pub script_bytecode_bytes: u64,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub script_expressions_decoded: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub known_opaque_regions: usize,
    /// Total bytes covered by `known_opaque` regions that carry a byte range.
    #[serde(skip_serializing_if = "is_zero_u64")]
    pub opaque_bytes: u64,
    /// Subset of [`Self::opaque_bytes`]: export tails a class's own `Serialize`
    /// override wrote after a cleanly closed property block (mesh render data,
    /// lightmaps, compiled bytecode). Expected bulk data, not a decoding gap.
    #[serde(skip_serializing_if = "is_zero_u64")]
    pub class_payload_bytes: u64,
    /// Subset of [`Self::opaque_bytes`]: export tails that follow a property block
    /// which did not close cleanly, so the decoder cannot say what they are. This
    /// is the counter to watch when judging decoder coverage.
    #[serde(skip_serializing_if = "is_zero_u64")]
    pub unattributed_tail_bytes: u64,
    /// Export payload bytes that are neither decoded nor classified as opaque.
    /// Always a defect: a non-zero value means an export window was not fully
    /// accounted for.
    #[serde(skip_serializing_if = "is_zero_u64")]
    pub unclassified_bytes: u64,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub diagnostic_errors: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub diagnostic_warnings: usize,
}

impl AddAssign<&ParseCoverage> for ParseCoverage {
    /// Field-complete accumulation used to aggregate per-asset coverage into a
    /// project total. Every field must be summed here; the drift test in
    /// `cc-uax-project` walks the serialized map to catch any omission.
    fn add_assign(&mut self, other: &ParseCoverage) {
        let ParseCoverage {
            bytes_total,
            export_bytes_total,
            exports_total,
            exports_analyzed,
            property_exports_total,
            property_exports_complete,
            property_exports_not_tagged,
            property_exports_failed,
            properties_decoded,
            graph_nodes_total,
            graph_nodes_decoded,
            pins_decoded,
            graph_edges_decoded,
            rigvm_graphs_total,
            rigvm_graphs_decoded,
            rigvm_nodes_total,
            rigvm_nodes_decoded,
            rigvm_pins_total,
            rigvm_pins_decoded,
            rigvm_links_total,
            rigvm_links_decoded,
            pcg_graphs_total,
            pcg_graphs_decoded,
            pcg_nodes_total,
            pcg_nodes_decoded,
            pcg_pins_total,
            pcg_pins_decoded,
            pcg_edges_total,
            pcg_edges_decoded,
            state_tree_graphs_total,
            state_tree_graphs_decoded,
            state_tree_states_total,
            state_tree_states_decoded,
            state_tree_tasks_decoded,
            state_tree_conditions_decoded,
            state_tree_transitions_decoded,
            script_structs_total,
            script_structs_decoded,
            script_properties_decoded,
            script_bytecode_bytes,
            script_expressions_decoded,
            known_opaque_regions,
            opaque_bytes,
            class_payload_bytes,
            unattributed_tail_bytes,
            unclassified_bytes,
            diagnostic_errors,
            diagnostic_warnings,
        } = other;
        self.bytes_total = self.bytes_total.saturating_add(*bytes_total);
        self.export_bytes_total = self.export_bytes_total.saturating_add(*export_bytes_total);
        self.exports_total = self.exports_total.saturating_add(*exports_total);
        self.exports_analyzed = self.exports_analyzed.saturating_add(*exports_analyzed);
        self.property_exports_total = self
            .property_exports_total
            .saturating_add(*property_exports_total);
        self.property_exports_complete = self
            .property_exports_complete
            .saturating_add(*property_exports_complete);
        self.property_exports_not_tagged = self
            .property_exports_not_tagged
            .saturating_add(*property_exports_not_tagged);
        self.property_exports_failed = self
            .property_exports_failed
            .saturating_add(*property_exports_failed);
        self.properties_decoded = self.properties_decoded.saturating_add(*properties_decoded);
        self.graph_nodes_total = self.graph_nodes_total.saturating_add(*graph_nodes_total);
        self.graph_nodes_decoded = self
            .graph_nodes_decoded
            .saturating_add(*graph_nodes_decoded);
        self.pins_decoded = self.pins_decoded.saturating_add(*pins_decoded);
        self.graph_edges_decoded = self
            .graph_edges_decoded
            .saturating_add(*graph_edges_decoded);
        self.rigvm_graphs_total = self.rigvm_graphs_total.saturating_add(*rigvm_graphs_total);
        self.rigvm_graphs_decoded = self
            .rigvm_graphs_decoded
            .saturating_add(*rigvm_graphs_decoded);
        self.rigvm_nodes_total = self.rigvm_nodes_total.saturating_add(*rigvm_nodes_total);
        self.rigvm_nodes_decoded = self
            .rigvm_nodes_decoded
            .saturating_add(*rigvm_nodes_decoded);
        self.rigvm_pins_total = self.rigvm_pins_total.saturating_add(*rigvm_pins_total);
        self.rigvm_pins_decoded = self.rigvm_pins_decoded.saturating_add(*rigvm_pins_decoded);
        self.rigvm_links_total = self.rigvm_links_total.saturating_add(*rigvm_links_total);
        self.rigvm_links_decoded = self
            .rigvm_links_decoded
            .saturating_add(*rigvm_links_decoded);
        self.pcg_graphs_total = self.pcg_graphs_total.saturating_add(*pcg_graphs_total);
        self.pcg_graphs_decoded = self.pcg_graphs_decoded.saturating_add(*pcg_graphs_decoded);
        self.pcg_nodes_total = self.pcg_nodes_total.saturating_add(*pcg_nodes_total);
        self.pcg_nodes_decoded = self.pcg_nodes_decoded.saturating_add(*pcg_nodes_decoded);
        self.pcg_pins_total = self.pcg_pins_total.saturating_add(*pcg_pins_total);
        self.pcg_pins_decoded = self.pcg_pins_decoded.saturating_add(*pcg_pins_decoded);
        self.pcg_edges_total = self.pcg_edges_total.saturating_add(*pcg_edges_total);
        self.pcg_edges_decoded = self.pcg_edges_decoded.saturating_add(*pcg_edges_decoded);
        self.state_tree_graphs_total = self
            .state_tree_graphs_total
            .saturating_add(*state_tree_graphs_total);
        self.state_tree_graphs_decoded = self
            .state_tree_graphs_decoded
            .saturating_add(*state_tree_graphs_decoded);
        self.state_tree_states_total = self
            .state_tree_states_total
            .saturating_add(*state_tree_states_total);
        self.state_tree_states_decoded = self
            .state_tree_states_decoded
            .saturating_add(*state_tree_states_decoded);
        self.state_tree_tasks_decoded = self
            .state_tree_tasks_decoded
            .saturating_add(*state_tree_tasks_decoded);
        self.state_tree_conditions_decoded = self
            .state_tree_conditions_decoded
            .saturating_add(*state_tree_conditions_decoded);
        self.state_tree_transitions_decoded = self
            .state_tree_transitions_decoded
            .saturating_add(*state_tree_transitions_decoded);
        self.script_structs_total = self
            .script_structs_total
            .saturating_add(*script_structs_total);
        self.script_structs_decoded = self
            .script_structs_decoded
            .saturating_add(*script_structs_decoded);
        self.script_properties_decoded = self
            .script_properties_decoded
            .saturating_add(*script_properties_decoded);
        self.script_bytecode_bytes = self
            .script_bytecode_bytes
            .saturating_add(*script_bytecode_bytes);
        self.script_expressions_decoded = self
            .script_expressions_decoded
            .saturating_add(*script_expressions_decoded);
        self.known_opaque_regions = self
            .known_opaque_regions
            .saturating_add(*known_opaque_regions);
        self.opaque_bytes = self.opaque_bytes.saturating_add(*opaque_bytes);
        self.class_payload_bytes = self
            .class_payload_bytes
            .saturating_add(*class_payload_bytes);
        self.unattributed_tail_bytes = self
            .unattributed_tail_bytes
            .saturating_add(*unattributed_tail_bytes);
        self.unclassified_bytes = self.unclassified_bytes.saturating_add(*unclassified_bytes);
        self.diagnostic_errors = self.diagnostic_errors.saturating_add(*diagnostic_errors);
        self.diagnostic_warnings = self
            .diagnostic_warnings
            .saturating_add(*diagnostic_warnings);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DecodedValue {
    Null,
    Bool(bool),
    Integer(i64),
    Unsigned(u64),
    Float(f64),
    String(String),
    Array(Vec<DecodedValue>),
    Object(BTreeMap<String, DecodedValue>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetSummary {
    pub package_name: String,
    pub tag: u32,
    pub legacy_file_version: i32,
    pub file_version_ue4: i32,
    pub file_version_ue5: i32,
    pub file_version_licensee: i32,
    pub package_flags: u32,
    pub filter_editor_only: bool,
    pub total_header_size: i32,
    pub bulk_data_start_offset: i64,
    pub name_count: i32,
    pub import_count: i32,
    pub export_count: i32,
    pub saved_by_engine_version: String,
    pub compatible_engine_version: String,
    pub custom_versions: Vec<CustomVersionInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomVersionInfo {
    pub guid: String,
    pub version: i32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetReferences {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assets: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scripts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub soft: Vec<String>,
}

/// Package paths found inside decoded values, checked against the linker tables
/// that [`AssetReferences`] reports.
///
/// `references` is authoritative for what the engine's own linker recorded, and
/// compiled Blueprint bytecode cannot hide a reference from it: an object
/// constant in the `Script` stream is an `FPackageIndex`, so it needs an import
/// row to serialize at all. What the tables genuinely cannot hold is a path that
/// was never a typed reference — an asset path typed as a string into a graph pin
/// and loaded at runtime. This block measures that residue instead of leaving it
/// as an unbounded caveat on every "unreferenced" claim.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceEvidence {
    /// Distinct packages named by any decoded value in this asset.
    pub value_packages: usize,
    /// Of those, the ones the import or soft-package tables also record.
    pub confirmed_by_tables: usize,
    /// Of those, the ones neither table records. These are exactly the references
    /// a linker-table-only reference graph cannot see.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub value_only_packages: Vec<String>,
    pub sources: ReferenceEvidenceSources,
}

/// Distinct packages contributed by each kind of decoded value. A package named
/// in more than one place is counted once per source, so these do not sum to
/// [`ReferenceEvidence::value_packages`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceEvidenceSources {
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub property_values: usize,
    /// Graph pin `DefaultValue` strings, where a typed-in asset path lives.
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub pin_default_values: usize,
    /// Graph pin `DefaultObject` references and pin type objects.
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub pin_default_objects: usize,
    /// Targets named by disassembled script bytecode. These carry attribution the
    /// linker tables cannot: the tables say the package depends on something, this
    /// says the compiled code reaches it.
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub bytecode: usize,
}

impl AssetReferences {
    fn is_empty(&self) -> bool {
        self.assets.is_empty() && self.scripts.is_empty() && self.soft.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetImport {
    pub index: i32,
    pub class_package: String,
    pub class: String,
    pub name: String,
    pub outer_index: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub outer_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package_name: Option<String>,
    pub full_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetExport {
    pub index: i32,
    pub name: String,
    pub class: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub super_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub template_name: String,
    pub outer_index: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub outer_name: String,
    pub full_name: String,
    #[serde(default, skip_serializing_if = "crate::model::is_false")]
    pub is_asset: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serialization: Option<ExportSerialization>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_guid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub property_status: Option<PropertyDecodeStatus>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<AssetProperty>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<DecodedValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub member: Option<MemberReference>,
    /// The `UStruct`/`UFunction` serializer block, present on the classes that
    /// carry compiled script: Blueprint functions, delegate signatures, and
    /// generated classes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<ScriptStructInfo>,
}

/// `UStruct::Serialize`: the reflected shape of a function or generated class,
/// and the compiled script that follows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptStructInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub super_struct: Option<String>,
    /// `UField` children: the functions a class owns, or nothing for a function.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<String>,
    /// `ChildProperties`: a function's parameters and locals, or a generated
    /// class's variables.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<ScriptProperty>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<ScriptFunctionInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class: Option<ScriptClassInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytecode: Option<ScriptBytecodeInfo>,
}

/// `UClass::Serialize`: what a generated class declares beyond its `UStruct`
/// shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptClassInfo {
    pub flags: u32,
    /// `FuncMap`, mapping each callable name to the function object.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub functions: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub class_within: Option<String>,
    #[serde(default, skip_serializing_if = "is_absent_name")]
    pub config_name: String,
    /// The Blueprint asset this class was compiled from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_by: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub interfaces: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_object: Option<String>,
}

/// One reflected `FProperty` declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptProperty {
    pub name: String,
    /// The `FFieldClass` name, e.g. `ObjectProperty` or `StructProperty`.
    pub type_name: String,
    /// What the property's type points at: the class it references, the struct it
    /// holds, or the signature of the delegate it stores.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_object: Option<String>,
    /// `EPropertyFlags`. Retained raw because the flag set is large, versioned,
    /// and only meaningful against UE's own table.
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub flags: u64,
    /// Static array length; absent means the usual single element.
    #[serde(default, skip_serializing_if = "is_one_i32")]
    pub array_dim: i32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rep_notify_func: Option<String>,
    /// An array's element, a map's key and value, an enum's underlying integer.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inner: Vec<ScriptProperty>,
}

/// `UFunction::Serialize`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptFunctionInfo {
    pub flags: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub flag_names: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_graph_function: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub event_graph_call_offset: i32,
}

/// A disassembled `Script` stream.
///
/// The two sizes are separate facts: `buffer_size` is what the VM executes and
/// `serialized_size` is what the file holds. Agreeing with both is what makes a
/// disassembly verifiable rather than merely plausible, so both are reported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptBytecodeInfo {
    pub buffer_size: u32,
    pub serialized_size: u32,
    /// Absent when the stream disassembled and both sizes agreed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub undecoded_reason: Option<String>,
    #[serde(default, skip_serializing_if = "is_zero_usize")]
    pub expressions: usize,
    /// How many of each opcode the stream contains.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub opcodes: BTreeMap<String, usize>,
    /// What the compiled code points at. This is the reference attribution a
    /// linker-table-only view cannot give: the tables say the package depends on
    /// a target, these say which compiled function reaches it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub references: Vec<ScriptBytecodeReference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScriptBytecodeReference {
    pub kind: String,
    pub target: String,
}

/// Byte-level export placement, emitted only for the `full` view. Focused views
/// (summary/logic/properties/references) omit this bookkeeping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExportSerialization {
    pub object_flags: u32,
    pub serial_offset: i64,
    pub serial_size: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script_serialization_start: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script_serialization_end: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropertyDecodeStatus {
    Complete,
    Empty,
    NonTaggedPayload,
    FailedAfterEntries,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssetProperty {
    pub name: String,
    pub type_name: String,
    #[serde(default, skip_serializing_if = "crate::model::is_zero_i32")]
    pub array_index: i32,
    pub value: DecodedValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemberReference {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<DecodedValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisCapability {
    pub kind: CapabilityKind,
    pub status: AnalysisStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityKind {
    PackageTables,
    ReferenceTables,
    TaggedProperties,
    EdGraphLogic,
    RigVmModel,
    RigVmBytecode,
    RigHierarchy,
    StateTreeSemantics,
    PcgSemantics,
    PackageVersion,
    /// Compiled Blueprint bytecode (`UStruct::Serialize`'s `Script`). Named for
    /// the same reason as `RigVmBytecode`: the source-level graph is decoded but
    /// the compiled form is not, and a consumer must be told so.
    BlueprintBytecode,
    /// Compiled Niagara VM/GPU payloads. Niagara editor graphs decode through the
    /// EdGraph model; the compiled representation does not.
    NiagaraCompiled,
}

impl CapabilityKind {
    /// Whether this capability is a compiled-payload gap: the source-level graph
    /// decodes but the engine's compiled form of it does not.
    ///
    /// These gaps say nothing about the package's reference or property evidence,
    /// which is why they are worth separating from every other reason an asset can
    /// be `partial`. A whole project of Blueprints is `partial` because of them.
    pub fn is_compiled_payload(self) -> bool {
        matches!(
            self,
            Self::BlueprintBytecode
                | Self::NiagaraCompiled
                | Self::RigVmBytecode
                | Self::RigHierarchy
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownOpaque {
    pub path: String,
    pub kind: KnownOpaqueKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_range: Option<OpaqueByteRange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnownOpaqueKind {
    PropertyValue,
    PreScriptRegion,
    PostPropertyTail,
    Metadata,
    Capability,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpaqueByteRange {
    pub start: u64,
    pub end: u64,
    pub size: u64,
    pub preview: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnalysisDiagnostic {
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub path: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<DecodedValue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Info,
}
