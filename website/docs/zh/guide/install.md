---
title: 安装
description: 安装预编译的 cc-uax，或从源码构建 workspace。
---

# 安装

预编译 Release 会安装二进制和完整的 Agent Skill 目录。

## Linux / macOS

```bash
curl -fsSL https://raw.githubusercontent.com/cyber-tao/cc-uax/master/install.sh | bash
```

## Windows PowerShell

```powershell
irm https://raw.githubusercontent.com/cyber-tao/cc-uax/master/install.ps1 | iex
```

<InstallTabs />

## 从源码构建

需要 Rust 1.88 或更高版本。

```bash
git clone https://github.com/cyber-tao/cc-uax.git
cd cc-uax
cargo build -p cc-uax-cli --release --locked
```

二进制位于 `target/release/cc-uax`（Windows 上为 `.exe`）。也可以从 checkout 安装：

```bash
cargo install --path crates/cc-uax-cli --locked
```

`PATH` 上的旧版 `cc-uax` 会静默给出和本仓库不一致的结果。在 clone 里更稳妥的做法是：

```powershell
cargo run -p cc-uax-cli --release --locked -- project D:/Games/MyGame --output project-report.json
```

或直接调用 `target/release/cc-uax`。

## 贡献者检查

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --release --locked
```
