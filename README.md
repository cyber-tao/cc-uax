<div align="center">

# cc-uax

**A from-scratch Rust reader for Unreal Engine 5 Blueprint (`.uasset`) files → JSON**

Parses UE5 package binaries by mirroring `CoreUObject` serialization — no third-party uasset crate involved.

[![Rust](https://img.shields.io/badge/Rust-2024%20edition-CE422B?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Release](https://img.shields.io/github/v/release/cyber-tao/cc-uax?logo=github)](https://github.com/cyber-tao/cc-uax/releases)
[![CI](https://img.shields.io/github/actions/workflow/status/cyber-tao/cc-uax/release.yml?branch=main&label=build)](https://github.com/cyber-tao/cc-uax/actions/workflows/release.yml)
[![UE5](https://img.shields.io/badge/Unreal%20Engine-5.7-0E1128?logo=unrealengine&logoColor=white)](https://www.unrealengine.com/)
[![License: MIT](https://img.shields.io/badge/License-MIT-2ea44f?style=flat)](https://opensource.org/licenses/MIT)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-5851DB)](#)
![Status](https://img.shields.io/badge/status-stable%20%201%2C423%20assets%20validated-1F6FEB)

**English** · [简体中文](README.zh-CN.md)

</div>

---

## 📖 About

`cc-uax` is a command-line tool that reads Unreal Engine 5 `.uasset` (Blueprint) files and dumps their contents as structured **JSON**. The parser is hand-written in Rust, tracing UE5.7 source (`CoreUObject`) rather than wrapping an existing library — keeping the dependency surface tiny and the binary self-contained.

> Scope: **versioned, uncooked editor assets** on UE5 (`FileVersionUE5 >= 1000`). Cooked / unversioned packages and UE4 legacy formats are intentionally out of scope.

## ✨ Features

- **Full package header** — `FPackageFileSummary`, name table, import & export maps, custom versions.
- **Versioned tagged properties** — UE5.7-style `FPropertyTag` + complete `FPropertyTypeName`.
- **Precise property windows** — locates each object's data via the `ScriptSerialization` range; correctly consumes `UClass` / `UBlueprint` header control bytes.
- **Rich value decoding**

  | Category | Types |
  |---|---|
  | Primitives | numbers, `bool`, enums, strings, `FName`, `FText` |
  | References | `ObjectProperty` → full name + package index, `SoftObjectPath`, `FieldPath` |
  | Containers | `ArrayProperty`, `SetProperty`, `MapProperty` |
  | Nested | recursive tagged structs |
  | Native structs | `Vector` / `Vector3f` / `Rotator` / `Quat` / `Color` / `LinearColor` / `Transform` / `Guid` / `DateTime` … |

- **Graceful hex fallback** — types with custom binary serialization (e.g. `EdGraphPinType`, Niagara, `AnimNotifyEvent`) emit a `type`+`size`-tagged hex preview that **preserves byte alignment**.
- **Reference graph**
  - `--references` — forward refs from the import table, split into `assets` vs `scripts`, de-duplicated & sorted.
  - `--scan-dir` — reverse refs: which assets reference *this* file (`referenced_by`), via `--mount` path mapping.
- **Incremental scan cache** — SQLite-backed (`.cc-uax-cache.sqlite`), keyed by mtime + size, with a live stderr progress bar. `--no-cache` opts out.

## 🛠️ Tech Stack

**Language & runtime**

`Rust (edition 2021)` · `byteorder` (LE byte stream) · `serde` + `serde_json` (output) · `clap` v4 (CLI, derive) · `anyhow` (errors) · `rusqlite` bundled SQLite (scan cache, **binary only**)

| Layer | Responsibility | Deps |
|---|---|---|
| Parser (`lib`) | Header, names, imports/exports, tagged properties | `byteorder`, `serde`, `serde_json`, `anyhow` |
| CLI (`bin`) | Args, output shaping, reverse-scan, cache | `clap`, `rusqlite` (+ parser) |

> The parser crate intentionally has **no** `rusqlite` dependency — reverse-scan caching lives in the binary only.

## 📦 Installation

### One-line installer (recommended)

Downloads the latest prebuilt binary for your platform, installs `cc-uax` on your `PATH`, and wires up the [agent skill](#-use-as-an-agent-skill) for both Claude Code and Codex.

**Linux / macOS**

```bash
curl -fsSL https://raw.githubusercontent.com/cyber-tao/cc-uax/master/install.sh | bash
```

**Windows (PowerShell)**

```powershell
irm https://raw.githubusercontent.com/cyber-tao/cc-uax/master/install.ps1 | iex
```

Prebuilt binaries are published on the [Releases](https://github.com/cyber-tao/cc-uax/releases) page:

| Platform | Target |
|---|---|
| Linux x86_64 / aarch64 | `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` |
| Windows x86_64 | `x86_64-pc-windows-msvc` |
| macOS x86_64 / Apple Silicon | `x86_64-apple-darwin`, `aarch64-apple-darwin` |

Installer options (set as env vars before invoking): `INSTALL_DIR` (binary location), `VERSION` (pin a tag), `NO_SKILL=1` (skip skill setup).

### Build from source

Requires Rust ≥ 1.85 (edition 2024):

```bash
git clone https://github.com/cyber-tao/cc-uax.git
cd cc-uax
cargo build --release    # binary at target/release/cc-uax[.exe]
```

Or install straight to `~/.cargo/bin`: `cargo install --path .`. No runtime dependencies; SQLite is statically linked.

## 🤖 Use as an Agent Skill

`cc-uax` doubles as an [agent skill](skills/cc-uax/SKILL.md) following the open agent-skills standard — **the same `SKILL.md` works in both Claude Code and OpenAI Codex**. Once installed, either agent automatically invokes `cc-uax` whenever you ask it to inspect a `.uasset`/`.umap` or trace asset references, so you never hand-read the binary.

The one-line installer configures the skill for both agents. To set it up manually, copy [skills/cc-uax/](skills/cc-uax/) into:

| Agent | User-level location | Project-level location |
|---|---|---|
| Claude Code | `~/.claude/skills/cc-uax/` | `<repo>/.claude/skills/cc-uax/` |
| Codex | `~/.agents/skills/cc-uax/` | `<repo>/.agents/skills/cc-uax/` |

> A skill is just a directory with a `SKILL.md` (YAML frontmatter: `name`, `description`). Drop it into the project-level path and commit it to share with every contributor.

## 🚀 Usage

```text
cc-uax <input.uasset> [options]

  -o, --output <FILE>   Write JSON to a file (default: stdout)
  -c, --compact         Compact JSON (default: pretty-printed)
  -n, --names           Include the full name table in the output
  -s, --summary-only    Output only the package header summary
  -P, --no-properties   Skip export property parsing, output structure only
  -r, --references      List only external resources referenced by the file
  -d, --scan-dir <DIR>  Recursively scan <DIR>; also list who references this file (with -r)
  -m, --mount <PREFIX>  Mount prefix corresponding to <DIR> (default /Game)
      --no-cache        Disable the reverse-scan disk cache
  -h, --help            Show help
  -V, --version         Show version
```

**Examples**

```pwsh
# Parse a blueprint, pretty-print to a file
cc-uax BP_MyActor.uasset -o out.json

# Inspect the header only
cc-uax BP_MyActor.uasset --summary-only

# Forward references — which packages this asset pulls in
cc-uax BP_MyActor.uasset --references

# Reverse references — who references me (scan a Content tree, mounted at /Game)
cc-uax BP_MyActor.uasset --references --scan-dir ./Content
```

## 📋 Output Schema

**Full mode**

```jsonc
{
  "summary":  { /* versions, engine version, table counts, custom versions, package name */ },
  "imports":  [ { "index": -1, "class": "...", "name": "...", "full_name": "..." } ],
  "exports":  [
    {
      "index": 1,
      "name": "...",
      "class": "/Script/Engine.Blueprint",
      "super": "...", "outer": "...",
      "serial_offset": 0, "serial_size": 0,
      "properties": [
        { "name": "ParentClass", "type": "ObjectProperty",
          "value": { "ref": "...", "index": -4 } }
      ]
    }
  ],
  "file": "input file path"
}
```

**References mode** (`self` / `referenced_by` appear only with `--scan-dir`)

```jsonc
{
  "summary":    { /* versions / engine / table counts */ },
  "references": {
    "assets":        [ "/Game/...", "/Engine/..." ],
    "scripts":       [ "/Script/CoreUObject", "/Script/Engine" ],
    "self":          "/Game/Foo/BP_MyActor",
    "referenced_by": [ "/Game/Foo/BP_Other" ]
  },
  "file": "input file path"
}
```

## 🏗️ Architecture

```
cc-uax/
├── src/
│   ├── lib.rs          # Library root — exports Package, JsonOptions
│   ├── main.rs         # CLI entry + reverse-scan + cache module
│   ├── package.rs      # Core: Package pipeline + byte Reader + Guid/RawName
│   ├── summary.rs      # FPackageFileSummary (magic, versions, table offsets)
│   ├── name.rs         # NameMap — name table parse & resolve
│   ├── object.rs       # PackageIndex (+/- ⇒ export/import), Import, Export
│   ├── property.rs     # Recursive tagged-property decoder + hex fallback
│   ├── version.rs      # UE5/UE4 file-version constants + PACKAGE_FILE_TAG
│   ├── reader.rs       # Little-endian byte-stream primitives
│   └── cache.rs        # SQLite reverse-ref cache (binary-only)
├── tests/
│   └── units.rs        # Hand-built byte-vector integration tests
├── skills/
│   └── cc-uax/
│       └── SKILL.md    # Agent skill (Claude Code + Codex compatible)
├── .github/workflows/
│   └── release.yml     # Multi-platform build + GitHub Release on tag
├── install.sh          # One-line installer (Linux / macOS)
├── install.ps1         # One-line installer (Windows)
├── Cargo.toml          # lib + bin dual targets
├── CLAUDE.md           # Architecture guide for Claude Code
└── README.md
```

**Parsing pipeline** (`Package::parse` orchestrates, each stage feeds the next):

1. `Reader` — LE primitives (`u8..u64`, `f32`/`f64`, `FString`, `FName`, `Guid`).
2. `PackageFileSummary::parse` — validates `PACKAGE_FILE_TAG` (`0x9E2A83C1`), detects endianness, reads versions + table offsets.
3. `NameMap::parse` — resolves names including number suffixes (`Foo_3`).
4. Import / Export tables — `PackageIndex` sign selects the table.
5. Per-export `ScriptSerialization` window → `property.rs` recursively decodes; unknown structs fall back to hex so alignment never breaks.

> See [CLAUDE.md](CLAUDE.md) for the full architectural guide.

## ⚠️ Scope & Limitations

- ✅ **Validated** on **1,423 `.uasset`** files from a UE5.7 project — all parsed, every object's property region fully byte-aligned.
- ❌ Cooked packages (unversioned / package compression) and UE4 legacy formats are **not** supported.
- 🔧 A few native-binary structs render as hex preview pending structured decoders.
- 🔧 `referenced_by` derives package paths from disk — the input file must live under `--scan-dir` mapped to `--mount`. Only hard references (imports) are counted, not soft ones.
- 🔧 Cache invalidates on mtime + size and auto-rebuilds when the built-in schema version changes.

## 🤝 Contributing

This is a focused single-purpose tool. If you extend a decoder, add a hand-built byte-vector test in [tests/units.rs](tests/units.rs) and ensure the export's property window stays byte-aligned. Run `cargo fmt && cargo clippy --all-targets && cargo test` before submitting.

## 📄 License

MIT
