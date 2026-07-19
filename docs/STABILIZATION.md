# Phase 2 Stabilization Program

This document is the control plane for the stabilization work that precedes
further Phase 2 feature expansion. A new thread should begin here, then follow
the linked evidence, work-package, and operating documents rather than relying
on prior conversation context.

## Authority and current state

- **Program status:** Active
- **Baseline:** `main` at `c70a528`
- **Completed gate:** S0 — process safety and correctness
- **Next feature milestone:** M19 — bitwise, shift, `Long`, and compound operators
- **Feature policy:** M18 is complete after the S0 exit criteria passed. M19
  and M20 retain the additional architecture prerequisites below.
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
| S0-01 | Frontend process safety and correctness | Complete (`c7d4ac7`, `2a8635d`; review approved; merged as `1b16312`, `ae3bf27`) | S0-00 merged | S0-02, S0-05 |
| S0-02 | Opt-in runtime instrumentation | Complete (`811294f`; review approved; merged as `41319d6`) | S0-00 merged | S0-01, S0-05 |
| S0-03 | Cycle-safe runtime value traversal | Complete (`8f3ce51`; review approved at `b8b190e`; merged as `c7f78e1`) | S0-02 merged | S0-01, S0-05 |
| S0-04 | Execution context and lazy class initialization | Complete (`ed830f2`; setup-mode evidence `53585f8`; reviews approved at `ad08c4c`; merged as `c847fb2`) | S0-02 and S0-03 merged | S0-01, S0-05 |
| S0-05 | CI, complexity ratchet, and release-document gates | Complete (`3471e45`; review approved; merged as `da1945f`) | S0-00 merged | S0-01, S0-02 |
| S0-GATE | Integrated S0 verification and owner review | Complete (reviewed candidate `8a360ac`; merged to `main` as `556d485`) | S0-01–S0-05 complete | Nothing |
| S1-01 | Compiler/runtime substrate ADRs | Active (`codex/milestone-19-bitwise-long-compound`, owner: `/root`) | S0-GATE | M18 implementation |
| S1-02 | Lossless type syntax and typed identities | Blocked (includes F-P1-14 and F-P1-15) | S1-01 | No other AST/HIR work |
| S1-03 | Runtime image and lowered executable targets | Blocked | S1-02 | No other HIR/runtime-image work |
| S1-04 | `Place` and centralized numeric operations | Blocked | S1-02 | Disjoint docs/tooling work |
| S1-05 | Intrinsic and compatibility-profile catalog | Blocked | S1-03 | Disjoint data work |
| S1-06 | Structured diagnostic model | Blocked | S1-03, S1-05 | Nothing cross-cutting |
| S2-01 | Transaction, host-capability, and DML contracts | Blocked | S0-GATE | S1 work with disjoint files |
| S2-02 | Project-scale performance and incremental compilation | Blocked | S1-03, S2-01 | Editor work |
| S2-03 | LSP/DAP/protocol correctness | Blocked | S1-06 | S2-02 |
| S2-04 | Open-source release gate | Blocked | S0-05, owner license decision | Disjoint implementation work |

The first parallel wave was limited to S0-01, S0-02, and S0-05.

## Package review evidence

### S0-04 — Execution context and lazy class initialization

- Implementation `ed830f2` and setup-mode evidence remediation `53585f8` on
  `codex/stab-execution-context`; fresh read-only runtime re-review and an
  adversarial merge guard approved immutable handoff `ad08c4c`.
- Fail-before CLI evidence at claim `9b8aead` reproduced both F-P0-06 cases:
  ordinary `Test.isRunningTest()` printed `true`, and an unused class with
  `static Integer broken = 1 / 0` terminated unrelated execution with a
  `MathException`. The corrected branch prints `false` and `1`, respectively.
- A private execution context now selects ordinary, test, or debugger mode
  independently of instrumentation. Queued work captures the submitting
  context, installs it for execution, and restores the caller's context after
  both successful and failed jobs. Ordinary and debugger entry points report
  non-test mode; test setup methods, test methods, and async work submitted by
  tests report test mode.
- The checked-in Apex fixture has an `@TestSetup` method that asserts
  `Test.isRunningTest()` and records a static setup-observed flag; the existing
  test method asserts that flag. This closes the setup-mode evidence gap
  without adding a Rust test, so the focused binary remains at nine tests.
- The first fresh reviewer requested that checked-in setup regression at
  `111de58`; the implementation branch moved back to Active, added it in
  `53585f8`, and returned to Review. Re-review found no remaining blocker. A
  separate adversarial audit approved the state machine, async restoration,
  scope, ancestry, and conflict-free merge.
- Static slots are allocated to typed null lazily for one active class, then
  field initializers run once in source order through explicit
  `Uninitialized`, `Initializing`, `Initialized`, and `Failed` states.
  Superclasses initialize first, successful and failed results are cached,
  unused classes are not visited, and A→B→A reentry raises a source-spanned
  catchable `TypeException`. A 64-class depth budget also turns adversarial
  acyclic dependency chains into catchable failures instead of host-stack
  aborts.
- Nine focused integration regressions cover ordinary/debug/test/async modes,
  async restoration after failure, unused/successful/failed initialization,
  catchable dependency cycles, owner recovery, operand order, typed-null
  preallocation, static field/property/call/mutation and constructor use,
  superclass ordering, and the depth stress case. A deterministic private cost
  assertion proves that two reads of one used class among 128 unused failing
  classes create exactly one initialized-class state and one static slot.
- Full remediation verification in a fresh isolated build target passed:
  `cargo fmt --check`; `cargo test --locked --no-fail-fast` (353 passed, 14
  ignored North Star indicators); `cargo clippy --locked --all-targets -- -D
  warnings`; the exact CLI reproductions; and the immutable Lizard ratchet with
  64 current violations against 73 debt caps.
  Relevant hotspot extractions reduced `call_platform` from 589/124 NLOC/CCN
  to 577/120, `evaluate_new_object` from 101/20 to 27/6, and
  `write_class_member` from 66/16 to 35/10.

### S0-03 — Cycle-safe runtime value traversal

- Initial implementation `1cda4e0`; semantic-rendering remediation `f241728`;
  debug trace-status correction `8f3ce51`; branch
  `codex/stab-runtime-graph-safety`.
- The first independent review reproduced a blocking regression at `657a118`:
  the 16 KiB debug presentation budget shortened a 20,480-character String to
  16,382 characters in concatenation, `String.valueOf`/`join`,
  `Object.toString`, assertion messages, and ordinary invocation output.
  `f241728` separates bounded debug presentation from semantic
  stringification and adds the fail-before/pass-after regressions. A preflight
  then proved that truncated `System.debug` output did not set debugger trace
  status; `8f3ce51` propagates that metadata under debugger instrumentation and
  adds the direct-literal regression.
- The recorded List display, equality, JSON, Set display, Map display, and
  object-identity CLI cases now terminate deterministically. Cycles render as
  `<cycle>`, while JSON raises a catchable `IllegalArgumentException`.
- Focused verification passed: eight integration tests covering the library and
  CLI reproduction, cyclic List/Set/Map equality, an adversarial equality
  backtracking case, 5,000-level iterative equality, catchable JSON failures,
  JSON structural limits, shared-DAG handling, debugger capture, complete
  semantic String paths, and acyclic compatibility. Runtime unit coverage also
  proves bounded multibyte rendering ends on a valid UTF-8 boundary.
- Full branch verification passed before handoff: `cargo fmt --check`; `cargo
  test` (323 passed, 14 ignored North Star indicators); and `cargo clippy
  --all-targets -- -D warnings`. The exact cyclic CLI reproduction emitted all
  eight expected lines.
- Comparable Lizard evidence removes the 112-NLOC `display_value` hotspot,
  introduces no new function above the 80-NLOC/15-CCN ratchet, and passes the
  immutable S0-05 baseline with 71 current violations against 73 debt caps.

## Integrated package evidence

### S0-01 — Frontend process safety and correctness

- Implementation `c7d4ac7`, initial handoff `f7693fe`, and integration merge
  `1b16312`; maintainability remediation `2a8635d`, final Review handoff
  `7029448`, and follow-up integration merge `ae3bf27`.
- Fresh read-only compiler review approved both observable behavior and the
  follow-up refactor. It reproduced the public parser boundary failures,
  grouped-expression/cast ambiguity, generic `Database.Batchable<Scope>`
  erasure, editor generic-argument navigation gap, and recursive hierarchy
  hazards before the fix. The final review also reproduced all three
  post-integration complexity failures before `2a8635d` and approved the
  focused parser, collection-type, and Batchable validation extractions.
- Integrated Rust verification passed: `cargo fmt --check`; `cargo test
  --locked` (331 passed, 14 ignored North Star indicators); explicit North
  Star reporting; `cargo clippy --locked --all-targets -- -D warnings`; and
  all 13 focused S0-01 regressions.
- Integrated release gates passed: website build/test/lint; editor smoke test
  and zero-vulnerability audit; 19 tooling tests; 54 Markdown files and 97
  local links; committed-whitespace range `1b16312..ae3bf27`; Actionlint
  1.7.12; and the immutable Lizard ratchet with 68 current violations against
  73 debt caps. RustSec and Cargo Deny passed, and the npm policy matched only
  the documented time-boxed PostCSS allowance.

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

### S0-03 — Cycle-safe runtime value traversal

- Initial implementation `1cda4e0`, semantic-rendering remediation `f241728`,
  trace-status correction `8f3ce51`, corrected Review handoff `b8b190e`, and
  integration merge `c7f78e1`.
- The first fresh runtime review requested changes after reproducing semantic
  String truncation. Preflight then found missing debugger trace-status
  propagation and a temporary `call_system` ratchet regression. Both blockers
  were reproduced before correction. A second fresh read-only review approved
  `b8b190e`; fail-before lengths at `657a118` were
  `16382/16382/16382/16382/16421`, while the corrected branch produced
  `20480/20480/20480/20480/20519`.
- Integrated Rust verification passed: `cargo fmt --check`; `cargo test
  --locked` (342 passed, 14 ignored North Star indicators); explicit North
  Star reporting; `cargo clippy --locked --all-targets -- -D warnings`; eight
  focused graph-safety regressions; and the exact eight-line cyclic CLI
  reproduction. The combined immutable Lizard ratchet passed with 66 current
  violations against 73 debt caps.
- Integrated release gates passed: website build/test/lint; editor smoke test
  and zero-vulnerability audit; 19 tooling tests; 54 Markdown files and 97
  local links; committed-whitespace range `8873aaa..c7f78e1`; Actionlint
  1.7.12; RustSec and Cargo Deny; and the npm policy with only the documented
  time-boxed PostCSS allowance. An independent immutable merge audit approved
  both parents, the exact 12-path delta, conflict resolution, and preserved
  owner-policy state.
- Residual non-blocking risks remain scheduled: vector-backed Set/Map equality
  and retained graph pairs can grow quadratically, and F-P1-13 still owns
  downstream debugger/DAP visibility for exhausted traces.

### S0-04 — Execution context and lazy class initialization

- Implementation `ed830f2`, setup-context regression `53585f8`, Review handoff
  `ad08c4c`, and integration merge `c847fb2`. The merge has parents `9b8aead`
  and `ad08c4c` and changed exactly the 13 reviewed runtime, focused-test, and
  required-documentation paths.
- Fresh runtime review initially requested one checked-in `@TestSetup`
  regression and approved the corrected immutable handoff. A separate
  adversarial reviewer found no functional blocker and confirmed context
  restoration across async transaction failures, lazy trigger-driven use,
  custom static properties, type-only dormancy, and bounded initialization.
- Integrated Rust verification used fresh candidate-specific build targets:
  `cargo fmt --check`; `cargo test --locked --no-fail-fast` (353 passed, 14
  ignored North Star indicators); explicit North Star reporting (2 passed, 14
  ignored); and `cargo clippy --locked --all-targets -- -D warnings`.
- All 31 focused S0 integration tests passed: 13 frontend, one instrumentation,
  eight runtime-graph, and nine execution-context tests. Six private
  cost/safety assertions also passed. The CLI instrumentation fixture emitted
  `1`; the runtime-graph fixture emitted its exact eight expected lines;
  ordinary `Test.isRunningTest()` emitted `false`; and the unused failing-class
  fixture emitted `1`.
- The pinned Lizard ratchet passed with 64 current violations against 73 debt
  caps. RustSec found no issue across 99 locked dependencies. Cargo Deny passed
  advisories, bans, and sources with only the documented duplicate-`hashbrown`
  warning.

### S0-05 — CI, complexity ratchet, and release-document gates

- Implementation checkpoints `a2fea18`, `0aaac4a`, `522e44f`, and `767d6a7`;
  Review handoff `51eacc4`; committed-range correction `9c08b87`;
  required-check documentation correction `3471e45`; and integration merge
  `da1945f`.
- Fresh read-only review by `review_s0_05_final`: **Approve** after the
  documented required-check context was corrected to the emitted job name,
  `Required CI gate`. The reviewer found no remaining blocking workflow,
  dependency-policy, complexity-ratchet, or release-document defect.
- Integrated Rust verification passed: `cargo fmt --check`; `cargo test
  --locked` (302 passed, 14 ignored North Star indicators); explicit North Star
  reporting; and `cargo clippy --locked --all-targets -- -D warnings`.
- Integrated non-Rust verification passed: website clean install/build/test
  (2 passed)/lint; editor clean install/smoke test/audit (zero findings);
  Actionlint 1.7.12; 19 tooling tests; 54 Markdown files and 97 local links;
  committed-whitespace range `4b70048..da1945f`; and the pinned Lizard ratchet
  (73 current violations against 73 recorded debt caps).
- `cargo audit --deny warnings` found no issues across 99 locked dependencies.
  Cargo Deny passed advisories, bans, and sources with only the documented
  duplicate-`hashbrown` warning. The npm policy accepted exactly one documented
  advisory across four dependency paths; the raw production audit still
  reports two moderate PostCSS records and its allowance expires 2026-08-18.
- GitHub rules remain an external activation step: `main` and
  `codex/stabilization` must require `Required CI gate`, require current
  branches, and prohibit bypass. No branch was pushed and no rule, release, or
  deployment was created.

### S0-GATE — Integrated verification

- Post-integration verification at immutable merge `c847fb2` passed from a
  clean `codex/stabilization` worktree with `c70a528` and documentation
  checkpoint `152e5d6` preserved as ancestors.
- Website clean install/build/test (2 passed)/lint and editor clean
  install/test/audit (zero vulnerabilities) passed under Node 22.15. Actionlint
  1.7.12, all 19 tooling tests, 54 Markdown files and 98 local links, the full
  committed-whitespace range, and the 64/73 Lizard ratchet passed.
- The npm policy matched four dependency records to the single documented
  `GHSA-qx2v-qp2m-jg93` allowance. The separate raw production audit returned
  its expected nonzero status with only two moderate PostCSS records; the
  allowance expires 2026-08-18.
- A shared local Cargo target exposed three stale, uncommitted reviewer-probe
  tests. SHA/source guards caught the mismatch. Every authoritative package and
  gate result above was rerun in a new candidate-specific target, where the
  canonical count was 353 passed and 14 ignored. The operations contract now
  requires isolated targets for review and integration verification.
- A fresh read-only review approved immutable evidence checkpoint `a7ac474`
  with no blocking findings after independently sampling the complete diff,
  all four CLI reproductions, 353 passing/14 ignored Rust tests, 31 focused S0
  tests, seven cost/safety assertions, and the release/security gates. The
  owner approved reviewed candidate `8a360ac`, which was merged to `main` as
  `556d485`. License, supported-public-API, S1 ADR, release, and deployment
  decisions remain untouched.

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
