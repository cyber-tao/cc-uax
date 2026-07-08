# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

`cc-uax` is a Rust CLI that parses Unreal Engine 5 `.uasset`/`.umap` package files and emits structured JSON — package header, tagged properties, the Blueprint node-and-pin graph, and forward/reverse asset references. Scope: **versioned, uncooked editor assets** for UE5 (`FileVersionUE5 >= 1000`). The header, tables, reference graph and tagged-property decoding support both legacy UE5 property tags and the newer complete-type-name tag layout (`FileVersionUE5 >= 1012`). Cooked/unversioned packages and UE4 legacy formats are explicitly out of scope.

## Commands

```pwsh
cargo build --release                    # binary at target/release/cc-uax
cargo run --release -- <file.uasset>     # dev run
cargo test --workspace --locked          # full workspace suite
cargo test -p cc-uax-core tests::reader::fstring_ansi
cargo fmt
cargo clippy --workspace --all-targets --all-features --locked
.\scripts\validate-real-assets.ps1 -ExpectedCount 2096 # real UE asset validation (Windows)
# rebuild from source + refresh the Claude Code / Codex skill locally (dev only)
./dev-install.sh          # Linux / macOS / Git Bash
.\dev-install.ps1         # Windows PowerShell
```

No benchmarks, no separate lint config. CI is whatever runs these locally.

## Workspace layout (important)

`Cargo.toml` defines a CLI package at the repository root and a parser crate under
`crates/cc-uax-core`:

- **core lib** (`crates/cc-uax-core/src/lib.rs`, crate name `cc_uax`): the parser and report model. It re-exports the stable surface (`Package`, `DecodeReport`, diagnostics, `OutputSections`, `MountMap`, and reference helpers). Internal parser modules are crate-private and are not external API.
- **CLI bin** (`src/main.rs` + `src/cli/`): the `cc-uax` binary. `main.rs` declares `mod cli`, whose submodules are `args` / `cache` / `reverse_refs`; the binary pulls in `clap` and `rusqlite`.

`src/cli/cache.rs` is **only** included by the binary, never by the core lib. Keep the SQLite reverse-scan cache out of the parser crate; `rusqlite` is a bin-side concern. When changing the parser, do not reach into `cli/cache.rs` or add parser deps through it.

## Parsing pipeline

The central entry point is `Package::parse` in [package.rs](crates/cc-uax-core/src/package.rs). The flow is strictly ordered because each stage's offsets come from the previous one:

1. **`Reader`** ([reader.rs](crates/cc-uax-core/src/reader.rs)) — little-endian byte-stream primitives (`u8/u16/u32/i32/u64/f32/f64`, `FString`, `FName`, `Guid`). All file I/O goes through this; the format is LE-only.
2. **`PackageFileSummary::parse`** ([summary.rs](crates/cc-uax-core/src/summary.rs)) — package header. Validates the `PACKAGE_FILE_TAG` (`0x9E2A83C1`) magic and detects byte order via `PACKAGE_FILE_TAG_SWAPPED`; reads engine/file versions, `CustomVersion`s, and the offset+count of every downstream table. `is_unversioned()` and `filter_editor_only()` gate later behavior.
3. **`NameMap`** ([name.rs](crates/cc-uax-core/src/name.rs)) — the name table. `resolve` returns a `String` (`<invalid_name#N>` on a bad index) with number-suffix semantics (e.g. `Foo_3`).
4. **Import / Export tables** ([object.rs](crates/cc-uax-core/src/object.rs)) — `PackageIndex` encodes the table: **positive = export, negative = import**. `ObjectExport` carries `serial_offset` / `serial_size`, which delimit each object's serialized property region.
5. **Decode report** ([decode/](crates/cc-uax-core/src/decode/)) — per-export decode orchestration. It builds serial windows, calls property parsing, consumes known UObject / metadata tails, retries pin candidates, distills graph members, and accumulates structured diagnostics.
6. **Per-export property region** ([property/](crates/cc-uax-core/src/property/)) — for each export, property parsing seeks to the `ScriptSerialization` window and recursively decodes legacy UE5 property tags and complete-type-name tags (`FPropertyTag` + `FPropertyTypeName` when present).
7. **Per-node pin region** ([pin.rs](crates/cc-uax-core/src/pin.rs)) — for graph-node exports, `parse_node_pins_report` decodes the pin array that follows the property window (`ScriptSerializationEndOffset` → `serial_size`), yielding pins, pin types, defaults, and `LinkedTo` edges. Decode report member distillation lifts `MemberReference` into `member` / `member_from`.
8. **Output serializers** ([output/](crates/cc-uax-core/src/output/)) — serialize the completed `DecodeReport` to JSON; output code must not drive parsing decisions.

The per-export `serial_offset`/`serial_size` windowing is what guarantees byte alignment across objects — never parse properties outside their window, and if you add a value decoder, it must consume exactly its bytes or fall back to hex preview (see below).

## Diagnostics

Diagnostics are structured values from [diagnostic.rs](crates/cc-uax-core/src/diagnostic.rs): `severity`, `code`, `path`, `message`, optional byte `offset`, and optional JSON `context`. Byte previews use `ByteRangePreview { start, end, size, preview }`. Do not add ad-hoc diagnostic strings in output serializers; parsing and decode layers should create `Diagnostic` values and `output/` should only serialize them.

## Version gating

[version.rs](crates/cc-uax-core/src/version.rs) holds the UE5 `CORE_UOBJECT` file-version constants (e.g. `INITIAL_VERSION = 1000`, `SCRIPT_SERIALIZATION_OFFSET = 1010`, `PROPERTY_TAG_COMPLETE_TYPE_NAME = 1012`) and the UE4 legacy ladder. Behavior branches on `FileVersionUE5` against these constants. When adding support keyed to a UE version, add the constant here and gate on it — do not hardcode magic version numbers at call sites. Plugin/module custom-version GUIDs and their thresholds also live in `version.rs::custom` (e.g. `NIAGARA_OBJECT_VERSION` + `NIAGARA_VARIABLES_USE_TYPE_DEF_REGISTRY`).

## Property decoding ([property/](crates/cc-uax-core/src/property/))

- `TypeName` — UE5.7 `FPropertyTypeName` (nested type name with parameters).
- `ParseCtx` — carries name/object resolution + the soft-object path list, plus the package's `FNiagaraCustomVersion` (`niagara_version`, gating the Niagara decoders) and `FFortniteMainBranchObjectVersion` (`fortnite_main_version`, gating the MovieScene channel `bShowCurve` tail).
- `parse_value` dispatches into `parse_collection` / `parse_map` / `parse_element` / `parse_struct` / `parse_native_struct` / `parse_soft_object` / `parse_text`, plus `OptionalProperty` (a 4-byte presence flag followed by the inner value when set). `LazyObjectProperty` is a 16-byte `FUniqueObjectGuid`, not an object index. Set/Map values start with `NumToRemove` followed by that many serialized keys (delta saves) — they are read and discarded before the live elements.
- `parse_native_struct` (in [property/native/](crates/cc-uax-core/src/property/native/)) dispatches by struct category (math / scalar / material / sequencer / graph-pin / gameplay / mesh-cloth / niagara) and decodes the common math / curve / material / Sequencer structs plus `GameplayTagContainer`, `Spline` (the `FSpline` int8 impl tag; non-empty payloads still hex), `GameplayEffectVersion`, and the Niagara core variable types (`NiagaraVariable` / `…Base` / `…WithOffset`; the nested `FNiagaraTypeDefinition` is itself tagged properties). Niagara decoders gate on `version::custom::NIAGARA_VARIABLES_USE_TYPE_DEF_REGISTRY` and fall back to hex below it. **Only add a struct here if UE5.7 really serializes it natively** (`immutable` core structs or `WithSerializer`/`WithStructuredSerializer` traits whose `Serialize` returns `true`): `FrameRate` and `Vector_NetQuantize*` look native but are tagged-property payloads in 5.7 and must stay out of this list.
- `is_tagged_fallback_struct` lists structs that declare `WithSerializer` but whose `Serialize` returns `false` (payload is still tagged properties) — e.g. `AlphaBlend`, `FloatCurve` / `TransformCurve` / `VectorCurve`, `GameplayEffectModifierMagnitude`, `LandscapeLayerComponentData`.
- **Hex fallback**: types with custom binary serialization that are not yet structured (a few specialized non-core Niagara VM/GPU and mesh/cloth structs) are emitted as a hex preview capped by `PREVIEW_MAX`, preserving `type` and `size`. The hex path exists specifically to keep byte alignment intact — any new unknown struct should go through `to_hex` rather than guessing fields.

## Blueprint pin decoding ([pin.rs](crates/cc-uax-core/src/pin.rs))

`UEdGraphNode` serializes its pins *after* the tagged-property window, so `pin.rs` runs once the property decoder has consumed the `ScriptSerialization` region. The byte layout mirrors UE5.7 `UEdGraphPin::Serialize` / `SerializePinArray` / `FEdGraphPinType::Serialize`:

- A 4-byte `PossiblySerializeObjectGuid` presence flag (the `UObject::Serialize` tail) sits between the property-window end and the pin array — consume it before reading the pin count.
- Each owned pin: id, name, optional `FText` friendly name, `SourceIndex`, tooltip, direction, `FEdGraphPinType`, default value/object/text, then `LinkedTo` / `SubPins` (pin references = node `PackageIndex` + pin `Guid`) and `ParentPin`.
- `EditablePinBase`-derived nodes (`K2Node_Event`, `K2Node_FunctionEntry`) append a `UserDefinedPins` array after the pins; the parser accepts trailing bytes once the count-prefixed pin array parses cleanly (`pos <= end`).
- Field presence is gated on custom versions read from the summary (`EdGraphPinSourceIndex`, `PinTypeIncludesUObjectWrapperFlag`, `SerializeFloatPinDefaultValuesAsSinglePrecision`) — GUIDs + thresholds live in `version.rs::custom`. **UE bools serialize as 4 bytes**, so use `read_bool32`.
- The decode layer only attempts pin parsing for graph-node classes (`is_graph_node_class`: `K2Node*` / `EdGraphNode*` / `NiagaraNode*` / `NiagaraOverviewNode` / `*GraphNode*`; the `*Binding*` exclusion for helpers such as `AnimGraphNodeBinding_Base` only applies to the fuzzy `*GraphNode*` match). The output layer resolves each `LinkedTo` reference to a readable `{ node, pin }` via a cross-node `(node_index, pin_guid) → name` map.

## Reference analysis

- **Forward references** (`collect_package_references` in [references.rs](crates/cc-uax-core/src/references.rs)): reads the import table and partitions external packages into `assets` vs `scripts` by the `/Script/` prefix (`SCRIPT_PATH_PREFIX`). The header's `SoftPackageReferences` table (one FName package per entry) is parsed too and emitted as `soft`. Output keys: `assets`, `scripts`, `soft`.
- **Reverse references** (CLI only, [src/cli/reverse_refs.rs](src/cli/reverse_refs.rs)): `--scan-dir <DIR>` walks the directory (`collect_asset_files`), maps disk paths to package paths via `--mount` using `MountMap` (default `/Game`; explicit mappings such as `/Game=Content,/Plugin=Plugins/Plugin/Content,/Engine=Engine/Content` are supported), parses every asset (imports + soft references, via `referenced_packages_from_bytes`), and computes `referenced_by`.
- **Cache** (`RefCache` in [src/cli/cache.rs](src/cli/cache.rs)): the reverse scan writes `.cc-uax-cache.sqlite` at the scan-dir root, keyed by file path + mtime + size; unparseable files are cached as negative results (`parse_ok = false`) so they are not re-read every scan. Bump `CACHE_SCHEMA_VERSION` whenever the reference-extraction logic changes — existing caches auto-invalidate on schema mismatch. `--no-cache` disables it.

## CLI surface

`Args` (clap derive) in [src/cli/args.rs](src/cli/args.rs). `OutputSections` (in [output/sections.rs](crates/cc-uax-core/src/output/sections.rs)) is the set of section flags — `summary` / `imports` / `names` / `references` / `exports` / `pins` / `properties` / `layout` — and the **only** content selector:

- `--sections` / `-S` (comma-separated) selects sections; it accepts section keys (with aliases `references`≡`refs`, `exports`≡`identity`, `properties`≡`props`) and the multi-section presets `logic` / `debug` / `dump` / `all` (`OutputSections::parse`). Default (no flag) is `dump`.
- There are no separate `-s` / `-P` / `-r` / `-n` flags — every content choice goes through `-S` (e.g. `-S summary`, `-S dump,names`).
- `references` (`-S refs`) + `--scan-dir` drives the reverse-reference scan, gated on the resolved `references` section; `--scan-dir` without it is a hard error.
- `--compact` / `--output <FILE>` shape the serialized text; `--mount` / `--no-cache` feed the reverse scan.

## Conventions

- **Endianness**: LE everywhere, via `byteorder`. Never use native/host byte order.
- **Minimal deps in the parser**: `byteorder` / `serde` / `serde_json` / `anyhow`. `clap` and `rusqlite` are bin-only. Do not add a new dependency to the lib without explicit reason — a from-scratch parser is a project goal, not an accident.
- **Testing**: core parser tests live under [crates/cc-uax-core/src/tests/](crates/cc-uax-core/src/tests/) — split into `model` / `package` / `pin` / `property` / `reader`, with shared byte-vector builders in `common`. Root [tests/](tests/) is reserved for CLI black-box coverage. They exercise `Reader` primitives, `NameMap` resolution (including number suffixes), `PackageIndex` semantics, `TypeName` display, the reference partition + path-mapping helpers, and the built CLI binary. They construct byte vectors by hand — when adding a decoder, add a matching hand-built vector test.
- **No ad-hoc inline `#[cfg(test)]` modules** except the CLI internals (`src/cli/cache.rs`, `src/cli/reverse_refs.rs`) and the core crate's `src/tests` harness.
