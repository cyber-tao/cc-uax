mod analysis_summary;
mod cache;
mod entry_points;
mod layout;
mod model;
mod mount;
mod scanner;

pub use analysis_summary::{
    AnalysisDiagnosticSummary, AssetAnalysisSummary, CapabilitySummary, GraphSummary,
    KnownOpaqueIdentity, KnownOpaqueSummary, PcgGraphSummary, ProjectAnalysisSummary,
    RigVmGraphSummary, StateTreeGraphSummary,
};
pub use cache::{CachePathError, CachePathPolicy};
pub use entry_points::{ConfigReference, ProjectEntryPoints};
pub use layout::{ProjectLayout, ProjectLayoutError};
pub use model::{
    Adjacency, AssetKind, AssetOwnership, AssetRecord, ExternalPackageKind, ProjectIndex,
    ScanDiagnostic, ScanDiagnosticSeverity, ScanFailure, ScanFailureStage, ScanStats,
};
pub use mount::{MountSpec, MountTable, MountTableError, package_path_from_relative};
pub use scanner::{ProjectScanError, ProjectScanner, ScanMode, ScanOptions};

#[cfg(test)]
mod tests;
