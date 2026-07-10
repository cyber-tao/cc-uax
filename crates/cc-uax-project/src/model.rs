use crate::{
    AssetAnalysisSummary, MountTable, ProjectAnalysisSummary, ProjectEntryPoints, ProjectLayout,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

pub type Adjacency = BTreeMap<String, BTreeSet<String>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Asset,
    Map,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalPackageKind {
    Actor,
    Object,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AssetOwnership {
    ProjectAsset,
    External {
        external_kind: ExternalPackageKind,
        owner_package: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetRecord {
    pub package_path: String,
    pub mount_root: String,
    pub file_path: PathBuf,
    pub relative_path: String,
    pub asset_kind: AssetKind,
    pub ownership: AssetOwnership,
    pub forward_references: BTreeSet<String>,
    pub analysis: AssetAnalysisSummary,
}

impl AssetRecord {
    pub fn is_external(&self) -> bool {
        matches!(self.ownership, AssetOwnership::External { .. })
    }

    pub fn owner_package(&self) -> Option<&str> {
        match &self.ownership {
            AssetOwnership::External { owner_package, .. } => owner_package.as_deref(),
            AssetOwnership::ProjectAsset => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanFailureStage {
    Mount,
    Config,
    Discovery,
    Read,
    Parse,
    Index,
    Ownership,
    Cache,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanDiagnosticSeverity {
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanDiagnostic {
    pub severity: ScanDiagnosticSeverity,
    pub stage: ScanFailureStage,
    pub path: PathBuf,
    pub message: String,
}

impl ScanDiagnostic {
    pub(crate) fn warning(
        path: impl Into<PathBuf>,
        stage: ScanFailureStage,
        message: impl Into<String>,
    ) -> Self {
        Self {
            severity: ScanDiagnosticSeverity::Warning,
            stage,
            path: path.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanFailure {
    pub path: PathBuf,
    pub stage: ScanFailureStage,
    pub message: String,
}

impl ScanFailure {
    pub(crate) fn new(
        path: impl Into<PathBuf>,
        stage: ScanFailureStage,
        message: impl Into<String>,
    ) -> Self {
        Self {
            path: path.into(),
            stage,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanStats {
    pub discovered: usize,
    pub indexed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub external_actors: usize,
    pub external_objects: usize,
    pub owned_external_packages: usize,
    pub unowned_external_packages: usize,
    pub cache_hits: usize,
    pub cache_misses: usize,
    pub cached_parse_failures: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectIndex {
    pub layout: ProjectLayout,
    pub mounts: MountTable,
    pub entry_points: ProjectEntryPoints,
    pub analysis: ProjectAnalysisSummary,
    pub assets: BTreeMap<String, AssetRecord>,
    pub forward: Adjacency,
    pub reverse: Adjacency,
    pub ownership: BTreeMap<String, BTreeSet<String>>,
    pub ownership_closure: BTreeMap<String, BTreeSet<String>>,
    pub stats: ScanStats,
    pub failures: Vec<ScanFailure>,
    pub diagnostics: Vec<ScanDiagnostic>,
}

impl ProjectIndex {
    pub fn asset(&self, package_path: &str) -> Option<&AssetRecord> {
        let key = self.canonical_package(package_path)?;
        self.assets.get(key)
    }

    pub fn forward_references(&self, package_path: &str) -> Option<&BTreeSet<String>> {
        let key = self.canonical_package(package_path)?;
        self.forward.get(key)
    }

    pub fn reverse_referencers(&self, package_path: &str) -> Option<&BTreeSet<String>> {
        let key = self
            .reverse
            .keys()
            .find(|candidate| candidate.eq_ignore_ascii_case(package_path))?;
        self.reverse.get(key)
    }

    pub fn ownership_root<'a>(&'a self, package_path: &'a str) -> Option<&'a str> {
        let mut current = self.canonical_package(package_path)?;
        let mut visited = BTreeSet::new();
        while visited.insert(current.to_ascii_lowercase()) {
            let record = self.assets.get(current)?;
            let Some(owner) = record.owner_package() else {
                return Some(current);
            };
            current = self.canonical_package(owner)?;
        }
        None
    }

    pub fn closure_for(&self, package_path: &str) -> Option<BTreeSet<String>> {
        let root = self.ownership_root(package_path)?;
        Some(
            self.ownership_closure
                .get(root)
                .cloned()
                .unwrap_or_else(|| BTreeSet::from([root.to_string()])),
        )
    }

    pub fn effective_forward_references(&self, package_path: &str) -> Option<BTreeSet<String>> {
        let closure = self.closure_for(package_path)?;
        let mut references = BTreeSet::new();
        for member in &closure {
            if let Some(member_references) = self.forward.get(member) {
                references.extend(member_references.iter().cloned());
            }
        }
        references.retain(|reference| {
            !closure
                .iter()
                .any(|member| member.eq_ignore_ascii_case(reference))
        });
        Some(references)
    }

    pub(crate) fn canonical_package(&self, package_path: &str) -> Option<&str> {
        self.assets
            .keys()
            .find(|candidate| candidate.eq_ignore_ascii_case(package_path))
            .map(String::as_str)
    }
}
