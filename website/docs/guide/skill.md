---
title: Agent skill
description: Install the bundled cc-uax skill for Claude Code, Codex, and compatible agents.
---

# Agent skill

Copy the entire [`skills/cc-uax/`](https://github.com/cyber-tao/cc-uax/tree/master/skills/cc-uax) directory, not only `SKILL.md`. The supporting `agents/` and `references/` content is part of the skill contract.

| Agent | User-level location | Project-level location |
|---|---|---|
| Claude Code | `~/.claude/skills/cc-uax/` | `<repo>/.claude/skills/cc-uax/` |
| Codex | `~/.codex/skills/cc-uax/` | `<repo>/.codex/skills/cc-uax/` |
| Agents-compatible clients | `~/.agents/skills/cc-uax/` | `<repo>/.agents/skills/cc-uax/` |

Prebuilt installers already place this directory next to the binary. From a source checkout, copy it yourself.

Ask the agent to:

1. Scan the project first (`cc-uax project`).
2. Cite `graphs[].edges`, canonical package paths, and `coverage` / `diagnostics`.
3. Keep `partial`, `unsupported`, and `known_opaque` in the conclusion instead of inventing missing edges.
