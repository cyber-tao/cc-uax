# Validation contract

This document defines the external evidence and acceptance gates for `cc-uax` 0.9. It deliberately separates a **required target** from a **recorded passing run**.

Nothing in this file certifies the current checkout by itself. A release is accepted only when the Rust validation harness produces a fresh report for the exact commit and every required gate below passes.

## External inputs

UE5.7 source is the implementation authority used to verify serialization gates and field order; the automated harness does not require or perform a meaningless source-directory existence check.

The automated external-corpus input is the StackOBot editor `Content` directory, supplied explicitly with `--content`:

```bash
cargo run -p cc-uax-stackobot-validation --locked -- --content "<STACKOBOT_CONTENT>"
```

These inputs are not repository content. Do not commit:

- Unreal Engine source;
- StackOBot `.uasset`, `.umap`, or sidecar files;
- machine-specific absolute paths;
- generated full reports or caches.

`validation/stackobot/` stores only the Rust harness, stable expectations, and `manifest.json`, whose package paths are relative to the external `Content` root.

## Corpus inventory

The StackOBot manifest covers **1,961** versioned, uncooked UE5 editor packages. This number identifies the intended external corpus; it is not a claim that a particular checkout currently passes it.

The harness must reject:

- a missing manifest entry;
- an unexpected extra package when exact-corpus mode is enabled;
- duplicate canonical package paths;
- an absolute path in the manifest;
- a package path that escapes the supplied Content root.

## Required gates

### Package and coverage integrity

| Gate | Required result |
|---|---|
| Discovery | `discovered = 1961` |
| Indexing | `indexed = 1961` |
| Failures | `failed = 0` and `skipped = 0` |
| Unclassified bytes | zero unclassified export/property/pin tails |
| Opaque evidence | every allowed opaque region has a stable type/reason entry in the opaque manifest |
| Status | every requested required capability is `complete`; an allowed non-required opaque capability remains explicitly reported |

An empty diagnostics list is not sufficient. Coverage, capability state, failures, and byte classification are evaluated independently.

### Blueprint graph correctness

- `LevelSaveObject` contains 13 legacy graph nodes. The harness must recover non-zero pin/edge evidence from that legacy layout.
- `PlayerSaveObject` contains a legitimate empty comment node. Zero pins on this fixture are expected and must not be treated as a parser failure.
- `GI_StackOBot` contains 10 distinct graphs. Nodes and edges must remain scoped to their owning graph; duplicate display names must not create cross-graph execution chains.
- K2 assertions use stable graph/node/pin identity and serialized exec/data edges. Rendered-text regular expressions are not accepted as graph evidence.
- `UserDefinedPins/FUserPinInfo` must be consumed according to its Framework custom version, with no unexplained trailing bytes.

### RigVM and ControlRig

- The two ControlRig fixtures must expose all **82** serialized RigVM source/target link pairs.
- RigVM model links are counted once; mirrored editor graphs must not duplicate them.
- Compiled RigVM bytecode and compressed RigHierarchy remain explicit `known_opaque` capabilities unless structured support is added. They may not be described as decoded source logic.

### PCG and StateTree

| Fixture | Required semantic evidence |
|---|---|
| `PCG_PickupSpline` | 7 semantic nodes (5 `Nodes` entries + default input/output), 6 exact `UPCGNode` exports, 1 `UPCGSpawnActorNode` subclass, 135 pins, 4 edges |
| `PCG_UnderRock` | 69 nodes, 713 pins, 74 edges; PropertyBag gaps listed explicitly |
| `STree_Bug` | 13 states plus task and transition structure |

PCG edge counts come from serialized node/pin identity, not display-text matching. StateTree counts must distinguish states, tasks, conditions, and transitions.

### Gameplay evidence

The project report must establish graph-local execution and relevant data dependencies for:

- main-menu startup and game entry;
- player spawning and possession;
- movement and jetpack behavior;
- grab/carry interactions;
- coins and portal progression;
- AI behavior;
- UI and save/load behavior.

Each conclusion cites the package, owning graph, stable node identity, and edge/pin evidence used. If any required link cannot be established, the gameplay capability is `partial`; the harness must not fill the gap with a plausible narrative.

### Project and World Partition

- The main-map closure includes **296** `ExternalActors` and **28** `ExternalObjects`.
- External package ownership, LevelInstance/PackedLevelActor relationships, forward references, and reverse references are derived from one shared project index.
- A validation run scans the external project once. Focused assertions reuse that index instead of rescanning per fixture.
- Strict mode returns non-zero for every mapped read/index/parse failure. A separate `--allow-partial` test proves that process acceptance changes while report truth remains `partial`.
- Default cache placement is outside the external project; explicit cache and no-cache modes are covered separately.

## Version and corruption regressions

Hand-built tests remain mandatory for:

- UE5 file versions 1009 and 1010 around the property/UObject-tail/pin boundary;
- `UserDefinedPins` before and at relevant Framework thresholds;
- `FInstancedStruct`, `FStateTreeInstanceData`, and `FPCGPoint` at threshold-1, threshold, missing GUID, and truncated payload;
- negative sizes/counts, `i32::MIN` package indices, invalid names/references, out-of-range soft paths, excessive type recursion, and invalid pin references;
- graph ownership uniqueness, no accidental cross-graph edges, and retained pin defaults/data edges;
- strict and partial project scan behavior plus cache placement.

## Engineering gates

Run these before the external corpus harness:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --release --locked
```

CI also checks Rust 1.88 compatibility and the supported Windows/Unix installer and CLI smoke paths.

## Recording a passing run

A generated validation summary used for release evidence records:

- git commit and `cc-uax --version`;
- UE source revision/identifier;
- corpus manifest digest and observed package count;
- command-line options, cache mode, and strict/partial mode;
- headline package, coverage, capability, graph, PCG, StateTree, RigVM, gameplay, and World Partition results;
- all failures, diagnostics, unsupported capabilities, and opaque-manifest deltas.

Machine paths must be redacted or represented by the runtime input names above. A previous report does not certify a later commit.

README files may say that UE5.7 source and real editor assets are used for validation and may link here. They must not copy a target count into a “validated successfully” badge or sentence unless a current, reproducible release report actually proves it.
