use crate::cache::{CacheEntry, CachedParse, ProjectCache};
use crate::entry_points::load_project_entry_points;
use crate::{
    Adjacency, AssetAnalysisSummary, AssetKind, AssetOwnership, AssetRecord, CachePathPolicy,
    ExternalPackageKind, MountTable, ProjectAnalysisSummary, ProjectEntryPoints, ProjectIndex,
    ProjectLayout, ProjectReachability, ProjectReachabilityRoot, RootResolution, ScanDiagnostic,
    ScanFailure, ScanFailureStage, ScanStats, package_path_from_relative, strip_asset_extension,
};
use cc_uax_core::{AnalysisStatus, AssetAnalysis, AssetView, DecodedValue, PackageView};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
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

    pub fn scan(&self, options: ScanOptions) -> Result<ProjectIndex, ProjectScanError> {
        let mut failures = Vec::new();
        let (entry_points, mut diagnostics) = load_project_entry_points(&self.layout);
        let (mut files, skipped_symlinks) =
            collect_mounted_files(&self.mounts, &mut failures, &mut diagnostics);
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

            let Some(asset_kind) = asset_kind(&file.path) else {
                failures.push(ScanFailure::new(
                    &file.path,
                    ScanFailureStage::Index,
                    "mapped file has no supported asset extension",
                ));
                continue;
            };
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
            match cached {
                Some(entry) => {
                    // Counted per branch, not up front: a fresh `Ok` entry with no
                    // stored analysis still needs a full re-read, and calling that
                    // a hit made warm-cache stats claim work that did not happen.
                    match entry.parse {
                        CachedParse::Ok => {
                            if let Some(analysis) = entry.analysis.clone() {
                                cache_hits += 1;
                                records.push(AssetRecord {
                                    package_path,
                                    mount_root: file.package_root,
                                    file_path: file.path,
                                    relative_path: file.relative_path.clone(),
                                    asset_kind,
                                    ownership: classify_ownership(&file.relative_path),
                                    forward_references: entry.references.iter().cloned().collect(),
                                    owned_sublevels: entry
                                        .owned_sublevels
                                        .iter()
                                        .cloned()
                                        .collect(),
                                    analysis,
                                });
                                current_cache.insert(cache_key, entry);
                                continue;
                            }
                        }
                        CachedParse::Unsupported => {
                            cache_hits += 1;
                            records.push(AssetRecord {
                                package_path,
                                mount_root: file.package_root,
                                file_path: file.path,
                                relative_path: file.relative_path.clone(),
                                asset_kind,
                                ownership: classify_ownership(&file.relative_path),
                                forward_references: BTreeSet::new(),
                                owned_sublevels: BTreeSet::new(),
                                analysis: AssetAnalysisSummary::unsupported(
                                    entry
                                        .reason
                                        .clone()
                                        .unwrap_or_else(unsupported_package_fallback_reason),
                                ),
                            });
                            current_cache.insert(cache_key, entry);
                            continue;
                        }
                        CachedParse::Failed => {
                            cache_hits += 1;
                            cached_parse_failures += 1;
                            failures.push(ScanFailure::new(
                                &file.path,
                                ScanFailureStage::Parse,
                                entry
                                    .reason
                                    .clone()
                                    .unwrap_or_else(|| "cached package parse failure".to_string()),
                            ));
                            current_cache.insert(cache_key, entry);
                            continue;
                        }
                    }
                    // Fresh entry that carried no analysis: re-read it, and count
                    // the miss, since that is the work actually done.
                    if cache.is_some() {
                        cache_misses += 1;
                    }
                }
                None => {
                    if cache.is_some() {
                        cache_misses += 1;
                    }
                }
            };
            let parsed = match read_asset(&file.path) {
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
                                parse: CachedParse::Failed,
                                references: Vec::new(),
                                owned_sublevels: Vec::new(),
                                analysis: None,
                                reason: Some(message),
                            },
                        );
                    }
                    continue;
                }
                // A package the parser deliberately does not target is truthful
                // `unsupported` evidence about a real asset, not a scan failure,
                // so it is indexed instead of aborting a strict scan.
                Err(ParseFileError::Unsupported(message)) => {
                    if cache.is_some() {
                        current_cache.insert(
                            cache_key.clone(),
                            CacheEntry {
                                mtime,
                                size,
                                parse: CachedParse::Unsupported,
                                references: Vec::new(),
                                owned_sublevels: Vec::new(),
                                analysis: None,
                                reason: Some(message.clone()),
                            },
                        );
                    }
                    records.push(AssetRecord {
                        package_path,
                        mount_root: file.package_root,
                        file_path: file.path,
                        relative_path: file.relative_path.clone(),
                        asset_kind,
                        ownership: classify_ownership(&file.relative_path),
                        forward_references: BTreeSet::new(),
                        owned_sublevels: BTreeSet::new(),
                        analysis: AssetAnalysisSummary::unsupported(message),
                    });
                    continue;
                }
            };
            if cache.is_some() {
                current_cache.insert(
                    cache_key,
                    CacheEntry {
                        mtime,
                        size,
                        parse: CachedParse::Ok,
                        references: parsed.references.clone(),
                        owned_sublevels: parsed.owned_sublevels.iter().cloned().collect(),
                        analysis: Some(parsed.analysis.clone()),
                        reason: None,
                    },
                );
            }
            records.push(AssetRecord {
                package_path,
                mount_root: file.package_root,
                file_path: file.path,
                relative_path: file.relative_path.clone(),
                asset_kind,
                ownership: classify_ownership(&file.relative_path),
                forward_references: parsed.references.into_iter().collect(),
                owned_sublevels: parsed.owned_sublevels,
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
        index.stats.skipped_symlinks = skipped_symlinks;
        index.stats.cache_hits = cache_hits;
        index.stats.cache_misses = cache_misses;
        index.stats.cached_parse_failures = cached_parse_failures;
        if options.mode == ScanMode::Strict && (fatal_cache_error || !index.failures.is_empty()) {
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
    let mut canonical = records
        .iter()
        .map(|record| {
            (
                record.package_path.to_ascii_lowercase(),
                record.package_path.clone(),
            )
        })
        .collect::<HashMap<_, _>>();
    // Normalize repeated external references (no project record) to their first-seen
    // casing so one target does not appear under multiple cases in adjacency.
    for record in &records {
        for reference in &record.forward_references {
            canonical
                .entry(reference.to_ascii_lowercase())
                .or_insert_with(|| reference.clone());
        }
    }
    for record in &mut records {
        // UE writes a package's own name into SoftPackageReferences (e.g. a
        // Blueprint's GeneratedClass). That is not a cross-package edge, so drop
        // the self-reference here; the asset-level `references.soft` still keeps
        // the serialized fact.
        let self_key = record.package_path.to_ascii_lowercase();
        record.forward_references = record
            .forward_references
            .iter()
            .filter(|reference| reference.to_ascii_lowercase() != self_key)
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
    let mut ownership_closure = build_ownership_closure(&assets, &ownership);
    attach_sublevel_ownership(&assets, &mut ownership_closure);
    let reachability = build_project_reachability(
        &entry_points,
        &assets,
        &forward,
        &reverse,
        &ownership_closure,
        failed_asset_count,
        &canonical,
    );
    // `stats.failed` counts asset-level failures (read/parse/index of a discovered
    // asset) so the accounting `discovered == indexed + failed + skipped` holds.
    // Infrastructure failures (mount/discovery/ownership/cache) are about the scan,
    // not a discovered asset; they stay visible in `failures` and `analysis.scan_failures`.
    stats.failed = failed_asset_count;
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
        reachability,
        stats,
        failures,
        diagnostics,
        canonical_lookup: HashMap::new(),
    }
    .with_canonical_lookup()
}

fn build_project_reachability(
    entry_points: &ProjectEntryPoints,
    assets: &BTreeMap<String, AssetRecord>,
    forward: &Adjacency,
    reverse: &Adjacency,
    ownership_closure: &BTreeMap<String, BTreeSet<String>>,
    failed_assets: usize,
    canonical: &HashMap<String, String>,
) -> ProjectReachability {
    let configured_roots = configured_roots(entry_points, assets, canonical);
    let mut reachable_runtime_packages = BTreeSet::new();
    let mut ownership_closure_members = BTreeSet::new();
    let mut queue = configured_roots
        .iter()
        .filter_map(|root| root.resolved_package.clone())
        .collect::<VecDeque<_>>();

    while let Some(package) = queue.pop_front() {
        if !reachable_runtime_packages.insert(package.clone()) {
            continue;
        }
        if let Some(closure) = ownership_closure.get(&package) {
            for member in closure {
                if member != &package {
                    ownership_closure_members.insert(member.clone());
                }
                if !reachable_runtime_packages.contains(member) {
                    queue.push_back(member.clone());
                }
            }
        }
        if let Some(references) = forward.get(&package) {
            for reference in references {
                if let Some(resolved) = canonical_package(canonical, reference)
                    && !reachable_runtime_packages.contains(&resolved)
                {
                    queue.push_back(resolved);
                }
            }
        }
    }

    let mut unreachable_project_assets = BTreeSet::new();
    let mut isolated_project_assets = BTreeSet::new();
    let mut partial_packages = BTreeSet::new();
    let mut unsupported_packages = BTreeSet::new();
    for (package, record) in assets {
        match record.analysis.status {
            AnalysisStatus::Partial => {
                partial_packages.insert(package.clone());
            }
            AnalysisStatus::Unsupported => {
                unsupported_packages.insert(package.clone());
            }
            AnalysisStatus::Complete => {}
        }
        if matches!(record.ownership, AssetOwnership::ProjectAsset)
            && !reachable_runtime_packages.contains(package)
        {
            unreachable_project_assets.insert(package.clone());
            let no_forward = forward.get(package).is_none_or(BTreeSet::is_empty);
            let no_reverse = reverse.get(package).is_none_or(BTreeSet::is_empty);
            if no_forward && no_reverse {
                isolated_project_assets.insert(package.clone());
            }
        }
    }

    ProjectReachability {
        configured_roots,
        reachable_runtime_packages,
        ownership_closure_members,
        unreachable_project_assets,
        isolated_project_assets,
        partial_packages,
        unsupported_packages,
        failed_assets,
    }
}

fn configured_roots(
    entry_points: &ProjectEntryPoints,
    assets: &BTreeMap<String, AssetRecord>,
    canonical: &HashMap<String, String>,
) -> Vec<ProjectReachabilityRoot> {
    let mut roots = Vec::new();
    for reference in entry_points.defaults.values() {
        roots.push(reachability_root(None, reference, assets, canonical));
    }
    for (platform, references) in &entry_points.platforms {
        for reference in references.values() {
            roots.push(reachability_root(
                Some(platform),
                reference,
                assets,
                canonical,
            ));
        }
    }
    // Cook roots are what a build actually ships, so they are roots in their own
    // right. `GameDefaultMap` is frequently a developer map, which left the real
    // shipped maps looking unreachable.
    for reference in &entry_points.cook_roots {
        roots.push(reachability_root(None, reference, assets, canonical));
    }
    for reference in &entry_points.cook_directories {
        let prefix = format!("{}/", reference.package_path.trim_end_matches('/'));
        let mut matched = false;
        for package in assets.keys() {
            if package
                .to_ascii_lowercase()
                .starts_with(&prefix.to_ascii_lowercase())
            {
                matched = true;
                roots.push(ProjectReachabilityRoot {
                    key: reference.key.clone(),
                    platform: None,
                    source: reference.source.clone(),
                    object_path: reference.object_path.clone(),
                    package_path: package.clone(),
                    resolved_package: Some(package.clone()),
                    resolution: RootResolution::Indexed,
                });
            }
        }
        if !matched {
            roots.push(reachability_root(None, reference, assets, canonical));
        }
    }
    roots
}

fn reachability_root(
    platform: Option<&String>,
    reference: &crate::ConfigReference,
    assets: &BTreeMap<String, AssetRecord>,
    canonical: &HashMap<String, String>,
) -> ProjectReachabilityRoot {
    let resolved_package = canonical_package(canonical, &reference.package_path);
    // `canonical` also holds every reference *target*, including packages under no
    // scanned mount, so a resolved name alone never proved the root exists here.
    let resolution = match &resolved_package {
        Some(package) if assets.contains_key(package) => RootResolution::Indexed,
        Some(_) => RootResolution::ReferencedOnly,
        None => RootResolution::Unresolved,
    };
    ProjectReachabilityRoot {
        key: reference.key.clone(),
        platform: platform.cloned(),
        source: reference.source.clone(),
        object_path: reference.object_path.clone(),
        package_path: reference.package_path.clone(),
        resolved_package,
        resolution,
    }
}

fn canonical_package(canonical: &HashMap<String, String>, package: &str) -> Option<String> {
    canonical.get(&package.to_ascii_lowercase()).cloned()
}

#[derive(Debug)]
struct MountedFile {
    path: PathBuf,
    package_root: String,
    relative_path: String,
}

fn collect_mounted_files(
    mounts: &MountTable,
    failures: &mut Vec<ScanFailure>,
    diagnostics: &mut Vec<ScanDiagnostic>,
) -> (Vec<MountedFile>, usize) {
    let mut files = Vec::new();
    let mut skipped_symlinks = 0usize;
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
        collect_asset_files(
            mount.disk_root(),
            &mut mounted_paths,
            failures,
            diagnostics,
            &mut skipped_symlinks,
        );
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
    (files, skipped_symlinks)
}

fn collect_asset_files(
    root: &Path,
    files: &mut Vec<PathBuf>,
    failures: &mut Vec<ScanFailure>,
    diagnostics: &mut Vec<ScanDiagnostic>,
    skipped_symlinks: &mut usize,
) {
    // Iterative walk with an explicit stack so deeply nested trees cannot overflow the stack.
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) => {
                failures.push(ScanFailure::new(
                    &dir,
                    ScanFailureStage::Discovery,
                    error.to_string(),
                ));
                continue;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => {
                    failures.push(ScanFailure::new(
                        &dir,
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
                // Not followed (avoids cycles), but the skipped gap is surfaced.
                *skipped_symlinks += 1;
                diagnostics.push(ScanDiagnostic::warning(
                    &path,
                    ScanFailureStage::Discovery,
                    "symbolic link skipped; not followed during discovery",
                ));
                continue;
            }
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file() && asset_kind(&path).is_some() {
                files.push(path);
            }
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
        Ok(cache) => {
            if let Some(reason) = cache.reset_reason() {
                diagnostics.push(ScanDiagnostic::info(
                    path.clone(),
                    ScanFailureStage::Cache,
                    reason.to_string(),
                ));
            }
            CacheOpenResult {
                cache: Some(cache),
                fatal_error: false,
            }
        }
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
        .unwrap_or(crate::cache::UNKNOWN_MTIME);
    Ok((mtime, size))
}

enum ParseFileError {
    Read(String),
    Parse(String),
    /// A readable package the parser deliberately does not target. Not a failure:
    /// the asset is indexed as `unsupported` evidence.
    Unsupported(String),
}

struct ParsedAsset {
    references: Vec<String>,
    owned_sublevels: BTreeSet<String>,
    analysis: AssetAnalysisSummary,
}

/// Used only when a cache row records an unsupported package without its
/// original explanation, which a hand-edited or truncated database could produce.
fn unsupported_package_fallback_reason() -> String {
    "package is outside the supported UE5 editor package range".to_string()
}

fn read_asset(path: &Path) -> Result<ParsedAsset, ParseFileError> {
    let data = fs::read(path).map_err(|error| ParseFileError::Read(error.to_string()))?;
    let view = PackageView::parse(&data).map_err(|error| {
        let message = error.to_string();
        if error.is_out_of_scope() {
            ParseFileError::Unsupported(message)
        } else {
            ParseFileError::Parse(message)
        }
    })?;
    let references = view.references();
    let references = references
        .assets
        .into_iter()
        .chain(references.scripts)
        .chain(references.soft)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let analysis = view.analyze(AssetView::Full);
    let owned_sublevels = collect_owned_sublevels(&analysis);
    Ok(ParsedAsset {
        references,
        owned_sublevels,
        analysis: AssetAnalysisSummary::from_analysis(&analysis),
    })
}

fn collect_owned_sublevels(analysis: &AssetAnalysis) -> BTreeSet<String> {
    let mut owned = BTreeSet::new();
    for export in &analysis.exports {
        if !is_level_instance_or_packed_level_actor(&export.class) {
            continue;
        }
        for property in &export.properties {
            if !is_world_asset_property(&property.name) {
                continue;
            }
            collect_package_paths_from_value(&property.value, &mut owned);
        }
    }
    owned
}

fn is_level_instance_or_packed_level_actor(class_full: &str) -> bool {
    class_full.rsplit(['.', '/']).next().is_some_and(|simple| {
        matches!(
            simple,
            "LevelInstance" | "PackedLevelActor" | "PackedLevelActorDesc"
        )
    })
}

fn is_world_asset_property(name: &str) -> bool {
    matches!(
        name,
        "WorldAsset" | "PackedWorldAsset" | "OverrideWorldAsset"
    )
}

fn collect_package_paths_from_value(value: &DecodedValue, out: &mut BTreeSet<String>) {
    if let Some(path) = value.as_str() {
        if let Some(package) = package_path_from_soft_asset_path(path) {
            out.insert(package);
        }
        return;
    }
    if let Some(object) = value.as_object() {
        if let Some(path) = object.get("asset_path").and_then(DecodedValue::as_str)
            && let Some(package) = package_path_from_soft_asset_path(path)
        {
            out.insert(package);
        }
        for nested in object.values() {
            collect_package_paths_from_value(nested, out);
        }
        return;
    }
    if let Some(array) = value.as_array() {
        for nested in array {
            collect_package_paths_from_value(nested, out);
        }
    }
}

fn package_path_from_soft_asset_path(path: &str) -> Option<String> {
    let path = path.trim();
    if path.is_empty() || path.eq_ignore_ascii_case("None") || !path.starts_with('/') {
        return None;
    }
    let package = path
        .split_once('.')
        .map(|(package, _)| package)
        .unwrap_or(path);
    (!package.is_empty()).then(|| package.to_string())
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

fn attach_sublevel_ownership(
    assets: &BTreeMap<String, AssetRecord>,
    closures: &mut BTreeMap<String, BTreeSet<String>>,
) {
    let nested = closures.clone();
    for record in assets.values() {
        if record.owned_sublevels.is_empty() {
            continue;
        }
        let root = record
            .owner_package()
            .unwrap_or(record.package_path.as_str())
            .to_string();
        let entry = closures.entry(root.clone()).or_default();
        entry.insert(root);
        for sublevel in &record.owned_sublevels {
            let Some(resolved) = resolve_scanned_package(sublevel, assets) else {
                continue;
            };
            entry.insert(resolved.clone());
            if let Some(members) = nested.get(&resolved) {
                entry.extend(members.iter().cloned());
            }
        }
    }
}

fn resolve_scanned_package(
    package: &str,
    assets: &BTreeMap<String, AssetRecord>,
) -> Option<String> {
    if assets.contains_key(package) {
        return Some(package.to_string());
    }
    assets
        .keys()
        .find(|key| key.eq_ignore_ascii_case(package))
        .cloned()
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
