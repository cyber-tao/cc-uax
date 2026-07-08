---
name: cc-uax
description: Parse and inspect Unreal Engine 5 .uasset/.umap package files, and query forward/reverse asset references. Use whenever you need to READ a UE5 asset (.uasset/.umap), extract its imports/exports/properties, find what packages an asset references, or find which assets reference a given package — invoke the cc-uax CLI instead of hand-reading the binary.
---

`cc-uax` is a from-scratch Rust reader for UE5 editor package files (`.uasset`, `.umap`). It parses the `CoreUObject` binary format and emits JSON — header, name table, import/export tables, tagged properties, and Blueprint graph-node pins with their `LinkedTo` connectivity. It also computes forward references (what this asset depends on) and, by scanning a directory, reverse references (who depends on this asset).

It is a **system-installed CLI tool** — `cc-uax` lives on `PATH` and behaves identically on Windows, Linux, and macOS. All commands below are plain `cc-uax` invocations.

Scope: **versioned, uncooked editor assets** for UE5 (`FileVersionUE5 >= 1000`, little-endian). Legacy UE5 property tags (`1000..1011`) and complete-type-name property tags (`>= 1012`) are decoded. Cooked / unversioned / big-endian / UE4-legacy packages are rejected — see Gotchas.

## Prerequisites

`cc-uax` must be on `PATH`. Verify:

```bash
cc-uax --version
# → cc-uax x.y.z
```

If missing, install via the one-line installer (downloads the latest prebuilt binary for your platform):

```bash
# Linux / macOS
curl -fsSL https://raw.githubusercontent.com/cyber-tao/cc-uax/master/install.sh | bash
# Windows (PowerShell)
irm https://raw.githubusercontent.com/cyber-tao/cc-uax/master/install.ps1 | iex
```

Or build from source: `cargo install --path .` (puts `cc-uax` in `~/.cargo/bin`).

To uninstall (remove the binary, PATH entry, and skills): `bash install.sh uninstall` / `.\install.ps1 -Uninstall` (or `UNINSTALL=1` for the piped one-liners).

## Run

`cc-uax <INPUT> [OPTIONS]` — JSON on stdout, progress on stderr. Exit code: `0` parsed ok, `1` not a valid UE5 package / file missing / parse error. Always check `$?` before consuming the JSON.

### Which command for which task

| Task | Command |
|---|---|
| Header — versions, counts, engine version | `cc-uax -c -S summary <file>` |
| Blueprint graph logic — nodes + pin connectivity | `cc-uax -c -S logic <file>` |
| Forward refs — what this asset imports (assets/scripts) | `cc-uax -c -S refs <file>` |
| Reverse refs — who references this asset | `cc-uax -c -S refs -d <Content-dir> <file>` |
| Property-focused dump (properties + byte layout, no pins) | `cc-uax -c -S debug <file>` |
| Export/import structure without property decode | `cc-uax -c -S exports,layout <file>` |
| Dump export parse (default: summary/imports/exports with pins, properties, layout) | `cc-uax -c <file>` |
| Exhaustive JSON including names and references | `cc-uax -c -S all <file>` |
| Pick exact sections | `cc-uax -c -S exports,pins,properties <file>` |
| Summary + full name table | `cc-uax -c -S summary,names <file>` |

`-S`/`--sections` is the single content selector — presets `logic` (graph nodes + pins), `debug` (properties + layout), `dump` (default: summary + imports + exports with pins/properties/layout; excludes `names` and `references` unless requested), and `all` (`dump` + `names` + `references`); or comma-separate section keys `summary,imports,exports`/`identity`, `pins,properties,layout,names,references` (`refs` aliases `references`). Omitting `-S` yields `dump`.

### Verified examples

```bash
CONTENT="<your-project>/Content"
TARGET="$CONTENT/<asset.uasset>"

# Header — versions and package name
cc-uax -c -S summary "$TARGET"
# → "summary": {"package_name": "/Game/..."}

# Blueprint graph logic — node members + pin LinkedTo edges (lean, no full properties)
cc-uax -c -S logic "<Blueprint.uasset>"
# → exports[].member ("SetMaterial"), member_from ("/Script/Engine.PrimitiveComponent"),
#   pins[].linked_to[] ({node, pin}) — the reconstructable node-to-node graph

# Forward references — assets/scripts this file imports
cc-uax -c -S refs "$TARGET"
# → "references": {"assets": ["/Game/...", ...], "scripts": ["/Script/Engine", ...]}

# Reverse references — scan the project's Content/ to find dependents
cc-uax -c -S refs -d "$CONTENT" --no-cache "$TARGET"
# → "referenced_by" includes "/Game/.../Map_..."
```

`-c` / `--compact` emits compact single-line JSON; `-o <FILE>` writes to a file instead of stdout.

> **Default to `-c` whenever the JSON goes back into your context.** Compact output trims roughly 15-30% of the tokens (indentation + newlines) at zero information loss — the model parses it just as easily. Omit `-c` (pretty-print) only when writing to a file for a human to read.

```bash
cc-uax -o out.json "<file.uasset>"    # pretty JSON to a file (for humans)
```

`.umap` (level) files use the exact same commands — they share the UE5 package format:

```bash
cc-uax -c -S summary "<Level.umap>"
# → "package_name": "/Game/..."
```

Flag reference:

```bash
cc-uax --help
```

### Reverse-reference scanning

`-S refs -d <Content-dir>` recursively scans `<Content-dir>` for `.uasset`/`.umap`, parses each mapped package, and reports which ones import `<file>`'s package path. The on-disk cache (`.cc-uax-cache.sqlite`) is written at the scan-dir root to speed up repeat runs; pass `--no-cache` to disable. Output adds `self` (the input's package path) and `referenced_by` under `references`. With explicit `--mount` mappings, the input file must be covered by a mount entry; scanned files outside all mount entries are skipped rather than assigned a guessed package path.

## Gotchas

- **Map disk roots explicitly for project-root or plugin scans.** `--mount /Game` (the default) maps `<scan-dir>/<relative>` → `/Game/<relative>` and is best when `-d` is the project `Content/` root. When scanning from a project root or including plugin/Engine content, pass a mapping such as `--mount /Game=Content,/MyPlugin=Plugins/MyPlugin/Content,/Engine=Engine/Content`.
- **`-d` writes `.cc-uax-cache.sqlite` into the scan-dir.** When scanning a UE5 project you do not own, pass `--no-cache` or delete the file afterwards. The cache key is path+mtime+size; malformed soft-reference tables are treated as parse failures so partial reference results are not cached as successful scans.
- **Git Bash / MSYS2 mangles `-m /Game`.** A leading-slash argument like `-m /Game` gets path-converted to `C:/Program Files/Git/Game`, corrupting the mount prefix (symptom: `self` starts with `/C:/Program Files/Git/Game/...`). Use a double slash — `-m //Game` — which MSYS2 restores to `/Game`; or run from PowerShell/cmd. Native Linux/macOS shells are unaffected.
- **Cooked / unversioned / big-endian packages are rejected by design.** cc-uax targets editor-saved versioned assets only. These are hard limits, not bugs — see Troubleshooting for the exact messages.
- **`@unparsed` means an unknown future custom payload, not normal UE5.7 coverage.** A clean UE5.7 validation run has zero `@unparsed` fallbacks. In the repo, re-run that check with `scripts/validate-real-assets.ps1 -ContentDir <your-project>/Content` or `CC_UAX_CONTENT_DIR=<your-project>/Content scripts/validate-real-assets.sh` when a UE5 project is available. If one appears on a new project, cc-uax still preserves the struct `type`, byte `size`, and hex preview so alignment is not lost and the next property can decode. Blueprint graph-node pins are decoded structurally — use `-S logic` for the pins.
- **Only `.uasset` and `.umap` are package files.** Companion files (`.uexp`, `.ubulk`, `.ini`) are not UE5 package summaries — don't pass them.

## Troubleshooting

- **`command not found: cc-uax`**: not installed. Run the one-line installer (see Prerequisites) or `cargo install --path .`, then ensure the install dir is on `PATH`.
- **`Error: Failed to parse: ... invalid package magic: 0x...`**: not a UE5 package file. Confirm it is a `.uasset`/`.umap` editor asset, not a cooked/`.pak`/`.uexp` companion.
- **`package is unversioned (... typically a cooked package)`**: the asset was saved by a cooked build. cc-uax cannot read it; find the source editor asset instead.
- **`package uses swapped (big-endian) byte order, possibly a cooked console package`**: console cooked package. Out of scope.
- **`looks like a legacy UE3 package`**: `LegacyFileVersion >= 0`. Out of scope.
- **`-S refs -d` returns `referenced_by: []` but you expect hits**: usually the scan-dir/mount mapping is wrong or MSYS2 mangled `-m /Game` (use `-m //Game` under Git Bash). Confirm a suspected dependent actually imports the target's package path with `cc-uax -S refs <dependent>`.
