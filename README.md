<div align="center">

# cc-uax

**Structured analysis of Unreal Engine 5 editor assets for Claude Code, Codex, and other engineering agents.**

[![Rust](https://img.shields.io/badge/Rust-2024%20edition-CE422B?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![CI](https://img.shields.io/github/actions/workflow/status/cyber-tao/cc-uax/ci.yml?branch=master&label=CI)](https://github.com/cyber-tao/cc-uax/actions/workflows/ci.yml)
[![UE5](https://img.shields.io/badge/UE5-5.0–5.8-0E1128?logo=unrealengine&logoColor=white)](https://www.unrealengine.com/)
[![License: MIT](https://img.shields.io/badge/license-MIT-2ea44f)](LICENSE)

**English** · [简体中文](README.zh-CN.md)

</div>

---

## Why cc-uax?

Most of an Unreal project lives in binary `.uasset` and `.umap` packages. Source-oriented agents can read C++ and configuration, but cannot otherwise inspect Blueprint execution flow, serialized properties, asset dependencies, PCG graphs, StateTrees, or World Partition packages.

`cc-uax` turns supported UE5 editor packages into typed, evidence-bearing reports. It can analyze one asset or build a project-wide index without loading Unreal Editor.

> Scope: versioned, uncooked UE5.0–5.8 editor packages (`FileVersionUE5` 1000–1018). That range is real-corpus-verified and may be `status=complete` when evidence is complete. Cooked/unversioned packages and UE4 packages are unsupported.

## What it provides

- **Typed package analysis** — package metadata, imports/exports, tagged properties, object references, diagnostics, and byte coverage.
- **Graph-aware logic** — K2/EdGraph graphs remain separated by their owning graph; execution and data edges are not inferred across unrelated graphs.
- **Specialized adapters** — K2/EdGraph, RigVM/ControlRig model links, StateTree state/task/condition/transition data, PCG nodes/pins/edges, and Niagara editor graphs where the serialized evidence supports them.
- **Project indexing** — one scan builds the asset inventory, forward/reverse adjacency, and World Partition external-package ownership closure.
- **Explicit uncertainty** — every report includes a schema version, overall status, machine-readable coverage, diagnostics, and capability evidence. Unsupported or intentionally opaque regions are named instead of being presented as successful decoding.
- **Agent skill** — the bundled skill teaches Claude Code and Codex to gather project evidence before describing gameplay or asset usage.

## Installation

Prebuilt releases install the binary and the complete agent-skill directory.

**Linux / macOS**

```bash
curl -fsSL https://raw.githubusercontent.com/cyber-tao/cc-uax/master/install.sh | bash
```

**Windows PowerShell**

```powershell
irm https://raw.githubusercontent.com/cyber-tao/cc-uax/master/install.ps1 | iex
```

Build the workspace from source with Rust 1.88 or newer:

```bash
git clone https://github.com/cyber-tao/cc-uax.git
cd cc-uax
cargo build -p cc-uax-cli --release --locked
```

The binary is written to `target/release/cc-uax[.exe]`. To install from the checkout:

```bash
cargo install --path crates/cc-uax-cli --locked
```

## CLI

The CLI has two explicit workflows.

### Analyze one asset

```text
cc-uax asset <FILE> [--view summary|logic|properties|references|full]
```

`--view` defaults to `full`.

```powershell
# High-level identity, status, coverage, and capabilities
cc-uax asset Content/Blueprints/BP_Player.uasset --view summary

# Graphs, nodes, exec/data edges, member references, and pin defaults
cc-uax asset Content/Blueprints/BP_Player.uasset --view logic

# Tagged properties and class defaults
cc-uax asset Content/Blueprints/BP_Player.uasset --view properties

# This file's imports and soft paths (outbound only)
cc-uax asset Content/Blueprints/BP_Player.uasset --view references

# Complete typed report
cc-uax asset Content/Blueprints/BP_Player.uasset --view full --output BP_Player.json
```

### Analyze a project

```text
cc-uax project <PROJECT_OR_CONTENT_DIR>
  [--focus <PACKAGE_OR_GLOB>]...
  [--mount <PACKAGE_PREFIX=RELATIVE_DIR>]...
  [--allow-partial]
  [--cache-file <FILE> | --no-cache]
```

```powershell
# Scan a .uproject directory or Content directory once.
# If several .uproject files share one Content tree, pass one file explicitly.
cc-uax project D:/Games/MyGame --output project-report.json
cc-uax project D:/Games/MyGame/MyGame.uproject --output project-report.json

# Add full analyses for matching packages while retaining one shared project index
cc-uax project D:/Games/MyGame --focus "/Game/Blueprints/**"

# Add explicit package mounts
cc-uax project D:/Games/MyGame --mount "/Plugin=Plugins/MyPlugin/Content"
```

Project analysis is **strict by default**. A mapped asset that cannot be read, indexed, or parsed produces a structured failure and exit code `2`. A package this tool deliberately does not target — a UE4 package, or a cooked, unversioned, UE3, big-endian or package-compressed one — is not a failure: it is indexed as `unsupported` evidence and the run still exits `0`. `--allow-partial` downgrades a hard scan failure to a zero exit while preserving the real status, failures, and reduced coverage in the report. Exit `1` means no report could be produced at all (unreadable asset, undiscoverable project, malformed `--mount`, failed write).

Project cache data defaults to the operating system's cache directory, never the analyzed project. Fresh cache entries reuse validated references and compact per-asset analysis summaries for unchanged packages. Use `--cache-file` for an explicit location or `--no-cache` for a cache-free run.

Global options apply to both commands: `--compact`, `--max-output-bytes <BYTES>`, and `-o`/`--output <FILE>`. Run `cc-uax asset --help` and `cc-uax project --help` for the full surface.

When a consumer's context window is limited, pass `--max-output-bytes <N>` to cap the rendered JSON at N UTF-8 bytes. Output stays valid JSON with the top-level evidence skeleton intact and a top-level `output` block recording `truncated` and every elided section, so you can re-query a narrower `--focus`/`--view` for the dropped detail. `output.truncated=true` is a render budget, not incomplete evidence. See [report-contract.md](skills/cc-uax/references/report-contract.md) for exactly which fields are preserved, the two documented limits, and the exit-code contract.

Step-by-step walkthroughs for common jobs are in [Usage tutorials](#usage-tutorials).

## Usage tutorials

The flags below work the same on Windows, macOS, and Linux. Examples use PowerShell; replace `D:/Games/MyGame` with your project path.

### 1. Start with one project scan

Do not run `cc-uax asset` once per Blueprint when you need references or reachability. Scan the project once, then drill in.

```powershell
cc-uax project D:/Games/MyGame --output project-report.json
```

Read these fields first, in this order:

1. `status`, `stats`, and `failures` — did the scan hold together?
2. `entry_points` — `GameDefaultMap`, `GameInstanceClass`, `GlobalDefaultGameMode`, and the other config-derived roots.
3. `reachability.configured_roots` and `reachability.reachable_runtime_packages` — what those roots actually reach.
4. `analysis` — complete / partial / unsupported / failed counts.
5. `forward` / `reverse` — cross-package adjacency for the whole scan.

A missing field is the default (empty / false / zero), not an unfinished scan. Field-level meaning lives in [report-contract.md](skills/cc-uax/references/report-contract.md).

If several `.uproject` files share one Content tree, pass one file explicitly:

```powershell
cc-uax project D:/Games/MyGame/MyGame.uproject --output project-report.json
```

### 2. Choose an asset view

`--view` defaults to `full`, which is large. Pick the smallest view that answers the question.

| View | Use when you need | Typical sections |
|---|---|---|
| `summary` | Identity, versions, coverage, capabilities | `summary`, `coverage`, `capabilities`, `diagnostics` |
| `logic` | Blueprint / Niagara / RigVM / StateTree / PCG graphs | `graphs`, `rigvm_graphs`, `state_tree_graphs`, `pcg_graphs` |
| `properties` | Tagged properties and class defaults | `exports[].properties` |
| `references` | This file's imports and soft paths | `references`, `imports` |
| `full` | One bounded asset, including export `serialization` | all of the above |

```powershell
cc-uax asset Content/Blueprints/BP_Player.uasset --view summary
cc-uax asset Content/Blueprints/BP_Player.uasset --view logic --output BP_Player.logic.json
cc-uax asset Content/Blueprints/BP_Player.uasset --view properties --output BP_Player.props.json
cc-uax asset Content/Blueprints/BP_Player.uasset --view references
```

### 3. Read a Blueprint's execution flow

```powershell
cc-uax asset Content/Blueprints/BP_Player.uasset --view logic --output BP_Player.logic.json
```

Then:

1. Confirm `status` and `coverage` (`graph_nodes_decoded`, `graph_edges_decoded`, `pins_decoded`). A `partial` report is still usable; do not invent missing edges.
2. Treat each entry in `graphs` as a separate graph. Identity is `full_name` / the owning export, not the display `name`. Never join two `BeginPlay` nodes across EventGraph and a function graph because the labels match.
3. Follow `edges` where `kind` is `exec` for control flow, and `kind` is `data` (or a pin `default_value` / `default_object`) for values, call targets, and spawn classes.
4. Use `edges` for in-graph connectivity. A pin's `linked_to` is only for cross-graph or unresolved links.
5. Read `nodes[].member` for the function, event, or variable the node calls. Read `user_defined_pins` on custom events and functions.
6. For Control Rig, use `rigvm_graphs` and `links` (`source_pin_path` → `target_pin_path`). Do not also count the editor mirror in `graphs`. Compiled VM bytecode stays `known_opaque`.
7. For StateTree / PCG, use `state_tree_graphs` / `pcg_graphs`. A node's full tagged properties live on the matching `exports[]` entry by `index`.

Need class defaults or a variable's serialized value as well? Run `--view properties` on the same file. Use `--view full` only when one asset is small enough.

### 4. Find who references an asset

Outbound references of one file:

```powershell
cc-uax asset Content/Blueprints/BP_Player.uasset --view references
```

Inbound ("what uses this?") needs the project index. After one `project` scan, look up the canonical package path in `reverse`:

```text
reverse["/Game/Blueprints/BP_Player"]  →  packages that reference it
forward["/Game/Blueprints/BP_Player"] →  packages it references
```

`reachability.isolated_project_assets` and `reachability.unreachable_project_assets` are graph facts under the scanned mounts, not proof that deletion is safe. Soft loads, primary asset rules, localization, and runtime-generated names sit outside that graph.

### 5. Trace gameplay from the startup map

```powershell
cc-uax project D:/Games/MyGame --output project-report.json --focus "/Game/Maps/L_Startup" --focus "/Game/Blueprints/BP_GameMode"
```

1. Resolve `entry_points.defaults` (`GameDefaultMap`, `GameInstanceClass`, `GlobalDefaultGameMode`, and the other keys). Platform overrides live under `entry_points.platforms`.
2. Walk `reachability.configured_roots` → `reachable_runtime_packages`.
3. Include World Partition members from `ownership_closure` / `reachability.ownership_closure_members` (ExternalActors, ExternalObjects, and Level Instance / Packed Level Actor sub-levels whose `WorldAsset` / `PackedWorldAsset` was decoded).
4. Attach full analyses with `--focus` for the map, GameMode, and any Blueprint you need to walk. `--focus` is repeatable.

### 6. Select packages with `--focus`

`--focus` matches canonical package paths, case-insensitively. A trailing `.uasset` / object suffix is stripped. `?` is one non-separator character, `*` is one path segment, `**` crosses `/`.

```powershell
# Direct children of Blueprints only
cc-uax project D:/Games/MyGame --focus "/Game/Blueprints/*"

# Whole subtree
cc-uax project D:/Games/MyGame --focus "/Game/Blueprints/**"

# Several selections in one scan
cc-uax project D:/Games/MyGame --focus "/Game/Maps/L_Startup" --focus "/Game/Characters/**"

# A single package, with or without the asset suffix
cc-uax project D:/Games/MyGame --focus "/Game/Blueprints/BP_Player.uasset"
```

A pattern that matches nothing is a hard failure (exit `2`) and is recorded under `failures` with `stage=focus`. The report is still written. Out-of-scope packages that match stay `unsupported` in `inventory` and are not focus failures.

### 7. Include plugin or extra content

The default mount is `/Game` → the project's `Content` directory. Add every other content root you need; otherwise those packages are invisible to inventory, adjacency, and reachability.

```powershell
cc-uax project D:/Games/MyGame `
  --mount "/MyPlugin=Plugins/MyPlugin/Content" `
  --mount "/Another=Plugins/Another/Content" `
  --output project-report.json
```

The path after `=` is project-relative. A malformed `--mount` exits `1` and produces no report.

### 8. Keep a report inside a context window

```powershell
cc-uax project D:/Games/MyGame --focus "/Game/Blueprints/BP_Player" --compact --max-output-bytes 200000 --output bp.json
cc-uax asset Content/Blueprints/BP_Player.uasset --view logic --max-output-bytes 80000
```

`--compact` removes pretty-print whitespace. `--max-output-bytes` elides heavy detail and adds a top-level `output` block (`truncated`, `elided`, …). `output.truncated=true` is a size cap, not incomplete evidence — re-query a narrower `--view` or `--focus` for dropped sections. See [report-contract.md](skills/cc-uax/references/report-contract.md) for what is always preserved.

### 9. Read status, coverage, and exit codes together

| You see | Meaning | Typical next step |
|---|---|---|
| `status=complete`, exit `0` | Requested evidence decoded | Use the graphs / references as-is |
| `status=partial`, exit `0` | Usable report with a named gap (`known_opaque`, a failed region, …) | Keep the gap in the conclusion; do not invent the missing path |
| `status=unsupported`, exit `0` | Cooked / UE4 / otherwise out of scope, or a requested capability cannot be derived | Treat as a limitation, not a crash |
| exit `2` (`project`) | Hard scan failure: unreadable mapped asset, in-scan mount/cache error, or `--focus` miss | Read `failures`; optionally rerun with `--allow-partial` |
| exit `1` | No report at all | Fix the path, `--mount` syntax, or output location |

`--allow-partial` only changes the exit code. It does not rewrite `status`, `failures`, or coverage.

### 10. Control the project cache

```powershell
# Default: OS cache directory, never inside the Unreal project
cc-uax project D:/Games/MyGame --output project-report.json

# Explicit cache file
cc-uax project D:/Games/MyGame --cache-file D:/caches/mygame-uax.sqlite --output project-report.json

# No cache (CI, or after a decoder change)
cc-uax project D:/Games/MyGame --no-cache --output project-report.json
```

Fresh cache entries reuse validated references and compact per-asset summaries for unchanged packages.

### 11. Run from a source checkout

An older `cc-uax` on `PATH` will silently disagree with this repository. From a clone:

```powershell
cargo run -p cc-uax-cli --release --locked -- project D:/Games/MyGame --output project-report.json
cargo run -p cc-uax-cli --release --locked -- asset Content/Blueprints/BP_Player.uasset --view logic
```

Or call `target/release/cc-uax.exe` directly.

### 12. Use it from Claude Code or Codex

Install the whole [`skills/cc-uax/`](skills/cc-uax/) directory (see [Agent skill](#agent-skill)). Ask the agent to scan the project first, then to cite `graphs[].edges`, package paths, and `coverage` / `diagnostics` rather than guessing from node display names.

## Report contract

Reports are typed internally and rendered to JSON only at the CLI boundary. Asset reports expose `coverage`, `capabilities`, and `diagnostics` directly. Project reports expose the same accounting through aggregate `analysis`, compact per-inventory analyses, generated `reachability`, and optional full `focused` analyses:

**Asset report (abbreviated):**

```jsonc
{
  "schema_version": 5,
  "status": "complete",
  "view": "full",
  "summary": { /* package name, file/custom versions, engine version, table counts, … */ },
  "coverage": {
    /* non-zero requested/decoded/opaque/failed counters (zero counters omitted) */
  },
  "capabilities": [
    /* capability-specific evidence and limitations */
  ],
  "exports": [ /* … */ ], "graphs": [ /* … */ ]
  /* Sparse output: empty or default fields (null, [], false, "", "None", 0) are omitted. */
}
```

**Project report (abbreviated):**

```jsonc
{
  "schema_version": 6,
  "status": "complete",
  "layout": {}, "mounts": [], "entry_points": {},
  "reachability": {
    /* configured roots, reachable runtime packages, closure members, isolated packages, and coverage gaps */
  },
  "stats": { /* every filesystem/index/cache counter, including zeros */ },
  "analysis": {
    /* aggregate coverage, capabilities, and per-asset summaries */
  },
  "inventory": [ /* one compact analysis per package */ ],
  "focused": { /* full AssetAnalysis for packages matching --focus */ }
  /* Sparse output: empty adjacency, failures, diagnostics, and reachability sets are omitted. */
}
```

Status meanings:

| Status | Meaning |
|---|---|
| `complete` | All evidence required by the requested view was decoded without an unresolved gap. |
| `partial` | The report is usable, but at least one requested region failed, remained opaque, or could not be linked. |
| `unsupported` | The requested capability cannot be derived from this package/version. |

`known_opaque` is a deliberate capability result, not success. Examples include compiled RigVM bytecode and compressed RigHierarchy payloads that cannot yet be represented as source-level logic. A report with such a requested gap must not be promoted to `complete`.

Stable public core types include `PackageView<'a>`, `AssetAnalysis`, `DecodedValue`, `LogicGraph`, `GraphNode`, `GraphEdge`, and `ParseCoverage`. `PackageView<'a>` binds parsing and decoding to the same byte slice so callers cannot accidentally parse one file and decode another.

## Architecture

The repository is a virtual Cargo workspace with three responsibilities:

```text
cc-uax/
├── crates/
│   ├── cc-uax-core/       # byte-bound package parsing, typed values, graphs, coverage
│   ├── cc-uax-project/    # project discovery, inventory, adjacency, ownership, cache policy
│   └── cc-uax-cli/        # asset/project commands and JSON rendering
└── skills/
    └── cc-uax/            # full Claude Code/Codex skill package
```

Dependency direction is one-way:

```text
cc-uax-cli ──> cc-uax-project ──> cc-uax-core
      └────────────────────────> cc-uax-core
```

- `cc-uax-core` does not own filesystem scanning, SQLite, CLI arguments, or JSON presentation policy.
- `cc-uax-project` owns mounts, project discovery, the shared inventory scan, reference adjacency, World Partition ownership, reachability/resource summaries, and cache placement.
- `cc-uax-cli` selects views/focuses, attaches requested full asset analyses, enforces exit behavior, and renders typed reports.

See [CLAUDE.md](CLAUDE.md) for contributor-level parsing rules.

## Agent skill

Copy the entire [`skills/cc-uax/`](skills/cc-uax/) directory, not only `SKILL.md`:

| Agent | User-level location | Project-level location |
|---|---|---|
| Claude Code | `~/.claude/skills/cc-uax/` | `<repo>/.claude/skills/cc-uax/` |
| Codex | `~/.codex/skills/cc-uax/` | `<repo>/.codex/skills/cc-uax/` |
| Agents-compatible clients | `~/.agents/skills/cc-uax/` | `<repo>/.agents/skills/cc-uax/` |

The supporting `agents/` and `references/` content is part of the skill contract.

## Validation and support boundary

Serialization decisions are checked against UE5.0–5.8 source and exercised against external, real editor assets across that range. Validation acceptance gates are defined by the real-corpus harness, which is maintained separately from the workspace crates and is not committed as a workspace member.

External assets and machine-specific absolute paths remain local. The workspace does not commit them.

Current limitations include:

- cooked/unversioned packages and UE4 package formats;
- source-level reconstruction of compiled RigVM bytecode and compressed RigHierarchy data;
- runtime behavior not evidenced by serialized graphs, properties, configuration, or references;
- plugin-native formats without a verified UE5.0–5.8 serialization contract.

When evidence is incomplete, consumers must retain `partial`, `unsupported`, diagnostics, and capability limitations in their conclusions.

## Contributing

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --release --locked
```

## License

[MIT](LICENSE)
