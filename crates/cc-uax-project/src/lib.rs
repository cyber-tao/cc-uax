mod analysis_summary;
mod cache;
mod entry_points;
mod layout;
mod model;
mod mount;
mod scanner;

pub use analysis_summary::{
    AnalysisDiagnosticSummary, AssetAnalysisSummary, CapabilitySummary, GraphSummary,
    KnownOpaqueGroup, KnownOpaqueSummary, PcgGraphSummary, ProjectAnalysisSummary,
    RigVmGraphSummary, StateTreeGraphSummary,
};
pub use cache::{CachePathError, CachePathPolicy};
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
