<div align="center">

# cc-uax

**Structured analysis of Unreal Engine 5 editor assets for Claude Code, Codex, and other engineering agents.**

[![Rust](https://img.shields.io/badge/Rust-2024%20edition-CE422B?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![CI](https://img.shields.io/github/actions/workflow/status/cyber-tao/cc-uax/ci.yml?branch=master&label=CI)](https://github.com/cyber-tao/cc-uax/actions/workflows/ci.yml)
[![UE5](https://img.shields.io/badge/UE5-5.0–5.8-0E1128?logo=unrealengine&logoColor=white)](https://www.unrealengine.com/)
[![License: MIT](https://img.shields.io/badge/license-MIT-2ea44f)](LICENSE)

[Website](https://cyber-tao.github.io/cc-uax/) · **English** · [简体中文](README.zh-CN.md)

</div>

---

`cc-uax` turns supported UE5 editor packages (`.uasset` and `.umap`) into typed, evidence-bearing reports. It analyzes one asset or builds a project-wide index without loading Unreal Editor.

## Why cc-uax?

Most of an Unreal project lives in binary packages. Source-oriented agents can read C++ and configuration, but cannot otherwise inspect Blueprint execution flow, serialized properties, asset dependencies, PCG graphs, StateTrees, or World Partition packages. `cc-uax` supplies that evidence.

## What it provides

- **Typed package analysis** — package metadata, imports/exports, tagged properties, object references, diagnostics, and byte coverage.
- **Graph-aware logic** — K2/EdGraph graphs stay separated by their owning graph; execution and data edges are never inferred across unrelated graphs.
- **Specialized adapters** — RigVM/ControlRig model links, StateTree, PCG, and Niagara editor graphs where the serialized evidence supports them.
- **Project indexing** — one scan builds the asset inventory, forward/reverse adjacency, reachability, and World Partition ownership closure.
- **Explicit uncertainty** — every report carries a schema version, status, machine-readable coverage, diagnostics, and capability evidence. Unsupported or opaque regions are named, not presented as successful decoding.
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

Building from source (Rust 1.88 or newer), installing from a checkout, and uninstalling are covered in the [install guide](website/docs/guide/install.md).

## Quick start

```powershell
# One asset: pick the smallest --view that answers the question
cc-uax asset Content/Blueprints/BP_Player.uasset --view logic

# A whole project: scan once, then drill in with --focus
cc-uax project D:/Games/MyGame --output project-report.json
```

Options, views, mounts, the cache, and exit codes are in the [CLI guide](website/docs/guide/cli.md); step-by-step walkthroughs are in the [tutorials](website/docs/guide/tutorials.md).

## Scope

`cc-uax` targets versioned, uncooked UE5.0–5.8 editor packages: `FileVersionUE5` 1000–1018, checked against UE5.0–5.8 source. Real projects have exercised 1002–1004, 1006–1009 and 1012–1018; 1000, 1001, 1005, 1010 and 1011 have not been seen in a real asset yet. A package may be `status=complete` when its evidence is complete.

Out of scope, and rejected rather than guessed at: UE4 and older, anything above 1018, cooked packages (including `PKG_Cooked` and `PKG_UnversionedProperties`), unversioned, big-endian, and compressed packages. `cc-uax asset` exits `1` for them; `cc-uax project` indexes them as `unsupported` evidence and still exits `0`. For a UE4-format package (`FileVersionUE5` = 0) a project scan also reads its linker reference tables, so it contributes reference edges and reachability. See [Scope and limits](website/docs/guide/limits.md).

## Documentation

| Topic | Page |
|---|---|
| Install | [website/docs/guide/install.md](website/docs/guide/install.md) |
| CLI | [website/docs/guide/cli.md](website/docs/guide/cli.md) |
| Tutorials | [website/docs/guide/tutorials.md](website/docs/guide/tutorials.md) |
| Reading reports | [website/docs/guide/reports.md](website/docs/guide/reports.md) |
| Scope and limits | [website/docs/guide/limits.md](website/docs/guide/limits.md) |
| Architecture | [website/docs/guide/architecture.md](website/docs/guide/architecture.md) |
| Agent skill | [website/docs/guide/skill.md](website/docs/guide/skill.md) |
| Report fields, codes, exit codes, schema versions | [report-contract.md](skills/cc-uax/references/report-contract.md) |

The same guides are published at [cyber-tao.github.io/cc-uax](https://cyber-tao.github.io/cc-uax/).

## Agent skill

Copy the entire [`skills/cc-uax/`](skills/cc-uax/) directory, not only `SKILL.md`; the `agents/` and `references/` content is part of the skill contract. Prebuilt installers place it for you, and `./dev-install.sh` / `.\dev-install.ps1` link the working-tree copy from a checkout. Locations for Claude Code, Codex, and Agents-compatible clients are in the [skill guide](website/docs/guide/skill.md).

## Contributing

Engineering rules live in [CLAUDE.md](CLAUDE.md). Before sending a change, run:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --release --locked
```

## License

[MIT](LICENSE)
