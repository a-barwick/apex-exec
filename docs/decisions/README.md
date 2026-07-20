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
- [0007 — Rebase cached source units into a project span space (superseded by 0009)](0007-project-span-space-and-incremental-cache.md)
- [0008 — Isolate test execution by interpreter](0008-isolated-test-execution-and-coverage.md)
- [0009 — Give every source unit an explicit identity](0009-file-aware-source-identity.md)
- [0010 — Keep normalized schema independent from record storage](0010-separate-schema-from-storage.md)
- [0011 — Use an additive SQLite schema registry](0011-additive-sqlite-schema-registry.md)
- [0012 — Use checked query plans and a database host](0012-checked-query-plans-and-database-host.md)
- [0013 — Orchestrate triggers with nested database checkpoints](0013-trigger-dispatch-and-transaction-checkpoints.md)
- [0014 — Keep curated platform APIs checked and host-backed](0014-checked-curated-platform-services.md)
- [0015 — Drain asynchronous Apex explicitly and deterministically](0015-explicit-deterministic-async-drain.md)
- [0016 — Debug through deterministic runtime snapshots](0016-deterministic-debug-snapshots.md)
- [0017 — Compare providers through normalized oracle snapshots](0017-normalized-differential-oracle.md)
- [0018 — Seal CI runs behind content-addressed manifests](0018-content-addressed-enterprise-ci.md)
- [0019 — Compose hybrid validation above hermetic CI](0019-compose-hybrid-validation-above-hermetic-ci.md)
- [0020 — Check runtime-type expressions before execution](0020-check-runtime-type-expressions-before-execution.md)
- [0021 — Bind live validation to sealed candidate evidence](0021-bind-live-validation-to-candidate-evidence.md)
- [0022 — Gate Phase 2 through a stabilization program](0022-gate-phase-2-through-stabilization.md)
- [0023 — Stage typed compiler and runtime substrate before M19](0023-stage-typed-compiler-runtime-substrate.md)
- [0024 — Separate corpus grammar acceptance from executable support](0024-separate-grammar-acceptance-from-execution.md)
- [0025 — Freeze enterprise compatibility by Salesforce test](0025-freeze-enterprise-compatibility-by-salesforce-test.md)
- [0026 — Recheck dynamic SOQL into checked plans](0026-recheck-dynamic-soql-into-checked-plans.md)
- [0027 — Structure partial DML requests and outcomes](0027-structure-partial-dml-outcomes.md)
- [0028 — Bind compatibility profiles per source](0028-bind-compatibility-profiles-per-source.md)
- [0029 — Separate metadata catalog, accounting, and org discovery](0029-separate-metadata-catalog-accounting-and-org-discovery.md)
