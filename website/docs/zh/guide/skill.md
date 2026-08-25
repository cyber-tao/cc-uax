---
title: Agent Skill
description: 为 Claude Code、Codex 和兼容客户端安装随附的 cc-uax skill。
---

# Agent Skill

请复制完整的 [`skills/cc-uax/`](https://github.com/cyber-tao/cc-uax/tree/master/skills/cc-uax) 目录，而不是只复制 `SKILL.md`。`agents/` 和 `references/` 是 skill 契约的一部分。

| Agent | 用户级目录 | 项目级目录 |
|---|---|---|
| Claude Code | `~/.claude/skills/cc-uax/` | `<repo>/.claude/skills/cc-uax/` |
| Codex | `~/.codex/skills/cc-uax/` | `<repo>/.codex/skills/cc-uax/` |
| Agents 兼容客户端 | `~/.agents/skills/cc-uax/` | `<repo>/.agents/skills/cc-uax/` |

预编译安装器已经会把该目录放到二进制旁边。从源码 checkout 运行 `./dev-install.sh` 或 `.\dev-install.ps1` 会把工作区里的 skill 链接到上述用户级目录。

请要求 Agent：

1. 先扫描项目（`cc-uax project`）。
2. 引用 `graphs[].edges`、规范包路径以及 `coverage` / `diagnostics`。
3. 在结论里保留 `partial`、`unsupported` 和 `known_opaque`，不要补造缺失的边。
