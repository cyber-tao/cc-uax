pub mod args;

use crate::args::{AssetArgs, AssetViewArg, Cli, Command, ProjectArgs};
use anyhow::{Context, Result};
use cc_uax_core::{AnalysisStatus, AssetAnalysis, AssetView, PackageView};
use cc_uax_project::{
    AssetKind, AssetOwnership, CachePathPolicy, MountTable, ProjectIndex, ProjectLayout,
    ProjectScanner, ScanDiagnosticSeverity, ScanFailureStage, ScanMode, ScanOptions,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

const PROJECT_REPORT_SCHEMA_VERSION: u32 = 1;

pub fn run(cli: Cli) -> ExitCode {
    match execute(&cli) {
        Ok(exit) => exit,
        Err(error) => {
            let failure = CommandFailure {
                schema_version: PROJECT_REPORT_SCHEMA_VERSION,
                status: "error",
                message: format!("{error:#}"),
            };
            let text = render_json(&failure, cli.compact)
                .unwrap_or_else(|_| "{\"status\":\"error\"}".to_string());
            let _ = writeln!(io::stderr().lock(), "{text}");
            ExitCode::FAILURE
        }
    }
}

fn execute(cli: &Cli) -> Result<ExitCode> {
    match &cli.command {
        Command::Asset(args) => {
            let analysis = analyze_asset(args)?;
            write_json(&analysis, cli.compact, cli.output.as_deref())?;
            Ok(if analysis.status == AnalysisStatus::Complete {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(2)
            })
        }
        Command::Project(args) => {
            let (report, strict_failure) = analyze_project(args)?;
            write_json(&report, cli.compact, cli.output.as_deref())?;
            Ok(if strict_failure {
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
    let (index, strict_failure) = match scanner.scan(options) {
        Ok(index) => (index, false),
        Err(error) => (error.into_index(), true),
    };
    let focused = analyze_focused_assets(&index, &args.focus)?;
    let report = ProjectReport::from_index(&index, focused);
    let strict_failure =
        strict_failure || (!args.allow_partial && report.status != AnalysisStatus::Complete);
    Ok((report, strict_failure))
}

fn project_mounts(layout: &ProjectLayout, requested: &[String]) -> Result<MountTable> {
    if requested.is_empty() {
        return Ok(MountTable::default_for(layout));
    }
    let explicit_game = requested.iter().any(|mount| {
        mount
            .split_once('=')
            .map(|(root, _)| root.trim().eq_ignore_ascii_case("/Game"))
            .unwrap_or_else(|| mount.trim().eq_ignore_ascii_case("/Game"))
    });
    let mut spec = String::new();
    if !explicit_game {
        spec.push_str("/Game");
    }
    for mount in requested {
        if !spec.is_empty() {
            spec.push(',');
        }
        spec.push_str(mount);
    }
    MountTable::parse(layout, &spec).context("invalid --mount mapping")
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

fn analyze_focused_assets(
    index: &ProjectIndex,
    focus: &[String],
) -> Result<BTreeMap<String, AssetAnalysis>> {
    if focus.is_empty() {
        return Ok(BTreeMap::new());
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
            anyhow::bail!("--focus pattern matched no indexed package: {pattern}");
        }
    }
    let mut analyses = BTreeMap::new();
    for package in selected {
        let record = &index.assets[&package];
        let bytes = fs::read(&record.file_path)
            .with_context(|| format!("failed to read focused package {package}"))?;
        let view = PackageView::parse(&bytes)
            .with_context(|| format!("failed to parse focused package {package}"))?;
        analyses.insert(package, view.analyze(AssetView::Full));
    }
    Ok(analyses)
}

fn package_matches(pattern: &str, package: &str) -> bool {
    let pattern = strip_package_extension(pattern);
    let pattern = strip_object_name(pattern);
    glob_match(pattern, package)
}

fn strip_package_extension(value: &str) -> &str {
    if value.len() >= 7 && value[value.len() - 7..].eq_ignore_ascii_case(".uasset") {
        &value[..value.len() - 7]
    } else if value.len() >= 5 && value[value.len() - 5..].eq_ignore_ascii_case(".umap") {
        &value[..value.len() - 5]
    } else {
        value
    }
}

fn strip_object_name(value: &str) -> &str {
    let slash = value.rfind('/').unwrap_or(0);
    match value[slash..].find('.') {
        Some(relative) => &value[..slash + relative],
        None => value,
    }
}

fn glob_match(pattern: &str, value: &str) -> bool {
    let pattern = pattern.as_bytes();
    let value = value.as_bytes();
    let mut row = vec![false; value.len() + 1];
    row[0] = true;
    for &token in pattern {
        let mut next = vec![false; value.len() + 1];
        if token == b'*' {
            next[0] = row[0];
            for index in 1..=value.len() {
                next[index] = row[index] || next[index - 1];
            }
        } else {
            for index in 1..=value.len() {
                next[index] = row[index - 1]
                    && (token == b'?' || token.eq_ignore_ascii_case(&value[index - 1]));
            }
        }
        row = next;
    }
    row[value.len()]
}

fn write_json<T: Serialize>(value: &T, compact: bool, output: Option<&Path>) -> Result<()> {
    let text = render_json(value, compact)?;
    match output {
        Some(path) => fs::write(path, format!("{text}\n"))
            .with_context(|| format!("failed to write {}", path.display())),
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
    project_file: Option<String>,
    entry_points: cc_uax_project::ProjectEntryPoints,
    analysis: cc_uax_project::ProjectAnalysisSummary,
    stats: cc_uax_project::ScanStats,
    inventory: Vec<ProjectAsset>,
    forward: BTreeMap<String, BTreeSet<String>>,
    reverse: BTreeMap<String, BTreeSet<String>>,
    ownership_closure: BTreeMap<String, BTreeSet<String>>,
    failures: Vec<ProjectIssue>,
    diagnostics: Vec<ProjectIssue>,
    focused: BTreeMap<String, AssetAnalysis>,
}

impl ProjectReport {
    fn from_index(index: &ProjectIndex, focused: BTreeMap<String, AssetAnalysis>) -> Self {
        let evidence_diagnostic = index
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.stage != ScanFailureStage::Cache);
        let status = if !index.failures.is_empty()
            || evidence_diagnostic
            || index.analysis.status != AnalysisStatus::Complete
            || focused
                .values()
                .any(|analysis| analysis.status != AnalysisStatus::Complete)
        {
            AnalysisStatus::Partial
        } else {
            AnalysisStatus::Complete
        };
        let inventory = index
            .assets
            .values()
            .map(ProjectAsset::from_record)
            .collect();
        let project_file = index
            .layout
            .project_file()
            .and_then(Path::file_name)
            .map(|name| name.to_string_lossy().into_owned());
        let failures = index
            .failures
            .iter()
            .map(|failure| ProjectIssue {
                stage: failure.stage,
                path: report_path(index, &failure.path),
                severity: None,
                message: failure.message.clone(),
            })
            .collect();
        let diagnostics = index
            .diagnostics
            .iter()
            .map(|diagnostic| ProjectIssue {
                stage: diagnostic.stage,
                path: report_path(index, &diagnostic.path),
                severity: Some(diagnostic.severity),
                message: diagnostic.message.clone(),
            })
            .collect();
        Self {
            schema_version: PROJECT_REPORT_SCHEMA_VERSION,
            status,
            project_file,
            entry_points: index.entry_points.clone(),
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
struct ProjectAsset {
    package: String,
    relative_path: String,
    kind: AssetKind,
    ownership: AssetOwnership,
    references: BTreeSet<String>,
    analysis: cc_uax_project::AssetAnalysisSummary,
}

impl ProjectAsset {
    fn from_record(record: &cc_uax_project::AssetRecord) -> Self {
        Self {
            package: record.package_path.clone(),
            relative_path: record.relative_path.clone(),
            kind: record.asset_kind,
            ownership: record.ownership.clone(),
            references: record.forward_references.clone(),
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

fn report_path(index: &ProjectIndex, path: &Path) -> String {
    path.strip_prefix(index.layout.project_root())
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_matching_is_case_insensitive_and_supports_wildcards() {
        assert!(glob_match("/Game/**/BP_*", "/game/Actors/BP_Player"));
        assert!(glob_match("/Game/Map?", "/Game/Map1"));
        assert!(!glob_match("/Game/Map?", "/Game/Map12"));
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
