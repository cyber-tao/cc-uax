<div align="center">

# cc-uax

**面向 Claude Code、Codex 等工程 Agent 的 Unreal Engine 5 编辑器资产结构化分析工具。**

[![Rust](https://img.shields.io/badge/Rust-2024%20edition-CE422B?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![CI](https://img.shields.io/github/actions/workflow/status/cyber-tao/cc-uax/ci.yml?branch=master&label=CI)](https://github.com/cyber-tao/cc-uax/actions/workflows/ci.yml)
[![UE5](https://img.shields.io/badge/UE5-5.0–5.8-0E1128?logo=unrealengine&logoColor=white)](https://www.unrealengine.com/)
[![License: MIT](https://img.shields.io/badge/license-MIT-2ea44f)](LICENSE)

[网站](https://cyber-tao.github.io/cc-uax/zh/) · [English](README.md) · **简体中文**

</div>

---

`cc-uax` 将受支持的 UE5 编辑器包（`.uasset` 和 `.umap`）转换为带类型和证据的报告。它既能分析单个资产，也能在不启动 Unreal Editor 的情况下建立项目级索引。

## 为什么需要 cc-uax？

Unreal 项目的大量逻辑和数据位于二进制包中。以源码为中心的 Agent 可以阅读 C++ 和配置，却无法直接检查蓝图执行流、序列化属性、资产依赖、PCG 图、StateTree 或 World Partition 外部包。`cc-uax` 补上这一层证据。

## 能力

- **强类型包分析**：包元数据、import/export、带标签属性、对象引用、诊断和字节覆盖率。
- **按图隔离的逻辑模型**：K2/EdGraph 节点始终归属具体图；不会把不同图中的同名节点拼成虚假链路。
- **专用适配器**：在序列化证据充分时分析 RigVM/ControlRig model links、StateTree、PCG 以及 Niagara 编辑器图。
- **项目级索引**：单次扫描建立资产清单、前向/反向引用邻接表、可达性和 World Partition 外部包归属闭包。
- **显式表达不确定性**：报告包含 schema 版本、状态、机器可读 coverage、diagnostics 和 capability 证据；不支持或 opaque 的区域会被具名标出，不会伪装成成功解码。
- **Agent Skill**：随附 skill 要求 Claude Code、Codex 在描述玩法和资源使用前先建立项目证据。

## 安装

预编译 Release 会安装二进制和完整的 Agent Skill 目录。

**Linux / macOS**

```bash
curl -fsSL https://raw.githubusercontent.com/cyber-tao/cc-uax/master/install.sh | bash
```

**Windows PowerShell**

```powershell
irm https://raw.githubusercontent.com/cyber-tao/cc-uax/master/install.ps1 | iex
```

从源码构建（需要 Rust 1.88 或更高版本）、从 checkout 安装和卸载见[安装指南](website/docs/zh/guide/install.md)。

## 快速开始

```powershell
# 单个资产：选择能回答问题的最小 --view
cc-uax asset Content/Blueprints/BP_Player.uasset --view logic

# 整个项目：扫描一次，再用 --focus 下钻
cc-uax project D:/Games/MyGame --output project-report.json
```

参数、view、mount、缓存和退出码见 [CLI 指南](website/docs/zh/guide/cli.md)；逐步教程见[使用教程](website/docs/zh/guide/tutorials.md)。

## 支持范围

`cc-uax` 面向有版本信息、未 Cook 的 UE5.0–5.8 编辑器包：`FileVersionUE5` 1000–1018，对照 UE5.0–5.8 源码核对。真实项目已经覆盖过 1002–1004、1006–1009 和 1012–1018；1000、1001、1005、1010 和 1011 尚未在真实资产中见到。证据完整时包可以为 `status=complete`。

范围之外的包会被直接拒绝而不是猜着解析：UE4 及更早、高于 1018、cooked 包（包括带 `PKG_Cooked` 和 `PKG_UnversionedProperties` 标志的包）、无版本包、大端包以及压缩包。`cc-uax asset` 对它们以退出码 `1` 结束；`cc-uax project` 把它们记为 `unsupported` 证据，进程仍以 `0` 退出。对 UE4 格式的包（`FileVersionUE5` = 0），项目扫描还会读取它的 linker 引用表，因此它提供引用边和可达性。详见[范围与限制](website/docs/zh/guide/limits.md)。

## 文档

| 主题 | 页面 |
|---|---|
| 安装 | [website/docs/zh/guide/install.md](website/docs/zh/guide/install.md) |
| CLI | [website/docs/zh/guide/cli.md](website/docs/zh/guide/cli.md) |
| 教程 | [website/docs/zh/guide/tutorials.md](website/docs/zh/guide/tutorials.md) |
| 阅读报告 | [website/docs/zh/guide/reports.md](website/docs/zh/guide/reports.md) |
| 范围与限制 | [website/docs/zh/guide/limits.md](website/docs/zh/guide/limits.md) |
| 架构 | [website/docs/zh/guide/architecture.md](website/docs/zh/guide/architecture.md) |
| Agent Skill | [website/docs/zh/guide/skill.md](website/docs/zh/guide/skill.md) |
| 报告字段、代码、退出码、schema 版本 | [report-contract.md](skills/cc-uax/references/report-contract.md) |

同样的指南也发布在 [cyber-tao.github.io/cc-uax/zh](https://cyber-tao.github.io/cc-uax/zh/)。

## Agent Skill

请复制完整的 [`skills/cc-uax/`](skills/cc-uax/) 目录，而不是只复制 `SKILL.md`；`agents/` 和 `references/` 是 skill 契约的一部分。预编译安装脚本会替你放好，在 checkout 里 `./dev-install.sh` / `.\dev-install.ps1` 会链接工作区里的副本。Claude Code、Codex 和 Agents 兼容客户端的目录见 [Skill 指南](website/docs/zh/guide/skill.md)。

## 贡献

工程规则见 [CLAUDE.md](CLAUDE.md)。提交改动前运行：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --release --locked
```

## 许可

[MIT](LICENSE)
