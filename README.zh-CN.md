<div align="center">

# cc-uax

**从零用 Rust 实现的 Unreal Engine 5 Blueprint（`.uasset`）文件读取器 → JSON**

镜像 `CoreUObject` 序列化逻辑手写解析，不依赖任何第三方 uasset crate。

[![Rust](https://img.shields.io/badge/Rust-2024%20edition-CE422B?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![Release](https://img.shields.io/github/v/release/cyber-tao/cc-uax?logo=github)](https://github.com/cyber-tao/cc-uax/releases)
[![CI](https://img.shields.io/github/actions/workflow/status/cyber-tao/cc-uax/release.yml?branch=main&label=build)](https://github.com/cyber-tao/cc-uax/actions/workflows/release.yml)
[![UE5](https://img.shields.io/badge/Unreal%20Engine-5.7-0E1128?logo=unrealengine&logoColor=white)](https://www.unrealengine.com/)
[![License: MIT](https://img.shields.io/badge/License-MIT-2ea44f?style=flat)](https://opensource.org/licenses/MIT)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-5851DB)](#)
![Status](https://img.shields.io/badge/status-stable%20%201%2C423%20assets%20validated-1F6FEB)

[English](README.md) · **简体中文**

</div>

---

## 📖 关于

`cc-uax` 是一个命令行工具，读取虚幻引擎 5 的 `.uasset`（Blueprint）文件，并将内容导出为结构化的 **JSON**。解析器用 Rust 手写，追踪 UE5.7 源码（`CoreUObject`）而非包装现成库 —— 依赖面极小，二进制完全自包含。

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
  | 原生结构体 | `Vector` / `Vector3f` / `Rotator` / `Quat` / `Color` / `LinearColor` / `Transform` / `Guid` / `DateTime` … |

- **优雅的十六进制回退** —— 带自定义二进制序列化的类型（如 `EdGraphPinType`、Niagara、`AnimNotifyEvent`）输出带 `type` + `size` 标注的十六进制预览，**保证字节对齐不被破坏**。
- **引用图谱**
  - `--references` —— 从 import 表提取前向引用，拆分为 `assets` 与 `scripts`，去重排序。
  - `--scan-dir` —— 反向引用：哪些资产引用了*当前文件*（`referenced_by`），通过 `--mount` 路径映射。
- **增量扫描缓存** —— 基于 SQLite（`.cc-uax-cache.sqlite`），按修改时间 + 大小作键，带实时 stderr 进度条。`--no-cache` 可关闭。

## 🛠️ 技术栈

**语言与运行时**

`Rust (edition 2021)` · `byteorder`（LE 字节流） · `serde` + `serde_json`（输出） · `clap` v4（CLI，derive） · `anyhow`（错误处理） · `rusqlite` 内置 SQLite（扫描缓存，**仅二进制侧**）

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
curl -fsSL https://raw.githubusercontent.com/cyber-tao/cc-uax/main/install.sh | bash
```

**Windows（PowerShell）**

```powershell
irm https://raw.githubusercontent.com/cyber-tao/cc-uax/main/install.ps1 | iex
```

预编译二进制发布在 [Releases](https://github.com/cyber-tao/cc-uax/releases) 页面：

| 平台 | Target |
|---|---|
| Linux x86_64 / aarch64 | `x86_64-unknown-linux-gnu`、`aarch64-unknown-linux-gnu` |
| Windows x86_64 | `x86_64-pc-windows-msvc` |
| macOS x86_64 / Apple Silicon | `x86_64-apple-darwin`、`aarch64-apple-darwin` |

安装脚本支持的环境变量（执行前设置）：`INSTALL_DIR`（二进制位置）、`VERSION`（指定 tag）、`NO_SKILL=1`（跳过 skill 配置）。

### 从源码构建

需要 Rust ≥ 1.85（edition 2024）：

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
  -n, --names           在输出中包含完整 name 表
  -s, --summary-only    仅输出包头摘要
  -P, --no-properties   不解析 export 属性，仅输出结构
  -r, --references      仅列出该文件引用的外部资源
  -d, --scan-dir <DIR>  递归扫描 <DIR>；附带列出谁引用了当前文件（配合 -r）
  -m, --mount <PREFIX>  <DIR> 对应的挂载前缀（默认 /Game）
      --no-cache        关闭反向扫描磁盘缓存
  -h, --help            显示帮助
  -V, --version         显示版本
```

**示例**

```pwsh
# 解析蓝图，美化输出到文件
cc-uax BP_MyActor.uasset -o out.json

# 只看包头信息
cc-uax BP_MyActor.uasset --summary-only

# 前向引用 —— 该资产依赖了哪些包
cc-uax BP_MyActor.uasset --references

# 反向引用 —— 谁引用了我（扫描 Content 目录树，挂载到 /Game）
cc-uax BP_MyActor.uasset --references --scan-dir ./Content
```

## 📋 输出结构

**完整模式**

```jsonc
{
  "summary":  { /* 版本、引擎版本、各表计数、自定义版本、包名 */ },
  "imports":  [ { "index": -1, "class": "...", "name": "...", "full_name": "..." } ],
  "exports":  [
    {
      "index": 1,
      "name": "...",
      "class": "/Script/Engine.Blueprint",
      "super": "...", "outer": "...",
      "serial_offset": 0, "serial_size": 0,
      "properties": [
        { "name": "ParentClass", "type": "ObjectProperty",
          "value": { "ref": "...", "index": -4 } }
      ]
    }
  ],
  "file": "输入文件路径"
}
```

**引用模式**（`self` / `referenced_by` 仅在配合 `--scan-dir` 时出现）

```jsonc
{
  "summary":    { /* 版本 / 引擎 / 各表计数 */ },
  "references": {
    "assets":        [ "/Game/...", "/Engine/..." ],
    "scripts":       [ "/Script/CoreUObject", "/Script/Engine" ],
    "self":          "/Game/Foo/BP_MyActor",
    "referenced_by": [ "/Game/Foo/BP_Other" ]
  },
  "file": "输入文件路径"
}
```

## 🏗️ 架构

```
cc-uax/
├── src/
│   ├── lib.rs          # 库入口 —— 导出 Package、JsonOptions
│   ├── main.rs         # CLI 入口 + 反向扫描 + cache 模块
│   ├── package.rs      # 核心：Package 流水线 + 字节 Reader + Guid/RawName
│   ├── summary.rs      # FPackageFileSummary（魔数、版本、表偏移）
│   ├── name.rs         # NameMap —— Name 表解析与解析
│   ├── object.rs       # PackageIndex（+/- ⇒ export/import）、Import、Export
│   ├── property.rs     # 递归 tagged property 解码器 + 十六进制回退
│   ├── version.rs      # UE5/UE4 文件版本常量 + PACKAGE_FILE_TAG
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

> 完整架构指引见 [CLAUDE.md](CLAUDE.md)。

## ⚠️ 支持范围与限制

- ✅ **已验证** 某 UE5.7 项目的 **1,423 个 `.uasset`** 文件 —— 全部成功解析，每个对象的属性区间字节完全对齐。
- ❌ Cooked 包（unversioned / 包级压缩）与 UE4 旧格式**不支持**。
- 🔧 部分原生二进制结构体暂以十六进制预览呈现，待补结构化解码器。
- 🔧 `referenced_by` 从磁盘推导包路径 —— 输入文件必须位于映射到 `--mount` 的 `--scan-dir` 内。仅统计硬引用（import），不含软引用。
- 🔧 缓存按修改时间 + 大小失效，内置 schema 版本变化时自动重建。

## 🤝 贡献

这是一个聚焦单一用途的工具。如扩展解码器，请在 [tests/units.rs](tests/units.rs) 中添加手写字节向量测试，并确保 export 的属性区间保持字节对齐。提交前运行 `cargo fmt && cargo clippy --all-targets && cargo test`。

## 📄 许可

MIT
