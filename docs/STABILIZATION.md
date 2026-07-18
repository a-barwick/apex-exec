# Phase 2 Stabilization Program

This document is the control plane for the stabilization work that precedes
further Phase 2 feature expansion. A new thread should begin here, then follow
the linked evidence, work-package, and operating documents rather than relying
on prior conversation context.

## Authority and current state

- **Program status:** Active
- **Baseline:** `main` at `c70a528`
- **Active gate:** S0 — process safety and correctness
- **Next feature milestone:** M18 — null-aware expressions
- **Feature policy:** M18 implementation is gated until the S0 exit criteria
  pass. M19 and M20 have additional architecture prerequisites below.
- **Integration policy:** Task branches merge into `codex/stabilization`. Only
  the integration owner writes directly to that branch. It merges into `main`
  only after the complete S0 gate passes.

The initial audit was read-only. At the audited baseline:

- `cargo fmt --check` passed.
- `cargo test` passed with 302 passing tests and 14 ignored future North Star
  indicators.
- `cargo clippy --all-targets -- -D warnings` passed.
- The website build, tests, and lint passed.
- `npm audit --omit=dev --audit-level=moderate` reported two moderate PostCSS
  advisories through Next with no available fix.
- The worktree was clean.

## Objectives

1. Eliminate known process-aborting and silently incorrect behavior.
2. Prevent ordinary execution from paying debugger-only cost.
3. Add enforceable maintainability and release gates.
4. Pull forward the typed identity, lowering, and runtime extension points
   needed by M19–M25 while leaving persistent cache serialization in M29.
5. Preserve feature momentum by resuming M18 after the bounded S0 gate rather
   than attempting a wholesale rewrite.

## Non-objectives

- Rewriting the compiler or runtime in one change.
- Completing M18–M30 as part of stabilization.
- Claiming Salesforce compatibility without the existing differential evidence
  process.
- Selecting a repository license without an explicit owner decision.
- Refactoring a large file solely to reduce its line count.

## Navigation

- [Audit evidence and reproduced failures](stabilization/FINDINGS.md)
- [Actionable work packages](stabilization/WORK_PACKAGES.md)
- [Branch, thread, review, and reporting operations](stabilization/OPERATIONS.md)
- [Ready-to-paste coordinator/swarm prompt](stabilization/OPERATOR_PROMPT.md)
- [ADR 0022 — Gate Phase 2 through a stabilization program](decisions/0022-gate-phase-2-through-stabilization.md)

## Work queue

Status values are **Ready**, **Active**, **Blocked**, **Review**, **Complete**,
and **Deferred**. An agent must update this table when claiming or completing a
package.

| ID | Work package | Status | Depends on | May run with |
|---|---|---|---|---|
| S0-00 | Durable control plane and handoff documentation | Complete (`b4519ff`) | None | Documentation only |
| S0-01 | Frontend process safety and correctness | Active (`codex/stab-frontend-safety`; `s0_01_frontend`) | S0-00 merged | S0-02, S0-05 |
| S0-02 | Opt-in runtime instrumentation | Complete (`811294f`; review approved; merged as `41319d6`) | S0-00 merged | S0-01, S0-05 |
| S0-03 | Cycle-safe runtime value traversal | Active (`codex/stab-runtime-graph-safety`; `s0_03_graph_safety`) | S0-02 merged | S0-01, S0-05 |
| S0-04 | Execution context and lazy class initialization | Blocked | S0-02, S0-03 | S0-01, S0-05 |
| S0-05 | CI, complexity ratchet, and release-document gates | Active (`codex/stab-release-gates`; `s0_05_release_gates`) | S0-00 merged | S0-01, S0-02 |
| S0-GATE | Integrated S0 verification and owner review | Blocked | S0-01–S0-05 | Nothing |
| S1-01 | Compiler/runtime substrate ADRs | Blocked | S0-GATE | M18 implementation |
| S1-02 | Lossless type syntax and typed identities | Blocked | S1-01 | No other AST/HIR work |
| S1-03 | Runtime image and lowered executable targets | Blocked | S1-02 | No other HIR/runtime-image work |
| S1-04 | `Place` and centralized numeric operations | Blocked | S1-02 | Disjoint docs/tooling work |
| S1-05 | Intrinsic and compatibility-profile catalog | Blocked | S1-03 | Disjoint data work |
| S1-06 | Structured diagnostic model | Blocked | S1-03, S1-05 | Nothing cross-cutting |
| S2-01 | Transaction, host-capability, and DML contracts | Blocked | S0-GATE | S1 work with disjoint files |
| S2-02 | Project-scale performance and incremental compilation | Blocked | S1-03, S2-01 | Editor work |
| S2-03 | LSP/DAP/protocol correctness | Blocked | S1-06 | S2-02 |
| S2-04 | Open-source release gate | Blocked | S0-05, owner license decision | Disjoint implementation work |

Only S0-01, S0-02, and S0-05 should start in the first parallel wave.

## Integrated package evidence

### S0-02 — Opt-in runtime instrumentation

- Implementation `811294f`, Review handoff `e6c6d27`, and integration merge
  `41319d6`.
- Fresh read-only review by `review_s0_02`: **Approve**, with no blocking
  findings. The downstream debugger/DAP trace-exhaustion visibility gap is
  recorded as F-P1-13 and scheduled for S2-03 rather than expanding S0-02.
- Integrated verification passed: `cargo fmt --check`; `cargo test` (312
  passed, 14 ignored North Star indicators); `cargo clippy --all-targets -- -D
  warnings`; website `npm test` (2 passed) and `npm run lint`; the focused
  instrumentation test; and the ordinary-run cyclic-list CLI reproduction
  (exit 0, output `1`).
- Complexity evidence: comparable Lizard runs reduced
  `execute_statement` CCN from 17 to 16, held the runtime warning count at
  five, and reported no threshold warnings in the extracted bounded
  instrumentation module.

## Roadmap gates

| Roadmap point | Required stabilization state |
|---|---|
| Resume M18 | S0-GATE complete |
| Start M19 | S1-04 complete; numeric and lvalue behavior no longer grows duplicated runtime matches |
| Start M20 | S1-02 and the typed-exception/class-initialization parts of S1-03 complete |
| Freeze M22 baseline | Instrumentation is opt-in; database checkpoints, collection/query hot paths, and incremental compilation have benchmark baselines |
| Start M23/M24 | Typed query-plan and structured DML request/outcome designs are accepted |
| Start M25–M27 | A typed compatibility/execution/access context is threaded end to end |
| Complete M29 | Persist and version the typed/lowered representation introduced earlier; add restart reuse and source-identity remapping |

## S0 exit criteria

S0 is complete only when all of the following are true:

- Cycles through class and interface hierarchy edges produce a diagnostic and
  cannot overflow the process stack.
- Cyclic collection/object graphs cannot overflow debug formatting, equality,
  JSON serialization, or debugger capture.
- Ordinary execution and ordinary tests do not allocate debugger snapshots.
- Generic arguments are preserved and checked or rejected explicitly; no
  parser path silently discards them.
- Parenthesized identifiers and sized custom/object arrays pass their
  regression programs.
- Public parser entry points validate their token-stream invariants instead of
  panicking.
- `Test.isRunningTest()` is false outside tests and true inside tests.
- Static initialization is lazy per class, detects cycles, and does not execute
  unused-class initializers.
- CI runs Rust and website verification and rejects maintainability-regression
  deltas.
- The full verification suite and every recorded CLI reproduction pass from
  the integrated stabilization branch.
- `docs/STATUS.md`, this tracker, and affected compatibility documentation are
  current.

## Owner decisions

These decisions must not be delegated to an implementation agent:

- Select the repository license.
- Decide whether the initial public product is binary-first or which library
  modules form a supported semver surface.
- Approve the S1 compiler/runtime substrate ADRs before implementation.
- Approve merging the completed stabilization branch into `main`.

## Status-update contract

Every work-package owner must update this file at claim and handoff time:

1. Change the package status and record the branch name.
2. Link the implementation/review commit or PR when one exists.
3. Record the exact verification commands and results.
4. Record newly discovered follow-up work without silently expanding scope.
5. Keep blocked packages blocked until every declared dependency is integrated,
   not merely implemented on another branch.

The detailed handoff format is in
[stabilization/OPERATIONS.md](stabilization/OPERATIONS.md).
