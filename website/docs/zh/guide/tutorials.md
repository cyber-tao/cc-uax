---
title: 教程
description: 常见 cc-uax 任务的逐步走法。
---

# 教程

下列参数在 Windows、macOS 和 Linux 上相同。示例使用 PowerShell；把 `D:/Games/MyGame` 换成你的项目路径。

## 1. 先做一次项目扫描

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

缺省字段表示默认值（空 / false / 零），不是扫描没做完。字段级含义见 [`report-contract.md`](https://github.com/cyber-tao/cc-uax/blob/master/skills/cc-uax/references/report-contract.md)。

同一 Content 树下有多个 `.uproject` 时，显式传入其中一个文件：

```powershell
cc-uax project D:/Games/MyGame/MyGame.uproject --output project-report.json
```

## 2. 选择资产 view

`--view` 默认为 `full`，体积很大。选能回答问题的最小 view。

```powershell
cc-uax asset Content/Blueprints/BP_Player.uasset --view summary
cc-uax asset Content/Blueprints/BP_Player.uasset --view logic --output BP_Player.logic.json
cc-uax asset Content/Blueprints/BP_Player.uasset --view properties --output BP_Player.props.json
cc-uax asset Content/Blueprints/BP_Player.uasset --view references
```

## 3. 读蓝图执行流

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

## 4. 查谁引用了某个资产

单个文件的出边：

```powershell
cc-uax asset Content/Blueprints/BP_Player.uasset --view references
```

入边（“谁在用它”）需要项目索引。做完一次 `project` 扫描后，用规范包路径查 `reverse`：

```text
reverse["/Game/Blueprints/BP_Player"]  →  引用它的包
forward["/Game/Blueprints/BP_Player"] →  它引用的包
```

`reachability.isolated_project_assets` 和 `reachability.unreachable_project_assets` 只是已扫描 mount 下的图事实，不能当成可以安全删除的证明。

## 5. 从启动地图追踪玩法

```powershell
cc-uax project D:/Games/MyGame --output project-report.json --focus "/Game/Maps/L_Startup" --focus "/Game/Blueprints/BP_GameMode"
```

1. 解析 `entry_points.defaults`（`GameDefaultMap`、`GameInstanceClass`、`GlobalDefaultGameMode` 等）。平台覆盖在 `entry_points.platforms`。
2. 沿 `reachability.configured_roots` → `reachable_runtime_packages` 走。
3. 把 `ownership_closure` / `reachability.ownership_closure_members` 里的 World Partition 成员算进去。
4. 对地图、GameMode 和需要逐步走的蓝图用 `--focus` 附加完整分析。`--focus` 可重复。

## 6. 用 `--focus` 选择包

`--focus` 按规范包路径匹配，不区分大小写。末尾的 `.uasset` / 对象后缀会被去掉。`?` 匹配一个非分隔符字符，`*` 匹配单个路径段，`**` 可跨越 `/`。

```powershell
cc-uax project D:/Games/MyGame --focus "/Game/Blueprints/*"
cc-uax project D:/Games/MyGame --focus "/Game/Blueprints/**"
cc-uax project D:/Games/MyGame --focus "/Game/Maps/L_Startup" --focus "/Game/Characters/**"
cc-uax project D:/Games/MyGame --focus "/Game/Blueprints/BP_Player.uasset"
```

一个模式一个都匹配不到，算 hard failure（退出码 `2`），并记入 `failures`（`stage=focus`）。报告仍会写出。匹配到但不在支持范围内的包在 `inventory` 里保持 `unsupported`，不算 focus failure。

## 7. 纳入插件或其他 Content

`Plugins/` 下的插件 content 会自动挂载，挂载名 `/{name}` 取自各自 `.uplugin` 文件的基名。目录叫 `MetaXR` 但内含 `OculusXR.uplugin` 时挂载为 `/OculusXR`——请看报告里的 `mounts`，不要猜。

```powershell
cc-uax project D:/Games/MyGame `
  --mount "/Extra=ExtraContent" `
  --output project-report.json
```

## 8. 把报告限制在上下文窗口内

```powershell
cc-uax project D:/Games/MyGame --focus "/Game/Blueprints/BP_Player" --compact --max-output-bytes 200000 --output bp.json
cc-uax asset Content/Blueprints/BP_Player.uasset --view logic --max-output-bytes 80000
```

`--compact` 去掉 pretty-print 空白。`--max-output-bytes` 会省略较重的细节，并在顶层增加 `output` 块。用更窄的 `--view` 或 `--focus` 把丢掉的区段再查回来。

## 9. 把 status、coverage 和退出码一起看

| 现象 | 含义 | 下一步 |
|---|---|---|
| `status=complete`，退出码 `0` | 请求的证据已解码 | 直接使用图 / 引用 |
| `status=partial`，退出码 `0` | 报告可用，但有具名缺口 | 结论里保留缺口，不要补造缺失路径 |
| `status=unsupported`，退出码 `0`（`project`） | 扫描到的每个包都超出支持范围 | 当作限制，不是崩溃 |
| 对 cooked / UE4 / 超范围包执行 `asset` 时退出码 `1` | 没有报告：本工具按设计不处理它 | 改扫整个项目 |
| 退出码 `2`（`project`） | Hard scan failure | 读 `failures`；必要时加 `--allow-partial` 重跑 |
| 退出码 `1` | 根本没有报告 | 检查路径、`--mount` 语法或输出位置 |

`--allow-partial` 只改退出码，不会改写 `status`、`failures` 或 coverage。

## 10. 控制项目缓存

```powershell
cc-uax project D:/Games/MyGame --output project-report.json
cc-uax project D:/Games/MyGame --cache-file D:/caches/mygame-uax.sqlite --output project-report.json
cc-uax project D:/Games/MyGame --no-cache --output project-report.json
```

对未变化的包，fresh cache entry 会复用已验证的引用列表和紧凑逐资产分析摘要。

## 11. 从源码 checkout 运行

```powershell
cargo run -p cc-uax-cli --release --locked -- project D:/Games/MyGame --output project-report.json
cargo run -p cc-uax-cli --release --locked -- asset Content/Blueprints/BP_Player.uasset --view logic
```

## 12. 在 Claude Code 或 Codex 里用

安装完整的 [`skills/cc-uax/`](https://github.com/cyber-tao/cc-uax/tree/master/skills/cc-uax) 目录。先让 Agent 扫项目，再要求它引用 `graphs[].edges`、包路径以及 `coverage` / `diagnostics`，而不是凭节点显示名猜测。
