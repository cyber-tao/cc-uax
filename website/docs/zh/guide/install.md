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

二进制位于 `target/release/cc-uax`（Windows 上为 `.exe`）。在 checkout 里安装二进制**并**刷新 Agent Skill：

```bash
./dev-install.sh
```

```powershell
.\dev-install.ps1
```

`dev-install` 会增量 `cargo build` 出 release 二进制并拷到 `~/.cargo/bin`，再把 `skills/cc-uax/` 链接到 Claude Code、Codex 和 Agents 的 skill 目录。`cargo install --path crates/cc-uax-cli --locked` 只装二进制，不会安装 skill。

如果正式安装和 checkout 安装同时存在，脚本会说明 `PATH` 会命中哪一份，并询问是否卸掉另一份。管道一行安装无法提问，默认保留两边。可用 `-ReplaceOther` / `REPLACE_OTHER=1` 或 `-KeepBoth` / `KEEP_BOTH=1` 跳过提问。

```bash
./dev-install.sh uninstall
```

```powershell
.\dev-install.ps1 -Uninstall
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
