# ADR 0007: Rebase cached source units into a project span space

**Status:** Accepted
**Date:** 2026-07-14

## Context

Every parsed `.cls` unit uses byte spans beginning at zero, while checked HIR
side tables use spans as keys. Cross-file compilation therefore needs a stable
way to distinguish identical local offsets. M5 also needs to avoid reparsing
every class on repeated project checks without prematurely introducing a
global interner or independently linkable class IR.

## Decision

The project compiler caches each immutable parsed unit by path and source hash.
For semantic linking it sorts files by path, clones cached units, and rebases
their spans into one project coordinate space. A source map translates project
diagnostics back to the owning file and local byte range.

The compiler builds file dependency edges from declared and referenced class
types. A changed unit is reparsed and reverse dependents are reported as
invalidated; unchanged parsed units are reused. An identical complete input set
reuses the prior checked HIR. After any source change, semantic linking still
runs across the merged project.

## Consequences

- Cross-file HIR tables retain the simple exact-span key used by anonymous
  compilation.
- File ordering and diagnostics are deterministic across runs.
- Unchanged source files avoid lexing/parsing, and no-change builds avoid all
  compiler phases.
- Rebased AST clones add linear work and memory during a changed build.
- True dependency-scoped semantic/HIR reuse remains a later optimization; the
  dependency graph and immutable unit cache provide the boundary for it.
