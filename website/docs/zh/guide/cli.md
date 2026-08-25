---
title: CLI
description: cc-uax 的两个工作流——分析单个资产，或扫描整个项目。
---

# CLI

CLI 使用两个明确的工作流。两个命令共用的全局参数：`--compact`、`--max-output-bytes <BYTES>`、`-o` / `--output <FILE>`。完整参数以 `cc-uax asset --help` 和 `cc-uax project --help` 为准。

当调用方上下文窗口有限时，用 `--max-output-bytes <N>` 把渲染出的 JSON 限制在 N 个 UTF-8 字节内。输出仍是合法 JSON，保留顶层证据骨架，并在顶层 `output` 块记录 `truncated` 及每个被省略的区段。`output.truncated=true` 只表示渲染预算，不表示证据不完整。

字段级含义、两条已知预算限制以及退出码契约见 [`report-contract.md`](https://github.com/cyber-tao/cc-uax/blob/master/skills/cc-uax/references/report-contract.md)。

## 分析单个资产

```text
cc-uax asset <FILE> [--view summary|logic|properties|references|full]
```

`--view` 默认为 `full`。选能回答问题的最小 view。

| View | 适用场景 | 主要区段 |
|---|---|---|
| `summary` | 身份、版本、coverage、capabilities | `summary`、`coverage`、`capabilities`、`diagnostics` |
| `logic` | 蓝图 / Niagara / RigVM / StateTree / PCG 图 | `graphs`、`rigvm_graphs`、`state_tree_graphs`、`pcg_graphs` |
| `properties` | 带标签属性和类默认值 | `exports[].properties` |
| `references` | 该文件的 import 与 soft path | `references`、`imports` |
| `full` | 单个体积可控的资产，含 export `serialization` | 以上全部 |

```powershell
cc-uax asset Content/Blueprints/BP_Player.uasset --view summary
cc-uax asset Content/Blueprints/BP_Player.uasset --view logic
cc-uax asset Content/Blueprints/BP_Player.uasset --view properties
cc-uax asset Content/Blueprints/BP_Player.uasset --view references
cc-uax asset Content/Blueprints/BP_Player.uasset --view full --output BP_Player.json
```

## 分析项目

```text
cc-uax project <PROJECT_OR_CONTENT_DIR>
  [--focus <PACKAGE_OR_GLOB>]...
  [--mount <PACKAGE_PREFIX=RELATIVE_DIR>]...
  [--allow-partial]
  [--cache-file <FILE> | --no-cache]
```

```powershell
cc-uax project D:/Games/MyGame --output project-report.json
cc-uax project D:/Games/MyGame/MyGame.uproject --output project-report.json
cc-uax project D:/Games/MyGame --focus "/Game/Blueprints/**"
cc-uax project D:/Games/MyGame --mount "/Plugin=Plugins/MyPlugin/Content"
```

显式传入 `.uproject` 文件时，即使同一 Content 树下有多个平台 `.uproject`，也会选中该文件。对目录或 Content 路径，若存在多个 `.uproject` 仍会报错。

### Mounts

默认挂载是 `/Game` 加上 `Plugins/` 下每一个插件的 content root，挂载名与 Unreal 一致：取自 `.uplugin` 文件的基名，而这个名字常常不等于插件目录名。没有它们，插件里的包对 inventory、邻接和可达性完全不可见。

`--mount` 用于追加其他 content root，或重定向其中一个。`=` 后面的路径相对项目根。`--mount` 语法错误会以退出码 `1` 结束，且不会产出报告。

### Configured roots

Configured root 同时来自 `GameMapsSettings` 和 `ProjectPackagingSettings` 的 cook 列表（`+MapsToCook`、`+DirectoriesToAlwaysCook`）。`GameDefaultMap` 经常是开发者地图，cook 列表才是真正会打包发布的内容。

### Strict 模式与缓存

项目分析默认采用 **strict** 模式。任何已映射资产读取、索引或解析失败都会生成结构化 failure 并以退出码 `2` 结束。本工具按设计不处理的包——UE4、cooked、unversioned、UE3、大端或包级压缩——会作为 `unsupported` 证据进入 inventory，进程仍以 `0` 退出。

`--allow-partial` 只是把 hard scan failure 降级为零退出，不会改写 `status`、`failures` 或 coverage。退出码 `1` 表示根本没能产出报告。

项目缓存默认放在操作系统缓存目录，不写入被分析项目。使用 `--cache-file` 指定位置，或用 `--no-cache` 完全禁用缓存。
