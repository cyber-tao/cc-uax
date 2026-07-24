use crate::cache::{CacheEntry, ProjectCache};
use crate::entry_points::load_project_entry_points;
use crate::{
    Adjacency, AssetAnalysisSummary, AssetKind, AssetOwnership, AssetRecord, CachePathPolicy,
    ExternalPackageKind, MountTable, ProjectAnalysisSummary, ProjectEntryPoints, ProjectIndex,
    ProjectLayout, ScanDiagnostic, ScanFailure, ScanFailureStage, ScanStats,
    package_path_from_relative, strip_asset_extension,
};
use cc_uax_core::{AssetView, PackageView};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ScanMode {
    #[default]
    Strict,
    AllowPartial,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ScanOptions {
    pub mode: ScanMode,
    pub cache: CachePathPolicy,
}

#[derive(Debug, Clone)]
pub struct ProjectScanner {
    layout: ProjectLayout,
    mounts: MountTable,
}

impl ProjectScanner {
    pub fn new(layout: ProjectLayout) -> Self {
        let mounts = MountTable::default_for(&layout);
        Self { layout, mounts }
    }

    pub fn with_mounts(layout: ProjectLayout, mounts: MountTable) -> Self {
        Self { layout, mounts }
    }

    pub fn layout(&self) -> &ProjectLayout {
        &self.layout
    }

    pub fn mounts(&self) -> &MountTable {
        &self.mounts
    }

    pub fn scan(&self, options: ScanOptions) -> Result<ProjectIndex, ProjectScanError> {
        let mut failures = Vec::new();
        let (entry_points, mut diagnostics) = load_project_entry_points(&self.layout);
        let mut files = collect_mounted_files(&self.mounts, &mut failures);
        files.sort_by(|left, right| {
            left.package_root
                .cmp(&right.package_root)
                .then_with(|| normalized_path(&left.path).cmp(&normalized_path(&right.path)))
        });

        let discovered = files.len();
        let cache_open = open_cache(
            &options.cache,
            &self.layout,
            &mut failures,
            &mut diagnostics,
        );
        let mut cache = cache_open.cache;
        let mut fatal_cache_error = cache_open.fatal_error;
        let mut current_cache = HashMap::<String, CacheEntry>::new();
        let mut cache_hits = 0usize;
        let mut cache_misses = 0usize;
        let mut cached_parse_failures = 0usize;
        let mut records = Vec::new();
        let mut seen_packages = HashMap::<String, PathBuf>::new();

        for file in files {
            let package_path =
                match package_path_from_relative(&file.relative_path, &file.package_root) {
                    Ok(package_path) => package_path,
                    Err(error) => {
                        failures.push(ScanFailure::new(
                            &file.path,
                            ScanFailureStage::Index,
                            error.to_string(),
                        ));
                        continue;
                    }
                };
            let duplicate_key = package_path.to_ascii_lowercase();
            if let Some(previous) = seen_packages.get(&duplicate_key) {
                failures.push(ScanFailure::new(
                    &file.path,
                    ScanFailureStage::Index,
                    format!(
                        "duplicate package path {package_path}; first seen at {}",
                        previous.display()
                    ),
                ));
                continue;
            }
            seen_packages.insert(duplicate_key, file.path.clone());

            let (mtime, size) = match file_stamp(&file.path) {
                Ok(stamp) => stamp,
                Err(error) => {
                    failures.push(ScanFailure::new(&file.path, ScanFailureStage::Read, error));
                    continue;
                }
            };
            let cache_key = normalized_path(&file.path);
            let cached = cache
                .as_ref()
                .and_then(|cache| cache.lookup(&cache_key, mtime, size))
                .cloned();
            let cached_references = match cached {
                Some(entry) => {
                    cache_hits += 1;
                    if entry.parse_ok {
                        Some(entry.references)
                    } else {
                        cached_parse_failures += 1;
                        failures.push(ScanFailure::new(
                            &file.path,
                            ScanFailureStage::Parse,
                            entry
                                .parse_error
                                .clone()
                                .unwrap_or_else(|| "cached package parse failure".to_string()),
                        ));
                        current_cache.insert(cache_key, entry);
                        continue;
                    }
                }
                None => {
                    if cache.is_some() {
                        cache_misses += 1;
                    }
                    None
                }
            };
            let parsed = match read_asset(&file.path, cached_references) {
                Ok(parsed) => parsed,
                Err(ParseFileError::Read(message)) => {
                    failures.push(ScanFailure::new(
                        &file.path,
                        ScanFailureStage::Read,
                        message,
                    ));
                    continue;
                }
                Err(ParseFileError::Parse(message)) => {
                    failures.push(ScanFailure::new(
                        &file.path,
                        ScanFailureStage::Parse,
                        &message,
                    ));
                    if cache.is_some() {
                        current_cache.insert(
                            cache_key,
                            CacheEntry {
                                mtime,
                                size,
                                parse_ok: false,
                                references: Vec::new(),
                                parse_error: Some(message),
                            },
                        );
                    }
                    continue;
                }
            };
            if cache.is_some() {
                current_cache.insert(
                    cache_key,
                    CacheEntry {
                        mtime,
                        size,
                        parse_ok: true,
                        references: parsed.references.clone(),
                        parse_error: None,
                    },
                );
            }
            let Some(asset_kind) = asset_kind(&file.path) else {
                failures.push(ScanFailure::new(
                    &file.path,
                    ScanFailureStage::Index,
                    "mapped file has no supported asset extension",
                ));
                continue;
            };
            records.push(AssetRecord {
                package_path,
                mount_root: file.package_root,
                file_path: file.path,
                relative_path: file.relative_path.clone(),
                asset_kind,
                ownership: classify_ownership(&file.relative_path),
                forward_references: parsed.references.into_iter().collect(),
                analysis: parsed.analysis,
            });
        }

        if let Some(cache) = cache.as_mut()
            && let Err(message) = cache.store(&current_cache)
        {
            fatal_cache_error |= record_cache_issue(
                &options.cache,
                self.layout.project_root(),
                message,
                &mut failures,
                &mut diagnostics,
            );
        }

        let mut index = build_project_index(
            self.layout.clone(),
            self.mounts.clone(),
            entry_points,
            records,
            failures,
            diagnostics,
            discovered,
        );
        index.stats.cache_hits = cache_hits;
        index.stats.cache_misses = cache_misses;
        index.stats.cached_parse_failures = cached_parse_failures;
        if fatal_cache_error || (options.mode == ScanMode::Strict && !index.failures.is_empty()) {
            return Err(ProjectScanError {
                index: Box::new(index),
            });
        }
        Ok(index)
    }
}

#[derive(Debug)]
pub struct ProjectScanError {
    index: Box<ProjectIndex>,
}

impl ProjectScanError {
    pub fn index(&self) -> &ProjectIndex {
        &self.index
    }

    pub fn into_index(self) -> ProjectIndex {
        *self.index
    }
}

impl fmt::Display for ProjectScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "project scan failed with {} failure(s) across {} discovered asset(s)",
            self.index.failures.len(),
            self.index.stats.discovered
        )
    }
}

impl std::error::Error for ProjectScanError {}

pub(crate) fn build_project_index(
    layout: ProjectLayout,
    mounts: MountTable,
    entry_points: ProjectEntryPoints,
    mut records: Vec<AssetRecord>,
    mut failures: Vec<ScanFailure>,
    diagnostics: Vec<ScanDiagnostic>,
    discovered: usize,
) -> ProjectIndex {
    let canonical = records
        .iter()
        .map(|record| {
            (
                record.package_path.to_ascii_lowercase(),
                record.package_path.clone(),
            )
        })
        .collect::<HashMap<_, _>>();
    for record in &mut records {
        record.forward_references = record
            .forward_references
            .iter()
            .map(|reference| {
                canonical
                    .get(&reference.to_ascii_lowercase())
                    .cloned()
                    .unwrap_or_else(|| reference.clone())
            })
            .collect();
    }

    resolve_external_ownership(&mut records, &mut failures);
    let mut assets = BTreeMap::new();
    for record in records {
        assets.insert(record.package_path.clone(), record);
    }

    let mut forward = Adjacency::new();
    let mut reverse = Adjacency::new();
    let mut ownership = BTreeMap::<String, BTreeSet<String>>::new();
    let mut stats = ScanStats {
        discovered,
        indexed: assets.len(),
        ..ScanStats::default()
    };
    for record in assets.values() {
        forward.insert(
            record.package_path.clone(),
            record.forward_references.clone(),
        );
        for reference in &record.forward_references {
            reverse
                .entry(reference.clone())
                .or_default()
                .insert(record.package_path.clone());
        }
        if let AssetOwnership::External {
            external_kind,
            owner_package,
        } = &record.ownership
        {
            match external_kind {
                ExternalPackageKind::Actor => stats.external_actors += 1,
                ExternalPackageKind::Object => stats.external_objects += 1,
            }
            if let Some(owner) = owner_package {
                stats.owned_external_packages += 1;
                ownership
                    .entry(owner.clone())
                    .or_default()
                    .insert(record.package_path.clone());
            } else {
                stats.unowned_external_packages += 1;
            }
        }
    }
    let ownership_closure = build_ownership_closure(&assets, &ownership);
    stats.failed = failures.len();
    let failed_asset_count = failures
        .iter()
        .filter(|failure| {
            matches!(
                failure.stage,
                ScanFailureStage::Read | ScanFailureStage::Parse | ScanFailureStage::Index
            )
        })
        .map(|failure| normalized_path(&failure.path))
        .collect::<BTreeSet<_>>()
        .len();
    stats.skipped = discovered.saturating_sub(assets.len() + failed_asset_count);
    let analysis = ProjectAnalysisSummary::aggregate(
        assets.values().map(|record| &record.analysis),
        failures.len(),
    );

    ProjectIndex {
        layout,
        mounts,
        entry_points,
        analysis,
        assets,
        forward,
        reverse,
        ownership,
        ownership_closure,
        stats,
        failures,
        diagnostics,
        canonical_lookup: HashMap::new(),
    }
    .with_canonical_lookup()
}

#[derive(Debug)]
struct MountedFile {
    path: PathBuf,
    package_root: String,
    relative_path: String,
}

fn collect_mounted_files(mounts: &MountTable, failures: &mut Vec<ScanFailure>) -> Vec<MountedFile> {
    let mut files = Vec::new();
    let mut seen_mounts = HashMap::<String, PathBuf>::new();
    let mut seen_roots = HashMap::<String, String>::new();
    let mut seen_files = HashMap::<String, String>::new();
    for mount in mounts.mounts() {
        let package_key = mount.package_root().to_ascii_lowercase();
        if let Some(previous) = seen_mounts.get(&package_key) {
            failures.push(ScanFailure::new(
                mount.disk_root(),
                ScanFailureStage::Mount,
                format!(
                    "duplicate mount package root {}; first mapped to {}",
                    mount.package_root(),
                    previous.display()
                ),
            ));
            continue;
        }
        let disk_key = normalized_path(mount.disk_root());
        if let Some(previous) = seen_roots.get(&disk_key) {
            failures.push(ScanFailure::new(
                mount.disk_root(),
                ScanFailureStage::Mount,
                format!(
                    "duplicate mount disk root {}; first mapped to {previous}",
                    mount.disk_root().display()
                ),
            ));
            continue;
        }
        seen_mounts.insert(package_key, mount.disk_root().to_path_buf());
        seen_roots.insert(disk_key, mount.package_root().to_string());

        let mut mounted_paths = Vec::new();
        collect_asset_files(mount.disk_root(), &mut mounted_paths, failures);
        for path in mounted_paths {
            let file_key = normalized_path(&path);
            if let Some(previous) = seen_files.get(&file_key) {
                failures.push(ScanFailure::new(
                    &path,
                    ScanFailureStage::Mount,
                    format!(
                        "asset is covered by multiple mounts: {previous} and {}",
                        mount.package_root()
                    ),
                ));
                continue;
            }
            let relative = match path.strip_prefix(mount.disk_root()) {
                Ok(relative) => relative,
                Err(error) => {
                    failures.push(ScanFailure::new(
                        &path,
                        ScanFailureStage::Mount,
                        format!("asset is outside mapped disk root: {error}"),
                    ));
                    continue;
                }
            };
            let relative_path = relative.to_string_lossy().replace('\\', "/");
            seen_files.insert(file_key, mount.package_root().to_string());
            files.push(MountedFile {
                path,
                package_root: mount.package_root().to_string(),
                relative_path,
            });
        }
    }
    files
}

fn collect_asset_files(root: &Path, files: &mut Vec<PathBuf>, failures: &mut Vec<ScanFailure>) {
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) => {
            failures.push(ScanFailure::new(
                root,
                ScanFailureStage::Discovery,
                error.to_string(),
            ));
            return;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                failures.push(ScanFailure::new(
                    root,
                    ScanFailureStage::Discovery,
                    error.to_string(),
                ));
                continue;
            }
        };
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                failures.push(ScanFailure::new(
                    &path,
                    ScanFailureStage::Discovery,
                    error.to_string(),
                ));
                continue;
            }
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_asset_files(&path, files, failures);
        } else if file_type.is_file() && asset_kind(&path).is_some() {
            files.push(path);
        }
    }
}

struct CacheOpenResult {
    cache: Option<ProjectCache>,
    fatal_error: bool,
}

fn open_cache(
    policy: &CachePathPolicy,
    layout: &ProjectLayout,
    failures: &mut Vec<ScanFailure>,
    diagnostics: &mut Vec<ScanDiagnostic>,
) -> CacheOpenResult {
    let path = match policy.resolve(layout) {
        Ok(Some(path)) => path,
        Ok(None) => {
            return CacheOpenResult {
                cache: None,
                fatal_error: false,
            };
        }
        Err(error) => {
            let fatal_error = record_cache_issue(
                policy,
                layout.project_root(),
                error.to_string(),
                failures,
                diagnostics,
            );
            return CacheOpenResult {
                cache: None,
                fatal_error,
            };
        }
    };
    match ProjectCache::open(&path) {
        Ok(cache) => CacheOpenResult {
            cache: Some(cache),
            fatal_error: false,
        },
        Err(message) => {
            let fatal_error = record_cache_issue(policy, path, message, failures, diagnostics);
            CacheOpenResult {
                cache: None,
                fatal_error,
            }
        }
    }
}

fn record_cache_issue(
    policy: &CachePathPolicy,
    path: impl Into<PathBuf>,
    message: impl Into<String>,
    failures: &mut Vec<ScanFailure>,
    diagnostics: &mut Vec<ScanDiagnostic>,
) -> bool {
    let path = path.into();
    let message = message.into();
    if matches!(policy, CachePathPolicy::CustomFile(_)) {
        failures.push(ScanFailure::new(path, ScanFailureStage::Cache, message));
        true
    } else {
        diagnostics.push(ScanDiagnostic::warning(
            path,
            ScanFailureStage::Cache,
            message,
        ));
        false
    }
}

fn file_stamp(path: &Path) -> Result<(i64, i64), String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    let size = i64::try_from(metadata.len()).map_err(|_| "file size exceeds i64".to_string())?;
    let mtime = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .and_then(|duration| i64::try_from(duration.as_nanos()).ok())
        .unwrap_or(0);
    Ok((mtime, size))
}

enum ParseFileError {
    Read(String),
    Parse(String),
}

struct ParsedAsset {
    references: Vec<String>,
    analysis: AssetAnalysisSummary,
}

fn read_asset(
    path: &Path,
    cached_references: Option<Vec<String>>,
) -> Result<ParsedAsset, ParseFileError> {
    let data = fs::read(path).map_err(|error| ParseFileError::Read(error.to_string()))?;
    let view =
        PackageView::parse(&data).map_err(|error| ParseFileError::Parse(format!("{error:#}")))?;
    let references = cached_references.unwrap_or_else(|| {
        let references = view.references();
        references
            .assets
            .into_iter()
            .chain(references.scripts)
            .chain(references.soft)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    });
    let analysis = view.analyze(AssetView::Full);
    Ok(ParsedAsset {
        references,
        analysis: AssetAnalysisSummary::from_analysis(&analysis),
    })
}

fn asset_kind(path: &Path) -> Option<AssetKind> {
    let extension = path.extension()?.to_str()?;
    if extension.eq_ignore_ascii_case("uasset") {
        Some(AssetKind::Asset)
    } else if extension.eq_ignore_ascii_case("umap") {
        Some(AssetKind::Map)
    } else {
        None
    }
}

fn classify_ownership(relative_path: &str) -> AssetOwnership {
    let first = relative_path.split('/').next().unwrap_or_default();
    if first.eq_ignore_ascii_case("__ExternalActors__") {
        AssetOwnership::External {
            external_kind: ExternalPackageKind::Actor,
            owner_package: None,
        }
    } else if first.eq_ignore_ascii_case("__ExternalObjects__") {
        AssetOwnership::External {
            external_kind: ExternalPackageKind::Object,
            owner_package: None,
        }
    } else {
        AssetOwnership::ProjectAsset
    }
}

fn resolve_external_ownership(records: &mut [AssetRecord], failures: &mut Vec<ScanFailure>) {
    let mut owner_index: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for record in records.iter() {
        let key = record.mount_root.to_ascii_lowercase();
        let relative = strip_asset_extension(&record.relative_path).to_string();
        owner_index
            .entry(key)
            .or_default()
            .push((relative, record.package_path.clone()));
    }

    for record in records.iter_mut().filter(|record| record.is_external()) {
        let tail = record
            .relative_path
            .split_once('/')
            .map(|(_, tail)| tail)
            .unwrap_or_default();
        let mount_key = record.mount_root.to_ascii_lowercase();
        let candidates = owner_index.get(&mount_key);
        let owner = candidates.and_then(|candidates| {
            candidates
                .iter()
                .filter(|(relative, package)| {
                    !package.eq_ignore_ascii_case(&record.package_path)
                        && path_has_prefix(tail, relative)
                })
                .max_by_key(|(relative, _)| relative.len())
                .map(|(_, package)| package.clone())
        });
        let AssetOwnership::External { owner_package, .. } = &mut record.ownership else {
            continue;
        };
        if owner.is_none() {
            failures.push(ScanFailure::new(
                &record.file_path,
                ScanFailureStage::Ownership,
                format!(
                    "could not resolve World Partition owner for {}",
                    record.package_path
                ),
            ));
        }
        *owner_package = owner;
    }
}

fn build_ownership_closure(
    assets: &BTreeMap<String, AssetRecord>,
    ownership: &BTreeMap<String, BTreeSet<String>>,
) -> BTreeMap<String, BTreeSet<String>> {
    let mut closures = BTreeMap::<String, BTreeSet<String>>::new();
    for package in assets.keys() {
        let mut root = package.as_str();
        let mut visited = HashSet::new();
        while visited.insert(root.to_ascii_lowercase()) {
            let Some(owner) = assets.get(root).and_then(AssetRecord::owner_package) else {
                break;
            };
            root = owner;
        }
        if root != package || ownership.contains_key(package) {
            closures
                .entry(root.to_string())
                .or_default()
                .insert(package.clone());
        }
    }
    for (root, closure) in &mut closures {
        closure.insert(root.clone());
    }
    closures
}

fn path_has_prefix(path: &str, prefix: &str) -> bool {
    path.eq_ignore_ascii_case(prefix)
        || path.get(prefix.len()..).is_some_and(|tail| {
            tail.starts_with('/') && path[..prefix.len()].eq_ignore_ascii_case(prefix)
        })
}

fn normalized_path(path: &Path) -> String {
    let value = path.to_string_lossy().replace('\\', "/");
    if cfg!(target_os = "windows") {
        value.to_ascii_lowercase()
    } else {
        value
    }
}
