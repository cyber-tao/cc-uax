---
title: Install
description: Install prebuilt cc-uax releases or build the workspace from source.
---

# Install

Prebuilt releases install the binary and the complete agent-skill directory.

## Linux / macOS

```bash
curl -fsSL https://raw.githubusercontent.com/cyber-tao/cc-uax/master/install.sh | bash
```

## Windows PowerShell

```powershell
irm https://raw.githubusercontent.com/cyber-tao/cc-uax/master/install.ps1 | iex
```

<InstallTabs />

## Build from source

Rust 1.88 or newer is required.

```bash
git clone https://github.com/cyber-tao/cc-uax.git
cd cc-uax
cargo build -p cc-uax-cli --release --locked
```

The binary is written to `target/release/cc-uax` (`.exe` on Windows). From a checkout, install the binary **and** refresh agent skills with:

```bash
./dev-install.sh
```

```powershell
.\dev-install.ps1
```

`dev-install` incrementally `cargo build`s the release binary into `~/.cargo/bin` and links `skills/cc-uax/` into the Claude Code, Codex, and Agents skill directories. `cargo install --path crates/cc-uax-cli --locked` installs only the binary; it does not install the skill.

If both a release install and a checkout install are present, the scripts explain which `cc-uax` `PATH` will run and ask whether to remove the other copy. Piped one-liners cannot prompt; they keep both. `-ReplaceOther` / `REPLACE_OTHER=1` and `-KeepBoth` / `KEEP_BOTH=1` select without a prompt.

```bash
./dev-install.sh uninstall
```

```powershell
.\dev-install.ps1 -Uninstall
```

An older `cc-uax` on `PATH` will silently disagree with this repository. From a clone, prefer:

```powershell
cargo run -p cc-uax-cli --release --locked -- project D:/Games/MyGame --output project-report.json
```

or call `target/release/cc-uax` directly.

## Contributor checks

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --release --locked
```
