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

The binary is written to `target/release/cc-uax` (`.exe` on Windows). To install from the checkout:

```bash
cargo install --path crates/cc-uax-cli --locked
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
