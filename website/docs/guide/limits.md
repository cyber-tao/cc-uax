---
title: Scope and limits
description: What cc-uax targets, what it rejects, and current named gaps.
---

# Scope and limits

Serialization decisions are checked against UE5.0–5.8 source and exercised against external, real editor assets across that range. `FileVersionUE5` 1000–1018 is the accepted window. Both ends are enforced in `PackageFileSummary::parse` as out of scope, not inferred later.

## In scope

Versioned, uncooked UE5.0–5.8 editor packages. That range is real-corpus-verified and may be `status=complete` when evidence is complete.

## Out of scope

Rejected rather than guessed at:

- below 1000 (UE4 and older)
- above 1018 (a layout this parser has not seen)
- cooked or unversioned packages
- UE3, big-endian, and package-level compression

`cc-uax asset` exits `1` with an error document for an out-of-scope package. `cc-uax project` indexes it as `unsupported` evidence and still exits `0`.

## Current limitations

- cooked/unversioned packages and UE4 package formats
- source-level reconstruction of compiled RigVM bytecode and compressed RigHierarchy data
- compiled Blueprint bytecode and compiled Niagara VM/GPU payloads (named capabilities, not anonymous tails)
- runtime behavior not evidenced by serialized graphs, properties, configuration, or references
- plugin-native formats without a verified UE5.0–5.8 serialization contract

Compiled Blueprint bytecode is a limit on **attribution**, not on reference completeness. An object reference inside the `Script` stream is an `FPackageIndex`, so it cannot serialize without an import row — the linker tables the reference graph is built from already list it. What the tables cannot hold is an asset path typed into a graph pin as a string, and `reference_evidence` measures that residue per asset instead of leaving it as an open-ended caveat.

When evidence is incomplete, consumers must retain `partial`, `unsupported`, diagnostics, and capability limitations in their conclusions.

The same `FileVersionUE5` does not guarantee the same layout: UE5.7 and UE5.8 share `1018` yet diverge. Custom versions and, where needed, the engine version gate those formats.

## Validation

Real-corpus acceptance is separate from ordinary workspace tests. External assets and machine-specific paths stay local; the repository does not commit them.

## License

[MIT](https://github.com/cyber-tao/cc-uax/blob/master/LICENSE)
