<div align="center">

# cc-uax

**面向 Claude Code、Codex 等工程 Agent 的 Unreal Engine 5 编辑器资产结构化分析工具。**

[![Rust](https://img.shields.io/badge/Rust-2024%20edition-CE422B?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![CI](https://img.shields.io/github/actions/workflow/status/cyber-tao/cc-uax/ci.yml?branch=master&label=CI)](https://github.com/cyber-tao/cc-uax/actions/workflows/ci.yml)
[![UE5](https://img.shields.io/badge/reference-UE%205.7-0E1128?logo=unrealengine&logoColor=white)](https://www.unrealengine.com/)
[![License: MIT](https://img.shields.io/badge/license-MIT-2ea44f)](LICENSE)

[English](README.md) · **简体中文**

</div>

---

## 为什么需要 cc-uax？

Unreal 项目的大量逻辑和数据位于二进制 `.uasset`、`.umap` 包中。以源码为中心的 Agent 可以阅读 C++ 和配置，却无法直接检查蓝图执行流、序列化属性、资产依赖、PCG 图、StateTree 或 World Partition 外部包。

`cc-uax` 将受支持的 UE5 编辑器包转换为带类型和证据的报告。它既能分析单个资产，也能在不启动 Unreal Editor 的情况下建立项目级索引。

> 支持范围：有版本信息、未 Cook 的 UE5 编辑器包（`FileVersionUE5 >= 1000`）。Cooked/无版本包及 UE4 包明确不支持。

## 能力

- **强类型包分析**：包元数据、import/export、带标签属性、对象引用、诊断和字节覆盖率。
- **按图隔离的逻辑模型**：K2/EdGraph 节点始终归属具体图；不会把不同 EventGraph 或函数图中的同名节点拼成虚假链路。
- **专用适配器**：在序列化证据充分时分析 K2/EdGraph、RigVM/ControlRig model links、StateTree 的 state/task/condition/transition、PCG 节点/pin/edge，以及 Niagara 编辑器图。
- **项目级索引**：单次扫描建立资产清单、前向/反向引用邻接表和 World Partition 外部包归属闭包。
- **显式表达不确定性**：报告包含 schema 版本、总体状态、机器可读 coverage、diagnostics 和 capability 证据；不支持或有意保持 opaque 的区域不会伪装成成功解码。
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

从源码构建 0.9 workspace 需要 Rust 1.88 或更高版本：

```bash
git clone https://github.com/cyber-tao/cc-uax.git
cd cc-uax
cargo build -p cc-uax-cli --release --locked
```

二进制位于 `target/release/cc-uax[.exe]`。也可以从 checkout 安装：

```bash
cargo install --path crates/cc-uax-cli --locked
```

## CLI

0.9 CLI 使用两个明确的工作流。

### 分析单个资产

```text
cc-uax asset <FILE> --view <summary|logic|properties|references|full>
```

```powershell
# 资产身份、状态、coverage 和 capabilities
cc-uax asset Content/Blueprints/BP_Player.uasset --view summary

# 图、节点、exec/data edge、成员引用和 pin 默认值
cc-uax asset Content/Blueprints/BP_Player.uasset --view logic

# 完整强类型报告
cc-uax asset Content/Blueprints/BP_Player.uasset --view full --output BP_Player.json
```

### 分析项目

```text
cc-uax project <PROJECT_OR_CONTENT_DIR>
  [--focus <PACKAGE_OR_GLOB>]
  [--mount <PACKAGE_PREFIX=RELATIVE_DIR>]...
  [--allow-partial]
  [--cache-file <FILE> | --no-cache]
```

```powershell
# 对 .uproject 目录或 Content 目录执行一次扫描
cc-uax project D:/Games/MyGame --output project-report.json

# 复用同一项目索引，并为匹配包附加完整分析
cc-uax project D:/Games/MyGame --focus "/Game/Blueprints/**"

# 添加显式 package mount
cc-uax project D:/Games/MyGame --mount "/Plugin=Plugins/MyPlugin/Content"
```

项目分析默认采用 **strict** 模式。任何已映射资产读取、索引或解析失败都会生成结构化 failure；只要请求的项目证据仍为 `partial` 或 `unsupported`，进程也会以非零状态退出。`--allow-partial` 只改变进程是否接受该结果，不会粉饰报告；真实 status、失败项和降低后的 coverage 都会保留。

项目缓存默认放在操作系统缓存目录，不写入被分析项目。使用 `--cache-file` 指定位置，或用 `--no-cache` 完全禁用缓存。

输出格式选项以 `cc-uax asset --help` 和 `cc-uax project --help` 为准。

## 报告契约

解析层内部使用强类型结果，只在 CLI 边界渲染 JSON。资产报告直接包含 `coverage`、`capabilities` 和 `diagnostics`；项目报告通过聚合 `analysis`、inventory 中的紧凑分析以及可选的完整 `focused` 分析提供同类证据：

```jsonc
{
  "schema_version": 1,
  "status": "complete",
  "coverage_or_analysis": {
    /* 请求、已解码、opaque、不支持和失败的证据 */
  },
  "capabilities": [
    /* 各能力的证据与限制 */
  ],
  "diagnostics": [],
  /* 其余强类型资产 view 或项目索引字段 */
}
```

状态语义：

| 状态 | 含义 |
|---|---|
| `complete` | 当前 view 所要求的证据全部解码，且没有未解决缺口。 |
| `partial` | 报告仍可用，但至少一个请求区域失败、保持 opaque 或无法连接。 |
| `unsupported` | 当前包/版本无法提供请求的能力。 |

`known_opaque` 是明确的能力结果，不等于成功。典型例子包括尚不能表示为源码级逻辑的 RigVM 编译字节码和压缩 RigHierarchy。只要请求的能力存在此类缺口，报告就不能升级为 `complete`。

核心公开类型包括 `PackageView<'a>`、`AssetAnalysis`、`DecodedValue`、`LogicGraph`、`GraphNode`、`GraphEdge` 和 `ParseCoverage`。`PackageView<'a>` 将解析和解码绑定到同一份字节，避免调用方用 A 文件解析结果解码 B 文件。

## 架构

仓库是包含三个职责层的虚拟 Cargo workspace：

```text
cc-uax/
├── crates/
│   ├── cc-uax-core/       # 绑定字节的包解析、强类型值、图和 coverage
│   ├── cc-uax-project/    # 项目发现、清单、邻接、归属和缓存策略
│   └── cc-uax-cli/        # asset/project 命令与 JSON 渲染
├── validation/
│   └── stackobot/         # 相对路径 manifest 和外部语料 harness
├── docs/
│   └── validation.md      # 语料契约和验收门禁
└── skills/
    └── cc-uax/            # 完整 Claude Code/Codex skill 包
```

依赖方向保持单向：

```text
cc-uax-cli ──> cc-uax-project ──> cc-uax-core
      └────────────────────────> cc-uax-core
```

- `cc-uax-core` 不负责文件系统扫描、SQLite、CLI 参数或 JSON 呈现策略。
- `cc-uax-project` 负责 mount、项目发现、共享清单扫描、引用邻接、World Partition 归属和缓存位置。
- `cc-uax-cli` 负责选择 view/focus、附加请求的完整资产分析、退出语义和强类型报告渲染。

贡献者应同时阅读 [CLAUDE.md](CLAUDE.md) 中的解析约束。

## Agent Skill

请复制完整的 [`skills/cc-uax/`](skills/cc-uax/) 目录，而不是只复制 `SKILL.md`：

| Agent | 用户级目录 | 项目级目录 |
|---|---|---|
| Claude Code | `~/.claude/skills/cc-uax/` | `<repo>/.claude/skills/cc-uax/` |
| Codex | `~/.codex/skills/cc-uax/` | `<repo>/.codex/skills/cc-uax/` |
| Agents 兼容客户端 | `~/.agents/skills/cc-uax/` | `<repo>/.agents/skills/cc-uax/` |

`agents/` 和 `references/` 是 skill 契约的一部分。

## 验证与支持边界

序列化判断以 UE5.7 源码为依据，解析器使用外部真实编辑器资产验证。精确语料清单、预期语义证据和发布门禁集中记录在 [docs/validation.md](docs/validation.md)。

StackOBot 语料和 Unreal Engine 源码都是外部输入；仓库不会提交其二进制资产或机器相关绝对路径。验证 harness 是发布门禁，但本 README 不会把尚待执行的验收目标写成当前 checkout 已通过的事实。

当前限制包括：

- Cooked/无版本包和 UE4 包格式；
- RigVM 编译字节码、压缩 RigHierarchy 的源码级还原；
- 无法由序列化图、属性、配置或引用证明的运行时行为；
- 尚未核对 UE5.7 序列化契约的插件原生格式。

证据不完整时，下游结论必须保留 `partial`、`unsupported`、diagnostics 和 capability 限制。

## 贡献

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked
cargo test --workspace --locked
cargo build --workspace --release --locked
```

真实语料验收是独立的必需门禁，详见 [docs/validation.md](docs/validation.md)。

## 许可

[MIT](LICENSE)
