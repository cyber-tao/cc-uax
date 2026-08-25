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

## 为什么需要 cc-uax？

Unreal 项目的大量逻辑和数据位于二进制 `.uasset`、`.umap` 包中。以源码为中心的 Agent 可以阅读 C++ 和配置，却无法直接检查蓝图执行流、序列化属性、资产依赖、PCG 图、StateTree 或 World Partition 外部包。

`cc-uax` 将受支持的 UE5 编辑器包转换为带类型和证据的报告。它既能分析单个资产，也能在不启动 Unreal Editor 的情况下建立项目级索引。

> 支持范围：有版本信息、未 Cook 的 UE5.0–5.8 编辑器包（`FileVersionUE5` 1000–1018）。该范围已经过真实语料验证，证据完整时可以为 `status=complete`。范围之外的包会被直接拒绝而不是猜着解析：低于 1000（UE4 及更早）、高于 1018（本解析器未见过的布局）、以及 cooked/无版本包。`cc-uax project` 仍会把它们记为 `unsupported` 证据。

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

从源码构建 workspace 需要 Rust 1.88 或更高版本：

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

CLI 使用两个明确的工作流。

### 分析单个资产

```text
cc-uax asset <FILE> [--view summary|logic|properties|references|full]
```

`--view` 默认为 `full`。

```powershell
# 资产身份、状态、coverage 和 capabilities
cc-uax asset Content/Blueprints/BP_Player.uasset --view summary

# 图、节点、exec/data edge、成员引用和 pin 默认值
cc-uax asset Content/Blueprints/BP_Player.uasset --view logic

# 带标签属性和类默认值
cc-uax asset Content/Blueprints/BP_Player.uasset --view properties

# 该文件的 import 与 soft path（仅出边）
cc-uax asset Content/Blueprints/BP_Player.uasset --view references

# 完整强类型报告
cc-uax asset Content/Blueprints/BP_Player.uasset --view full --output BP_Player.json
```

### 分析项目

```text
cc-uax project <PROJECT_OR_CONTENT_DIR>
  [--focus <PACKAGE_OR_GLOB>]...
  [--mount <PACKAGE_PREFIX=RELATIVE_DIR>]...
  [--allow-partial]
  [--cache-file <FILE> | --no-cache]
```

```powershell
# 对 .uproject 目录或 Content 目录执行一次扫描。
# 同一 Content 树下有多个 .uproject 时，显式传入其中一个文件。
cc-uax project D:/Games/MyGame --output project-report.json
cc-uax project D:/Games/MyGame/MyGame.uproject --output project-report.json

# 复用同一项目索引，并为匹配包附加完整分析
cc-uax project D:/Games/MyGame --focus "/Game/Blueprints/**"

# 添加显式 package mount
cc-uax project D:/Games/MyGame --mount "/Plugin=Plugins/MyPlugin/Content"
```

默认挂载是 `/Game` 加上 `Plugins/` 下每一个插件的 content root，挂载名与 Unreal 一致：取自 `.uplugin` 文件的基名，而这个名字常常不等于插件目录名。没有它们，插件里的包对 inventory、邻接和可达性完全不可见。`--mount` 用于追加其他 content root，或重定向其中一个。

Configured root 同时来自 `GameMapsSettings` 和 `ProjectPackagingSettings` 的 cook 列表（`+MapsToCook`、`+DirectoriesToAlwaysCook`）——`GameDefaultMap` 经常是开发者地图，而 cook 列表才是真正会打包发布的内容。

项目分析默认采用 **strict** 模式。任何已映射资产读取、索引或解析失败都会生成结构化 failure 并以退出码 `2` 结束。本工具按设计不处理的包——UE4 包，以及 cooked、unversioned、UE3、大端或使用包级压缩的包——不算失败：它们会作为 `unsupported` 证据进入 inventory，进程仍以 `0` 退出。`--allow-partial` 只是把 hard scan failure 降级为零退出，不会粉饰报告；真实 status、失败项和降低后的 coverage 都会保留。退出码 `1` 表示根本没能产出报告（资产不可读、项目无法发现、`--mount` 语法错误、写出失败）。

项目缓存默认放在操作系统缓存目录，不写入被分析项目。对未变化的包，fresh cache entry 会复用已验证的引用列表和紧凑逐资产分析摘要。使用 `--cache-file` 指定位置，或用 `--no-cache` 完全禁用缓存。

两个命令共用的全局参数：`--compact`、`--max-output-bytes <BYTES>`、`-o`/`--output <FILE>`。完整参数以 `cc-uax asset --help` 和 `cc-uax project --help` 为准。

当调用方（如 AI 工具）上下文窗口有限时，可用 `--max-output-bytes <N>` 把渲染出的 JSON 限制在 N 个 UTF-8 字节内。输出仍是合法 JSON，保留顶层证据骨架，并在顶层新增 `output` 块记录 `truncated` 及每个被省略的区段，便于据此改用更窄的 `--focus`/`--view` 复查被丢弃的细节。`output.truncated=true` 只表示渲染预算，不表示证据不完整。具体保留哪些字段、两条已知限制以及退出码契约见 [report-contract.md](skills/cc-uax/references/report-contract.md)。

常见任务的逐步走法见 [使用教程](#使用教程)。

## 使用教程

下列参数在 Windows、macOS 和 Linux 上相同。示例使用 PowerShell；把 `D:/Games/MyGame` 换成你的项目路径。

### 1. 先做一次项目扫描

需要引用关系或 reachability 时，不要对每个蓝图单独跑一遍 `cc-uax asset`。先扫整个项目，再下钻。

```powershell
cc-uax project D:/Games/MyGame --output project-report.json
```

按这个顺序读字段：

1. `status`、`stats`、`failures` — 这次扫描本身有没有撑住。
2. `entry_points` — `GameDefaultMap`、`GameInstanceClass`、`GlobalDefaultGameMode` 等配置根。
3. `reachability.configured_roots` 和 `reachability.reachable_runtime_packages` — 这些根实际能到达什么。
4. `analysis` — complete / partial / unsupported / failed 计数。
5. `forward` / `reverse` — 整次扫描的跨包邻接。

缺省字段表示默认值（空 / false / 零），不是扫描没做完。字段级含义见 [report-contract.md](skills/cc-uax/references/report-contract.md)。

同一 Content 树下有多个 `.uproject` 时，显式传入其中一个文件：

```powershell
cc-uax project D:/Games/MyGame/MyGame.uproject --output project-report.json
```

### 2. 选择资产 view

`--view` 默认为 `full`，体积很大。选能回答问题的最小 view。

| View | 适用场景 | 主要区段 |
|---|---|---|
| `summary` | 身份、版本、coverage、capabilities | `summary`、`coverage`、`capabilities`、`diagnostics` |
| `logic` | 蓝图 / Niagara / RigVM / StateTree / PCG 图 | `graphs`、`rigvm_graphs`、`state_tree_graphs`、`pcg_graphs` |
| `properties` | 带标签属性和类默认值 | `exports[].properties` |
| `references` | 该文件的 import 与 soft path | `references`、`imports` |
| `full` | 单个体积可控的资产，含 export `serialization` | 以上全部 |

```powershell
cc-uax asset Content/Blueprints/BP_Player.uasset --view summary
cc-uax asset Content/Blueprints/BP_Player.uasset --view logic --output BP_Player.logic.json
cc-uax asset Content/Blueprints/BP_Player.uasset --view properties --output BP_Player.props.json
cc-uax asset Content/Blueprints/BP_Player.uasset --view references
```

### 3. 读蓝图执行流

```powershell
cc-uax asset Content/Blueprints/BP_Player.uasset --view logic --output BP_Player.logic.json
```

然后：

1. 先看 `status` 和 `coverage`（`graph_nodes_decoded`、`graph_edges_decoded`、`pins_decoded`）。`partial` 报告仍可用，不要补造缺失的边。
2. 把 `graphs` 里的每一项当成独立的图。身份是 `full_name` / 所属 export，不是显示用的 `name`。不要因为 EventGraph 和某个函数图都有 `BeginPlay` 就把它们接在一起。
3. `edges` 里 `kind=exec` 是控制流；`kind=data`（或 pin 的 `default_value` / `default_object`）是数值、调用目标和 spawn class。
4. 图内连通性看 `edges`。pin 的 `linked_to` 只表示跨图或未解析的连接。
5. `nodes[].member` 是节点调用的函数、事件或变量。自定义事件/函数看 `user_defined_pins`。
6. Control Rig 以 `rigvm_graphs` 和 `links`（`source_pin_path` → `target_pin_path`）为准，不要再把 `graphs` 里的编辑器镜像算一遍。编译后的 VM 字节码保持 `known_opaque`。
7. StateTree / PCG 用 `state_tree_graphs` / `pcg_graphs`。节点的完整带标签属性在对应 `exports[]` 项上，按 `index` 对齐。

还需要类默认值或变量的序列化值时，对同一文件再跑 `--view properties`。只有资产足够小时才用 `--view full`。

### 4. 查谁引用了某个资产

单个文件的出边：

```powershell
cc-uax asset Content/Blueprints/BP_Player.uasset --view references
```

入边（“谁在用它”）需要项目索引。做完一次 `project` 扫描后，用规范包路径查 `reverse`：

```text
reverse["/Game/Blueprints/BP_Player"]  →  引用它的包
forward["/Game/Blueprints/BP_Player"] →  它引用的包
```

`reachability.isolated_project_assets` 和 `reachability.unreachable_project_assets` 只是已扫描 mount 下的图事实，不能当成可以安全删除的证明。软加载、Primary Asset 规则、本地化和运行时拼出来的名字都不在这张图里。

### 5. 从启动地图追踪玩法

```powershell
cc-uax project D:/Games/MyGame --output project-report.json --focus "/Game/Maps/L_Startup" --focus "/Game/Blueprints/BP_GameMode"
```

1. 解析 `entry_points.defaults`（`GameDefaultMap`、`GameInstanceClass`、`GlobalDefaultGameMode` 等）。平台覆盖在 `entry_points.platforms`。
2. 沿 `reachability.configured_roots` → `reachable_runtime_packages` 走。
3. 把 `ownership_closure` / `reachability.ownership_closure_members` 里的 World Partition 成员算进去（ExternalActors、ExternalObjects，以及已解码 `WorldAsset` / `PackedWorldAsset` 的 Level Instance / Packed Level Actor 子关卡）。
4. 对地图、GameMode 和需要逐步走的蓝图用 `--focus` 附加完整分析。`--focus` 可重复。

### 6. 用 `--focus` 选择包

`--focus` 按规范包路径匹配，不区分大小写。末尾的 `.uasset` / 对象后缀会被去掉。`?` 匹配一个非分隔符字符，`*` 匹配单个路径段，`**` 可跨越 `/`。

```powershell
# 只选 Blueprints 的直接子项
cc-uax project D:/Games/MyGame --focus "/Game/Blueprints/*"

# 整棵子树
cc-uax project D:/Games/MyGame --focus "/Game/Blueprints/**"

# 一次扫描里选多组
cc-uax project D:/Games/MyGame --focus "/Game/Maps/L_Startup" --focus "/Game/Characters/**"

# 单个包，带不带资产后缀都可以
cc-uax project D:/Games/MyGame --focus "/Game/Blueprints/BP_Player.uasset"
```

一个模式一个都匹配不到，算 hard failure（退出码 `2`），并记入 `failures`（`stage=focus`）。报告仍会写出。匹配到但不在支持范围内的包在 `inventory` 里保持 `unsupported`，不算 focus failure。

### 7. 纳入插件或其他 Content

`Plugins/` 下的插件 content 会自动挂载，挂载名 `/{name}` 取自各自 `.uplugin` 文件的基名。这个名字常常不是目录名——目录叫 `MetaXR` 但内含 `OculusXR.uplugin` 时挂载为 `/OculusXR`——所以请看报告里的 `mounts`，不要猜。

`Plugins/` 之外的内容根，或需要重定向某个已发现的根时，用 `--mount`：

```powershell
cc-uax project D:/Games/MyGame `
  --mount "/Extra=ExtraContent" `
  --output project-report.json
```

`=` 后面的路径相对项目根；显式 mount 是在自动发现的集合上追加（指定同名根时只替换那一个）。`--mount` 语法错误会以退出码 `1` 结束，且不会产出报告。

### 8. 把报告限制在上下文窗口内

```powershell
cc-uax project D:/Games/MyGame --focus "/Game/Blueprints/BP_Player" --compact --max-output-bytes 200000 --output bp.json
cc-uax asset Content/Blueprints/BP_Player.uasset --view logic --max-output-bytes 80000
```

`--compact` 去掉 pretty-print 空白。`--max-output-bytes` 会省略较重的细节，并在顶层增加 `output` 块（`truncated`、`elided` 等）。`output.truncated=true` 只表示体积被截断，不表示证据不完整 — 用更窄的 `--view` 或 `--focus` 把丢掉的区段再查回来。始终保留哪些字段见 [report-contract.md](skills/cc-uax/references/report-contract.md)。

### 9. 把 status、coverage 和退出码一起看

| 现象 | 含义 | 下一步 |
|---|---|---|
| `status=complete`，退出码 `0` | 请求的证据已解码 | 直接使用图 / 引用 |
| `status=partial`，退出码 `0` | 报告可用，但有具名缺口（`known_opaque`、某区域失败等） | 结论里保留缺口，不要补造缺失路径 |
| `status=unsupported`，退出码 `0`（`project`） | 扫描到的每个包都超出支持范围 | 当作限制，不是崩溃 |
| 对 cooked / UE4 / 超范围包执行 `asset` 时退出码 `1` | 本工具按设计不处理它，因此没有报告 | 改扫整个项目：同一个包会作为 `unsupported` 证据进入 inventory |
| 退出码 `2`（`project`） | Hard scan failure：已映射资产不可读、扫描中的 mount/cache 错误，或 `--focus` 未命中 | 读 `failures`；必要时加 `--allow-partial` 重跑 |
| 退出码 `1` | 根本没有报告 | 检查路径、`--mount` 语法或输出位置 |

超出支持范围的包不会产出 `status=unsupported` 的资产报告：`cc-uax asset` 无法为它生成报告，会以退出码 `1` 输出 error 文档；而 `cc-uax project` 会把它记入 `inventory` 并标为 `unsupported` 附带原因，进程仍以 `0` 退出。

`--allow-partial` 只改退出码，不会改写 `status`、`failures` 或 coverage。

### 10. 控制项目缓存

```powershell
# 默认：操作系统缓存目录，不会写进 Unreal 项目
cc-uax project D:/Games/MyGame --output project-report.json

# 指定缓存文件
cc-uax project D:/Games/MyGame --cache-file D:/caches/mygame-uax.sqlite --output project-report.json

# 不用缓存（CI，或刚改过 decoder）
cc-uax project D:/Games/MyGame --no-cache --output project-report.json
```

对未变化的包，fresh cache entry 会复用已验证的引用列表和紧凑逐资产分析摘要。

### 11. 从源码 checkout 运行

`PATH` 上的旧版 `cc-uax` 会静默给出和本仓库不一致的结果。在 clone 里这样跑：

```powershell
cargo run -p cc-uax-cli --release --locked -- project D:/Games/MyGame --output project-report.json
cargo run -p cc-uax-cli --release --locked -- asset Content/Blueprints/BP_Player.uasset --view logic
```

也可以直接调用 `target/release/cc-uax.exe`。

### 12. 在 Claude Code 或 Codex 里用

安装完整的 [`skills/cc-uax/`](skills/cc-uax/) 目录（见 [Agent Skill](#agent-skill)）。先让 Agent 扫项目，再要求它引用 `graphs[].edges`、包路径以及 `coverage` / `diagnostics`，而不是凭节点显示名猜测。

## 报告契约

解析层内部使用强类型结果，只在 CLI 边界渲染 JSON。资产报告直接包含 `coverage`、`capabilities` 和 `diagnostics`；项目报告通过聚合 `analysis`、inventory 中的紧凑分析、生成的 `reachability` 以及可选的完整 `focused` 分析提供同类证据：

**资产报告（缩略）：**

```jsonc
{
  "schema_version": /* see report-contract.md */,
  "status": "complete",
  "view": "full",
  "summary": { /* 包名、文件版本与 custom version、引擎版本、各表计数…… */ },
  "coverage": {
    /* 非零的请求/已解码/opaque/失败计数（零值计数被省略） */
  },
  "capabilities": [
    /* 各能力的证据与限制 */
  ],
  "exports": [ /* … */ ], "graphs": [ /* … */ ]
  /* 稀疏输出：空值或默认字段（null、[]、false、""、"None"、0）被省略。 */
}
```

**项目报告（缩略）：**

```jsonc
{
  "schema_version": /* see report-contract.md */,
  "status": "complete",
  "layout": {}, "mounts": [], "entry_points": {},
  "reachability": {
    /* 配置根、运行时可达包、closure 成员、孤立包和 coverage 缺口 */
  },
  "stats": { /* 全部文件系统/索引/缓存计数，含零值 */ },
  "analysis": {
    /* 聚合 coverage、capabilities 及逐资产摘要 */
  },
  "inventory": [ /* 每个包一条紧凑分析 */ ],
  "focused": { /* 匹配 --focus 的完整 AssetAnalysis */ }
  /* 稀疏输出：空的邻接、failures、diagnostics 和 reachability 集合被省略。 */
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
└── skills/
    └── cc-uax/            # 完整 Claude Code/Codex skill 包
```

依赖方向保持单向：

```text
cc-uax-cli ──> cc-uax-project ──> cc-uax-core
      └────────────────────────> cc-uax-core
```

- `cc-uax-core` 不负责文件系统扫描、SQLite、CLI 参数或 JSON 呈现策略。
- `cc-uax-project` 负责 mount、项目发现、共享清单扫描、引用邻接、World Partition 归属、reachability/resource 摘要和缓存位置。
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

序列化判断对照 UE5.0–5.8 源码核对，并在该范围内用外部真实编辑器资产验证。验收门禁由外部语料 harness 定义，该 harness 独立于 workspace crate 维护，不作为 workspace 成员提交。

外部资产和机器相关的绝对路径均保留在本地，仓库不提交此类内容。

当前限制包括：

- Cooked/无版本包和 UE4 包格式；
- RigVM 编译字节码、压缩 RigHierarchy 的源码级还原；
- 无法由序列化图、属性、配置或引用证明的运行时行为；
- 尚未核对 UE5.0–5.8 序列化契约的插件原生格式。

证据不完整时，下游结论必须保留 `partial`、`unsupported`、diagnostics 和 capability 限制。

## 贡献

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --locked
cargo build --workspace --release --locked
```

## 网站

公开站点是 [`website/`](website/) 下的 VitePress 应用，由 [`.github/workflows/pages.yml`](.github/workflows/pages.yml) 发布到 [https://cyber-tao.github.io/cc-uax/zh/](https://cyber-tao.github.io/cc-uax/zh/)。

```bash
bun install --cwd website
bun run --cwd website dev
```

## 许可

[MIT](LICENSE)
