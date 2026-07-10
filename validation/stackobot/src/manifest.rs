use anyhow::{Context, Result, bail, ensure};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path};

#[derive(Debug, Deserialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub fixture: String,
    pub path_contract: PathContract,
    pub corpus: CorpusContract,
    pub fixtures: Fixtures,
    pub opaque_policy: OpaquePolicy,
    pub gameplay_assertions: Vec<GameplayAssertion>,
}

#[derive(Debug, Deserialize)]
pub struct PathContract {
    pub base: String,
    pub separator: String,
    pub filesystem_paths_must_be_relative: bool,
    pub edge_identity: Vec<String>,
    pub cross_graph_edge_matching: String,
}

#[derive(Debug, Deserialize)]
pub struct CorpusContract {
    pub expected_packages: usize,
    pub required_result: RequiredResult,
    pub strict_by_default: bool,
    pub single_project_scan: bool,
}

#[derive(Debug, Deserialize)]
pub struct RequiredResult {
    pub indexed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub unexpected_diagnostics: usize,
    pub unclassified_tails: usize,
}

#[derive(Debug, Deserialize)]
pub struct Fixtures {
    pub legacy_blueprints: Vec<LegacyBlueprintFixture>,
    pub control_rigs: ControlRigFixtures,
    pub pcg_graphs: Vec<PcgFixture>,
    pub state_tree: StateTreeFixture,
    pub world_partition: WorldPartitionFixture,
}

#[derive(Debug, Deserialize)]
pub struct LegacyBlueprintFixture {
    pub id: String,
    pub asset: String,
    pub expected_graph_nodes: usize,
    pub minimum_pins: Option<usize>,
    pub minimum_edges: Option<usize>,
    pub expected_comment_nodes: Option<usize>,
    pub expected_pins: Option<usize>,
    pub expected_edges: Option<usize>,
    pub zero_pin_nodes_must_be_explained: bool,
    pub zero_pin_explanation: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ControlRigFixtures {
    pub expected_total_link_pairs: usize,
    pub assets: Vec<ControlRigFixture>,
}

#[derive(Debug, Deserialize)]
pub struct ControlRigFixture {
    pub asset: String,
    pub expected_link_pairs: usize,
}

#[derive(Debug, Deserialize)]
pub struct PcgFixture {
    pub id: String,
    pub asset: String,
    pub expected_nodes_array: usize,
    pub expected_default_nodes: usize,
    pub expected_semantic_nodes: usize,
    pub expected_exact_base_node_exports: usize,
    pub expected_node_subclasses: Vec<PcgNodeSubclass>,
    pub expected_pins: usize,
    pub expected_edges: usize,
    pub property_bag_gaps: Vec<PropertyBagGap>,
}

#[derive(Debug, Deserialize)]
pub struct PcgNodeSubclass {
    pub class: String,
    pub count: usize,
    pub must_be_in_nodes_array: bool,
}

#[derive(Debug, Deserialize)]
pub struct PropertyBagGap {
    pub export_index: i32,
    pub export_name: String,
    pub property_path: String,
    pub type_name: String,
    pub serialized_bytes: u64,
}

#[derive(Debug, Deserialize)]
pub struct StateTreeFixture {
    pub asset: String,
    pub state_export_class: String,
    pub expected_states: usize,
    pub expected_tasks: usize,
    pub expected_enter_conditions: usize,
    pub expected_transitions: usize,
    pub expected_child_links: usize,
}

#[derive(Debug, Deserialize)]
pub struct WorldPartitionFixture {
    pub asset: String,
    pub expected_external_actors: usize,
    pub expected_external_objects: usize,
    pub require_reference_closure: bool,
}

#[derive(Debug, Deserialize)]
pub struct OpaquePolicy {
    pub default: String,
    pub allowed: Vec<OpaqueAllowance>,
}

#[derive(Debug, Deserialize)]
pub struct OpaqueAllowance {
    pub kind: String,
    pub type_name: String,
    #[serde(default)]
    pub assets: Vec<String>,
    #[serde(default)]
    pub asset_globs: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Deserialize)]
pub struct GameplayAssertion {
    pub id: String,
    pub category: String,
    pub asset: String,
    pub edges: Vec<GameplayEdge>,
}

#[derive(Debug, Deserialize)]
pub struct GameplayEdge {
    pub graph_full_name: String,
    pub kind: String,
    pub from: GameplayEndpoint,
    pub to: GameplayEndpoint,
}

#[derive(Debug, Deserialize)]
pub struct GameplayEndpoint {
    pub node_index: i32,
    pub node_name: String,
    pub pin_guid: String,
}

impl Manifest {
    pub fn load(path: &Path) -> Result<Self> {
        let bytes = fs::read(path)
            .with_context(|| format!("failed to read manifest {}", path.display()))?;
        let manifest = serde_json::from_slice::<Self>(&bytes)
            .with_context(|| format!("failed to parse manifest {}", path.display()))?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1,
            "manifest schema_version must be 1"
        );
        ensure!(self.fixture == "StackOBot", "fixture must be StackOBot");
        ensure!(
            self.path_contract.base == "Content",
            "path base must be Content"
        );
        ensure!(
            self.path_contract.separator == "/",
            "path separator must be /"
        );
        ensure!(
            self.path_contract.filesystem_paths_must_be_relative,
            "manifest paths must be relative"
        );
        ensure!(
            self.path_contract.cross_graph_edge_matching == "forbidden",
            "cross-graph matching must be forbidden"
        );
        ensure!(
            self.path_contract.edge_identity
                == [
                    "asset",
                    "graph_full_name",
                    "kind",
                    "from.node_index",
                    "from.pin_guid",
                    "to.node_index",
                    "to.pin_guid",
                ],
            "edge identity contract changed"
        );
        ensure!(
            self.corpus.expected_packages == self.corpus.required_result.indexed,
            "corpus discovery/index target mismatch"
        );
        ensure!(self.corpus.strict_by_default, "corpus must be strict");
        ensure!(self.corpus.single_project_scan, "corpus must use one scan");
        ensure!(
            self.opaque_policy.default == "forbid",
            "opaque default must be forbid"
        );
        self.validate_stackobot_contract()?;

        let paths = self.asset_paths();
        for path in &paths {
            validate_asset_path(path)?;
        }

        let fixture_ids = self
            .fixtures
            .legacy_blueprints
            .iter()
            .map(|fixture| fixture.id.as_str())
            .chain(
                self.fixtures
                    .pcg_graphs
                    .iter()
                    .map(|fixture| fixture.id.as_str()),
            )
            .collect::<Vec<_>>();
        ensure_unique(fixture_ids, "fixture id")?;
        ensure_unique(
            self.gameplay_assertions
                .iter()
                .map(|assertion| assertion.id.as_str()),
            "gameplay assertion id",
        )?;

        for fixture in &self.fixtures.legacy_blueprints {
            if fixture.expected_pins == Some(0) {
                ensure!(
                    fixture.zero_pin_nodes_must_be_explained
                        && fixture
                            .zero_pin_explanation
                            .as_deref()
                            .is_some_and(|reason| !reason.is_empty()),
                    "{} needs a zero-pin explanation",
                    fixture.id
                );
            }
        }
        ensure!(
            self.fixtures
                .control_rigs
                .assets
                .iter()
                .map(|fixture| fixture.expected_link_pairs)
                .sum::<usize>()
                == self.fixtures.control_rigs.expected_total_link_pairs,
            "ControlRig link totals disagree"
        );
        for fixture in &self.fixtures.pcg_graphs {
            ensure!(
                fixture.expected_nodes_array + fixture.expected_default_nodes
                    == fixture.expected_semantic_nodes,
                "{} PCG node accounting disagrees",
                fixture.id
            );
            let mut gaps = BTreeSet::new();
            for gap in &fixture.property_bag_gaps {
                ensure!(
                    gap.export_index > 0,
                    "{} has an invalid gap export",
                    fixture.id
                );
                ensure!(
                    !gap.export_name.is_empty(),
                    "{} has an unnamed gap export",
                    fixture.id
                );
                ensure!(
                    !gap.property_path.is_empty(),
                    "{} has an empty gap path",
                    fixture.id
                );
                ensure!(
                    gap.type_name == "InstancedPropertyBag",
                    "{} has an unexpected gap type",
                    fixture.id
                );
                ensure!(gap.serialized_bytes > 0, "{} has an empty gap", fixture.id);
                ensure!(
                    gaps.insert((
                        gap.export_index,
                        gap.export_name.as_str(),
                        gap.property_path.as_str()
                    )),
                    "{} has a duplicate PropertyBag gap",
                    fixture.id
                );
            }
        }

        let mut opaque_allowances = BTreeSet::new();
        let mut opaque_types = BTreeSet::new();
        for allowance in &self.opaque_policy.allowed {
            ensure!(
                matches!(
                    allowance.kind.as_str(),
                    "capability" | "property_value" | "post_property_tail" | "metadata"
                ),
                "invalid opaque kind {}",
                allowance.kind
            );
            ensure!(
                !allowance.type_name.is_empty(),
                "opaque allowance type is empty"
            );
            ensure!(
                !allowance.reason.is_empty(),
                "opaque allowance reason is empty"
            );
            ensure!(
                !allowance.assets.is_empty() || !allowance.asset_globs.is_empty(),
                "opaque allowance has no asset scope"
            );
            for pattern in &allowance.asset_globs {
                validate_asset_glob(pattern)?;
            }
            opaque_types.insert(allowance.type_name.as_str());
            let mut assets = allowance
                .assets
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            assets.sort_unstable();
            let mut globs = allowance
                .asset_globs
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            globs.sort_unstable();
            ensure!(
                opaque_allowances.insert((
                    allowance.kind.as_str(),
                    allowance.type_name.as_str(),
                    assets,
                    globs,
                )),
                "duplicate opaque allowance for {}",
                allowance.type_name
            );
        }
        for required in ["RigVMBytecode", "RigHierarchy", "InstancedPropertyBag"] {
            ensure!(
                opaque_types.contains(required),
                "missing opaque allowance {required}"
            );
        }

        let required_categories = [
            "main_menu",
            "spawn_possess",
            "movement",
            "jetpack",
            "grab",
            "coin",
            "portal",
            "ai",
            "ui",
            "save",
        ];
        let categories = self
            .gameplay_assertions
            .iter()
            .map(|assertion| assertion.category.as_str())
            .collect::<BTreeSet<_>>();
        for category in required_categories {
            ensure!(
                categories.contains(category),
                "missing gameplay category {category}"
            );
        }

        let mut edges = BTreeSet::new();
        let mut node_names = BTreeMap::new();
        for assertion in &self.gameplay_assertions {
            ensure!(!assertion.edges.is_empty(), "{} has no edges", assertion.id);
            let mut kinds = BTreeSet::new();
            for edge in &assertion.edges {
                ensure!(
                    edge.graph_full_name.contains('.'),
                    "{} requires a full graph name",
                    assertion.id
                );
                ensure!(
                    edge.kind == "exec" || edge.kind == "data",
                    "{} has invalid edge kind {}",
                    assertion.id,
                    edge.kind
                );
                kinds.insert(edge.kind.as_str());
                validate_endpoint(&assertion.id, &edge.from)?;
                validate_endpoint(&assertion.id, &edge.to)?;
                for endpoint in [&edge.from, &edge.to] {
                    let key = (
                        assertion.asset.as_str(),
                        edge.graph_full_name.as_str(),
                        endpoint.node_index,
                    );
                    if let Some(previous) = node_names.insert(key, endpoint.node_name.as_str()) {
                        ensure!(
                            previous == endpoint.node_name,
                            "{} has inconsistent node identity",
                            assertion.id
                        );
                    }
                }
                let key = (
                    assertion.asset.as_str(),
                    edge.graph_full_name.as_str(),
                    edge.kind.as_str(),
                    edge.from.node_index,
                    edge.from.pin_guid.as_str(),
                    edge.to.node_index,
                    edge.to.pin_guid.as_str(),
                );
                ensure!(
                    edges.insert(key),
                    "duplicate gameplay edge in {}",
                    assertion.id
                );
            }
            ensure!(
                kinds.contains("exec") && kinds.contains("data"),
                "{} must assert exec and data evidence",
                assertion.id
            );
        }
        ensure!(
            self.edge_count() == 58,
            "StackOBot must assert exactly 58 edges"
        );
        Ok(())
    }

    fn validate_stackobot_contract(&self) -> Result<()> {
        ensure!(
            self.corpus.expected_packages == 1961,
            "package target must be 1961"
        );
        ensure!(
            self.corpus.required_result.indexed == 1961,
            "indexed target must be 1961"
        );
        ensure!(
            self.corpus.required_result.failed == 0,
            "failed target must be zero"
        );
        ensure!(
            self.corpus.required_result.skipped == 0,
            "skipped target must be zero"
        );
        ensure!(
            self.corpus.required_result.unexpected_diagnostics == 0,
            "unexpected diagnostic target must be zero"
        );
        ensure!(
            self.corpus.required_result.unclassified_tails == 0,
            "unclassified tail target must be zero"
        );

        let level_save = self
            .fixtures
            .legacy_blueprints
            .iter()
            .find(|fixture| fixture.id == "level_save_object")
            .context("missing level_save_object fixture")?;
        ensure!(
            level_save.expected_graph_nodes == 13,
            "LevelSaveObject node target changed"
        );
        ensure!(
            level_save.minimum_pins.is_some_and(|value| value > 0),
            "LevelSaveObject needs pins"
        );
        ensure!(
            level_save.minimum_edges.is_some_and(|value| value > 0),
            "LevelSaveObject needs edges"
        );
        let player_save = self
            .fixtures
            .legacy_blueprints
            .iter()
            .find(|fixture| fixture.id == "player_save_object")
            .context("missing player_save_object fixture")?;
        ensure!(
            player_save.expected_graph_nodes == 1,
            "PlayerSaveObject node target changed"
        );
        ensure!(
            player_save.expected_comment_nodes == Some(1),
            "PlayerSaveObject comment target changed"
        );
        ensure!(
            player_save.expected_pins == Some(0),
            "PlayerSaveObject pin target changed"
        );
        ensure!(
            player_save.expected_edges == Some(0),
            "PlayerSaveObject edge target changed"
        );

        ensure!(
            self.fixtures.control_rigs.assets.len() == 2,
            "expected two ControlRig fixtures"
        );
        ensure!(
            self.fixtures.control_rigs.expected_total_link_pairs == 82,
            "ControlRig link target must be 82"
        );

        let pickup = self
            .fixtures
            .pcg_graphs
            .iter()
            .find(|fixture| fixture.id == "pcg_pickup_spline")
            .context("missing pcg_pickup_spline fixture")?;
        ensure!(
            (
                pickup.expected_nodes_array,
                pickup.expected_default_nodes,
                pickup.expected_semantic_nodes,
                pickup.expected_exact_base_node_exports,
                pickup.expected_pins,
                pickup.expected_edges
            ) == (5, 2, 7, 6, 135, 4),
            "PCG_PickupSpline contract changed"
        );
        let spawn_actor = pickup
            .expected_node_subclasses
            .iter()
            .find(|subclass| subclass.class == "/Script/PCG.PCGSpawnActorNode")
            .context("missing PCGSpawnActorNode fixture")?;
        ensure!(
            spawn_actor.count == 1 && spawn_actor.must_be_in_nodes_array,
            "PCGSpawnActorNode contract changed"
        );
        let under_rock = self
            .fixtures
            .pcg_graphs
            .iter()
            .find(|fixture| fixture.id == "pcg_under_rock")
            .context("missing pcg_under_rock fixture")?;
        ensure!(
            (
                under_rock.expected_nodes_array,
                under_rock.expected_default_nodes,
                under_rock.expected_semantic_nodes,
                under_rock.expected_exact_base_node_exports,
                under_rock.expected_pins,
                under_rock.expected_edges,
                under_rock.property_bag_gaps.len()
            ) == (67, 2, 69, 69, 713, 74, 3),
            "PCG_UnderRock contract changed"
        );

        let state_tree = &self.fixtures.state_tree;
        ensure!(
            (
                state_tree.expected_states,
                state_tree.expected_tasks,
                state_tree.expected_enter_conditions,
                state_tree.expected_transitions,
                state_tree.expected_child_links
            ) == (13, 20, 3, 12, 12),
            "StateTree contract changed"
        );
        let world = &self.fixtures.world_partition;
        ensure!(
            (
                world.expected_external_actors,
                world.expected_external_objects
            ) == (296, 28),
            "World Partition contract changed"
        );
        ensure!(
            world.require_reference_closure,
            "World Partition closure is required"
        );
        Ok(())
    }

    pub fn asset_paths(&self) -> Vec<&str> {
        let mut paths = Vec::new();
        paths.extend(
            self.fixtures
                .legacy_blueprints
                .iter()
                .map(|fixture| fixture.asset.as_str()),
        );
        paths.extend(
            self.fixtures
                .control_rigs
                .assets
                .iter()
                .map(|fixture| fixture.asset.as_str()),
        );
        paths.extend(
            self.fixtures
                .pcg_graphs
                .iter()
                .map(|fixture| fixture.asset.as_str()),
        );
        paths.push(self.fixtures.state_tree.asset.as_str());
        paths.push(self.fixtures.world_partition.asset.as_str());
        for allowance in &self.opaque_policy.allowed {
            paths.extend(allowance.assets.iter().map(String::as_str));
        }
        paths.extend(
            self.gameplay_assertions
                .iter()
                .map(|assertion| assertion.asset.as_str()),
        );
        paths
    }

    pub fn edge_count(&self) -> usize {
        self.gameplay_assertions
            .iter()
            .map(|assertion| assertion.edges.len())
            .sum()
    }
}

fn validate_asset_path(value: &str) -> Result<()> {
    ensure!(
        !value.contains('\\'),
        "asset path contains backslash: {value}"
    );
    let path = Path::new(value);
    ensure!(!path.is_absolute(), "asset path is absolute: {value}");
    ensure!(
        value.starts_with("StackOBot/"),
        "asset path leaves fixture: {value}"
    );
    ensure!(
        path.components()
            .all(|component| matches!(component, Component::Normal(_))),
        "asset path contains traversal or prefix: {value}"
    );
    let extension = path.extension().and_then(|extension| extension.to_str());
    ensure!(
        matches!(extension, Some("uasset" | "umap")),
        "asset path has unsupported extension: {value}"
    );
    Ok(())
}

fn validate_asset_glob(value: &str) -> Result<()> {
    ensure!(!value.is_empty(), "opaque asset glob is empty");
    ensure!(
        !value.contains('\\'),
        "asset glob contains backslash: {value}"
    );
    ensure!(
        !Path::new(value).is_absolute(),
        "asset glob is absolute: {value}"
    );
    ensure!(
        !value.contains(".."),
        "asset glob contains traversal: {value}"
    );
    ensure!(
        !value.contains(':'),
        "asset glob contains a prefix: {value}"
    );
    Ok(())
}

fn validate_endpoint(owner: &str, endpoint: &GameplayEndpoint) -> Result<()> {
    ensure!(
        endpoint.node_index >= 0,
        "{owner} has a negative node index"
    );
    ensure!(
        !endpoint.node_name.is_empty(),
        "{owner} has an empty node name"
    );
    ensure!(
        endpoint.pin_guid.len() == 32
            && endpoint
                .pin_guid
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'A'..=b'F').contains(&byte)),
        "{owner} has an invalid pin GUID"
    );
    Ok(())
}

fn ensure_unique<'a>(values: impl IntoIterator<Item = &'a str>, label: &str) -> Result<()> {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            bail!("duplicate {label}: {value}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_in_manifest_is_relative_and_self_consistent() {
        let manifest = Manifest::load(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/manifest.json"
        )))
        .unwrap();
        assert_eq!(manifest.corpus.expected_packages, 1961);
        assert_eq!(manifest.edge_count(), 58);
        assert_eq!(manifest.fixtures.control_rigs.expected_total_link_pairs, 82);
    }

    #[test]
    fn rejects_absolute_fixture_paths() {
        let bytes = fs::read(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/manifest.json"
        )))
        .unwrap();
        let mut value = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap();
        value["fixtures"]["world_partition"]["asset"] =
            serde_json::Value::String("D:/private/LVL_StackOBot.umap".to_string());
        let manifest = serde_json::from_value::<Manifest>(value).unwrap();
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn rejects_weakened_corpus_gates() {
        let bytes = fs::read(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/manifest.json"
        )))
        .unwrap();
        let mut value = serde_json::from_slice::<serde_json::Value>(&bytes).unwrap();
        value["corpus"]["required_result"]["failed"] = serde_json::Value::from(1);
        let manifest = serde_json::from_value::<Manifest>(value).unwrap();
        assert!(manifest.validate().is_err());
    }
}
