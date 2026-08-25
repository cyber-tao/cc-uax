# CLAUDE.md

This file is the repository source of truth for engineering agents working on `cc-uax`.

## Project

`cc-uax` analyzes versioned, uncooked Unreal Engine 5 editor packages (`.uasset` and `.umap`) without loading Unreal Editor. It serves AI engineering tools that need evidence about serialized properties, Blueprint and plugin graph logic, asset references, gameplay structure, and project resource usage.

UE5.0–5.8 source is the serialization authority (UE5.7/5.8 share `FileVersionUE5 = 1018`; UE5.6 is 1017; UE5.1 is 1008; UE5.0 is 1000–1007). The parser targets versioned, uncooked UE5.0–5.8 editor packages (`FileVersionUE5` 1000–1018). `VERIFIED_FILE_VERSION_FLOOR` currently matches `SUPPORTED_FILE_VERSION_FLOOR` (UE5.0 / 1000), so accepted packages may be `status=complete` when their evidence is complete.

Both ends of that range are enforced in `PackageFileSummary::parse` as `OutOfScope`, not inferred later: below `ue5::INITIAL_VERSION` (UE4, UE3, cooked/unversioned) and above `ue5::HIGHEST`. UE itself stops reading a package whose file version it does not know — `PackageFileSummary.cpp` bails right after `FileVersionLicensee` when `IsFileVersionTooNew` — because every field past that point may have moved, so parsing one with the 5.8 layout would produce tables read from the wrong offsets. `cc-uax asset` therefore exits `1` with an error document for an out-of-scope package, while `cc-uax project` indexes it as `unsupported` evidence and exits `0`.

Development policy: this repository is in active development. Prefer the cleanest correct API and representation; do not retain obsolete 0.8 CLI/JSON compatibility unless a task explicitly requires it.

## Commands

```pwsh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --release --locked

cargo run -p cc-uax-cli -- asset <file.uasset> --view summary
cargo run -p cc-uax-cli -- asset <file.uasset> --view logic
cargo run -p cc-uax-cli -- project <project-or-content-dir>
```

Real-corpus acceptance is separate from ordinary workspace tests.

## Workspace layout

The root `Cargo.toml` is a virtual workspace. Keep these responsibilities separate:

- **`crates/cc-uax-core`** (library import `cc_uax_core`) — byte-bound package parsing, typed decoded values, logic graphs, diagnostics, coverage, and capability results.
- **`crates/cc-uax-project`** (library import `cc_uax_project`) — project/Content discovery, mounts, one-pass inventory, forward/reverse adjacency, World Partition ownership closure, and cache placement.
- **`crates/cc-uax-cli`** — the `cc-uax` binary, `asset`/`project` commands, focus selection/full-analysis attachment, process exit policy, and JSON rendering.
- **`website/`** — VitePress documentation site (not a Cargo member). GitHub Actions builds it via `.github/workflows/pages.yml` and publishes to `https://cyber-tao.github.io/cc-uax/`.

Dependency direction is `cli -> project -> core`, with `cli -> core` allowed. Core must never depend on project or CLI concerns.

The full agent package lives under `skills/cc-uax/`. Installation and release packaging must copy the whole directory, including `agents/`, `references/`, and future supporting assets.

## Core public model

The core interface is typed. JSON is a CLI renderer concern.

Important public types:

- `PackageView<'a>` binds the parsed package to the same source byte slice for its entire lifetime. Do not reintroduce an API that parses bytes A and later accepts unrelated bytes B for decoding.
- `AssetAnalysis` is the top-level single-asset result.
- `DecodedValue` represents decoded property/native values without constructing `serde_json::Value` inside the parser.
- `LogicGraph`, `GraphNode`, and `GraphEdge` preserve graph ownership and distinguish execution from data flow.
- `ParseCoverage` records requested, decoded, opaque, unsupported, and failed evidence.

Every rendered report has:

- `schema_version` (declared by `ASSET_ANALYSIS_SCHEMA_VERSION` in [crates/cc-uax-core/src/model.rs](crates/cc-uax-core/src/model.rs) and `PROJECT_REPORT_SCHEMA_VERSION` in [crates/cc-uax-cli/src/lib.rs](crates/cc-uax-cli/src/lib.rs); [report-contract.md](skills/cc-uax/references/report-contract.md) is the field-level explanation of those numbers). A third version, `PROJECT_INDEX_SCHEMA_VERSION` in [crates/cc-uax-project/src/lib.rs](crates/cc-uax-project/src/lib.rs), stamps the scanned index and gates cache reuse: bump it whenever a cached `AssetAnalysisSummary` changes meaning, or a warm scan will replay summaries built under the old one;
- `status`: `complete`, `partial`, or `unsupported`;
- machine-readable `coverage`;
- capability evidence and limitations;
- structured diagnostics.

`known_opaque` is an explicit limitation, not a successful decode. If an opaque region blocks a requested capability, the result cannot be `complete`.

## Parsing pipeline

The byte pipeline remains strictly ordered and bounded:

1. `Reader` reads little-endian primitives, `FString`, `FName`, `FGuid`, and bounded byte ranges.
2. `PackageFileSummary` validates the package tag and reads file/custom versions plus all table locations.
3. Name, import, export, soft-path, and reference tables are decoded within their declared ranges.
4. Each export uses a single bounded cursor over its `serial_offset/serial_size` window. Track the property terminator, UObject tail, pin end, and any remaining bytes.
5. Tagged-property and native-struct decoders return typed values. A decoder must consume exactly its declared payload or return a structured error/opaque result.
6. Graph adapters convert decoded exports into graph-specific typed models.
7. Coverage and capability aggregation determines the final status.

Never guess a cursor position after a failed parse. Never parse beyond an export or property value window. Counts, indices, recursion depth, and byte arithmetic must be checked before allocation or seeking.

## Version and native-struct policy

[crates/cc-uax-core/src/version.rs](crates/cc-uax-core/src/version.rs) owns UE file thresholds, custom-version GUIDs, and named thresholds. Call sites must not contain unexplained version numbers. Byte-width constants belong next to the reader that consumes them ([crates/cc-uax-core/src/reader.rs](crates/cc-uax-core/src/reader.rs) has `RAW_NAME_BYTES` and `FSTRING_LENGTH_BYTES`), and every header table goes through `seek_to_table` so a new one cannot come with a weaker guard than the rest.

`SerializationPolicy` carries package custom-version decisions into native decoders. A missing custom-version GUID is `-1` and normally selects the legacy layout.

Important version-gated formats include:

- `FVector`/`FRotator`/`FQuat`/`FBox`/`FBox2D`/`FMatrix`: `float` below `LARGE_WORLD_COORDINATES` (1004), `double` from 1004. `FTransform` is *not* in this list: alone among the core math types its USTRUCT is not `immutable` and `TTransformStructOpsTypeTraits` leaves `WithSerializer` commented out in every 5.0–5.8 branch, so UE writes a tagged Rotation/Translation/Scale3D block. `FTransform3f`/`FTransform3d` are immutable and keep the binary layout;
- Legacy `FPropertyTag` `BoolVal` and `HasPropertyGuid` are uint8 (PropertyTag.h), not 4-byte UBOOL, for `FileVersionUE5` < 1012;
- Legacy container tags below `PROPERTY_TAG_COMPLETE_TYPE_NAME` (1012) record only the element's *property* type. `FArrayProperty::SerializeItem` compensates by writing a full inner `FPropertyTag` between the element count and the elements, so struct arrays are decodable and that tag must be read; `FSetProperty` and `FMapProperty` write none, so a set element or map key/value struct genuinely needs UE's reflection registry and stays opaque;
- `UMetaData::Serialize` runs `Super::Serialize` first, so the metadata maps begin after `UObject`'s `PossiblySerializeObjectGuid`, and the root map exists only from `FEditorObjectVersion::RootMetaDataSupport`;
- `FInstancedStruct`: legacy optional editor header/version versus modern payload;
- `FInstancedPropertyBag`: `FPropertyBagCustomVersion` desc layout, verified through UE5.8; version 4 (`PropertyFlags`) appends a `uint64` per desc and version 5 (`KeyTypes`) appends a map key type after it, so 5 is the highest known layout;
- `FStateTreeInstanceData`: legacy tagged instance data versus custom instance storage;
- `FPCGPoint`: legacy tagged properties versus structured field-mask serialization;
- Niagara, Sequencer, and EdGraph pin fields controlled by their owning custom versions;
- PropertyTag extensions: `OverridableInformation` (0x02, `FileVersionUE5` ≥ 1011 / UE5.4+) and `HasExternalsObjects` (0x04, UE5.8+);
- `FText` Base history `DevNotes` when FortniteMain ≥ 260 and the archive is not FilterEditorOnly (UE5.8 editor packages).

The same `FileVersionUE5` does not guarantee the same layout: UE5.7 and UE5.8 share `1018` yet diverge (for example `FObjectImport::PackageName` gating and the `FInstancedPropertyBag` desc fields), so custom versions and, where needed, the engine version must gate those formats.

Only classify a struct as native when UE5.0–5.8 source actually provides binary/structured custom serialization. Two things make that true: `WithSerializer = true` in the struct's `TStructOpsTypeTraits`, or `immutable` on its `USTRUCT` (which sets `STRUCT_Immutable` and routes `UScriptStruct::SerializeItem` through `SerializeBin`). A `WithSerializer` that is commented out or returns `false`, on a struct that is not immutable, means tagged properties — and inventing a layout for a struct the engine does not declare at all is never acceptable.

Known native formats require exact consumption. Unknown registry-dependent or compiled payloads must retain type, byte range, size, reason, and preview as `known_opaque`; do not silently discard a tail. A project report aggregates those regions per (kind, type, reason) with region and byte totals rather than listing each one — the full per-region ranges stay in the `asset` and `--focus` output — so opaque bytes remain attributable at both levels.

Opaque evidence must say *why*, not just *that*. An export tail after a property block that closed where it should is the class's own serializer data and is expected; one after a block that never closed is unattributed and is the signal that a decoder is missing something. `coverage` separates the two (`class_payload_bytes` versus `unattributed_tail_bytes`) because on a real project the first runs to gigabytes of mesh and lightmap data and would otherwise bury the second.

## Graph adapters

Graph identity is part of correctness:

- **K2/EdGraph** — group nodes by their owning graph; preserve exec/data edges, member references, defaults, pin types, and `UserDefinedPins/FUserPinInfo`. Never join nodes solely by display name.
- **RigVM/ControlRig** — use the RigVM model as the authoritative graph and decode `URigVMLink` source/target paths. Do not double-count editor mirror graphs. Compiled VM bytecode and compressed hierarchy remain named opaque capabilities until structured support exists.
- **StateTree** — expose states, tasks, conditions, and transitions, including the fields that are easy to miss because they live elsewhere: `SingleTask` and `Considerations` on the state, and `Evaluators`, `GlobalTasks` and `RootParameterPropertyBag` on the editor data. Retain PropertyBag gaps as PCG does.
- **PCG** — expose nodes, pins, and edges; retain explicit PropertyBag gaps.
- **Niagara** — normalize supported editor graphs through the EdGraph model and retain unsupported VM/GPU payloads as the `niagara_compiled` capability limitation.
- **Compiled bytecode** — `blueprint_bytecode`, `niagara_compiled`, `rig_vm_bytecode` and `rig_hierarchy` are all the same shape of gap: the source-level graph decodes, the compiled form does not, and each must be a named capability rather than an anonymous export tail.

Stable node identity must include graph ownership and serialized identity. Edges must not cross graph boundaries unless the serialized format contains an explicit cross-graph reference.

## Project analysis

`cc-uax-project` discovers either a project directory/`.uproject` or a `Content` directory and scans mapped assets once. Passing an explicit `.uproject` file selects that file even when sibling platform `.uproject` files share the same Content tree; a directory or Content path still errors if more than one `.uproject` is present.

The default mount set is `/Game` plus every plugin content root under `Plugins/`, mounted as `/{.uplugin base name}/` the way `FPluginManager` does. The descriptor name is authoritative, not the directory name — they differ often enough in real projects that guessing produces package paths the project does not contain. An explicit `--mount` augments the discovered set and replaces only a root it names.

Configured roots come from `GameMapsSettings` *and* the `ProjectPackagingSettings` cook lists (`+MapsToCook`, `+DirectoriesToAlwaysCook`), because `GameDefaultMap` is frequently a developer map while the cook list is what a build ships. A `Config/<Platform>/` directory counts as a platform override only when its config actually changes an entry point; most hold packaging or SDK settings only.

The index contains:

- asset inventory and canonical package paths, each with its `FileVersionUE5`;
- forward and reverse reference adjacency;
- configured-root reachability and resource classification summaries, with each root marked `indexed`, `referenced_only`, or `unresolved` — a resolved name alone never proved the root exists in the scan;
- read/index/parse failures with paths and stages;
- World Partition `ExternalActors`/`ExternalObjects` ownership;
- external-package ownership closure: World Partition external packages owned by their map, plus Level Instance / Packed Level Actor sub-levels whose `WorldAsset` / `PackedWorldAsset` was decoded on those actor classes;
- per-asset logic, capability, and coverage summaries needed by the requested focus.

Strict mode is the default. Any mapped read/index/parse failure returns the partial index as a structured error and causes a non-zero CLI exit. Inherent partial or unsupported evidence (for example known-opaque compiled RigVM bytecode, or a package version outside the supported range) keeps a truthful non-complete `status` but does not by itself fail the process. `--allow-partial` downgrades a hard scan failure to a zero exit; it must not change report truth.

A package rejection must stay classified. `PackageRejection::OutOfScope` covers readable formats this tool deliberately does not target (UE4 `FileVersionUE5` = 0, cooked/unversioned, UE3, big-endian, package-level compression); the scanner indexes those as `unsupported` evidence with a reason, counts them in `analysis.unsupported_assets` and `reachability.unsupported_packages`, and keeps them out of `failures`. `PackageRejection::Malformed` covers bytes that are not a readable package and stays a `Parse` failure. Collapsing the two back into one error is the defect this split exists to prevent.

Project cache data defaults to the operating system cache directory. Never create a cache inside the analyzed project by default. Fresh cache entries may reuse validated references and compact per-asset analysis summaries for unchanged packages. `--cache-file` explicitly selects a file and `--no-cache` disables caching.

Cache validity is `mtime` and `size` plus the crate version *and* the analysis/index schema versions. The crate version alone is not enough: during development it stays put across decoder fixes, so a warm scan would replay pre-fix summaries for every unchanged file. Any change to what a cached summary means must bump a schema version, and `--no-cache` is the right default while iterating on a decoder.

## CLI contract

The supported command shape is:

```text
cc-uax asset <FILE> [--view summary|logic|properties|references|full]
cc-uax project <PROJECT_OR_CONTENT_DIR>
  [--focus <PACKAGE_OR_GLOB>]...
  [--mount <PACKAGE_PREFIX=RELATIVE_DIR>]...
  [--allow-partial]
  [--cache-file <FILE> | --no-cache]

Global options (apply to both commands):
  [--compact]                   # Emit compact JSON (no pretty-printing)
  [--max-output-bytes <BYTES>]  # Cap rendered JSON at N UTF-8 bytes; elides heavy detail and adds an `output` truncation block
  [-o, --output <FILE>]         # Write JSON report to FILE instead of stdout
```

`--view` defaults to `full` for the `asset` command. `--focus` and `--mount` are repeatable.

Keep the command surface centered on the explicit `asset` and `project` workflows; do not add alternate content-selection APIs.

The CLI renders typed reports, writes output, and maps outcomes to exit codes: `0` for any produced report including `partial` and `unsupported`, `2` for a `project` hard scan failure (mapped read/index/parse, in-scan mount/cache, or a `--focus` miss), and `1` when no report could be produced at all — which emits a `{"schema_version", "status": "error", "message"}` document instead. `"error"` is that fatal document only and is never a report `status`. An out-of-scope package is one of the `1` cases for `asset`: there is no report to produce, and `project` is where it becomes `unsupported` inventory evidence. [report-contract.md](skills/cc-uax/references/report-contract.md) is the field-level and exit-code contract; keep it in sync when the rendered shape changes, and do not restate its field lists in the README or SKILL.md.

The CLI must not drive parser decisions or infer graph edges from rendered text.

## Diagnostics, coverage, and capabilities

Diagnostics use stable machine-readable fields: `severity`, `code`, `path`, `message`, an optional byte `offset`, and optional typed `details`.

Coverage is evidence accounting, not a marketing counter. At minimum it must distinguish:

- requested evidence;
- structured decoded evidence;
- classified opaque evidence;
- unsupported capabilities;
- errors/failures;
- unclassified bytes, which are always a defect.

An empty diagnostics array alone does not prove completeness. Status is computed from diagnostics, coverage, capability requirements, and project scan failures.

## Validation

Real-corpus acceptance is separate from ordinary workspace tests, and is run by `tools/corpus-acceptance.ps1`:

```pwsh
./tools/corpus-acceptance.ps1 -Project <PROJECT_OR_CONTENT_DIR>[,<MORE>...] [-UpdateBaseline]
```

It is not a workspace member. Corpus paths are arguments, and every report and the baseline go to `-OutputDirectory` (default: a directory under the OS temp dir), so nothing corpus-specific enters the repository. Do not commit external assets, generated corpus reports, caches, absolute local paths, or secrets.

The harness checks invariants that hold for any corpus — `unclassified_bytes == 0`, `discovered == indexed + failed + skipped`, asset-status accounting, grouped opaque regions and bytes reconciling with `coverage` — then compares against a recorded baseline in one direction only: evidence may improve, never degrade. Run it after any decoder, classification, or report-shape change; a change that silently reclassifies packages or loses evidence shows up there and not in `cargo test`. Record the FileVersionUE5 distribution it reports, since it is the only statement of which version gates real assets actually exercised.

## Conventions

- Preserve little-endian behavior and checked window arithmetic.
- Prefer existing dependencies; core remains small and filesystem-independent.
- Add hand-built byte-vector regression tests for parser changes and integration tests for project/CLI behavior.
- Test version gates at threshold-1, threshold, missing GUID, and truncation.
- Keep English identifiers, log messages, diagnostic codes, API fields, and commit messages.
- Do not hide parse errors, silently count them as skipped, or convert partial evidence into success.
- Do not commit external assets, generated corpus reports, caches, absolute local paths, or secrets.
