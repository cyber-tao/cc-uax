<div align="center">

# cc-uax

**Parse Unreal Engine 5 `.uasset`/`.umap` packages to JSON — properties, Blueprint graph, and asset references.**

A single CLI that turns opaque UE5 editor assets into structured JSON — so Claude Code can finally read your game's Blueprints, properties, and asset references.

[![Rust](https://img.shields.io/badge/Rust-2024%20edition-CE422B?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Release](https://img.shields.io/github/v/release/cyber-tao/cc-uax?logo=github)](https://github.com/cyber-tao/cc-uax/releases)
[![CI](https://img.shields.io/github/actions/workflow/status/cyber-tao/cc-uax/release.yml?branch=master&label=build)](https://github.com/cyber-tao/cc-uax/actions/workflows/release.yml)
[![UE5](https://img.shields.io/badge/Unreal%20Engine-5.7-0E1128?logo=unrealengine&logoColor=white)](https://www.unrealengine.com/)
[![License: MIT](https://img.shields.io/badge/License-MIT-2ea44f?style=flat)](https://opensource.org/licenses/MIT)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-5851DB)](#)
![Status](https://img.shields.io/badge/status-stable%20%202%2C096%20assets%20validated-1F6FEB)

**English** · [简体中文](README.zh-CN.md)

</div>

---

## 📖 About

I'm a UE5 game developer, and I built `cc-uax` for one very specific reason: **to let Claude Code read Unreal Engine assets.**

A real UE5 project lives inside opaque `.uasset`/`.umap` binaries — every Blueprint graph, Data Asset, and level is a blob that AI coding assistants simply cannot open. They can write C++ and edit text, but they can't see the node wiring, the tagged properties, or which materials and data tables an actor references. `cc-uax` bridges that gap: it reads any UE5 editor asset and emits it as structured **JSON** — full header, tagged properties, the Blueprint node-and-pin graph, and both forward and reverse asset references — so an agent can reason about your game's content the same way it reasons about your code. It ships as a single self-contained binary with no runtime dependencies.

The name says it plainly: **cc** = Claude Code, **uax** = uasset. The tool also doubles as an [agent skill](#-use-as-an-agent-skill) — once wired up, Claude Code (or OpenAI Codex) calls `cc-uax` automatically the moment you ask it to inspect a `.uasset`/`.umap`, so you never hand-read the binary.

> Scope: **versioned, uncooked editor assets** on UE5 (`FileVersionUE5 >= 1000`). Header, tables, the reference graph, legacy UE5 property tags (`1000..1011`), and complete-type-name property tags (`>= 1012`) are decoded. Cooked / unversioned packages and UE4 legacy formats are intentionally out of scope.

## ✨ Features

- **Full package header** — `FPackageFileSummary`, name table, import & export maps, custom versions.
- **Versioned tagged properties** — UE5 legacy `FPropertyTag` plus UE5.7-style complete `FPropertyTypeName` tags.
- **Precise property windows** — locates each object's data via the `ScriptSerialization` range; correctly consumes `UClass` / `UBlueprint` header control bytes.
- **Rich value decoding**

  | Category | Types |
  |---|---|
  | Primitives | numbers, `bool`, enums, strings, `FName`, `FText` |
  | References | `ObjectProperty` → full name + package index, `SoftObjectPath`, `FieldPath` |
  | Containers | `ArrayProperty`, `SetProperty`, `MapProperty`, `OptionalProperty` |
  | Nested | recursive tagged structs |
  | Native structs | `Vector` / `Vector3f` / `Vector4` / `Vector4f` / `Rotator` / `Quat` / `Color` / `LinearColor` / `Transform` / `Transform3f` / `Box` / `Box2D` / `Box2f` / `Guid` / `DateTime` / `FrameNumber` / `IntVector2` / `IntVector4` / `RichCurveKey` … |
  | Material inputs | `ExpressionInput` + Scalar / Vector / Vector2 / Color / ShadingModel / Substrate / MaterialAttributes |
  | Sequencer & curves | `FrameRange`, `FloatChannel`, `DoubleChannel`, per-platform Float / Int / Bool / FrameRate, anim curves (`FloatCurve` / `TransformCurve`) |
  | Runtime structs | `InstancedStruct`, `PerQualityLevelInt` / `Float`, delegates (`Delegate` / `MulticastInline` / `MulticastSparse`), `EdGraphPinType` |
  | Gameplay & FX | `GameplayTagContainer`, `GameplayEffectVersion`, `Spline`, `AlphaBlend`, Niagara core (`NiagaraVariable` / `NiagaraVariableBase` / `NiagaraVariableWithOffset` / `NiagaraTypeDefinition`) |

- **Blueprint graph logic** — `UEdGraphNode` pins are decoded right after the tagged-property region: every node's pins, pin types, default values/objects, and `LinkedTo` edges, so the full node-to-node execution & data graph is reconstructable — covering both Blueprint (`K2Node_*`) and Niagara (`NiagaraNode*`) graphs. Graph nodes also expose a distilled `member` (the function / event / variable they reference) plus `member_from` (its owning C++ class) for quick cross-referencing with source.
- **Selectable output sections** — `--sections` (alias `-S`) composes exactly the blocks you want, or picks a preset (`logic`, `debug`, `full`) — keeping logic analysis lean and bug-hunting thorough.
- **Graceful hex fallback** — future custom binary structs that are not yet structured emit a `type`+`size`-tagged hex preview that **preserves byte alignment**; the current UE5.7 validation set has zero `@unparsed` fallbacks.
- **Reference graph**
  - `-S refs` — forward refs from the import table (`assets` / `scripts`) plus the header's soft-package-reference table (`soft`, e.g. `TSoftObjectPtr`/`TSoftClassPtr`), de-duplicated & sorted.
  - `--scan-dir` — reverse refs: which assets reference *this* file (`referenced_by`, hard **or** soft), via `--mount` path mapping.
- **Incremental scan cache** — SQLite-backed (`.cc-uax-cache.sqlite`), keyed by mtime + size, with a live stderr progress bar. `--no-cache` opts out.

## 🛠️ Tech Stack

**Language & runtime**

`Rust (edition 2024)` · `byteorder` (LE byte stream) · `serde` + `serde_json` (output) · `clap` v4 (CLI, derive) · `anyhow` (errors) · `rusqlite` bundled SQLite (scan cache, **binary only**)

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

### Uninstall

Removes the binary, the user `PATH` entry (Windows), and the Claude Code / Codex skills. Honors `NO_SKILL=1` to leave skills in place.

**Linux / macOS**

```bash
bash install.sh uninstall
# piped: curl -fsSL https://raw.githubusercontent.com/cyber-tao/cc-uax/master/install.sh | bash -s -- uninstall
```

**Windows (PowerShell)**

```powershell
.\install.ps1 -Uninstall
# piped: $env:UNINSTALL='1'; irm https://raw.githubusercontent.com/cyber-tao/cc-uax/master/install.ps1 | iex
```

### Build from source

Requires Rust ≥ 1.88 (edition 2024, uses let-chains):

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
| Codex | `~/.codex/skills/cc-uax/` | `<repo>/.codex/skills/cc-uax/` |
| Codex / Agents legacy | `~/.agents/skills/cc-uax/` | `<repo>/.agents/skills/cc-uax/` |

> A skill is just a directory with a `SKILL.md` (YAML frontmatter: `name`, `description`). Drop it into the project-level path and commit it to share with every contributor.

## 🚀 Usage

```text
cc-uax <input.uasset> [options]

  -o, --output <FILE>   Write JSON to a file (default: stdout)
  -c, --compact         Compact JSON (default: pretty-printed)
  -S, --sections <LIST> Sections to emit, comma-separated, or a preset (see Output sections)
  -d, --scan-dir <DIR>  Recursively scan <DIR>; also list who references this file (with -S refs)
  -m, --mount <PREFIX>  Mount prefix corresponding to <DIR> (default /Game)
      --no-cache        Disable the reverse-scan disk cache
  -h, --help            Show help
  -V, --version         Show version
```

**Examples**

```pwsh
# Parse a blueprint, pretty-print to a file
cc-uax BP_MyActor.uasset -o out.json

# Graph logic only — nodes + pin connectivity (lean view for framework analysis)
cc-uax BP_MyActor.uasset -S logic

# Bug-hunting view — summary + imports + full properties + byte layout
cc-uax BP_MyActor.uasset -S debug

# Inspect the header only
cc-uax BP_MyActor.uasset -S summary

# Forward references — which packages this asset pulls in
cc-uax BP_MyActor.uasset -S refs

# Reverse references — who references me (scan a Content tree, mounted at /Game)
cc-uax BP_MyActor.uasset -S refs --scan-dir ./Content
```

## 📋 Output Schema

**Full mode**

```jsonc
{
  "diagnostics": [],
  "summary":  { /* versions, engine version, table counts, custom versions, package name */ },
  "imports":  [ { "index": -1, "class": "...", "name": "...", "full_name": "..." } ],
  "exports":  [
    {
      "index": 15,
      "name": "K2Node_CallFunction_14",
      "class": "/Script/BlueprintGraph.K2Node_CallFunction",
      "member": "SetMaterial",                       // distilled node identity
      "member_from": { "ref": "/Script/Engine.PrimitiveComponent", "index": -19 },
      "properties": [ /* tagged properties — omitted by -S logic */ ],
      "pins": [
        { "name": "execute", "direction": "input", "category": "exec",
          "linked_to": [ { "node": "...K2Node_Knot_7", "node_index": 25, "pin": "OutputPin" } ] },
        { "name": "Material", "direction": "input", "category": "object",
          "container_type": "none", "is_reference": true, "is_const": false,
          "is_weak_pointer": false, "is_uobject_wrapper": false,
          "default_object": { "ref": "/Game/.../MI_Box_Destroyed", "index": -45 } }
      ]
    }
  ],
  "file": "input file path"
}
```

> `member` / `member_from` and `pins` appear only on graph-node exports (`K2Node_*`, `EdGraphNode_*`). The low-level `super` / `outer` / `serial_offset` / `object_flags` / `script_serialization_*` fields move under the `layout` section (`-S layout`, or any preset that includes it).

**References mode** (`self` / `referenced_by` appear only with `--scan-dir`)

```jsonc
{
  "references": {
    "assets":        [ "/Game/...", "/Engine/..." ],
    "scripts":       [ "/Script/CoreUObject", "/Script/Engine" ],
    "soft":          [ "/Game/.../SoftReferencedAsset" ],
    "self":          "/Game/Foo/BP_MyActor",
    "referenced_by": [ "/Game/Foo/BP_Other" ]
  },
  "file": "input file path"
}
```

Add `summary` explicitly, e.g. `-S summary,refs`, if you want header fields alongside reference output.

### Output sections

`--sections <LIST>` (alias `-S`) selects which blocks to emit; items are comma-separated and may mix section keys with a preset. When omitted, the default is `full`.

| Preset | Expands to | Use for |
|---|---|---|
| `logic` | `summary` + exports (identity + `member` + `pins`) | Graph / framework analysis alongside C++ |
| `debug` | `summary` + `imports` + exports (`properties` + `layout`) | Bug hunting / serialization checks |
| `full`  | `summary` + `imports` + exports (`pins` + `properties` + `layout`) — the default; excludes `names` and `references` unless requested | Complete export dump |

Section keys (composable, e.g. `-S exports,pins,properties` or `-S full,names`): `summary`, `imports`, `exports` (identity base), `pins`, `properties`, `layout` (serial offsets / flags / script window), `names`, `references` (alias `refs`).

For a single block just name it: `-S summary` (header only), or `-S refs` (forward refs; add `--scan-dir` for reverse refs).

## 🏗️ Architecture

```
cc-uax/
├── src/
│   ├── lib.rs          # Library root — exports Package, OutputSections
│   ├── main.rs         # CLI entry orchestration
│   ├── cli/
│   │   ├── mod.rs
│   │   ├── args.rs     # Clap arguments and section parsing
│   │   ├── reverse_refs.rs # Reverse-reference scan and worker coordination
│   │   └── cache.rs    # SQLite reverse-ref cache (binary-only)
│   ├── package.rs      # Core: Package pipeline + JSON output (sections) + pin orchestration
│   ├── summary.rs      # FPackageFileSummary (magic, versions, table offsets)
│   ├── name.rs         # NameMap — name table parse & resolve
│   ├── object.rs       # PackageIndex (+/- ⇒ export/import), Import, Export
│   ├── property/
│   │   ├── mod.rs      # Property parser entry points and shared types
│   │   ├── tag.rs      # Legacy and complete-type-name FPropertyTag layouts
│   │   ├── value.rs    # Recursive tagged-property value decoder
│   │   ├── native.rs   # Native struct decoders + alignment fallbacks
│   │   └── text.rs     # FText parsing
│   ├── pin.rs          # EdGraphNode pin decoder — pins, pin types, LinkedTo edges
│   ├── version.rs      # UE5/UE4 file-version constants + custom-version GUIDs
│   └── reader.rs       # Little-endian byte-stream primitives
├── tests/
│   ├── common/         # Shared byte-vector builders
│   ├── model.rs
│   ├── package.rs
│   ├── pin.rs
│   ├── property.rs
│   └── reader.rs       # Hand-built byte-vector integration tests by module
├── skills/
│   └── cc-uax/
│       └── SKILL.md    # Agent skill (Claude Code + Codex compatible)
├── .github/workflows/
│   └── release.yml     # Multi-platform build + GitHub Release on tag
├── install.sh          # One-line installer (Linux / macOS)
├── install.ps1         # One-line installer (Windows)
├── dev-install.sh      # Dev: rebuild from source + refresh skills (Linux / macOS)
├── dev-install.ps1     # Dev: rebuild from source + refresh skills (Windows)
├── Cargo.toml          # lib + bin dual targets
├── CLAUDE.md           # Architecture guide for Claude Code
└── README.md
```

**Parsing pipeline** (`Package::parse` orchestrates, each stage feeds the next):

1. `Reader` — LE primitives (`u8..u64`, `f32`/`f64`, `FString`, `FName`, `Guid`).
2. `PackageFileSummary::parse` — validates `PACKAGE_FILE_TAG` (`0x9E2A83C1`), detects endianness, reads versions + table offsets.
3. `NameMap::parse` — resolves names including number suffixes (`Foo_3`).
4. Import / Export tables — `PackageIndex` sign selects the table.
5. Per-export `ScriptSerialization` window → `property/` recursively decodes legacy and complete property tags; unknown future structs fall back to hex so alignment never breaks.
6. Graph nodes — after the property window, `pin.rs` decodes the `UEdGraphNode` pin region (`pins` + `LinkedTo`), and node identities are distilled into `member` / `member_from`.

> See [CLAUDE.md](CLAUDE.md) for the full architectural guide.

## ⚠️ Scope & Limitations

- ✅ **Validated** on **2,096 `.uasset` / `.umap` files** from a UE5.7 project — failed = 0, diagnostics = 0, `@unparsed` = 0.
- ❌ Cooked packages (unversioned / package compression) and UE4 legacy formats are **not** supported.
- 🔧 Native-binary structs used by the current UE5.7 validation set — including Niagara, GPU binding, groom dataflow, skeletal-mesh sampling, and cloth LOD payloads — are decoded structurally; unknown future custom payloads still use the alignment-preserving `@unparsed` preview.
- 🔧 `referenced_by` derives package paths from disk — the input file must live under `--scan-dir` mapped to `--mount`. Both hard references (imports) and soft references (`TSoftObjectPtr`/`TSoftClassPtr`) are counted.
- 🔧 Cache invalidates on mtime + size and auto-rebuilds when the built-in schema version changes.

## 🤝 Contributing

This is a focused single-purpose tool. If you extend a decoder, add a hand-built byte-vector test under [tests/](tests/) and ensure the export's property window stays byte-aligned. Run `cargo fmt -- --check`, `cargo clippy --all-targets`, `cargo test`, and `cargo test --no-default-features` before submitting.

## 📄 License

MIT
