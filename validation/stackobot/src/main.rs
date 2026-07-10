mod manifest;

use anyhow::{Context, Result, bail, ensure};
use cc_uax_core::{
    AnalysisStatus, AssetAnalysis, AssetView, CapabilityKind, EdgeKind, KnownOpaqueKind,
    PackageView,
};
use cc_uax_project::{
    AssetOwnership, CachePathPolicy, ProjectIndex, ProjectLayout, ProjectScanner, ScanMode,
    ScanOptions, package_path_from_relative,
};
use manifest::{
    ControlRigFixtures, GameplayEdge, LegacyBlueprintFixture, Manifest, PcgFixture,
    StateTreeFixture, WorldPartitionFixture,
};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

const CONTENT_ENV: &str = "STACKOBOT_CONTENT_ROOT";

fn main() {
    match run() {
        Ok(()) => {}
        Err(error) => {
            eprintln!("StackOBot validation failed: {error:#}");
            std::process::exit(1);
        }
    }
}

fn run() -> Result<()> {
    let Some(args) = Args::parse()? else {
        print_usage();
        return Ok(());
    };
    let manifest = Manifest::load(&args.manifest)?;
    let layout = ProjectLayout::discover(&args.content)
        .with_context(|| format!("failed to discover project from {}", args.content.display()))?;
    ensure!(
        layout.content_root() == fs::canonicalize(&args.content)?,
        "--content must point directly at the project Content directory"
    );

    let index = ProjectScanner::new(layout)
        .scan(ScanOptions {
            mode: ScanMode::Strict,
            cache: CachePathPolicy::Disabled,
        })
        .map_err(|error| {
            anyhow::anyhow!(
                "strict project scan failed: {} failure(s), {} indexed of {} discovered",
                error.index().failures.len(),
                error.index().stats.indexed,
                error.index().stats.discovered
            )
        })?;

    validate_corpus(&manifest, &index)?;
    validate_diagnostics_and_opaque(&manifest, &index)?;
    let analyses = load_fixture_analyses(&manifest, &index)?;
    validate_legacy_blueprints(&manifest.fixtures.legacy_blueprints, &analyses)?;
    validate_gameplay_edges(&manifest, &analyses)?;
    validate_control_rigs(&manifest.fixtures.control_rigs, &analyses)?;
    validate_pcg_graphs(&manifest.fixtures.pcg_graphs, &analyses)?;
    validate_state_tree(&manifest.fixtures.state_tree, &analyses)?;
    validate_world_partition(&manifest.fixtures.world_partition, &index)?;

    println!(
        "StackOBot validation passed: {} packages, {} exact K2 edges, {} RigVM links, {} ExternalActors, {} ExternalObjects.",
        index.stats.indexed,
        manifest.edge_count(),
        manifest.fixtures.control_rigs.expected_total_link_pairs,
        manifest.fixtures.world_partition.expected_external_actors,
        manifest.fixtures.world_partition.expected_external_objects,
    );
    Ok(())
}

#[derive(Debug)]
struct Args {
    content: PathBuf,
    manifest: PathBuf,
}

impl Args {
    fn parse() -> Result<Option<Self>> {
        let mut content = None;
        let mut manifest = None;
        let mut arguments = env::args_os().skip(1);
        while let Some(argument) = arguments.next() {
            match argument.to_str() {
                Some("--help" | "-h") => return Ok(None),
                Some("--content") => {
                    content = Some(required_path(&mut arguments, "--content")?);
                }
                Some("--manifest") => {
                    manifest = Some(required_path(&mut arguments, "--manifest")?);
                }
                Some(value) => bail!("unknown argument: {value}"),
                None => bail!("arguments must be valid Unicode"),
            }
        }
        let content = content
            .or_else(|| env::var_os(CONTENT_ENV).map(PathBuf::from))
            .with_context(|| format!("--content or {CONTENT_ENV} is required"))?;
        let manifest =
            manifest.unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("manifest.json"));
        Ok(Some(Self { content, manifest }))
    }
}

fn required_path(arguments: &mut impl Iterator<Item = OsString>, option: &str) -> Result<PathBuf> {
    let value = arguments
        .next()
        .with_context(|| format!("{option} requires a path"))?;
    ensure!(!value.is_empty(), "{option} requires a non-empty path");
    Ok(PathBuf::from(value))
}

fn print_usage() {
    println!(
        "Usage: cc-uax-stackobot-validation --content <Content> [--manifest <manifest.json>]\n\
         Environment fallback: {CONTENT_ENV}=<Content>"
    );
}

fn validate_corpus(manifest: &Manifest, index: &ProjectIndex) -> Result<()> {
    let expected = &manifest.corpus.required_result;
    ensure!(
        index.stats.discovered == manifest.corpus.expected_packages,
        "discovered {} packages, expected {}",
        index.stats.discovered,
        manifest.corpus.expected_packages
    );
    ensure!(
        index.stats.indexed == expected.indexed,
        "indexed {} packages, expected {}",
        index.stats.indexed,
        expected.indexed
    );
    ensure!(
        index.stats.failed == expected.failed,
        "recorded {} scan failures, expected {}",
        index.stats.failed,
        expected.failed
    );
    ensure!(
        index.stats.skipped == expected.skipped,
        "reported {} skipped packages, expected {}",
        index.stats.skipped,
        expected.skipped
    );
    ensure!(
        index.failures.len() == expected.failed,
        "failure accounting disagrees"
    );
    ensure!(
        index.analysis.assets == expected.indexed,
        "analysis coverage misses assets"
    );
    ensure!(
        index.analysis.scan_failures == expected.failed,
        "analysis scan-failure accounting disagrees"
    );
    ensure!(
        index.analysis.status != AnalysisStatus::Unsupported,
        "project analysis is unsupported"
    );
    Ok(())
}

fn validate_diagnostics_and_opaque(manifest: &Manifest, index: &ProjectIndex) -> Result<()> {
    let asset_diagnostics = index
        .assets
        .values()
        .map(|asset| {
            let summary = &asset.analysis.diagnostics;
            summary.errors + summary.warnings + summary.info
        })
        .sum::<usize>();
    let diagnostics = index.diagnostics.len() + asset_diagnostics;
    ensure!(
        diagnostics == manifest.corpus.required_result.unexpected_diagnostics,
        "found {diagnostics} unexpected diagnostics"
    );

    let mut observed_allowances = BTreeSet::new();
    let mut unclassified = Vec::new();
    for asset in index.assets.values() {
        for opaque in &asset.analysis.known_opaque.identities {
            let kind = opaque_kind(opaque.kind);
            let type_name = opaque.type_name.as_deref().unwrap_or("");
            let matches = manifest
                .opaque_policy
                .allowed
                .iter()
                .enumerate()
                .filter(|(_, allowance)| {
                    allowance_matches(allowance, kind, type_name, &asset.relative_path)
                })
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            if !matches.is_empty() {
                observed_allowances.extend(matches);
            } else {
                unclassified.push(format!(
                    "{}: {} {} at {}",
                    asset.relative_path, kind, type_name, opaque.path
                ));
            }
        }
    }
    ensure!(
        unclassified.len() == manifest.corpus.required_result.unclassified_tails,
        "found {} unclassified opaque regions: {}",
        unclassified.len(),
        unclassified.join("; ")
    );
    let missing = manifest
        .opaque_policy
        .allowed
        .iter()
        .enumerate()
        .filter(|(index, _)| !observed_allowances.contains(index))
        .map(|(_, allowance)| {
            format!(
                "{} {} in {}",
                allowance.kind,
                allowance.type_name,
                allowance
                    .assets
                    .iter()
                    .chain(&allowance.asset_globs)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(",")
            )
        })
        .collect::<Vec<_>>();
    ensure!(
        missing.is_empty(),
        "opaque allowances no longer match evidence: {}",
        missing.join("; ")
    );
    Ok(())
}

fn allowance_matches(
    allowance: &manifest::OpaqueAllowance,
    kind: &str,
    type_name: &str,
    asset: &str,
) -> bool {
    allowance.kind == kind
        && allowance.type_name == type_name
        && (allowance
            .assets
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(asset))
            || allowance
                .asset_globs
                .iter()
                .any(|pattern| glob_match(pattern, asset)))
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

fn opaque_kind(kind: KnownOpaqueKind) -> &'static str {
    match kind {
        KnownOpaqueKind::PropertyValue => "property_value",
        KnownOpaqueKind::PostPropertyTail => "post_property_tail",
        KnownOpaqueKind::Metadata => "metadata",
        KnownOpaqueKind::Capability => "capability",
    }
}

fn load_fixture_analyses(
    manifest: &Manifest,
    index: &ProjectIndex,
) -> Result<BTreeMap<String, AssetAnalysis>> {
    let mut paths = BTreeSet::new();
    paths.extend(
        manifest
            .fixtures
            .legacy_blueprints
            .iter()
            .map(|fixture| fixture.asset.as_str()),
    );
    paths.extend(
        manifest
            .fixtures
            .control_rigs
            .assets
            .iter()
            .map(|fixture| fixture.asset.as_str()),
    );
    paths.extend(
        manifest
            .fixtures
            .pcg_graphs
            .iter()
            .map(|fixture| fixture.asset.as_str()),
    );
    paths.insert(manifest.fixtures.state_tree.asset.as_str());
    paths.extend(
        manifest
            .gameplay_assertions
            .iter()
            .map(|assertion| assertion.asset.as_str()),
    );

    let mut analyses = BTreeMap::new();
    for relative_path in paths {
        let package_path = package_path_from_relative(relative_path, "/Game")?;
        let record = index
            .asset(&package_path)
            .with_context(|| format!("fixture asset is not indexed: {relative_path}"))?;
        ensure!(
            record.relative_path == relative_path,
            "fixture path resolved to a different indexed asset: {relative_path}"
        );
        let bytes = fs::read(&record.file_path)
            .with_context(|| format!("failed to read indexed fixture {relative_path}"))?;
        let view = PackageView::parse(&bytes)
            .with_context(|| format!("failed to parse indexed fixture {relative_path}"))?;
        analyses.insert(relative_path.to_string(), view.analyze(AssetView::Full));
    }
    Ok(analyses)
}

fn validate_legacy_blueprints(
    fixtures: &[LegacyBlueprintFixture],
    analyses: &BTreeMap<String, AssetAnalysis>,
) -> Result<()> {
    for fixture in fixtures {
        let analysis = fixture_analysis(analyses, &fixture.asset)?;
        let nodes = analysis
            .graphs
            .iter()
            .flat_map(|graph| graph.nodes.iter())
            .collect::<Vec<_>>();
        let pin_count = nodes.iter().map(|node| node.pins.len()).sum::<usize>();
        let edge_count = analysis
            .graphs
            .iter()
            .map(|graph| graph.edges.len())
            .sum::<usize>();
        let comment_count = nodes
            .iter()
            .filter(|node| node.class.to_ascii_lowercase().contains("comment"))
            .count();
        ensure!(
            nodes.len() == fixture.expected_graph_nodes,
            "{} has {} graph nodes, expected {}",
            fixture.id,
            nodes.len(),
            fixture.expected_graph_nodes
        );
        if let Some(minimum) = fixture.minimum_pins {
            ensure!(
                pin_count >= minimum,
                "{} recovered too few pins",
                fixture.id
            );
        }
        if let Some(minimum) = fixture.minimum_edges {
            ensure!(
                edge_count >= minimum,
                "{} recovered too few edges",
                fixture.id
            );
        }
        if let Some(expected) = fixture.expected_comment_nodes {
            ensure!(
                comment_count == expected,
                "{} comment-node count changed",
                fixture.id
            );
        }
        if let Some(expected) = fixture.expected_pins {
            ensure!(pin_count == expected, "{} pin count changed", fixture.id);
        }
        if let Some(expected) = fixture.expected_edges {
            ensure!(edge_count == expected, "{} edge count changed", fixture.id);
        }
        let unexplained_zero_pin_nodes = nodes
            .iter()
            .filter(|node| node.pins.is_empty())
            .filter(|node| !node.class.to_ascii_lowercase().contains("comment"))
            .map(|node| format!("{}#{}", node.name, node.index))
            .collect::<Vec<_>>();
        if fixture.zero_pin_nodes_must_be_explained {
            ensure!(
                unexplained_zero_pin_nodes.is_empty(),
                "{} has unexplained zero-pin nodes: {}",
                fixture.id,
                unexplained_zero_pin_nodes.join(", ")
            );
        }
    }
    Ok(())
}

fn validate_gameplay_edges(
    manifest: &Manifest,
    analyses: &BTreeMap<String, AssetAnalysis>,
) -> Result<()> {
    let mut matched = 0usize;
    for assertion in &manifest.gameplay_assertions {
        let analysis = fixture_analysis(analyses, &assertion.asset)?;
        for expected in &assertion.edges {
            validate_gameplay_edge(&assertion.id, analysis, expected)?;
            matched += 1;
        }
    }
    ensure!(
        matched == 58,
        "matched {matched} gameplay edges, expected 58"
    );
    ensure!(
        matched == manifest.edge_count(),
        "gameplay edge accounting disagrees"
    );
    Ok(())
}

fn validate_gameplay_edge(
    assertion_id: &str,
    analysis: &AssetAnalysis,
    expected: &GameplayEdge,
) -> Result<()> {
    let graph = analysis
        .graphs
        .iter()
        .find(|graph| graph.full_name == expected.graph_full_name)
        .with_context(|| {
            format!(
                "{assertion_id}: graph not found: {}",
                expected.graph_full_name
            )
        })?;
    validate_endpoint_name(assertion_id, graph, &expected.from)?;
    validate_endpoint_name(assertion_id, graph, &expected.to)?;
    let kind = match expected.kind.as_str() {
        "exec" => EdgeKind::Exec,
        "data" => EdgeKind::Data,
        other => bail!("{assertion_id}: unsupported edge kind {other}"),
    };
    ensure!(
        graph.edges.iter().any(|edge| {
            edge.kind == kind
                && edge.from.node_index == expected.from.node_index
                && edge.from.pin_id == expected.from.pin_guid
                && edge.to.node_index == expected.to.node_index
                && edge.to.pin_id == expected.to.pin_guid
        }),
        "{assertion_id}: exact edge was not recovered in {}",
        expected.graph_full_name
    );
    Ok(())
}

fn validate_endpoint_name(
    assertion_id: &str,
    graph: &cc_uax_core::LogicGraph,
    expected: &manifest::GameplayEndpoint,
) -> Result<()> {
    let node = graph
        .nodes
        .iter()
        .find(|node| node.index == expected.node_index)
        .with_context(|| format!("{assertion_id}: node {} not found", expected.node_index))?;
    let label_matches = node.name == expected.node_name
        || node
            .member
            .as_ref()
            .is_some_and(|member| member.name == expected.node_name);
    ensure!(
        label_matches,
        "{assertion_id}: node {} has export/member labels {}/{:?}, expected {}",
        node.index,
        node.name,
        node.member.as_ref().map(|member| member.name.as_str()),
        expected.node_name
    );
    ensure!(
        node.pins.iter().any(|pin| pin.pin_id == expected.pin_guid),
        "{assertion_id}: pin {} not found on node {}",
        expected.pin_guid,
        expected.node_index
    );
    Ok(())
}

fn validate_control_rigs(
    fixtures: &ControlRigFixtures,
    analyses: &BTreeMap<String, AssetAnalysis>,
) -> Result<()> {
    let mut total = 0usize;
    for fixture in &fixtures.assets {
        let analysis = fixture_analysis(analyses, &fixture.asset)?;
        let links = analysis
            .rigvm_graphs
            .iter()
            .map(|graph| graph.links.len())
            .sum::<usize>();
        ensure!(
            links == fixture.expected_link_pairs,
            "{} has {links} RigVM links, expected {}",
            fixture.asset,
            fixture.expected_link_pairs
        );
        ensure!(
            analysis.rigvm_graphs.iter().all(|graph| {
                graph.unresolved_node_references == 0
                    && graph.unresolved_pin_references == 0
                    && graph.unresolved_link_references == 0
            }),
            "{} has unresolved RigVM references",
            fixture.asset
        );
        total += links;
    }
    ensure!(
        total == fixtures.expected_total_link_pairs,
        "recovered {total} RigVM links, expected {}",
        fixtures.expected_total_link_pairs
    );
    Ok(())
}

fn validate_pcg_graphs(
    fixtures: &[PcgFixture],
    analyses: &BTreeMap<String, AssetAnalysis>,
) -> Result<()> {
    for fixture in fixtures {
        let analysis = fixture_analysis(analyses, &fixture.asset)?;
        let graph = analysis
            .pcg_graphs
            .first()
            .with_context(|| format!("{} has no PCG semantic graph", fixture.id))?;
        ensure!(
            analysis.pcg_graphs.len() == 1,
            "{} emitted {} PCG graphs, expected one",
            fixture.id,
            analysis.pcg_graphs.len()
        );
        ensure!(
            graph.nodes_array_count == fixture.expected_nodes_array,
            "{} Nodes array has {}, expected {}",
            fixture.id,
            graph.nodes_array_count,
            fixture.expected_nodes_array
        );
        ensure!(
            graph.default_node_count == fixture.expected_default_nodes,
            "{} has {} default nodes, expected {}",
            fixture.id,
            graph.default_node_count,
            fixture.expected_default_nodes
        );
        ensure!(
            graph.nodes.len() == fixture.expected_semantic_nodes,
            "{} has {} semantic nodes, expected {}",
            fixture.id,
            graph.nodes.len(),
            fixture.expected_semantic_nodes
        );
        ensure!(
            graph.base_node_export_count == fixture.expected_exact_base_node_exports,
            "{} has {} exact PCGNode exports, expected {}",
            fixture.id,
            graph.base_node_export_count,
            fixture.expected_exact_base_node_exports
        );
        let pins = graph
            .nodes
            .iter()
            .map(|node| node.pins.len())
            .sum::<usize>();
        ensure!(
            pins == fixture.expected_pins,
            "{} has {pins} pins, expected {}",
            fixture.id,
            fixture.expected_pins
        );
        ensure!(
            graph.edges.len() == fixture.expected_edges,
            "{} has {} edges, expected {}",
            fixture.id,
            graph.edges.len(),
            fixture.expected_edges
        );
        ensure!(
            graph.unresolved_node_references == 0
                && graph.unresolved_pin_references == 0
                && graph.unresolved_edge_references == 0,
            "{} has unresolved PCG references",
            fixture.id
        );
        for subclass in &fixture.expected_node_subclasses {
            let count = graph
                .nodes
                .iter()
                .filter(|node| node.class == subclass.class)
                .count();
            ensure!(
                count == subclass.count,
                "{} has {count} {} nodes, expected {}",
                fixture.id,
                subclass.class,
                subclass.count
            );
            if subclass.must_be_in_nodes_array {
                ensure!(
                    graph.nodes_array_count
                        >= graph
                            .nodes
                            .iter()
                            .filter(|node| node.class != "/Script/PCG.PCGNode")
                            .count(),
                    "{} subclass nodes cannot be accounted for by Nodes",
                    fixture.id
                );
            }
        }

        let observed_gaps = analysis
            .known_opaque
            .iter()
            .filter(|opaque| opaque.type_name.as_deref() == Some("InstancedPropertyBag"))
            .collect::<Vec<_>>();
        ensure!(
            observed_gaps.len() == fixture.property_bag_gaps.len(),
            "{} has {} PropertyBag gaps, expected {}",
            fixture.id,
            observed_gaps.len(),
            fixture.property_bag_gaps.len()
        );
        for gap in &fixture.property_bag_gaps {
            let export = analysis
                .exports
                .iter()
                .find(|export| export.index == gap.export_index)
                .with_context(|| {
                    format!("{} export {} is missing", fixture.id, gap.export_index)
                })?;
            ensure!(
                export.name == gap.export_name,
                "{} export {} is named {}, expected {}",
                fixture.id,
                gap.export_index,
                export.name,
                gap.export_name
            );
            let path = format!(
                "/exports/{}/properties/{}",
                gap.export_index,
                gap.property_path.replace('.', "/")
            );
            let opaque = observed_gaps
                .iter()
                .find(|opaque| opaque.path == path)
                .with_context(|| format!("{} missing opaque gap {path}", fixture.id))?;
            ensure!(
                opaque.type_name.as_deref() == Some(gap.type_name.as_str()),
                "{} gap {path} has the wrong type",
                fixture.id
            );
            let range = opaque
                .byte_range
                .as_ref()
                .with_context(|| format!("{} gap {path} has no byte range", fixture.id))?;
            ensure!(
                range.size == gap.serialized_bytes,
                "{} gap {path} is {} bytes, expected {}",
                fixture.id,
                range.size,
                gap.serialized_bytes
            );
        }
        let expected_status = if fixture.property_bag_gaps.is_empty() {
            AnalysisStatus::Complete
        } else {
            AnalysisStatus::Partial
        };
        ensure_capability(
            analysis,
            CapabilityKind::PcgSemantics,
            expected_status,
            &fixture.id,
        )?;
    }
    Ok(())
}

fn validate_state_tree(
    fixture: &StateTreeFixture,
    analyses: &BTreeMap<String, AssetAnalysis>,
) -> Result<()> {
    let analysis = fixture_analysis(analyses, &fixture.asset)?;
    let graph = analysis
        .state_tree_graphs
        .first()
        .context("StateTree semantic graph is missing")?;
    ensure!(
        analysis.state_tree_graphs.len() == 1,
        "StateTree emitted {} graphs, expected one",
        analysis.state_tree_graphs.len()
    );
    let states = graph.states.len();
    let tasks = graph
        .states
        .iter()
        .map(|state| state.tasks.len())
        .sum::<usize>();
    let enter_conditions = graph
        .states
        .iter()
        .map(|state| state.enter_conditions.len())
        .sum::<usize>();
    let transitions = graph
        .states
        .iter()
        .map(|state| state.transitions.len())
        .sum::<usize>();
    let child_links = graph
        .states
        .iter()
        .map(|state| state.child_indices.len())
        .sum::<usize>();
    ensure!(
        states == fixture.expected_states,
        "StateTree has {states} states"
    );
    ensure!(
        tasks == fixture.expected_tasks,
        "StateTree has {tasks} tasks"
    );
    ensure!(
        enter_conditions == fixture.expected_enter_conditions,
        "StateTree has {enter_conditions} enter conditions"
    );
    ensure!(
        transitions == fixture.expected_transitions,
        "StateTree has {transitions} transitions"
    );
    ensure!(
        child_links == fixture.expected_child_links,
        "StateTree has {child_links} child links"
    );
    ensure!(
        graph.unresolved_state_references == 0,
        "StateTree has unresolved state references"
    );
    let state_exports = analysis
        .exports
        .iter()
        .filter(|export| export.class == fixture.state_export_class)
        .count();
    ensure!(
        state_exports == fixture.expected_states,
        "StateTree has {state_exports} state exports, expected {}",
        fixture.expected_states
    );
    ensure_capability(
        analysis,
        CapabilityKind::StateTreeSemantics,
        AnalysisStatus::Complete,
        "state_tree",
    )?;
    Ok(())
}

fn ensure_capability(
    analysis: &AssetAnalysis,
    kind: CapabilityKind,
    status: AnalysisStatus,
    owner: &str,
) -> Result<()> {
    let capability = analysis
        .capabilities
        .iter()
        .find(|capability| capability.kind == kind)
        .with_context(|| format!("{owner} is missing capability {kind:?}"))?;
    ensure!(
        capability.status == status,
        "{owner} capability {kind:?} is {:?}, expected {status:?}",
        capability.status
    );
    Ok(())
}

fn validate_world_partition(fixture: &WorldPartitionFixture, index: &ProjectIndex) -> Result<()> {
    let package_path = package_path_from_relative(&fixture.asset, "/Game")?;
    let closure = index
        .closure_for(&package_path)
        .with_context(|| format!("World Partition root is not indexed: {}", fixture.asset))?;
    let mut external_actors = 0usize;
    let mut external_objects = 0usize;
    for member in &closure {
        let Some(asset) = index.asset(member) else {
            continue;
        };
        match &asset.ownership {
            AssetOwnership::External { external_kind, .. } => match external_kind {
                cc_uax_project::ExternalPackageKind::Actor => external_actors += 1,
                cc_uax_project::ExternalPackageKind::Object => external_objects += 1,
            },
            AssetOwnership::ProjectAsset => {}
        }
    }
    ensure!(
        external_actors == fixture.expected_external_actors,
        "World Partition closure has {external_actors} actors, expected {}",
        fixture.expected_external_actors
    );
    ensure!(
        external_objects == fixture.expected_external_objects,
        "World Partition closure has {external_objects} objects, expected {}",
        fixture.expected_external_objects
    );
    if fixture.require_reference_closure {
        ensure!(
            closure.len() == 1 + external_actors + external_objects,
            "World Partition closure contains unexpected or missing members"
        );
    }
    Ok(())
}

fn fixture_analysis<'a>(
    analyses: &'a BTreeMap<String, AssetAnalysis>,
    asset: &str,
) -> Result<&'a AssetAnalysis> {
    analyses
        .get(asset)
        .with_context(|| format!("fixture analysis was not loaded: {asset}"))
}
