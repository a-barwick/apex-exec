# Stabilization Work Packages

Each package below is intended for one implementation thread and one coherent,
buildable branch. Agents must follow
[OPERATIONS.md](OPERATIONS.md), update
[`docs/STABILIZATION.md`](../STABILIZATION.md), and stop rather than silently
expanding scope.

The `Status` fields in this catalog preserve each package's initial dependency
state when the stabilization program was defined. They are not the live
execution tracker. The canonical current status, claim, review, and integration
evidence is the work queue in
[`docs/STABILIZATION.md`](../STABILIZATION.md).

## S0-00 — Durable control plane

**Status:** Complete
**Branch:** `codex/stabilization`
**Checkpoint:** `b4519ff`

Deliverables:

- Canonical program tracker, findings, work packages, operating rules, and
  coordinator prompt.
- Roadmap, status, ADR index, and `AGENTS.md` routing updates.
- Documentation validation and a committed checkpoint.

Acceptance:

- A fresh thread can identify the next ready work without conversation context.
- Dependencies and conflicting file ownership are explicit.
- No implementation behavior changes are included.
- Relative documentation links resolve, `git diff --check` passes, and the
  required Rust fmt/test/clippy suite passes.

## S0-01 — Frontend process safety and correctness

**Status:** Ready after S0-00 integration
**Suggested branch:** `codex/stab-frontend-safety`
**May run with:** S0-02 and S0-05

Scope:

- Validate cycles across every superclass and interface edge.
- Add visited tracking to recursive interface-method collection.
- Reject invalid interface-edge syntax explicitly.
- Fix cast/group disambiguation for parenthesized identifiers.
- Fix sized arrays of custom, object, and exception-compatible element types.
- Validate public parser token-stream invariants.
- Preserve and check generic interface arguments; never consume and discard
  them.
- Correct the qualified Schema type recognition cases recorded in the audit.
- Add focused parser/semantic and CLI regressions.

Non-scope:

- General parser recovery.
- Nested declarations, enums, or M20 features.
- Full HIR identity/lowering redesign.
- Broad annotation grammar.

Likely files:

- `src/parser.rs`
- `src/parser/types.rs`
- `src/parser/expressions.rs`
- `src/parser/declarations.rs`
- `src/ast.rs`
- Hierarchy/type portions of `src/semantic.rs`
- Focused tests

Required regressions:

- Cyclic interface input returns a diagnostic without panic or abort.
- `(foo) + bar` is grouped correctly.
- `Foo[] values = new Foo[3]` parses and checks where supported.
- `Database.Batchable<Integer>` with String scope methods is rejected.
- Empty/malformed raw token streams cannot panic the public API.
- Qualified Schema types resolve consistently.

## S0-02 — Opt-in runtime instrumentation

**Status:** Ready after S0-00 integration
**Suggested branch:** `codex/stab-runtime-instrumentation`
**May run with:** S0-01 and S0-05
**Must finish before:** S0-03 and S0-04

Scope:

- Introduce an explicit instrumentation policy such as `None`, `Coverage`, and
  `Debugger`.
- Ordinary execute/invoke paths use `None`.
- Test execution collects only the coverage/trace facts it actually consumes.
- Debug entry points alone allocate debugger snapshots.
- Make debugger snapshot count and memory growth bounded.
- Preserve existing debugger and coverage behavior through focused tests.
- Add a benchmark or deterministic instrumentation-count assertion proving
  normal execution creates zero snapshots.

Non-scope:

- Graph-cycle handling, which is S0-03.
- Lazy class initialization, which is S0-04.
- DAP feature expansion.

Likely files:

- `src/runtime.rs`
- `src/runtime/image.rs`
- `src/debugger.rs`
- `src/dap.rs`
- `src/test_runner.rs`
- Focused runtime/debugger tests

Acceptance:

- Ordinary `execute`, static `invoke`, and non-debug CLI runs create no
  snapshots.
- Debug entry points retain deterministic pre-statement behavior.
- Coverage totals remain unchanged for existing fixtures.
- Long ordinary loops do not accumulate debugger state.

## S0-03 — Cycle-safe runtime value traversal

**Status:** Blocked by S0-02
**Suggested branch:** `codex/stab-runtime-graph-safety`
**May run with:** Frontend or release work only

Scope:

- Introduce shared graph traversal state for runtime values.
- Add visited-pair handling for equality.
- Add visited-identity and depth/node/element/output budgets for display.
- Define explicit JSON cycle behavior as a catchable runtime error.
- Use a deterministic cycle marker in debug/string display.
- Ensure debugger snapshot rendering uses the bounded traversal.
- Add cyclic List, Set, Map, SObject/object-field, JSON, and equality tests as
  the current value model permits.

Non-scope:

- Changing Apex collection equality beyond what cycle safety requires.
- Replacing collection storage.
- General JSON feature expansion.

Acceptance:

- The recorded self-referential List reproduction cannot abort.
- Equality terminates for isomorphic and non-isomorphic cyclic graphs.
- Debug output is deterministic and bounded.
- JSON cycles return a typed/catchable error.
- Acyclic output remains compatible with existing tests.

## S0-04 — Execution context and lazy class initialization

**Status:** Blocked by S0-02 and S0-03
**Suggested branch:** `codex/stab-execution-context`
**May run with:** Disjoint frontend or documentation work

Scope:

- Introduce an explicit execution context carrying at least test/debug mode.
- Make `Test.isRunningTest()` false in ordinary runs and true in tests.
- Define async inheritance/isolation for the context.
- Replace global eager static initialization with per-class
  `Uninitialized`, `Initializing`, `Initialized`, and `Failed` state.
- Initialize a class on first semantic/runtime use.
- Detect initialization cycles and cache failure consistently.
- Add ordering, unused-class, failure, and cycle regressions.

Non-scope:

- M20 static initializer blocks.
- M25 API-version profiles.
- M27 sharing/security behavior beyond leaving an extensible context seam.

Acceptance:

- Unused failing classes do not affect unrelated execution.
- Repeated access does not re-run a successful or failed initializer.
- Initialization cycles return a diagnostic/exception without aborting.
- Test mode is correct in ordinary, test, and async test contexts.

## S0-05 — CI, maintainability ratchet, and release-document gates

**Status:** Ready after S0-00 integration
**Suggested branch:** `codex/stab-release-gates`
**May run with:** S0-01 and S0-02

Scope:

- Add CI for Rust fmt/test/clippy and website build/test/lint.
- Run the North Star progress report in CI without pretending ignored future
  goals are ordinary passes.
- Add dependency audit/deny jobs with an explicit advisory policy.
- Check documentation links and formatting.
- Record a Lizard complexity baseline and reject regressions rather than
  immediately failing the existing debt.
- Add contribution, security, code-of-conduct, and release/changelog
  scaffolding where no owner policy choice is required.
- Add missing Cargo metadata except license fields pending owner selection.
- Synchronize README and website milestone status with canonical status.
- Decide or document VS Code extension lockfile/test follow-up.

Non-scope:

- Selecting a license.
- Choosing the supported public Rust API.
- Refactoring production hotspots.
- Suppressing dependency findings without a documented rationale.

Acceptance:

- A pull request cannot merge with failing required verification.
- Complexity above the recorded baseline cannot grow unnoticed.
- Public milestone/status claims agree.
- Remaining owner decisions are explicit blockers, not TODOs hidden in prose.

## S0-GATE — Integrated stabilization verification

**Status:** Blocked by S0-01 through S0-05
**Owned by:** Integration thread

Procedure:

1. Merge reviewed work-package branches into `codex/stabilization`.
2. Resolve conflicts at the abstraction level; do not select one side
   mechanically when runtime context/instrumentation changes overlap.
3. Run all Rust and website verification.
4. Run every reproduction in `FINDINGS.md`.
5. Run new performance/instrumentation assertions.
6. Request a fresh read-only review of the integrated diff.
7. Update status, compatibility, tracker evidence, and commit SHAs.
8. Report readiness to the owner. Do not merge to `main` without that approval.

## S1-01 — Compiler/runtime substrate ADRs

**Status:** Blocked by S0-GATE
**Suggested branch:** `codex/stab-substrate-adrs`

This is an architecture-only package. Produce accepted designs for:

- Lossless syntax `TypeRef` versus resolved `TypeId`
- `UnitId`, `DefId`, class/member IDs, schema object/field IDs
- Per-unit ownership and source-identity remapping
- Lowered executable calls, references, members, and expressions
- Runtime image dispatch and lineage metadata
- Typed runtime exceptions and call frames
- Compatibility/execution/access context propagation
- Intrinsic descriptor catalog boundaries

The ADRs must explain migration slices, cache/versioning implications, and how
M19, M20, M23, M25, M27, and M29 consume the result. A fresh review thread
must challenge the design before implementation.

## S1-02 — Lossless type syntax and typed identities

**Status:** Blocked by S1-01
**Suggested branch:** `codex/stab-typed-identities`

Scope:

- Implement the approved syntax `TypeRef`.
- Preserve qualified segments, arbitrary generic arguments, array suffixes,
  source spelling, and exact spans.
- Replace raw cross-layer declaration/type identities in approved slices.
- Build transactional lookahead from the same grammar rather than duplicating
  `type_end_at`.
- Normalize hierarchy-edge identities and reject duplicate interface
  declarations case-insensitively.
- Validate inherited interface method contracts, including return types,
  before deduplicating requirements.
- Migrate in coherent vertical slices with compatibility tests.

Do not combine persistent cache serialization or all HIR lowering into one PR.

## S1-03 — Runtime image and lowered executable targets

**Status:** Blocked by S1-02
**Suggested branch:** `codex/stab-runtime-image`

Scope:

- Precompute class lineage, dispatch, member, and initialization metadata.
- Replace span-keyed/raw-index executable targets in approved vertical slices.
- Unify callable frame setup.
- Introduce typed runtime exceptions with conversion to public diagnostics at
  the boundary.
- Preserve deterministic behavior and project source mapping.

## S1-04 — `Place` and centralized numeric operations

**Status:** Blocked by S1-02
**Suggested branch:** `codex/stab-place-numerics`

Scope:

- Resolve assignable expressions once into a `Place`.
- Share read/write/mutate behavior across assignment and increment/decrement.
- Centralize numeric coercion, checked arithmetic, comparison, overflow, and
  null behavior.
- Preserve evaluation order and single evaluation.

Exit criterion: M19 can add `Long`, bitwise, shifts, and compound assignment
without duplicating target resolution or numeric policy.

## S1-05 — Intrinsic and compatibility-profile catalog

**Status:** Blocked by S1-03. M25 completed the typed compatibility-profile
transport; the authoritative descriptor catalog and handler split remain.
**Suggested branch:** `codex/stab-intrinsic-catalog`

Scope:

- Create one authoritative intrinsic descriptor catalog.
- Cover ID, owner, name, static/instance style, arguments, return type, profile
  disposition, effects, handler, and documentation/test parity.
- Split runtime handlers by platform family.
- Introduce a typed compatibility profile carried through compile, HIR/runtime
  image, host, oracle, report, and cache boundaries.

Exit criterion: adding an API cannot require unsynchronized edits to independent
semantic/runtime name tables.

## S1-06 — Structured diagnostic model

**Status:** Blocked by S1-03 and S1-05
**Suggested branch:** `codex/stab-structured-diagnostics`

Scope:

- Add stable code, compiler/runtime phase, severity, labels/help, typed primary
  and related locations, and unsupported-capability identity.
- Preserve concise human rendering.
- Stop CI and oracle logic from parsing rendered messages/locations.
- Carry typed runtime exceptions internally and convert at public boundaries.

This package is cross-cutting and must run without concurrent compiler/tool
changes.

## S2-01 — Transaction, host-capability, and DML contracts

**Status:** Blocked by S0-GATE
**Suggested branch:** `codex/stab-data-contracts`

Begin with an ADR. Then:

- Use native transactions/savepoints or bounded journals instead of full
  database snapshots.
- Split host capabilities or make missing correctness behavior explicitly
  unsupported.
- Add a reusable host/storage conformance suite.
- Define structured DML request and per-row outcome types for M24.
- Add rollback/failure-injection, nested-trigger, async, and migration tests.

## S2-02 — Project-scale performance and incremental compilation

**Status:** Blocked by S1-03 and S2-01
**Suggested branch:** `codex/stab-project-scale`

Scope:

- Add cold, unchanged, leaf-change, dependency-change, and restart benchmark
  harnesses.
- Replace avoidable full-AST/Compilation clones with immutable shared units.
- Limit semantic work to the measured invalidation closure.
- Benchmark deterministic Map/Set operations, relationship queries, DML
  checkpoints, static initialization, and async graphs.
- Optimize only against committed reproducible measurements.

Persistent typed/lowered cache serialization remains the final M29 slice.

## S2-03 — LSP/DAP/protocol correctness

**Status:** Blocked by S1-06
**Suggested branch:** `codex/stab-editor-protocols`

Scope:

- Correct UTF-16 LSP position conversion and advertise encoding.
- Use standards-compliant URI encoding/decoding.
- Surface saved-project compilation failures.
- Add protocol message-size limits.
- Split large request dispatchers into typed handlers.
- Add Unicode, reserved-path-character, malformed-message, and stale-document
  tests.

## S2-04 — Open-source release gate

**Status:** Blocked by S0-05 and owner decisions
**Suggested branch:** `codex/open-source-readiness`

Scope after owner decisions:

- Add the selected license and Cargo license metadata.
- Define and document the supported Rust public API or binary-first policy.
- Complete community and security documentation.
- Add reproducible editor/package locks and tests.
- Resolve or explicitly policy-gate dependency advisories.
- Reconcile README, website, status, compatibility, architecture, and roadmap.
- Run a clean clone/build/test/package rehearsal.

Exit criterion: a new external contributor can legally use the project, build
it reproducibly, understand supported behavior, run required checks, and report
security or contribution issues without private context.
