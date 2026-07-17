# Architecture Decision Records

Architecture Decision Records preserve the reason behind consequential or
expensive-to-reverse choices.

Use the next numeric filename and this structure:

```markdown
# ADR NNNN: Decision title

Status: Proposed | Accepted | Superseded
Date: YYYY-MM-DD

## Context

What forces or constraints require a decision?

## Decision

What was selected?

## Consequences

What becomes easier, harder, required, or deliberately deferred?
```

Do not create ADRs for routine implementation choices. If an accepted decision
changes, add a new ADR and mark the prior one superseded rather than rewriting
history.

## Index

- [0001 — Begin with a tree-walking interpreter](0001-tree-walking-interpreter.md)
- [0002 — Canonicalize identifiers for case-insensitive lookup](0002-case-insensitive-identifiers.md)
- [0003 — Isolate platform behavior behind host interfaces](0003-platform-host-boundary.md)
- [0004 — Store collections by interpreter-owned identity](0004-arena-backed-collection-identity.md)
- [0005 — Record checked calls and unify abrupt runtime flow](0005-checked-calls-and-runtime-unwinding.md)
- [0006 — Keep typed HIR facts beside immutable parsed syntax](0006-typed-hir-side-tables.md)
- [0007 — Rebase cached source units into a project span space](0007-project-span-space-and-incremental-cache.md)
- [0008 — Isolate test execution by interpreter](0008-isolated-test-execution-and-coverage.md)
- [0009 — Give every source unit an explicit identity](0009-file-aware-source-identity.md)
