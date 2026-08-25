---
title: 报告
description: 如何阅读 cc-uax 的资产报告和项目报告，不要把缺口当成成功。
---

# 报告

解析层内部使用强类型结果，只在 CLI 边界渲染 JSON。资产报告直接包含 `coverage`、`capabilities` 和 `diagnostics`；项目报告通过聚合 `analysis`、inventory 中的紧凑分析、生成的 `reachability` 以及可选的完整 `focused` 分析提供同类证据。

字段级契约是 [`report-contract.md`](https://github.com/cyber-tao/cc-uax/blob/master/skills/cc-uax/references/report-contract.md)。本页是阅读指南，不重复每一个字段。

## 状态

| 状态 | 含义 |
|---|---|
| `complete` | 当前 view 所要求的证据全部解码，且没有未解决缺口。 |
| `partial` | 报告仍可用，但至少一个请求区域失败、保持 opaque 或无法连接。 |
| `unsupported` | 当前包/版本无法提供请求的能力。 |

`known_opaque` 是明确的能力结果，不等于成功。典型例子包括尚不能表示为源码级逻辑的 RigVM 编译字节码和压缩 RigHierarchy。只要请求的能力存在此类缺口，报告就不能升级为 `complete`。

空的 diagnostics 数组本身不能证明完整性。要把 `status`、`coverage`、字节消耗和 `capabilities` 放在一起看。

## 资产报告（缩略）

```jsonc
{
  "schema_version": /* see report-contract.md */,
  "status": "complete",
  "view": "full",
  "summary": { /* 包名、文件版本与 custom version、引擎版本、各表计数 */ },
  "coverage": {
    /* 非零的请求/已解码/opaque/失败计数 */
  },
  "capabilities": [ /* 各能力的证据与限制 */ ],
  "exports": [ /* … */ ],
  "graphs": [ /* … */ ]
}
```

稀疏输出：空值或默认字段（`null`、`[]`、`false`、`""`、`"None"`、`0`）被省略。缺省字段表示默认值，不是错误。

## 项目报告（缩略）

```jsonc
{
  "schema_version": /* see report-contract.md */,
  "status": "complete",
  "layout": {},
  "mounts": [],
  "entry_points": {},
  "reachability": { /* 配置根、可达包、closure 成员和缺口 */ },
  "stats": { /* 全部文件系统/索引/缓存计数，含零值 */ },
  "analysis": { /* 聚合 coverage、capabilities 及逐资产摘要 */ },
  "inventory": [ /* 每个包一条紧凑分析 */ ],
  "focused": { /* 匹配 --focus 的完整 AssetAnalysis */ }
}
```

## 身份

用包路径加上适配器与图/模型身份作为命名空间。显示名只是标签。

- K2/EdGraph 的边是 `kind=exec|data`。图内连通性由 `edges` 承载。pin 的 `linked_to` 只保留跨图或未解析的连接。
- RigVM 的 link 同时保存源/目标 pin 路径。不要把编辑器镜像图再算一遍。
- StateTree 和 PCG 节点带有提炼字段和稳定 `index`。完整带标签属性在对应 `exports[]` 项上。

## Opaque 区域

每一块未结构化区域都应该说明*为什么*，而不只是*有*。资产报告列出逐区域范围；项目报告按 `(kind, type, reason)` 分组并给出区域数和字节合计，这样 opaque 字节仍然可归因，而不必列出每一段 mesh 尾巴。

coverage 把类自身 serializer 写下的尾巴（`class_payload_bytes`）和属性块未正常关闭后的无归属尾巴（`unattributed_tail_bytes`）分开。后者才是 decoder 漏了东西的信号。

## 退出码

`status` 描述证据，退出码描述这次运行本身有没有撑住。

| 退出码 | 含义 |
|---|---|
| `0` | 产出了报告。包括 `partial` 和 `unsupported`。 |
| `2` | `project` 的 hard scan failure。报告仍会写出。 |
| `1` | 没有报告：资产不可读/超范围、项目无法发现、`--mount` 错误，或写出失败。 |

`"error"` 只用于致命失败文档，永远不是报告的 `status`。
