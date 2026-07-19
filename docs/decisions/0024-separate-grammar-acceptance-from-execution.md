# ADR 0024: Separate corpus grammar acceptance from executable support

**Status:** Accepted

**Date:** 2026-07-19

## Context

M21 must parse seven byte-pinned real-world sources completely without
pretending that every annotation, switch, DML, modifier, or platform behavior
already exists locally. Token skipping, source rewriting, and generic recovery
would satisfy a parser counter while violating the compiler phase boundaries
and losing syntax needed by later milestones.

Some remaining forms are ordinary local language behavior that can be
implemented coherently now. Others require platform contracts or runtime
semantics owned by later milestones.

## Decision

- Preserve arbitrary annotations, positional/named annotation arguments,
  switch arms, external-ID DML fields, multi-declarator fields, and remaining
  modifiers in dedicated AST data with source spelling and spans.
- Keep known test/future annotations on their existing checked path. Other
  annotations produce an explicit semantic unsupported diagnostic.
- Represent `switch on`/`when`, external-ID DML, `transient`, local `final`,
  and multi-declarator fields as checked-only syntax. Semantic analysis rejects
  them before runtime.
- Execute uninitialized locals as typed null and multi-declarator locals in
  source order. Each earlier declarator enters the same scope before the next
  initializer is checked or evaluated.
- Represent comma-separated traditional-`for` initializer/update expressions
  as an explicit sequence container. The container preserves structure but is
  not itself an executable coverage line or debugger snapshot.
- Keep SOQL/SOSL in their existing dedicated query AST. The executable census
  records which query shapes actually occur in the pinned corpus.
- Bind the comment-aware census to exact AST-derived counts in an ordinary
  test. Corpus text and fingerprints remain immutable.

## Consequences

- Passing 14/14 proves complete lexing/parsing of this corpus without expanding
  runtime compatibility claims.
- Later switch, annotation-platform, external-ID DML, and transient/profile
  milestones can lower preserved syntax instead of reconstructing source text.
- The AST temporarily contains checked-only variants that a valid checked HIR
  will never send to runtime; runtime still guards those impossible paths.
- Uninitialized and multi-declarator locals become a complete executable slice,
  including typed-null behavior, ordering, duplicates, for clauses, CLI
  execution, instrumentation cost, and negative tests.
