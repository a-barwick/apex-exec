# ADR 0006: Keep typed HIR facts beside immutable parsed syntax

**Status:** Accepted
**Date:** 2026-07-14

## Context

M4 temporarily stored a resolved method index in an interior-mutable cell on
each parsed call expression. M5 needs substantially more semantic facts for
class members, constructors, inheritance, and project compilation. Extending
the parsed AST with more mutable resolution state would blur the parser and
checker boundary and make cached syntax unsafe to reuse between compilations.

## Decision

Semantic analysis produces a checked HIR program that owns immutable parsed
syntax and side tables keyed by source spans. The first tables record every
checked expression type and selected call target. The runtime accepts only this
checked program and reads the selected targets directly. The parsed AST no
longer contains semantic cells.

Class and project compilation will extend the checked representation with
member, constructor, and dispatch targets. Parsing caches retain only syntax;
checked HIR is rebuilt for a source unit when it or one of its dependencies
changes.

## Consequences

- Parsing is deterministic and independent of whether semantic analysis has
  run before.
- Runtime execution cannot redo or disagree with overload resolution.
- Cached parsed units can be shared safely across incremental compilations.
- Span-keyed tables require every resolved expression to retain an exact source
  span; multi-file compilation namespaces spans by source-unit identity.
- A future lowered executable IR can replace the syntax-plus-side-table layout
  without changing the parser boundary.
