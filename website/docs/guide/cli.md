---
title: CLI
description: The two cc-uax workflows — analyze one asset, or scan a project.
---

# CLI

The CLI has two explicit workflows. Global options apply to both: `--compact`, `--max-output-bytes <BYTES>`, and `-o` / `--output <FILE>`. Run `cc-uax asset --help` and `cc-uax project --help` for the full surface.

When a consumer's context window is limited, pass `--max-output-bytes <N>` to cap rendered JSON at N UTF-8 bytes. Output stays valid JSON with the top-level evidence skeleton intact and a top-level `output` block recording `truncated` and every elided section. `output.truncated=true` is a render budget, not incomplete evidence.

Field-level meaning, the two documented budget limits, and the exit-code contract live in [`report-contract.md`](https://github.com/cyber-tao/cc-uax/blob/master/skills/cc-uax/references/report-contract.md).

## Analyze one asset

```text
cc-uax asset <FILE> [--view summary|logic|properties|references|full]
```

`--view` defaults to `full`. Pick the smallest view that answers the question.

| View | Use when you need | Typical sections |
|---|---|---|
| `summary` | Identity, versions, coverage, capabilities | `summary`, `coverage`, `capabilities`, `diagnostics` |
| `logic` | Blueprint / Niagara / RigVM / StateTree / PCG graphs | `graphs`, `rigvm_graphs`, `state_tree_graphs`, `pcg_graphs` |
| `properties` | Tagged properties and class defaults | `exports[].properties` |
| `references` | This file's imports and soft paths | `references`, `imports` |
| `full` | One bounded asset, including export `serialization` | all of the above |

```powershell
cc-uax asset Content/Blueprints/BP_Player.uasset --view summary
cc-uax asset Content/Blueprints/BP_Player.uasset --view logic
cc-uax asset Content/Blueprints/BP_Player.uasset --view properties
cc-uax asset Content/Blueprints/BP_Player.uasset --view references
cc-uax asset Content/Blueprints/BP_Player.uasset --view full --output BP_Player.json
```

## Analyze a project

```text
cc-uax project <PROJECT_OR_CONTENT_DIR>
  [--focus <PACKAGE_OR_GLOB>]...
  [--mount <PACKAGE_PREFIX=RELATIVE_DIR>]...
  [--allow-partial]
  [--cache-file <FILE> | --no-cache]
```

```powershell
cc-uax project D:/Games/MyGame --output project-report.json
cc-uax project D:/Games/MyGame/MyGame.uproject --output project-report.json
cc-uax project D:/Games/MyGame --focus "/Game/Blueprints/**"
cc-uax project D:/Games/MyGame --mount "/Plugin=Plugins/MyPlugin/Content"
```

Passing an explicit `.uproject` file selects that file even when sibling platform `.uproject` files share the same Content tree. A directory or Content path still errors if more than one `.uproject` is present.

### Mounts

The default mounts are `/Game` plus every plugin content root under `Plugins/`, mounted the way Unreal mounts them: `/{name}` from the `.uplugin` file's base name, which is often not the plugin's directory name. Without them, plugin packages are invisible to inventory, adjacency, and reachability.

`--mount` adds other content roots or redirects one of the discovered roots. The path after `=` is project-relative. A malformed `--mount` exits `1` and produces no report.

### Configured roots

Configured roots come from `GameMapsSettings` and from the `ProjectPackagingSettings` cook lists (`+MapsToCook`, `+DirectoriesToAlwaysCook`). `GameDefaultMap` is frequently a developer map; the cook list is what a build actually ships.

### Strict mode and cache

Project analysis is **strict by default**. A mapped asset that cannot be read, indexed, or parsed produces a structured failure and exit code `2`. A package this tool deliberately does not target — UE4, cooked, unversioned, UE3, big-endian, or package-compressed — is indexed as `unsupported` evidence and the run still exits `0`.

`--allow-partial` downgrades a hard scan failure to a zero exit. It does not rewrite `status`, `failures`, or coverage. Exit `1` means no report could be produced at all.

Project cache data defaults to the operating system's cache directory, never the analyzed project. Use `--cache-file` for an explicit location or `--no-cache` for a cache-free run.
