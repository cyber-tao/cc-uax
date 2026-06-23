# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

`cc-uax` is a from-scratch Rust reader for Unreal Engine 5 Blueprint (`.uasset`) files. It parses the UE5 package binary format (no existing uasset crate) and emits JSON. The parsing logic mirrors UE5.7 `CoreUObject` source. Scope: **versioned, uncooked editor assets** for UE5 (`FileVersionUE5 >= 1000`). Cooked/unversioned packages and UE4 legacy formats are explicitly out of scope.

## Commands

```pwsh
cargo build --release                    # binary at target/release/cc-uax
cargo run --release -- <file.uasset>     # dev run
cargo test                               # full test suite (tests/units.rs)
cargo test --test units <test_name>      # single integration test (e.g. fstring_ansi)
cargo fmt
cargo clippy --all-targets
```

No benchmarks, no separate lint config. CI is whatever runs these locally.

## Dual-target layout (important)

`Cargo.toml` defines both a `lib` (`cc_uax`) and a `bin` (`cc-uax`):

- **lib** (`src/lib.rs`): the parser. Declares 7 modules — `name`, `object`, `package`, `property`, `reader`, `summary`, `version` — and re-exports `Package` + `JsonOptions`.
- **bin** (`src/main.rs`): the CLI. Declares its own `mod cache` and pulls in `rusqlite`.

`src/cache.rs` is **only** included by the binary, never by the lib. Keep the SQLite reverse-scan cache out of the parser crate; `rusqlite` is a bin-side concern. When changing the parser, do not reach into `cache.rs` or add parser deps through it.

## Parsing pipeline

The central entry point is `Package::parse` in [src/package.rs](src/package.rs). The flow is strictly ordered because each stage's offsets come from the previous one:

1. **`Reader`** ([src/reader.rs](src/reader.rs)) — little-endian byte-stream primitives (`u8/u16/u32/i32/u64/f32/f64`, `FString`, `FName`, `Guid`). All file I/O goes through this; the format is LE-only.
2. **`PackageFileSummary::parse`** ([src/summary.rs](src/summary.rs)) — package header. Validates the `PACKAGE_FILE_TAG` (`0x9E2A83C1`) magic and detects byte order via `PACKAGE_FILE_TAG_SWAPPED`; reads engine/file versions, `CustomVersion`s, and the offset+count of every downstream table. `is_unversioned()` and `filter_editor_only()` gate later behavior.
3. **`NameMap`** ([src/name.rs](src/name.rs)) — the name table. `resolve` returns `Option<String>` with number suffix semantics (e.g. `Foo_3`).
4. **Import / Export tables** ([src/object.rs](src/object.rs)) — `PackageIndex` encodes the table: **positive = export, negative = import**. `ObjectExport` carries `serial_offset` / `serial_size`, which delimit each object's serialized property region.
5. **Per-export property region** ([src/property.rs](src/property.rs)) — for each export, `parse_object_properties` seeks to the `ScriptSerialization` window and recursively decodes UE5.7 tagged properties (`FPropertyTag` + full `FPropertyTypeName`).

The per-export `serial_offset`/`serial_size` windowing is what guarantees byte alignment across objects — never parse properties outside their window, and if you add a value decoder, it must consume exactly its bytes or fall back to hex preview (see below).

## Version gating

[src/version.rs](src/version.rs) holds the UE5 `CORE_UOBJECT` file-version constants (e.g. `INITIAL_VERSION = 1000`, `SCRIPT_SERIALIZATION_OFFSET = 1010`, `PROPERTY_TAG_COMPLETE_TYPE_NAME = 1012`) and the UE4 legacy ladder. Behavior branches on `FileVersionUE5` against these constants. When adding support keyed to a UE version, add the constant here and gate on it — do not hardcode magic version numbers at call sites.

## Property decoding ([src/property.rs](src/property.rs))

- `TypeName` — UE5.7 `FPropertyTypeName` (nested type name with parameters).
- `ParseCtx` — carries a `&Package` reference so object-property values can be resolved to full names + package indices.
- `parse_value` dispatches into `parse_collection` / `parse_map` / `parse_element` / `parse_struct` / `parse_native_struct` / `parse_soft_object` / `parse_text`.
- **Hex fallback**: types with custom binary serialization that are not yet structured (e.g. `EdGraphPinType`, Niagara, `AnimNotifyEvent`) are emitted as a hex preview capped by `PREVIEW_MAX`, preserving `type` and `size`. The hex path exists specifically to keep byte alignment intact — any new unknown struct should go through `to_hex` rather than guessing fields.

## Reference analysis

- **Forward references** (`collect_package_references` in [src/package.rs](src/package.rs)): reads the import table and partitions external packages into `assets` vs `scripts` by the `/Script/` prefix (`SCRIPT_PATH_PREFIX`). Output keys: `assets`, `scripts`.
- **Reverse references** (CLI only, [src/main.rs](src/main.rs)): `--scan-dir <DIR>` walks the directory (`collect_asset_files`), maps disk paths to package paths via `--mount` (default `/Game`) using `package_path_from_relative`, parses every asset, and computes `referenced_by`.
- **Cache** (`RefCache` in [src/cache.rs](src/cache.rs)): the reverse scan writes `.cc-uax-cache.sqlite` at the scan-dir root, keyed by file path + mtime + size. Bump `CACHE_SCHEMA_VERSION` whenever the reference-extraction logic changes — existing caches auto-invalidate on schema mismatch. `--no-cache` disables it.

## CLI surface

`Args` (clap derive) in [src/main.rs](src/main.rs). Output modes are mutually layered, not all independent:

- default → full `summary` + `imports` + `exports` JSON
- `--summary-only` → header only
- `--references` → `assets`/`scripts` (and `self`/`referenced_by` when paired with `--scan-dir`)
- `--no-properties` (`-P`) → structural output, skip property decode
- `--compact` / `--names` / `--output <FILE>` shape the JSON

## Conventions

- **Endianness**: LE everywhere, via `byteorder`. Never use native/host byte order.
- **Minimal deps in the parser**: `byteorder` / `serde` / `serde_json` / `clap` / `anyhow`. `rusqlite` is bin-only. Do not add a new dependency to the lib without explicit reason — a from-scratch parser is a project goal, not an accident.
- **Testing**: integration tests live in [tests/units.rs](tests/units.rs) and exercise `Reader` primitives, `NameMap` resolution (including number suffixes), `PackageIndex` semantics, `TypeName` display, and the reference partition + path-mapping helpers. They construct byte vectors by hand — when adding a decoder, add a matching hand-built vector test.
- **No inline `#[cfg(test)]` modules** except `src/cache.rs`'s `tests` module.
