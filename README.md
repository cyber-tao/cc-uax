<div align="center">

# cc-uax

**Structured analysis of Unreal Engine 5 editor assets for Claude Code, Codex, and other engineering agents.**

[![Rust](https://img.shields.io/badge/Rust-2024%20edition-CE422B?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![CI](https://img.shields.io/github/actions/workflow/status/cyber-tao/cc-uax/ci.yml?branch=master&label=CI)](https://github.com/cyber-tao/cc-uax/actions/workflows/ci.yml)
[![UE5](https://img.shields.io/badge/reference-UE%205.7-0E1128?logo=unrealengine&logoColor=white)](https://www.unrealengine.com/)
[![License: MIT](https://img.shields.io/badge/license-MIT-2ea44f)](LICENSE)

**English** · [简体中文](README.zh-CN.md)

</div>

---

## Why cc-uax?

Most of an Unreal project lives in binary `.uasset` and `.umap` packages. Source-oriented agents can read C++ and configuration, but cannot otherwise inspect Blueprint execution flow, serialized properties, asset dependencies, PCG graphs, StateTrees, or World Partition packages.

`cc-uax` turns supported UE5 editor packages into typed, evidence-bearing reports. It can analyze one asset or build a project-wide index without loading Unreal Editor.

> Scope: versioned, uncooked UE5 editor packages (`FileVersionUE5 >= 1000`). Cooked/unversioned packages and UE4 packages are intentionally unsupported.

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

Build the 0.9 workspace from source with Rust 1.88 or newer:

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

The 0.9 CLI has two explicit workflows.

### Analyze one asset

```text
cc-uax asset <FILE> --view <summary|logic|properties|references|full>
```

```powershell
# High-level identity, status, coverage, and capabilities
cc-uax asset Content/Blueprints/BP_Player.uasset --view summary

# Graphs, nodes, exec/data edges, member references, and pin defaults
cc-uax asset Content/Blueprints/BP_Player.uasset --view logic

# Complete typed report
cc-uax asset Content/Blueprints/BP_Player.uasset --view full --output BP_Player.json
```

### Analyze a project

```text
cc-uax project <PROJECT_OR_CONTENT_DIR>
  [--focus <PACKAGE_OR_GLOB>]
  [--mount <PACKAGE_PREFIX=RELATIVE_DIR>]...
  [--allow-partial]
  [--cache-file <FILE> | --no-cache]
```

```powershell
# Scan a .uproject directory or Content directory once
cc-uax project D:/Games/MyGame --output project-report.json

# Add full analyses for matching packages while retaining one shared project index
cc-uax project D:/Games/MyGame --focus "/Game/Blueprints/**"

# Add explicit package mounts
cc-uax project D:/Games/MyGame --mount "/Plugin=Plugins/MyPlugin/Content"
```

Project analysis is **strict by default**. A mapped asset that cannot be read, indexed, or parsed produces a structured failure, and any project report whose requested evidence remains `partial` or `unsupported` exits non-zero. `--allow-partial` permits a successful process exit while preserving the real status, failures, and reduced coverage in the report.

Project cache data defaults to the operating system's cache directory, never the analyzed project. Fresh cache entries reuse validated references and compact per-asset analysis summaries for unchanged packages. Use `--cache-file` for an explicit location or `--no-cache` for a cache-free run.

Run `cc-uax asset --help` and `cc-uax project --help` for output formatting options.

## Report contract

Reports are typed internally and rendered to JSON only at the CLI boundary. Asset reports expose `coverage`, `capabilities`, and `diagnostics` directly. Project reports expose the same accounting through aggregate `analysis`, compact per-inventory analyses, generated `reachability`, and optional full `focused` analyses:

**Asset report (abbreviated):**

```jsonc
{
  "schema_version": 1,
  "status": "complete",
  "view": "full",
  "summary": { /* package name, class, file version, … */ },
  "coverage": {
    /* requested, decoded, opaque, unsupported, and failed evidence */
  },
  "capabilities": [
    /* capability-specific evidence and limitations */
  ],
  "diagnostics": [],
  "exports": [], "graphs": [], "references": {}, "known_opaque": []
}
```

**Project report (abbreviated):**

```jsonc
{
  "schema_version": 2,
  "status": "complete",
  "layout": {}, "mounts": {}, "entry_points": {},
  "reachability": {
    /* configured roots, reachable runtime packages, closure members, isolated packages, and coverage gaps */
  },
  "stats": { "discovered": 1961, "indexed": 1961, "failed": 0, "skipped": 0 },
  "analysis": {
    /* aggregate coverage, capabilities, and per-asset summaries */
  },
  "focused": [
    /* full AssetAnalysis for packages matching --focus */
  ],
  "failures": [], "diagnostics": []
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

Serialization decisions are checked against UE5.7 source and the parser is exercised against external, real editor assets. Validation acceptance gates are defined by the real-corpus harness, which is maintained separately from the workspace crates and is not committed as a workspace member.

External assets and machine-specific absolute paths remain local. The workspace does not commit them.

Current limitations include:

- cooked/unversioned packages and UE4 package formats;
- source-level reconstruction of compiled RigVM bytecode and compressed RigHierarchy data;
- runtime behavior not evidenced by serialized graphs, properties, configuration, or references;
- plugin-native formats without a verified UE5.7 serialization contract.

When evidence is incomplete, consumers must retain `partial`, `unsupported`, diagnostics, and capability limitations in their conclusions.

## Contributing

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked
cargo test --workspace --locked
cargo build --workspace --release --locked
```

## License

[MIT](LICENSE)
