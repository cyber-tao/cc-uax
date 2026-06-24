<div align="center">

# cc-uax

**把 Unreal Engine 5 `.uasset`/`.umap` 资产包解析为 JSON —— 属性、蓝图节点图、资源引用关系。**

单一 CLI：把不透明的 UE5 编辑器资产转成结构化 JSON —— 让 Claude Code 终于能读懂你游戏里的蓝图、属性与资产引用。

[![Rust](https://img.shields.io/badge/Rust-2024%20edition-CE422B?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Release](https://img.shields.io/github/v/release/cyber-tao/cc-uax?logo=github)](https://github.com/cyber-tao/cc-uax/releases)
[![CI](https://img.shields.io/github/actions/workflow/status/cyber-tao/cc-uax/release.yml?branch=master&label=build)](https://github.com/cyber-tao/cc-uax/actions/workflows/release.yml)
[![UE5](https://img.shields.io/badge/Unreal%20Engine-5.7-0E1128?logo=unrealengine&logoColor=white)](https://www.unrealengine.com/)
[![License: MIT](https://img.shields.io/badge/License-MIT-2ea44f?style=flat)](https://opensource.org/licenses/MIT)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-5851DB)](#)
![Status](https://img.shields.io/badge/status-stable%20%201%2C423%20assets%20validated-1F6FEB)

[English](README.md) · **简体中文**

</div>

---

## 📖 关于

我是 UE5 游戏开发者，做 `cc-uax` 这个工具只为了一件事：**让 Claude Code 能读懂虚幻引擎的资产文件。**

一个真实的 UE5 项目，其内容全在不透明的 `.uasset`/`.umap` 二进制里——每一张蓝图图、每一个数据资产、每一个关卡都是一团 AI 编程助手打不开的 blob。它们能写 C++、能改文本，却看不见节点的连线、tagged 属性，也看不见一个 Actor 引用了哪些材质和数据表。`cc-uax` 正是补上这一环：它读取任意 UE5 编辑器资产，把内容导出为结构化的 **JSON** —— 完整包头、tagged 属性、蓝图节点与 pin 连线图，以及前向与反向资源引用——让 agent 能像理解你的代码一样，去理解你游戏里的内容。以单一自包含二进制发布，无运行时依赖。

名字说得很直白：**cc** = Claude Code，**uax** = uasset。它同时还附带一个 [agent skill](#-作为-agent-skill-使用)——配置好后，Claude Code（或 OpenAI Codex）会在你要求检查 `.uasset`/`.umap` 时自动调用 `cc-uax`，无需手读二进制。

> 目标范围：UE5（`FileVersionUE5 >= 1000`）的 **versioned、未 cooked 的编辑器资产**。Cooked / unversioned 包与 UE4 旧格式明确不在支持范围内。

## ✨ 功能

- **完整包头** —— `FPackageFileSummary`、Name 表、Import 与 Export 映射、自定义版本。
- **Versioned tagged property** —— UE5.7 新式 `FPropertyTag` + 完整 `FPropertyTypeName`。
- **精确属性区间** —— 通过 `ScriptSerialization` 范围定位每个对象的数据，正确消费 `UClass` / `UBlueprint` 头部控制字节。
- **丰富的值类型解码**

  | 类别 | 类型 |
  |---|---|
  | 基础类型 | 数值、`bool`、枚举、字符串、`FName`、`FText` |
  | 引用 | `ObjectProperty` → 全名 + 包索引、`SoftObjectPath`、`FieldPath` |
  | 容器 | `ArrayProperty`、`SetProperty`、`MapProperty` |
  | 嵌套 | 递归 tagged 结构体 |
  | 原生结构体 | `Vector` / `Vector3f` / `Rotator` / `Quat` / `Color` / `LinearColor` / `Transform` / `Transform3f` / `Box` / `Box2D` / `Guid` / `DateTime` / `FrameNumber` / `FrameRate` / `IntVector2` / `IntVector4` / `RichCurveKey` … |
  | 材质输入 | `ExpressionInput` + Scalar / Vector / Vector2 / Color / ShadingModel / Substrate / MaterialAttributes |
  | 序列器与曲线 | `FrameRange`、`FloatChannel`、`DoubleChannel`、per-platform Float / Int / Bool / FrameRate |
  | 运行时结构体 | `InstancedStruct`、`PerQualityLevelInt` / `Float`、委托（`Delegate` / `MulticastInline` / `MulticastSparse`）、`EdGraphPinType` |

- **蓝图图逻辑** —— 紧随 tagged property 区间之后解码 `UEdGraphNode` 的 pin：每个节点的 pins、pin 类型、默认值/默认对象，以及 `LinkedTo` 连线，从而可重建完整的节点间执行与数据流图。图节点还会蒸馏出 `member`（其引用的函数 / 事件 / 变量）与 `member_from`（所属 C++ 类），便于与源码交叉对照。
- **可选输出区块** —— `--sections`（别名 `-S`）按需组合要输出的区块，或直接选预设（`logic`、`debug`、`full`）—— 让逻辑分析精简、查 BUG 全面。
- **优雅的十六进制回退** —— 暂未结构化、带自定义二进制序列化的类型（如 Niagara 节点）输出带 `type` + `size` 标注的十六进制预览，**保证字节对齐不被破坏**。
- **引用图谱**
  - `-S refs` —— 从 import 表提取前向引用，拆分为 `assets` 与 `scripts`，去重排序。
  - `--scan-dir` —— 反向引用：哪些资产引用了*当前文件*（`referenced_by`），通过 `--mount` 路径映射。
- **增量扫描缓存** —— 基于 SQLite（`.cc-uax-cache.sqlite`），按修改时间 + 大小作键，带实时 stderr 进度条。`--no-cache` 可关闭。

## 🛠️ 技术栈

**语言与运行时**

`Rust (edition 2024)` · `byteorder`（LE 字节流） · `serde` + `serde_json`（输出） · `clap` v4（CLI，derive） · `anyhow`（错误处理） · `rusqlite` 内置 SQLite（扫描缓存，**仅二进制侧**）

| 层 | 职责 | 依赖 |
|---|---|---|
| 解析器（`lib`） | 包头、Name、Import/Export、tagged property | `byteorder`、`serde`、`serde_json`、`anyhow` |
| CLI（`bin`） | 参数、输出整形、反向扫描、缓存 | `clap`、`rusqlite`（+ 解析器） |

> 解析器 crate **刻意不依赖** `rusqlite` —— 反向扫描缓存只存在于二进制侧。

## 📦 安装

### 一键安装（推荐）

自动下载当前平台最新的预编译二进制，把 `cc-uax` 装到 `PATH`，并为 Claude Code 与 Codex 同时配置好 [agent skill](#-作为-agent-skill 使用)。

**Linux / macOS**

```bash
curl -fsSL https://raw.githubusercontent.com/cyber-tao/cc-uax/master/install.sh | bash
```

**Windows（PowerShell）**

```powershell
irm https://raw.githubusercontent.com/cyber-tao/cc-uax/master/install.ps1 | iex
```

预编译二进制发布在 [Releases](https://github.com/cyber-tao/cc-uax/releases) 页面：

| 平台 | Target |
|---|---|
| Linux x86_64 / aarch64 | `x86_64-unknown-linux-gnu`、`aarch64-unknown-linux-gnu` |
| Windows x86_64 | `x86_64-pc-windows-msvc` |
| macOS x86_64 / Apple Silicon | `x86_64-apple-darwin`、`aarch64-apple-darwin` |

安装脚本支持的环境变量（执行前设置）：`INSTALL_DIR`（二进制位置）、`VERSION`（指定 tag）、`NO_SKILL=1`（跳过 skill 配置）。

### 从源码构建

需要 Rust ≥ 1.88（edition 2024，使用 let-chains）：

```bash
git clone https://github.com/cyber-tao/cc-uax.git
cd cc-uax
cargo build --release    # 产物在 target/release/cc-uax[.exe]
```

或直接安装到 `~/.cargo/bin`：`cargo install --path .`。无运行时依赖；SQLite 静态链接。

## 🤖 作为 Agent Skill 使用

`cc-uax` 同时附带一个遵循开放 agent-skills 标准的 [agent skill](skills/cc-uax/SKILL.md) —— **同一份 `SKILL.md` 在 Claude Code 和 OpenAI Codex 中都能用**。配置完成后，当你要求任一 agent 检查 `.uasset`/`.umap` 或追踪资产引用时，它会自动调用 `cc-uax`，无需手读二进制。

一键安装脚本会同时为两个 agent 配置好 skill。如需手动配置，把 [skills/cc-uax/](skills/cc-uax/) 复制到：

| Agent | 用户级位置 | 项目级位置 |
|---|---|---|
| Claude Code | `~/.claude/skills/cc-uax/` | `<repo>/.claude/skills/cc-uax/` |
| Codex | `~/.agents/skills/cc-uax/` | `<repo>/.agents/skills/cc-uax/` |

> skill 就是一个带 `SKILL.md`（YAML frontmatter 含 `name`、`description`）的目录。放进项目级路径并提交，团队每个成员都能自动获得该 skill。

## 🚀 用法

```text
cc-uax <input.uasset> [选项]

  -o, --output <FILE>   输出 JSON 到文件（默认：标准输出）
  -c, --compact         紧凑 JSON（默认：美化）
  -S, --sections <LIST> 要输出的区块（逗号分隔）或预设（见“输出区块”）
  -d, --scan-dir <DIR>  递归扫描 <DIR>；附带列出谁引用了当前文件（配合 -S refs）
  -m, --mount <PREFIX>  <DIR> 对应的挂载前缀（默认 /Game）
      --no-cache        关闭反向扫描磁盘缓存
  -h, --help            显示帮助
  -V, --version         显示版本
```

**示例**

```pwsh
# 解析蓝图，美化输出到文件
cc-uax BP_MyActor.uasset -o out.json

# 只看图逻辑 —— 节点 + pin 连线（框架分析的精简视图）
cc-uax BP_MyActor.uasset -S logic

# 查 BUG 视图 —— 包头 + imports + 完整属性 + 字节布局
cc-uax BP_MyActor.uasset -S debug

# 只看包头信息
cc-uax BP_MyActor.uasset -S summary

# 前向引用 —— 该资产依赖了哪些包
cc-uax BP_MyActor.uasset -S refs

# 反向引用 —— 谁引用了我（扫描 Content 目录树，挂载到 /Game）
cc-uax BP_MyActor.uasset -S refs --scan-dir ./Content
```

## 📋 输出结构

**完整模式**

```jsonc
{
  "summary":  { /* 版本、引擎版本、各表计数、自定义版本、包名 */ },
  "imports":  [ { "index": -1, "class": "...", "name": "...", "full_name": "..." } ],
  "exports":  [
    {
      "index": 15,
      "name": "K2Node_CallFunction_14",
      "class": "/Script/BlueprintGraph.K2Node_CallFunction",
      "member": "SetMaterial",                       // 蒸馏出的节点身份
      "member_from": { "ref": "/Script/Engine.PrimitiveComponent", "index": -19 },
      "properties": [ /* tagged property —— -S logic 时省略 */ ],
      "pins": [
        { "name": "execute", "direction": "input", "category": "exec",
          "linked_to": [ { "node": "...K2Node_Knot_7", "node_index": 25, "pin": "OutputPin" } ] },
        { "name": "Material", "direction": "input", "category": "object",
          "default_object": { "ref": "/Game/.../MI_Box_Destroyed", "index": -45 } }
      ]
    }
  ],
  "file": "输入文件路径"
}
```

> `member` / `member_from` 与 `pins` 仅出现在图节点导出（`K2Node_*`、`EdGraphNode_*`）。底层的 `super` / `outer` / `serial_offset` / `object_flags` / `script_serialization_*` 字段归入 `layout` 区块（`-S layout`，或任何包含它的预设）。

**引用模式**（`self` / `referenced_by` 仅在配合 `--scan-dir` 时出现）

```jsonc
{
  "references": {
    "assets":        [ "/Game/...", "/Engine/..." ],
    "scripts":       [ "/Script/CoreUObject", "/Script/Engine" ],
    "self":          "/Game/Foo/BP_MyActor",
    "referenced_by": [ "/Game/Foo/BP_Other" ]
  },
  "file": "输入文件路径"
}
```

如果希望引用输出同时包含包头字段，请显式使用 `-S summary,refs`。

### 输出区块

`--sections <LIST>`（别名 `-S`）选择输出哪些区块；用逗号分隔，可混用区块键与预设。省略时默认 `full`。

| 预设 | 展开为 | 适用 |
|---|---|---|
| `logic` | `summary` + exports（identity + `member` + `pins`） | 对照 C++ 的图 / 框架分析 |
| `debug` | `summary` + `imports` + exports（`properties` + `layout`） | 查 BUG / 序列化核对 |
| `full`  | `summary` + `imports` + exports（`pins` + `properties` + `layout`）— 默认；除非显式请求，否则不包含 `names` 和 `references` | 完整 export 导出 |

区块键（可组合，如 `-S exports,pins,properties` 或 `-S full,names`）：`summary`、`imports`、`exports`（identity 基础）、`pins`、`properties`、`layout`（serial 偏移 / flags / script 窗口）、`names`、`references`（别名 `refs`）。

只要单个区块直接写名字即可：`-S summary`（仅包头），或 `-S refs`（前向引用；加 `--scan-dir` 得反向引用）。

## 🏗️ 架构

```
cc-uax/
├── src/
│   ├── lib.rs          # 库入口 —— 导出 Package、OutputSections
│   ├── main.rs         # CLI 入口 + 反向扫描 + cache 模块
│   ├── package.rs      # 核心：Package 流水线 + JSON 输出（区块）+ pin 编排
│   ├── summary.rs      # FPackageFileSummary（魔数、版本、表偏移）
│   ├── name.rs         # NameMap —— Name 表解析与解析
│   ├── object.rs       # PackageIndex（+/- ⇒ export/import）、Import、Export
│   ├── property.rs     # 递归 tagged property 解码器 + 十六进制回退
│   ├── pin.rs          # EdGraphNode pin 解码器 —— pins、pin 类型、LinkedTo 连线
│   ├── version.rs      # UE5/UE4 文件版本常量 + 自定义版本 GUID
│   ├── reader.rs       # 小端字节流读取原语
│   └── cache.rs        # SQLite 反向引用缓存（仅二进制侧）
├── tests/
│   └── units.rs        # 手写字节向量集成测试
├── skills/
│   └── cc-uax/
│       └── SKILL.md    # Agent skill（Claude Code + Codex 兼容）
├── .github/workflows/
│   └── release.yml     # 多平台构建 + tag 触发 GitHub Release
├── install.sh          # 一键安装（Linux / macOS）
├── install.ps1         # 一键安装（Windows）
├── dev-install.sh      # 开发：从源码重编译 + 刷新 skill（Linux / macOS）
├── dev-install.ps1     # 开发：从源码重编译 + 刷新 skill（Windows）
├── Cargo.toml          # lib + bin 双 target
├── CLAUDE.md           # 给 Claude Code 的架构指引
└── README.md
```

**解析流水线**（由 `Package::parse` 编排，每个阶段为下一阶段提供偏移）：

1. `Reader` —— LE 原语（`u8..u64`、`f32`/`f64`、`FString`、`FName`、`Guid`）。
2. `PackageFileSummary::parse` —— 校验 `PACKAGE_FILE_TAG`（`0x9E2A83C1`），检测字节序，读取版本与表偏移。
3. `NameMap::parse` —— 解析 Name，含数字后缀（`Foo_3`）。
4. Import / Export 表 —— `PackageIndex` 正负号选择表。
5. 每个 export 的 `ScriptSerialization` 窗口 → `property.rs` 递归解码；未知结构体回退到十六进制，对齐永不破坏。
6. 图节点 —— 属性窗口之后，`pin.rs` 解码 `UEdGraphNode` 的 pin 区域（`pins` + `LinkedTo`），并把节点身份蒸馏为 `member` / `member_from`。

> 完整架构指引见 [CLAUDE.md](CLAUDE.md)。

## ⚠️ 支持范围与限制

- ✅ **已验证** 某 UE5.7 项目的 **1,423 个 `.uasset`** 文件 —— 全部成功解析，每个对象的属性区间字节完全对齐。
- ❌ Cooked 包（unversioned / 包级压缩）与 UE4 旧格式**不支持**。
- 🔧 多数原生二进制结构体已结构化解码；少数（如 Niagara）仍以十六进制预览呈现，待补解码器。
- 🔧 `referenced_by` 从磁盘推导包路径 —— 输入文件必须位于映射到 `--mount` 的 `--scan-dir` 内。仅统计硬引用（import），不含软引用。
- 🔧 缓存按修改时间 + 大小失效，内置 schema 版本变化时自动重建。

## 🤝 贡献

这是一个聚焦单一用途的工具。如扩展解码器，请在 [tests/units.rs](tests/units.rs) 中添加手写字节向量测试，并确保 export 的属性区间保持字节对齐。提交前运行 `cargo fmt && cargo clippy --all-targets && cargo test`。

## 📄 许可

MIT
