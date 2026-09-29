---
title: 架构
description: 三个 crate 的 workspace 与单向依赖。
---

# 架构

仓库是包含三个职责层的虚拟 Cargo workspace：

```text
cc-uax/
├── crates/
│   ├── cc-uax-core/       # 绑定字节的包解析、强类型值、图和 coverage
│   ├── cc-uax-project/    # 项目发现、清单、邻接、归属和缓存策略
│   └── cc-uax-cli/        # asset/project 命令与 JSON 渲染
└── skills/
    └── cc-uax/            # 完整 Claude Code / Codex skill 包
```

依赖方向保持单向：

```text
cc-uax-cli ──> cc-uax-project ──> cc-uax-core
      └────────────────────────> cc-uax-core
```

- `cc-uax-core` 不负责文件系统扫描、SQLite、CLI 参数或 JSON 呈现策略。
- `cc-uax-project` 负责 mount、项目发现、共享清单扫描、引用邻接、World Partition 归属、reachability/resource 摘要和缓存位置。
- `cc-uax-cli` 负责选择 view/focus、附加请求的完整资产分析、退出语义和强类型报告渲染。

核心公开类型包括 `PackageView<'a>`、`AssetAnalysis`、`DecodedValue`、`LogicGraph`、`GraphNode`、`GraphEdge` 和 `ParseCoverage`。`PackageView<'a>` 将解析和解码绑定到同一份字节，避免调用方用 A 文件解析结果解码 B 文件。

`website/` 中的公开站点不是 Cargo workspace 成员，由 GitHub Actions（`.github/workflows/pages.yml`）构建并发布到 GitHub Pages。在本地开发站点：

```bash
bun install --cwd website
bun run --cwd website dev
```

贡献者应同时阅读 [`CLAUDE.md`](https://github.com/cyber-tao/cc-uax/blob/master/CLAUDE.md) 中的解析约束。
