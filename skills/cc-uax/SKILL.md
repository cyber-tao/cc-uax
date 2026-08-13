---
name: cc-uax
description: Analyze versioned, uncooked Unreal Engine 5 editor assets and projects from .uasset/.umap binaries. Use for Blueprint gameplay logic, asset properties, forward/reverse references, Control Rig, StateTree, PCG, World Partition closure, project inventory, or evidence-backed UE project audits with cc-uax.
---

# Analyze UE5 projects with cc-uax

Use `cc-uax` as the binary-evidence source. Treat its structured graph, reference, diagnostic, and coverage fields as evidence; never infer connectivity from rendered text or node names alone.

When analyzing a cc-uax checkout that is under active development, run the checkout binary with `cargo run -p cc-uax-cli -- ...` or an explicit `target/release/cc-uax` path so results do not come from an older installed binary on `PATH`.

Scope the result to versioned, uncooked UE5.0–5.8 editor packages (`FileVersionUE5` 1000–1018). That range is real-corpus-verified and may be `status=complete` when evidence is complete. Report cooked, unversioned, UE4, unsupported, missing, or corrupt inputs as limitations instead of guessing.

## Establish the project report

1. Locate the `.uproject` or `Content` directory and any plugin content roots. If several `.uproject` files share one Content tree, pass one file path explicitly; a directory scan still fails when more than one `.uproject` is present.
2. Read the project report's config-derived `entry_points` first. Inspect raw `Config/DefaultEngine.ini`, `DefaultGame.ini`, or platform overrides only when a reported diagnostic or missing key requires it; do not copy unrelated config values into the analysis.
3. Run exactly one project scan for the investigation:

```bash
cc-uax project "<PROJECT_OR_CONTENT_DIR>" --output "<REPORT.json>"
```

Add each nonstandard content root with `--mount <PACKAGE=RELATIVE>`. Use `--focus <PACKAGE_OR_GLOB>` to attach full typed analyses for selected packages while retaining the single project inventory and reference graph.

Keep strict mode enabled: it exits nonzero only for hard scan failures (unreadable, unparseable, or unindexable mapped assets), while inherent partial or unsupported evidence keeps a truthful non-complete `status` and exits zero. Use `--allow-partial` only when the user explicitly accepts hard failures with a zero exit, and carry every failure and non-complete status into the conclusion.

4. Inspect `schema_version`, `status`, `stats`, `reachability`, aggregate `analysis`, per-asset coverage/capabilities, `failures`, and `diagnostics` before analyzing gameplay. Read [references/report-contract.md](references/report-contract.md) when interpreting these fields.

Reports are sparse: empty or default fields (`null`, `[]`, `false`, `""`, `"None"`, `0`) are omitted. A missing field means the default — read it as absent, not as an error or an unfinished scan. See [references/report-contract.md](references/report-contract.md) for the current asset and project `schema_version` numbers.

Do not run one reverse scan per asset. Reuse the project report's inventory and bidirectional adjacency.

## Trace gameplay from configured roots

Start with the report's generated `reachability.configured_roots` and `reachability.reachable_runtime_packages`, then traverse both graph edges and asset references where focused evidence is needed:

1. Resolve the startup map and `GameInstance`/`GameMode` chain.
2. Include World Partition `ExternalActors`/`ExternalObjects` from the reported ownership closure. Include Level Instance / Packed Level Actor sub-levels when they appear in that closure (derived from decoded `WorldAsset` / `PackedWorldAsset`). If a LI/PLA world asset is only a soft reference and is not a closure member, follow the soft reference and mark the claim `partial`.
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

When the caller's context window is limited, pass `--max-output-bytes <N>` (UTF-8 bytes) to cap any `asset` or `project` render to the space that is actually available. The report stays valid JSON and preserves the evidence skeleton (`status`, `coverage`, `capabilities`, `diagnostics`, `known_opaque`); a top-level `output` block reports `truncated` and every elided section with its dropped-element count. `output.truncated=true` means the render was size-capped, not that evidence is incomplete — keep using `status` / coverage / capabilities, and re-query a narrower `--focus` or `--view` only to recover elided detail.

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
