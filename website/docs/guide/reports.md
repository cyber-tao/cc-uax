---
title: Reports
description: How to read cc-uax asset and project reports without treating gaps as success.
---

# Reports

Reports are typed internally and rendered to JSON only at the CLI boundary. Asset reports expose `coverage`, `capabilities`, and `diagnostics` directly. Project reports expose the same accounting through aggregate `analysis`, compact per-inventory analyses, generated `reachability`, and optional full `focused` analyses.

The field-level contract is [`report-contract.md`](https://github.com/cyber-tao/cc-uax/blob/master/skills/cc-uax/references/report-contract.md). This page is the reading guide, not a restatement of every field.

## Status

| Status | Meaning |
|---|---|
| `complete` | All evidence required by the requested view was decoded without an unresolved gap. |
| `partial` | The report is usable, but at least one requested region failed, remained opaque, or could not be linked. |
| `unsupported` | The requested capability cannot be derived from this package/version. |

`known_opaque` is a deliberate capability result, not success. Examples include compiled RigVM bytecode and compressed RigHierarchy payloads. A report with such a requested gap must not be promoted to `complete`.

An empty diagnostics array alone does not prove completeness. Check `status`, `coverage`, exact byte consumption, and `capabilities` together.

## Asset report (abbreviated)

```jsonc
{
  "schema_version": /* see report-contract.md */,
  "status": "complete",
  "view": "full",
  "summary": { /* package name, file/custom versions, engine version, table counts */ },
  "coverage": {
    /* non-zero requested/decoded/opaque/failed counters */
  },
  "capabilities": [ /* capability-specific evidence and limitations */ ],
  "exports": [ /* … */ ],
  "graphs": [ /* … */ ]
}
```

Sparse output: empty or default fields (`null`, `[]`, `false`, `""`, `"None"`, `0`) are omitted. A missing field means the default, not an error.

## Project report (abbreviated)

```jsonc
{
  "schema_version": /* see report-contract.md */,
  "status": "complete",
  "layout": {},
  "mounts": [],
  "entry_points": {},
  "reachability": { /* configured roots, reachable packages, closure members, gaps */ },
  "stats": { /* every filesystem/index/cache counter, including zeros */ },
  "analysis": { /* aggregate coverage, capabilities, and per-asset summaries */ },
  "inventory": [ /* one compact analysis per package */ ],
  "focused": { /* full AssetAnalysis for packages matching --focus */ }
}
```

## Identities

Use package path plus adapter and graph/model identity as the namespace. Display names are labels.

- K2/EdGraph edges have `kind=exec|data`. Intra-graph connectivity is carried by `edges`. A pin's `linked_to` retains only cross-graph and unresolved connections.
- RigVM links store source and target pin paths. Do not double-count the editor mirror graph.
- StateTree and PCG nodes carry distilled fields plus a stable `index`. Read full tagged properties from the matching `exports[]` entry.

## Opaque regions

Every unstructured region should say *why*, not just *that*. Asset reports list per-region ranges. Project reports group by `(kind, type, reason)` with region and byte totals so opaque bytes stay attributable without listing every mesh tail.

Coverage separates expected class serializer tails (`class_payload_bytes`) from unattributed tails after a property block that never closed (`unattributed_tail_bytes`). The second is the signal that a decoder is missing something.

## Exit codes

`status` describes the evidence. The exit code describes whether the run itself held together.

| Code | Meaning |
|---|---|
| `0` | A report was produced. Includes `partial` and `unsupported`. |
| `2` | `project` hard scan failure. The report is still written. |
| `1` | No report: unreadable/out-of-scope asset, undiscoverable project, bad `--mount`, or a failed write. |

`"error"` is the fatal document only. It is never a report `status`.
