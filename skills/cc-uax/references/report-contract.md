# cc-uax report contract

Read this reference when interpreting `asset` or `project` JSON.

## Completion fields

- `schema_version`: Version of the report contract, independent from the CLI version. Asset reports use schema version `5`; project reports use schema version `5`.
- `status`: `complete`, `partial`, or `unsupported` for the requested analysis scope.
- Asset reports expose `coverage` counters for exports, properties, EdGraph, RigVM, PCG, StateTree, opaque regions (count and `opaque_bytes` total), and diagnostics within the requested view. Byte conservation over export payloads is reported by `export_bytes_total` (sum of analyzed `serial_size`) and `unclassified_bytes`, which is always `0` in a correct report — a non-zero value is a defect.
- Project reports use schema version `5` and expose sanitized `layout`/`mounts`, filesystem/index accounting in `stats` (`discovered`, `indexed`, `failed`, `skipped`, `skipped_symlinks`), generated runtime/resource `reachability`, and semantic accounting in aggregate `analysis` (`assets`, `complete_assets`, `partial_assets`, `unsupported_assets`, and summed `coverage`). `stats.failed` counts only asset-level failures (a discovered asset that failed to read, parse, or index), so `discovered == indexed + failed + skipped` holds; infrastructure failures (mount/discovery/ownership/cache) are reported in `failures` and counted in `analysis.scan_failures`, not in `stats.failed`. Symbolic links are not followed; each skipped link is counted in `stats.skipped_symlinks` and surfaced as a `discovery` diagnostic.
- Cross-package references are carried once by the top-level `forward`/`reverse` adjacency maps; inventory items do not repeat them. Each project inventory item retains its own compact analysis status, coverage, capabilities, graph counts, diagnostics, and opaque identities. Focused packages additionally appear under `focused` with their full typed analysis; a `--focus` pattern that matches nothing, or a focused package that fails to read or parse, is recorded as a `focus`-stage entry in `failures` rather than aborting the report.

Reports are **sparse**: empty and default-valued fields are omitted rather than emitted as `null`, `[]`, `false`, `""`, `"None"`, or `0`. A missing field therefore means the default value (no value, empty collection, `false`, or a zero count) — read it as absent, not as an error. `coverage` keeps its non-zero counters and always retains `bytes_total`, `exports_total`, and `exports_analyzed`; zero-valued counters are omitted. Sparseness applies to the typed report fields; a decoded property `value` (inside `exports[].properties` or a nested `@struct` block) is the exact serialized value and may be an explicit `null`, `""`, `[]`, or `"None"` when the asset stored one.

Pin `direction` and pin-type `container` render as plain strings (for example `"output"`, `"array"`); an out-of-range value renders as `{"unknown": <n>}`. Export byte placement (`object_flags`, `serial_offset`, `serial_size`, and the script-serialization range) is grouped under an optional `serialization` object emitted only in the `full` asset view; the focused `summary`/`logic`/`properties`/`references` views omit it.

Typed graph nodes and pins (PCG, RigVM, StateTree) carry distilled semantic fields plus their stable `index`; they do not repeat the raw tagged-property block. Read a node's full properties from the matching `exports[]` entry by `index`. StateTree task/condition `node_properties`/`instance_properties` and transition data remain on the typed node because they are distilled from nested arrays, not a whole export.

Counts describe the requested scope; they are not interchangeable. An indexed package is not necessarily semantically analyzed or complete.

## Focus selection

`--focus <PACKAGE_OR_GLOB>` selects packages by canonical path. A trailing asset/object suffix is stripped, matching is case-insensitive, `?` matches one non-separator character, `*` matches within a single path segment, and `**` matches across `/` separators (so `/Game/Blueprints/*` selects direct children and `/Game/Blueprints/**` selects the whole subtree).

## Output budgeting

`--max-output-bytes <N>` caps the rendered JSON at N UTF-8 bytes. Budgeting is a presentation concern: it never changes evidence. The skeleton — `schema_version`, `status`, `summary`, `coverage`, `capabilities`, `diagnostics`, `known_opaque`, and (for projects) `reachability`, `stats`, `analysis`, `layout`, `mounts`, `entry_points` — is always preserved. Heavy detail is elided in a fixed priority order: tagged-property values, then pins, then graph elements (nodes/states/edges/links), then the large top-level sections (`exports`, graphs, `inventory`, `focused`, adjacency), then large `reachability` package lists (keeping `configured_roots` and counts). Every large section, including the nested `reachability` lists, is first truncated to as many leading elements as the remaining budget allows before any section is dropped whole, so a tight budget still returns leading structural detail across sections; a truncated array keeps its leading entries followed by an `{"@elided": <count>}` marker, and a truncated object keeps its leading entries plus an `"@elided": <count>` key. A top-level `output` block records `truncated`, `budget_bytes`, `emitted_bytes`, and `elided` (each reduced section with its dropped-element count; section truncation is reported under `section_truncation`). A fully elided array or map is replaced by an `{"@elided": <count>}` marker. `output.truncated=true` means the report is size-reduced, not evidence-incomplete; re-query a narrower `--focus`/`--view` for the dropped detail.

## Evidence identities

Use package path plus adapter and graph/model identity as the namespace. Within it, use stable node/pin/state identifiers and explicit edges. Display names are labels, not identities.

K2/EdGraph edges have `kind=exec|data`. A gameplay path normally needs ordered exec edges plus the data edges/defaults that determine branch inputs, call parameters, spawn classes, or object targets. Intra-graph connectivity is carried once by the graph's `edges`; a pin's `linked_to` retains only cross-graph and unresolved connections (those a per-graph edge cannot express), so use `edges` for in-graph flow and `linked_to` only for links that leave the graph.

RigVM links store both source and target pin paths. Count each canonical model link once. StateTree transitions must retain source, target, trigger, conditions, and task ownership. PCG edges must retain source/target node and pin identities.

## Opaque and failure records

Every byte-backed unstructured region must include a capability/type, reason, and byte range. A capability-level opaque record may have no byte range when it describes several serialized regions. `known_opaque` preserves alignment but does not prove source logic. An `error` means the requested structure was not reliably decoded.

Never treat an empty diagnostics array alone as completeness. Check `status`, `coverage`, exact byte consumption, and `capabilities` together.

## Project graph

Project reports expose one inventory and bidirectional adjacency for all scanned mounts. `reachability.configured_roots` records config-derived package/class roots and whether they resolved to scanned packages. `reachability.reachable_runtime_packages` is computed from those roots, scanned references, and ownership closure; it is a runtime-reachability set, so it includes externally-referenced packages (engine, plugin, and `/Script/...` packages) that reachable assets depend on, not only project-owned packages. `reachability.ownership_closure_members` records World Partition external packages owned by their map. Level Instance and Packed Level Actor sub-level ownership is **not yet derived**: those sub-levels are reached only through soft references and are not currently closure members.

`reachability.unreachable_project_assets` and `reachability.isolated_project_assets` are scanned graph facts, not deletion proof. They still require review for primary asset rules, localization, runtime-generated names, soft loads outside scanned mounts, and failed or unsupported evidence.

Strict mode returns nonzero only for hard scan failures (mapped read/index/parse/mount/cache errors, or a `--focus` selection failure). Inherent partial or unsupported evidence — for example known-opaque compiled RigVM bytecode or an unsupported package version — keeps a truthful non-complete `status` but exits zero. `--allow-partial` downgrades a hard scan failure to a zero exit; it does not erase `status`, coverage gaps, or `failures`.
