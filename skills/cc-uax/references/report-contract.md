# cc-uax report contract

Read this reference when interpreting `asset` or `project` JSON.

## Completion fields

- `schema_version`: Version of the report contract, independent from the CLI version.
- `status`: `complete`, `partial`, or `unsupported` for the requested analysis scope.
- Asset reports expose `coverage` counters for exports, properties, EdGraph, RigVM, PCG, StateTree, opaque regions, and diagnostics within the requested view.
- Project reports use schema version `2` and expose sanitized `layout`/`mounts`, filesystem/index accounting in `stats` (`discovered`, `indexed`, `failed`, `skipped`), generated runtime/resource `reachability`, and semantic accounting in aggregate `analysis` (`assets`, `complete_assets`, `partial_assets`, `unsupported_assets`, and summed `coverage`).
- Each project inventory item retains its own compact analysis status, coverage, capabilities, graph counts, diagnostics, and opaque identities. Focused packages additionally appear under `focused` with their full typed analysis.

Counts describe the requested scope; they are not interchangeable. An indexed package is not necessarily semantically analyzed or complete.

## Evidence identities

Use package path plus adapter and graph/model identity as the namespace. Within it, use stable node/pin/state identifiers and explicit edges. Display names are labels, not identities.

K2/EdGraph edges have `kind=exec|data`. A gameplay path normally needs ordered exec edges plus the data edges/defaults that determine branch inputs, call parameters, spawn classes, or object targets.

RigVM links store both source and target pin paths. Count each canonical model link once. StateTree transitions must retain source, target, trigger, conditions, and task ownership. PCG edges must retain source/target node and pin identities.

## Opaque and failure records

Every byte-backed unstructured region must include a capability/type, reason, and byte range. A capability-level opaque record may have no byte range when it describes several serialized regions. `known_opaque` preserves alignment but does not prove source logic. An `error` means the requested structure was not reliably decoded.

Never treat an empty diagnostics array alone as completeness. Check `status`, `coverage`, exact byte consumption, and `capabilities` together.

## Project graph

Project reports expose one inventory and bidirectional adjacency for all scanned mounts. `reachability.configured_roots` records config-derived package/class roots and whether they resolved to scanned packages. `reachability.reachable_runtime_packages` is computed from those roots, scanned references, and ownership closure; `reachability.ownership_closure_members` records World Partition external packages and similar closure members. Level Instances, Packed Level Actors, and World Partition external packages are closure members, not independent roots.

`reachability.unreachable_project_assets` and `reachability.isolated_project_assets` are scanned graph facts, not deletion proof. They still require review for primary asset rules, localization, runtime-generated names, soft loads outside scanned mounts, and failed or unsupported evidence.

Strict mode returns nonzero for mapped read/parse failures and for a requested project result that remains partial or unsupported. `--allow-partial` changes process acceptance only; it does not erase `status`, coverage gaps, or `failures`.
