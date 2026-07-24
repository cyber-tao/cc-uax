---
name: cc-uax
description: Analyze versioned, uncooked Unreal Engine 5 editor assets and projects from .uasset/.umap binaries. Use for Blueprint gameplay logic, asset properties, forward/reverse references, Control Rig, StateTree, PCG, World Partition closure, project inventory, or evidence-backed UE project audits with cc-uax.
---

# Analyze UE5 projects with cc-uax

Use `cc-uax` as the binary-evidence source. Treat its structured graph, reference, diagnostic, and coverage fields as evidence; never infer connectivity from rendered text or node names alone.

When analyzing a cc-uax checkout that is under active development, run the checkout binary with `cargo run -p cc-uax-cli -- ...` or an explicit `target/release/cc-uax` path so results do not come from an older installed binary on `PATH`.

Scope the result to versioned, uncooked UE5 editor packages. Report cooked, unversioned, unsupported, missing, or corrupt inputs as limitations instead of guessing.

## Establish the project report

1. Locate the `.uproject` or `Content` directory and any plugin content roots.
2. Read the project report's config-derived `entry_points` first. Inspect raw `Config/DefaultEngine.ini`, `DefaultGame.ini`, or platform overrides only when a reported diagnostic or missing key requires it; do not copy unrelated config values into the analysis.
3. Run exactly one project scan for the investigation:

```bash
cc-uax project "<PROJECT_OR_CONTENT_DIR>" --output "<REPORT.json>"
```

Add each nonstandard content root with `--mount <PACKAGE=RELATIVE>`. Use `--focus <PACKAGE_OR_GLOB>` to attach full typed analyses for selected packages while retaining the single project inventory and reference graph.

Keep strict mode enabled. Use `--allow-partial` only when the user explicitly accepts incomplete evidence, and carry every failure into the conclusion.

4. Inspect `schema_version`, `status`, `stats`, `reachability`, aggregate `analysis`, per-asset coverage/capabilities, `failures`, and `diagnostics` before analyzing gameplay. Read [references/report-contract.md](references/report-contract.md) when interpreting these fields.

Do not run one reverse scan per asset. Reuse the project report's inventory and bidirectional adjacency.

## Trace gameplay from configured roots

Start with the report's generated `reachability.configured_roots` and `reachability.reachable_runtime_packages`, then traverse both graph edges and asset references where focused evidence is needed:

1. Resolve the startup map and `GameInstance`/`GameMode` chain.
2. Include World Partition `ExternalActors`/`ExternalObjects`, Level Instances, and Packed Level Actors from the reported closure.
3. Analyze each K2/EdGraph by its stable graph identity. Follow `exec` edges for control flow and `data` edges/defaults for values. Never join nodes across graphs because their display names match.
4. Follow call targets, delegates, interfaces, component ownership, spawned classes, possessed pawns, widgets, save objects, and referenced data assets.
5. Use the native adapter that owns the source of truth:
   - K2/EdGraph for Blueprint and Niagara editor graphs.
   - RigVM model/links for Control Rig; do not double-count editor mirror graphs.
   - StateTree states, tasks, conditions, and transitions.
   - PCG nodes, pins, and edges.
6. Request a focused asset view when the project report lacks needed detail:

```bash
cc-uax asset "<FILE.uasset>" --view logic --output "<ASSET.json>"
cc-uax asset "<FILE.uasset>" --view properties --output "<ASSET.json>"
cc-uax asset "<FILE.uasset>" --view references --output "<ASSET.json>"
```

Use `--view full` only for a bounded asset; it can be large.

## Build an evidence-backed explanation

For each gameplay claim, retain:

- package path and asset class;
- graph/state/model identity;
- stable node/pin/state identities;
- ordered exec path;
- required data edges or default values;
- cross-asset reference or call target;
- relevant diagnostics and coverage status.

Separate findings into:

- `confirmed`: complete structured evidence supports the full claim;
- `partial`: some required path, data dependency, referenced package, or adapter is missing;
- `unsupported`: cc-uax declares the required capability opaque or unsupported;
- `contradicted`: structured evidence disproves the proposed behavior.

Do not upgrade `partial` to `confirmed` from naming conventions, screenshots, regex matches, opaque byte previews, or general UE conventions.

## Audit resource use

Use project `reachability` and adjacency to distinguish configured roots, reachable runtime dependencies, editor-only assets, isolated assets, and failed/unsupported assets. Treat “unreferenced” as a graph fact under the scanned mounts, not proof that deletion is safe; account for soft loads, primary asset rules, config paths, localization, and runtime-generated names.

When proposing deletion, require both no reachable hard/soft/config reference and adequate scan coverage.

## Finish with coverage

Summarize gameplay, resource use, and architecture alongside:

- indexed, analyzed, complete, partial, unsupported, and failed package counts;
- adapter-specific node/pin/edge/state/link counts;
- opaque capability types and byte ranges;
- excluded mounts or filters;
- every evidence gap that could change the conclusion.

If any required evidence is partial, unsupported, or failed, say exactly which conclusion remains unverified.
