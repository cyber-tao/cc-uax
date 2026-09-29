mod analysis_summary;
mod cache;
mod entry_points;
mod layout;
mod model;
mod mount;
mod scanner;

/// Shape version of the scanned index and its per-asset summaries.
///
/// Distinct from the CLI's report schema: this one also gates cache reuse, so any
/// change to what a cached `AssetAnalysisSummary` means must bump it or a warm
/// scan will replay summaries built under the old meaning.
///
/// 9: native-flag-driven struct decoding and the strict tagged fallback change
/// cached property values and statuses.
///
/// 8: under-consumed fixed-width containers become opaque, incomplete values
/// downgrade `tagged_properties`, and nested diagnostic paths are attributed per
/// property, so cached statuses, capabilities and diagnostics differ.
///
/// 7: legacy containers with ByteProperty elements are resolved by exact fit, so
/// cached property values and diagnostics for 5.0–5.3 packages differ.
///
/// 6: cached summaries built before typed opaque values carry no `value_bytes`
/// and group value-level regions under the old ad-hoc reasons.
///
/// 5: cached summaries built before native-only payload classification, decoder
/// span accounting, the import-data prefix decoder, the TextConst layout gate and
/// the widened graph-node list carry different statuses and counters.
pub const PROJECT_INDEX_SCHEMA_VERSION: u32 = 9;

pub use analysis_summary::{
    AnalysisDiagnosticSummary, AssetAnalysisSummary, CapabilitySummary, GraphSummary,
    KnownOpaqueGroup, KnownOpaqueSummary, PcgGraphSummary, ProjectAnalysisSummary,
    ProjectCapabilityCount, ProjectReferenceEvidence, RigVmGraphSummary, StateTreeGraphSummary,
};
pub use cache::{CACHE_ROOT_ENV, CachePathError, CachePathPolicy};
pub use entry_points::{ConfigReference, ProjectEntryPoints};
pub use layout::{PluginContentRoot, ProjectLayout, ProjectLayoutError};
pub use model::{
    Adjacency, AssetKind, AssetOwnership, AssetRecord, ExternalPackageKind, ProjectIndex,
    ProjectReachability, ProjectReachabilityRoot, RootResolution, ScanDiagnostic,
    ScanDiagnosticSeverity, ScanFailure, ScanFailureStage, ScanStats,
};
pub use mount::{
    MountSpec, MountTable, MountTableError, package_path_from_relative, strip_asset_extension,
};
pub use scanner::{ProjectScanError, ProjectScanner, ScanMode, ScanOptions};

#[cfg(test)]
mod tests;
