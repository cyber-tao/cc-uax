pub mod args;
mod budget;

use crate::args::{AssetArgs, AssetViewArg, Cli, Command, ProjectArgs};
use anyhow::{Context, Result};
use cc_uax_core::{AnalysisStatus, AssetAnalysis, AssetView, PackageView};
use cc_uax_project::{
    AssetKind, AssetOwnership, CachePathPolicy, MountTable, ProjectIndex, ProjectLayout,
    ProjectScanner, ScanDiagnosticSeverity, ScanFailureStage, ScanMode, ScanOptions,
    strip_asset_extension,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Bumped to 6 when out-of-scope packages became `unsupported` inventory entries
/// with an `unsupported_reason`, and per-asset `known_opaque` replaced its
/// per-region `identities` list with aggregated `groups`.
const PROJECT_REPORT_SCHEMA_VERSION: u32 = 6;

pub fn run(cli: Cli) -> ExitCode {
    match execute(&cli) {
        Ok(exit) => exit,
        Err(error) => {
            // Error documents use the schema of the command that produced them, so an
            // asset failure is not mislabeled with the project report schema.
            let schema_version = match &cli.command {
                Command::Asset(_) => cc_uax_core::ASSET_ANALYSIS_SCHEMA_VERSION,
                Command::Project(_) => PROJECT_REPORT_SCHEMA_VERSION,
            };
            let failure = CommandFailure {
                schema_version,
                status: "error",
                message: format!("{error:#}"),
            };
            let text = render_json(&failure, cli.compact)
                .unwrap_or_else(|_| "{\"status\":\"error\"}".to_string());
            // Overwrite --output with the error document so a stale prior report
            // is never left behind to be read as a fresh success.
            if let Some(path) = cli.output.as_deref() {
                let _ = fs::write(path, format!("{text}\n"));
            }
            let _ = writeln!(io::stderr().lock(), "{text}");
            ExitCode::FAILURE
        }
    }
}

fn execute(cli: &Cli) -> Result<ExitCode> {
    match &cli.command {
        Command::Asset(args) => {
            let analysis = analyze_asset(args)?;
            write_json(
                &analysis,
                cli.compact,
                cli.max_output_bytes,
                cli.output.as_deref(),
            )?;
            // A produced report exits 0. `status` carries evidence completeness;
            // inherent partial/unsupported evidence is not a process failure. Only
            // an unreadable or unparseable file (the `?` above) exits nonzero.
            Ok(ExitCode::SUCCESS)
        }
        Command::Project(args) => {
            let (report, hard_failure) = analyze_project(args)?;
            write_json(
                &report,
                cli.compact,
                cli.max_output_bytes,
                cli.output.as_deref(),
            )?;
            Ok(if hard_failure {
                ExitCode::from(2)
            } else {
                ExitCode::SUCCESS
            })
        }
    }
}

fn analyze_asset(args: &AssetArgs) -> Result<AssetAnalysis> {
    let bytes =
        fs::read(&args.file).with_context(|| format!("failed to read {}", args.file.display()))?;
    let package = PackageView::parse(&bytes)
        .with_context(|| format!("failed to parse {}", args.file.display()))?;
    Ok(package.analyze(args.view.into()))
}

fn analyze_project(args: &ProjectArgs) -> Result<(ProjectReport, bool)> {
    let layout = ProjectLayout::discover(&args.root)
        .with_context(|| format!("failed to discover project from {}", args.root.display()))?;
    let mounts = project_mounts(&layout, &args.mount)?;
    let scanner = ProjectScanner::with_mounts(layout.clone(), mounts);
    let options = ScanOptions {
        mode: if args.allow_partial {
            ScanMode::AllowPartial
        } else {
            ScanMode::Strict
        },
        cache: cache_policy(args),
    };
    // Strict exit 2 is reserved for hard scan failures (read/parse/index/mount/
    // cache). Inherent partial evidence — e.g. known-opaque compiled RigVM
    // bytecode or an unsupported package version — keeps a truthful non-complete
    // status but is not a process failure.
    let (index, hard_failure) = match scanner.scan(options) {
        Ok(index) => (index, false),
        Err(error) => (error.into_index(), true),
    };
    let (focused, focus_issues) = analyze_focused_assets(&index, &args.focus);
    // A focus failure is a hard failure under strict mode, but the report is
    // still produced so the caller never reads a stale --output as success.
    let focus_failure = !focus_issues.is_empty() && !args.allow_partial;
    let report = ProjectReport::from_index(&index, focused, focus_issues);
    Ok((report, hard_failure || focus_failure))
}

fn project_mounts(layout: &ProjectLayout, requested: &[String]) -> Result<MountTable> {
    MountTable::resolve(layout, requested).context("invalid --mount mapping")
}

fn cache_policy(args: &ProjectArgs) -> CachePathPolicy {
    if args.no_cache {
        CachePathPolicy::Disabled
    } else if let Some(path) = &args.cache_file {
        CachePathPolicy::CustomFile(path.clone())
    } else {
        CachePathPolicy::System
    }
}

/// A focus selection failure recorded in the report instead of aborting the
/// command, so the whole project scan is never discarded over one bad pattern.
struct FocusIssue {
    path: String,
    message: String,
}

fn analyze_focused_assets(
    index: &ProjectIndex,
    focus: &[String],
) -> (BTreeMap<String, AssetAnalysis>, Vec<FocusIssue>) {
    let mut analyses = BTreeMap::new();
    let mut issues = Vec::new();
    if focus.is_empty() {
        return (analyses, issues);
    }
    let mut selected = BTreeSet::new();
    for pattern in focus {
        let mut matched = false;
        for package in index.assets.keys() {
            if package_matches(pattern, package) {
                matched = true;
                selected.insert(package.clone());
            }
        }
        if !matched {
            issues.push(FocusIssue {
                path: pattern.clone(),
                message: format!("--focus pattern matched no indexed package: {pattern}"),
            });
        }
    }
    for package in selected {
        let record = &index.assets[&package];
        match fs::read(&record.file_path) {
            Ok(bytes) => match PackageView::parse(&bytes) {
                Ok(view) => {
                    analyses.insert(package, view.analyze(AssetView::Full));
                }
                // A focused package the parser deliberately does not target has no
                // full analysis to attach; the inventory record already reports it
                // as `unsupported`, so this is not a focus failure.
                Err(error) if error.is_out_of_scope() => {}
                Err(error) => issues.push(FocusIssue {
                    path: package.clone(),
                    message: format!("failed to parse focused package {package}: {error}"),
                }),
            },
            Err(error) => issues.push(FocusIssue {
                path: package.clone(),
                message: format!("failed to read focused package {package}: {error}"),
            }),
        }
    }
    (analyses, issues)
}

fn package_matches(pattern: &str, package: &str) -> bool {
    let pattern = strip_asset_extension(pattern);
    let pattern = strip_object_name(pattern);
    glob_match(pattern, package)
}

fn strip_object_name(value: &str) -> &str {
    let slash = value.rfind('/').unwrap_or(0);
    match value[slash..].find('.') {
        Some(relative) => &value[..slash + relative],
        None => value,
    }
}

/// A glob token. `*` matches within a path segment; `**` matches across `/`.
enum GlobToken {
    Star,
    GlobStar,
    AnyChar,
    Literal(u8),
}

fn glob_tokens(pattern: &str) -> Vec<GlobToken> {
    let bytes = pattern.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'*' => {
                if bytes.get(i + 1) == Some(&b'*') {
                    tokens.push(GlobToken::GlobStar);
                    i += 2;
                    while bytes.get(i) == Some(&b'*') {
                        i += 1;
                    }
                } else {
                    tokens.push(GlobToken::Star);
                    i += 1;
                }
            }
            b'?' => {
                tokens.push(GlobToken::AnyChar);
                i += 1;
            }
            byte => {
                tokens.push(GlobToken::Literal(byte));
                i += 1;
            }
        }
    }
    tokens
}

fn glob_match(pattern: &str, value: &str) -> bool {
    let value = value.as_bytes();
    let value_len = value.len();
    let mut prev = vec![false; value_len + 1];
    let mut curr = vec![false; value_len + 1];
    prev[0] = true;
    for token in glob_tokens(pattern) {
        curr.iter_mut().for_each(|slot| *slot = false);
        match token {
            // `*` extends the match only over non-separator characters.
            GlobToken::Star => {
                curr[0] = prev[0];
                for index in 1..=value_len {
                    curr[index] = prev[index] || (curr[index - 1] && value[index - 1] != b'/');
                }
            }
            // `**` extends the match over any characters, separators included.
            GlobToken::GlobStar => {
                curr[0] = prev[0];
                for index in 1..=value_len {
                    curr[index] = prev[index] || curr[index - 1];
                }
            }
            GlobToken::AnyChar => {
                for index in 1..=value_len {
                    curr[index] = prev[index - 1] && value[index - 1] != b'/';
                }
            }
            GlobToken::Literal(byte) => {
                for index in 1..=value_len {
                    curr[index] = prev[index - 1] && byte.eq_ignore_ascii_case(&value[index - 1]);
                }
            }
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[value_len]
}

fn write_json<T: Serialize>(
    value: &T,
    compact: bool,
    max_output_bytes: Option<usize>,
    output: Option<&Path>,
) -> Result<()> {
    let text = match max_output_bytes {
        Some(budget) => budget::render_within_budget(value, budget, compact)?,
        None => render_json(value, compact)?,
    };
    match output {
        Some(path) => {
            // Write atomically: render to a sibling temp file, then rename it over the
            // target, so an interrupted run never leaves a truncated report that a
            // caller could read as a successful one.
            let mut tmp = path.as_os_str().to_os_string();
            tmp.push(".tmp");
            let tmp = PathBuf::from(tmp);
            fs::write(&tmp, format!("{text}\n"))
                .with_context(|| format!("failed to write {}", tmp.display()))?;
            fs::rename(&tmp, path).with_context(|| format!("failed to write {}", path.display()))
        }
        None => {
            let mut stdout = io::stdout().lock();
            stdout.write_all(text.as_bytes())?;
            stdout.write_all(b"\n")?;
            Ok(())
        }
    }
}

fn render_json<T: Serialize>(value: &T, compact: bool) -> Result<String> {
    if compact {
        serde_json::to_string(value).context("failed to render JSON")
    } else {
        serde_json::to_string_pretty(value).context("failed to render JSON")
    }
}

impl From<AssetViewArg> for AssetView {
    fn from(value: AssetViewArg) -> Self {
        match value {
            AssetViewArg::Summary => Self::Summary,
            AssetViewArg::Logic => Self::Logic,
            AssetViewArg::Properties => Self::Properties,
            AssetViewArg::References => Self::References,
            AssetViewArg::Full => Self::Full,
        }
    }
}

#[derive(Debug, Serialize)]
struct CommandFailure {
    schema_version: u32,
    status: &'static str,
    message: String,
}

#[derive(Debug, Serialize)]
struct ProjectReport {
    schema_version: u32,
    status: AnalysisStatus,
    layout: ProjectLayoutReport,
    mounts: Vec<ProjectMountReport>,
    entry_points: cc_uax_project::ProjectEntryPoints,
    reachability: cc_uax_project::ProjectReachability,
    analysis: cc_uax_project::ProjectAnalysisSummary,
    stats: cc_uax_project::ScanStats,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    inventory: Vec<ProjectAsset>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    forward: BTreeMap<String, BTreeSet<String>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    reverse: BTreeMap<String, BTreeSet<String>>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    ownership_closure: BTreeMap<String, BTreeSet<String>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    failures: Vec<ProjectIssue>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    diagnostics: Vec<ProjectIssue>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    focused: BTreeMap<String, AssetAnalysis>,
}

impl ProjectReport {
    fn from_index(
        index: &ProjectIndex,
        focused: BTreeMap<String, AssetAnalysis>,
        focus_issues: Vec<FocusIssue>,
    ) -> Self {
        let evidence_diagnostic = index
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.stage != ScanFailureStage::Cache);
        let has_hard_failure =
            !index.failures.is_empty() || !focus_issues.is_empty() || evidence_diagnostic;
        let status = if has_hard_failure {
            AnalysisStatus::Partial
        } else if index.analysis.status == AnalysisStatus::Unsupported {
            // Every scanned asset is unsupported (e.g. all future-version packages) and
            // nothing failed to scan; reflect that honestly instead of flattening to
            // partial, which the contract lists as a distinct status.
            AnalysisStatus::Unsupported
        } else if index.analysis.status == AnalysisStatus::Complete
            && focused
                .values()
                .all(|analysis| analysis.status == AnalysisStatus::Complete)
        {
            AnalysisStatus::Complete
        } else {
            AnalysisStatus::Partial
        };
        let inventory = index
            .assets
            .values()
            .map(ProjectAsset::from_record)
            .collect();
        let mut failures: Vec<ProjectIssue> = index
            .failures
            .iter()
            .map(|failure| ProjectIssue {
                stage: failure.stage,
                path: project_relative_path(index, &failure.path),
                severity: None,
                message: failure.message.clone(),
            })
            .collect();
        for issue in focus_issues {
            failures.push(ProjectIssue {
                stage: ScanFailureStage::Focus,
                path: issue.path,
                severity: None,
                message: issue.message,
            });
        }
        let diagnostics = index
            .diagnostics
            .iter()
            .map(|diagnostic| ProjectIssue {
                stage: diagnostic.stage,
                path: project_relative_path(index, &diagnostic.path),
                severity: Some(diagnostic.severity),
                message: diagnostic.message.clone(),
            })
            .collect();
        Self {
            schema_version: PROJECT_REPORT_SCHEMA_VERSION,
            status,
            layout: ProjectLayoutReport::from_index(index),
            mounts: index
                .mounts
                .mounts()
                .iter()
                .map(|mount| ProjectMountReport {
                    package_root: mount.package_root().to_string(),
                    relative_path: project_relative_path(index, mount.disk_root()),
                })
                .collect(),
            entry_points: index.entry_points.clone(),
            reachability: index.reachability.clone(),
            analysis: index.analysis.clone(),
            stats: index.stats.clone(),
            inventory,
            forward: index.forward.clone(),
            reverse: index.reverse.clone(),
            ownership_closure: index.ownership_closure.clone(),
            failures,
            diagnostics,
            focused,
        }
    }
}

#[derive(Debug, Serialize)]
struct ProjectLayoutReport {
    project_root: String,
    content_root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    project_file: Option<String>,
}

impl ProjectLayoutReport {
    fn from_index(index: &ProjectIndex) -> Self {
        Self {
            project_root: project_relative_path(index, index.layout.project_root()),
            content_root: project_relative_path(index, index.layout.content_root()),
            project_file: index
                .layout
                .project_file()
                .map(|path| project_relative_path(index, path)),
        }
    }
}

#[derive(Debug, Serialize)]
struct ProjectMountReport {
    package_root: String,
    relative_path: String,
}

#[derive(Debug, Serialize)]
struct ProjectAsset {
    package: String,
    relative_path: String,
    kind: AssetKind,
    ownership: AssetOwnership,
    analysis: cc_uax_project::AssetAnalysisSummary,
}

impl ProjectAsset {
    fn from_record(record: &cc_uax_project::AssetRecord) -> Self {
        Self {
            package: record.package_path.clone(),
            relative_path: record.relative_path.clone(),
            kind: record.asset_kind,
            ownership: record.ownership.clone(),
            analysis: record.analysis.clone(),
        }
    }
}

#[derive(Debug, Serialize)]
struct ProjectIssue {
    stage: ScanFailureStage,
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    severity: Option<ScanDiagnosticSeverity>,
    message: String,
}

fn project_relative_path(index: &ProjectIndex, path: &Path) -> String {
    let relative = path
        .strip_prefix(index.layout.project_root())
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
        .trim_matches('/')
        .to_string();
    if relative.is_empty() {
        ".".to_string()
    } else {
        relative
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_contract_declares_the_code_schema_versions() {
        // report-contract.md is the single source of truth for schema numbers,
        // so the versions it declares must equal the constants the code emits.
        let contract = include_str!("../../../skills/cc-uax/references/report-contract.md");
        let marker = "schema version `";
        let declared: std::collections::BTreeSet<u32> = contract
            .match_indices(marker)
            .filter_map(|(index, _)| {
                let rest = &contract[index + marker.len()..];
                rest[..rest.find('`')?].parse::<u32>().ok()
            })
            .collect();
        assert!(
            !declared.is_empty(),
            "no schema versions declared in report-contract.md"
        );
        let expected: std::collections::BTreeSet<u32> = [
            cc_uax_core::ASSET_ANALYSIS_SCHEMA_VERSION,
            PROJECT_REPORT_SCHEMA_VERSION,
        ]
        .into();
        assert_eq!(
            declared, expected,
            "report-contract.md schema numbers drifted from the code constants"
        );
    }

    #[test]
    fn glob_matching_is_case_insensitive_and_supports_wildcards() {
        assert!(glob_match("/Game/**/BP_*", "/game/Actors/BP_Player"));
        assert!(glob_match("/Game/Map?", "/Game/Map1"));
        assert!(!glob_match("/Game/Map?", "/Game/Map12"));
        // `*` stays within a path segment; `**` crosses separators.
        assert!(glob_match(
            "/Game/Blueprints/*",
            "/Game/Blueprints/BP_Player"
        ));
        assert!(!glob_match(
            "/Game/Blueprints/*",
            "/Game/Blueprints/Sub/BP_Player"
        ));
        assert!(glob_match(
            "/Game/Blueprints/**",
            "/Game/Blueprints/Sub/BP_Player"
        ));
    }

    #[test]
    fn exact_package_match_is_supported() {
        assert!(package_matches(
            "/Game/Actors/BP_Player",
            "/Game/Actors/BP_Player"
        ));
        assert!(package_matches(
            "/Game/Actors/BP_Player.BP_Player_C",
            "/Game/Actors/BP_Player"
        ));
    }
}
