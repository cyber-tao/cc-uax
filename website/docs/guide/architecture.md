---
title: Architecture
description: The three-crate workspace and the one-way dependency rule.
---

# Architecture

The repository is a virtual Cargo workspace with three responsibilities:

```text
cc-uax/
├── crates/
│   ├── cc-uax-core/       # byte-bound package parsing, typed values, graphs, coverage
│   ├── cc-uax-project/    # project discovery, inventory, adjacency, ownership, cache
│   └── cc-uax-cli/        # asset/project commands and JSON rendering
└── skills/
    └── cc-uax/            # full Claude Code / Codex skill package
```

Dependency direction is one-way:

```text
cc-uax-cli ──> cc-uax-project ──> cc-uax-core
      └────────────────────────> cc-uax-core
```

- `cc-uax-core` does not own filesystem scanning, SQLite, CLI arguments, or JSON presentation policy.
- `cc-uax-project` owns mounts, project discovery, the shared inventory scan, reference adjacency, World Partition ownership, reachability/resource summaries, and cache placement.
- `cc-uax-cli` selects views/focuses, attaches requested full asset analyses, enforces exit behavior, and renders typed reports.

The public site in `website/` is not a Cargo workspace member. It is built by GitHub Actions and published to GitHub Pages.

Contributor-level parsing rules live in [`CLAUDE.md`](https://github.com/cyber-tao/cc-uax/blob/master/CLAUDE.md).
