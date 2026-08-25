---
title: Tutorials
description: Step-by-step walkthroughs for common cc-uax jobs.
---

# Tutorials

The flags below work the same on Windows, macOS, and Linux. Examples use PowerShell; replace `D:/Games/MyGame` with your project path.

## 1. Start with one project scan

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

A missing field is the default (empty / false / zero), not an unfinished scan. Field-level meaning lives in [`report-contract.md`](https://github.com/cyber-tao/cc-uax/blob/master/skills/cc-uax/references/report-contract.md).

If several `.uproject` files share one Content tree, pass one file explicitly:

```powershell
cc-uax project D:/Games/MyGame/MyGame.uproject --output project-report.json
```

## 2. Choose an asset view

`--view` defaults to `full`, which is large. Pick the smallest view that answers the question.

```powershell
cc-uax asset Content/Blueprints/BP_Player.uasset --view summary
cc-uax asset Content/Blueprints/BP_Player.uasset --view logic --output BP_Player.logic.json
cc-uax asset Content/Blueprints/BP_Player.uasset --view properties --output BP_Player.props.json
cc-uax asset Content/Blueprints/BP_Player.uasset --view references
```

## 3. Read a Blueprint's execution flow

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

## 4. Find who references an asset

Outbound references of one file:

```powershell
cc-uax asset Content/Blueprints/BP_Player.uasset --view references
```

Inbound ("what uses this?") needs the project index. After one `project` scan, look up the canonical package path in `reverse`:

```text
reverse["/Game/Blueprints/BP_Player"]  →  packages that reference it
forward["/Game/Blueprints/BP_Player"] →  packages it references
```

`reachability.isolated_project_assets` and `reachability.unreachable_project_assets` are graph facts under the scanned mounts, not proof that deletion is safe.

## 5. Trace gameplay from the startup map

```powershell
cc-uax project D:/Games/MyGame --output project-report.json --focus "/Game/Maps/L_Startup" --focus "/Game/Blueprints/BP_GameMode"
```

1. Resolve `entry_points.defaults` (`GameDefaultMap`, `GameInstanceClass`, `GlobalDefaultGameMode`, and the other keys). Platform overrides live under `entry_points.platforms`.
2. Walk `reachability.configured_roots` → `reachable_runtime_packages`.
3. Include World Partition members from `ownership_closure` / `reachability.ownership_closure_members`.
4. Attach full analyses with `--focus` for the map, GameMode, and any Blueprint you need to walk. `--focus` is repeatable.

## 6. Select packages with `--focus`

`--focus` matches canonical package paths, case-insensitively. A trailing `.uasset` / object suffix is stripped. `?` is one non-separator character, `*` is one path segment, `**` crosses `/`.

```powershell
cc-uax project D:/Games/MyGame --focus "/Game/Blueprints/*"
cc-uax project D:/Games/MyGame --focus "/Game/Blueprints/**"
cc-uax project D:/Games/MyGame --focus "/Game/Maps/L_Startup" --focus "/Game/Characters/**"
cc-uax project D:/Games/MyGame --focus "/Game/Blueprints/BP_Player.uasset"
```

A pattern that matches nothing is a hard failure (exit `2`) and is recorded under `failures` with `stage=focus`. The report is still written. Out-of-scope packages that match stay `unsupported` in `inventory` and are not focus failures.

## 7. Include plugin or extra content

Plugin content under `Plugins/` is mounted for you, as `/{name}` taken from each `.uplugin` file's base name. A `MetaXR` folder shipping `OculusXR.uplugin` mounts as `/OculusXR` — check `mounts` in the report rather than guessing.

```powershell
cc-uax project D:/Games/MyGame `
  --mount "/Extra=ExtraContent" `
  --output project-report.json
```

## 8. Keep a report inside a context window

```powershell
cc-uax project D:/Games/MyGame --focus "/Game/Blueprints/BP_Player" --compact --max-output-bytes 200000 --output bp.json
cc-uax asset Content/Blueprints/BP_Player.uasset --view logic --max-output-bytes 80000
```

`--compact` removes pretty-print whitespace. `--max-output-bytes` elides heavy detail and adds a top-level `output` block. Re-query a narrower `--view` or `--focus` for dropped sections.

## 9. Read status, coverage, and exit codes together

| You see | Meaning | Typical next step |
|---|---|---|
| `status=complete`, exit `0` | Requested evidence decoded | Use the graphs / references as-is |
| `status=partial`, exit `0` | Usable report with a named gap | Keep the gap; do not invent the missing path |
| `status=unsupported`, exit `0` (`project`) | Every scanned package is out of scope | Treat as a limitation, not a crash |
| exit `1` (`asset`) on a cooked / UE4 / out-of-range package | No report: the parser does not target it | Scan the project instead |
| exit `2` (`project`) | Hard scan failure | Read `failures`; optionally rerun with `--allow-partial` |
| exit `1` | No report at all | Fix the path, `--mount` syntax, or output location |

`--allow-partial` only changes the exit code. It does not rewrite `status`, `failures`, or coverage.

## 10. Control the project cache

```powershell
cc-uax project D:/Games/MyGame --output project-report.json
cc-uax project D:/Games/MyGame --cache-file D:/caches/mygame-uax.sqlite --output project-report.json
cc-uax project D:/Games/MyGame --no-cache --output project-report.json
```

Fresh cache entries reuse validated references and compact per-asset summaries for unchanged packages.

## 11. Run from a source checkout

```powershell
cargo run -p cc-uax-cli --release --locked -- project D:/Games/MyGame --output project-report.json
cargo run -p cc-uax-cli --release --locked -- asset Content/Blueprints/BP_Player.uasset --view logic
```

## 12. Use it from Claude Code or Codex

Install the whole [`skills/cc-uax/`](https://github.com/cyber-tao/cc-uax/tree/master/skills/cc-uax) directory. Ask the agent to scan the project first, then to cite `graphs[].edges`, package paths, and `coverage` / `diagnostics` rather than guessing from node display names.
