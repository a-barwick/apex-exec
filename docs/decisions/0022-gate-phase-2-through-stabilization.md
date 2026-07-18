# ADR 0022: Gate Phase 2 through a stabilization program

**Status:** Accepted
**Date:** 2026-07-18

## Context

M1–M17 established a broad local compiler, runtime, data, async, editor,
oracle, CI, and hybrid-validation surface quickly. The normal Rust and website
verification suites pass, but a pre-open-source audit found process-aborting
interface and runtime-value cycles, silent generic argument erasure, debugger
snapshot work on every ordinary statement, globally eager static
initialization, and several extension points whose complexity will grow
directly under M18–M27.

The current roadmap places dependency-scoped semantics and persistent typed or
lowered IR in M29. Stable compiler identities, lossless type syntax, lowered
executable targets, explicit execution context, and centralized mutation/
numeric policy are already prerequisites for nested declarations, `Long`,
compound assignment, compatibility profiles, and enterprise-scale execution.
Deferring every part of that substrate until M29 would require M19–M28 to deepen
representations that M29 must then replace.

The audit is too broad for one thread or one implementation change. Parallel
work without explicit ownership would also concentrate conflicts in
`runtime.rs`, `semantic.rs`, `ast.rs`, and HIR.

## Decision

- Insert an active S0 stabilization gate before M18 feature implementation.
  S0 is bounded to process safety, silent correctness failures, opt-in
  instrumentation, execution context/lazy class initialization, and automated
  maintainability/release gates.
- Store the authoritative tracker, evidence, work packages, operating rules,
  and coordinator prompt in repository documentation under
  `docs/STABILIZATION.md` and `docs/stabilization/`.
- Use a single `codex/stabilization` integration branch and isolated
  `codex/` task branches. Only the integration owner writes directly to the
  integration branch.
- Run at most three implementation threads concurrently and only for work
  packages with disjoint hotspot ownership. The first parallel wave is frontend
  safety, runtime instrumentation, and release gates. Runtime graph safety and
  execution-context/static work follow serially.
- Require executable reproductions, full verification, coherent commits, and a
  fresh read-only review before integration. Implementation threads move
  packages to Review; the integration owner alone marks them Complete.
- Resume M18 after S0 passes.
- Pull the foundational portion of M29 forward through reviewed S1 ADRs and
  incremental implementation: lossless type syntax, typed declaration/schema
  identities, lowered executable targets, runtime-image metadata, typed
  exceptions/call frames, `Place`, numeric policy, and typed compatibility
  context.
- Keep persistent IR serialization, versioned restart reuse, corruption
  rejection, and source-identity remapping in M29.
- Require owner approval for license selection, supported public API policy,
  S1 architecture ADRs, and the final merge into `main`.

## Consequences

- Phase 2 pauses briefly at a measurable gate rather than beginning an
  unbounded rewrite.
- Known process-aborting and silently incorrect behavior is handled before new
  language surface is added.
- M18 can resume after S0, preserving product momentum.
- M19 and M20 acquire explicit prerequisites that reduce duplicated runtime and
  type-system logic.
- Work-package boundaries and repository-owned status survive thread
  termination and context loss.
- Some early simplifications accepted by ADRs 0001, 0004, 0006, 0007, 0013,
  and 0016 will be narrowed or superseded through future focused ADRs rather
  than rewritten implicitly.
- The stabilization program adds documentation and integration overhead, but
  makes parallel work reviewable and prevents multiple agents from expanding
  the same central methods independently.
- M29 remains necessary; it becomes the persistence and measured-reuse
  milestone rather than the first point at which stable executable identities
  exist.
