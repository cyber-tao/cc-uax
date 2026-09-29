---
title: Scope and limits
description: What cc-uax targets, what it rejects, and current named gaps.
---

# Scope and limits

Serialization decisions are checked against UE5.0–5.8 source. `FileVersionUE5` 1000–1018 is the accepted window. Both ends are enforced in `PackageFileSummary::parse` as out of scope, not inferred later.

## In scope

Versioned, uncooked UE5.0–5.8 editor packages (`FileVersionUE5` 1000–1018). A package may be `status=complete` when its evidence is complete.

Real projects have exercised `FileVersionUE5` 1002–1004, 1006–1009 and 1012–1018. 1000, 1001, 1005, 1010 and 1011 have not been seen in a real asset yet: they decode per version gates checked against source, but no real package has confirmed them.

## Out of scope

Rejected rather than guessed at:

- UE4 and older (`FileVersionUE5` below 1000)
- above 1018 (a layout this parser has not seen)
- cooked packages, including anything flagged `PKG_Cooked` or `PKG_UnversionedProperties`
- unversioned packages
- UE3, big-endian, and package-level compression

`cc-uax asset` exits `1` with an error document for an out-of-scope package. `cc-uax project` indexes it as `unsupported` evidence and still exits `0`.

A UE4-format package (`FileVersionUE5` = 0) stays `unsupported`, but a project scan still reads its linker reference tables (names, imports, soft package references). It contributes reference edges and reachability and carries `file_version_ue4`; its properties and graphs are not analysed.

## Current limitations

- source-level reconstruction of compiled RigVM bytecode (`rig_vm_bytecode`) and compressed RigHierarchy data (`rig_hierarchy`)
- compiled Niagara VM/GPU payloads (`niagara_compiled`, a named capability, not an anonymous tail)
- set and map element structs in legacy (below 1012) property tags, which record no struct name and so stay opaque unless the property's declaration is known from UE source (the engine declarations cc-uax carries, scoped to the type that declares them)
- payloads whose layout depends on UE's reflection or class registry
- runtime behavior not evidenced by serialized graphs, properties, configuration, or references
- plugin-native formats without a verified UE5.0–5.8 serialization contract

Compiled Blueprint script is no longer on that list: `UStruct`, `UFunction` and `UClass` are decoded as structured fields and the Kismet stream is disassembled, so a Blueprint's functions, variables and the targets its compiled code reaches are reported evidence. What the linker tables still cannot hold is an asset path typed into a graph pin as a string, and `reference_evidence` measures that residue per asset instead of leaving it as an open-ended caveat.

When evidence is incomplete, consumers must retain `partial`, `unsupported`, diagnostics, and capability limitations in their conclusions.

The same `FileVersionUE5` does not guarantee the same layout: UE5.7 and UE5.8 share `1018` yet diverge. Custom versions and, where needed, the engine version gate those formats.

## Validation

Serialization decisions are checked against UE5.0–5.8 source and exercised against external, real editor assets. Real-corpus acceptance gates are defined by a harness that is maintained separately from the workspace crates and is not committed as a workspace member; it is separate from ordinary workspace tests. External assets and machine-specific paths stay local; the repository does not commit them.

## License

[MIT](https://github.com/cyber-tao/cc-uax/blob/master/LICENSE)
