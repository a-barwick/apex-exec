# Roadmap

This roadmap works backward from the product vision in `docs/VISION.md`.
Milestones describe coherent, demonstrable capabilities rather than release
dates. A milestone is complete only when its exit criteria and verification
requirements pass.

Status values: **Complete**, **Active**, **Planned**, and **Deferred**.

The evidence behind the second phase is recorded in
`docs/PHASE_2_BASELINE.md`. North Star percentages in this roadmap always refer
to the 14 pinned lexer/parser indicators; they are not semantic, runtime, or
Salesforce compatibility percentages.

## Phase 1 — Core local development loop

### M1 — Primitive anonymous execution

**Status:** Complete

#### Scope

- Lexer, parser, AST, semantic analysis, and tree-walking execution
- Explicitly initialized `String`, `Boolean`, and `Integer` variables
- Assignment and variable references
- Case-insensitive name lookup
- `System.debug(variable)` with plain stdout output
- `tokens`, `ast`, `check`, and `run` commands
- Source-span diagnostics

#### Exit criterion

```apex
String message = 'Hello, world!';
System.debug(message);
```

prints `Hello, world!` and all compiler stages can inspect the program.

### M2 — Expressions and control flow

**Status:** Complete

#### Scope

- Arithmetic, comparison, equality, and Boolean operators
- Apex operator precedence and associativity
- String concatenation
- Prefix and postfix increment/decrement
- `null`
- Blocks and nested lexical scopes
- `if`/`else`, `for`, `while`, and `do` statements
- `break`, `continue`, and `return`
- Apex-compatible assignment checks for the supported primitive types

#### Non-scope

- Collections and generic types
- User-defined methods and classes
- SOQL, SOSL, and DML

#### Exit criterion

```apex
Integer total = 0;
for (Integer i = 0; i < 10; i++) {
    total = total + i;
}
System.debug(total);
```

prints `45`, with tests covering precedence, scopes, loop control, and invalid
operand types.

### M3 — Collections and core standard library

**Status:** Complete

#### Scope

- `List<T>`, `Set<T>`, `Map<K,V>`, and array syntax
- Generic type checking, construction, access, mutation, and iteration
- Common collection methods
- Essential `String`, `Math`, and `System` methods
- Method-call expressions

#### Exit criterion

The original project acceptance program runs unchanged:

```apex
List<String> strs = new List<String>();

for (Integer i = 0; i < 100; i++) {
    String s = String.valueOf(i);
    strs.add(s);
}

System.debug(String.join(strs, ''));
```

### M4 — Methods, exceptions, and runtime correctness

**Status:** Complete

#### Scope

- Method declarations, parameters, calls, and return values
- Overload resolution
- Recursion
- `try`, `catch`, `finally`, and `throw`
- Core Apex exception types
- Null dereference, invalid cast, bounds, and arithmetic failures
- Source-mapped runtime stack traces

#### Exit criterion

Multi-method programs compile with static type checks and report runtime
failures with useful Apex source stacks.

### M5 — Classes and project compilation

**Status:** Complete

#### Scope

- SFDX project discovery and `.cls` loading
- Classes, constructors, fields, properties, and methods
- Static and instance members
- Access modifiers
- Interfaces, inheritance, abstract/virtual methods, and overrides
- Cross-file resolution and dependency graphs
- Incremental project compilation

#### Exit criterion

An ordinary multi-file Apex service layer compiles and can be invoked locally.

The milestone introduces the typed HIR boundary described in
`docs/ARCHITECTURE.md` before expanding class/member resolution.

### M6 — Apex test runner

**Status:** Complete

#### Scope

- `@isTest`, test discovery, test methods, and setup methods
- Assertions and expected failures
- Per-test isolation
- Filtering and deterministic execution
- JUnit output and line/branch coverage
- Parallel execution where semantic isolation permits it

#### Exit criterion

`apex-exec test force-app` runs a useful subset of a real project's unit tests
without an org.

This is the first major enterprise-value checkpoint.

### M7 — SObject schema and SQLite

**Status:** Complete

#### Scope

- Import SFDX custom object and field metadata
- Generate and migrate a local SQLite schema
- Typed and dynamic SObjects
- Salesforce-style IDs and field access
- Relationships and storage-level transactions, savepoints, rollback, and
  fixtures
- Fast database reset for test isolation

#### Exit criterion

SFDX metadata produces a normalized object schema, and SQLite-backed platform
storage can create, retrieve, update, and delete records in isolated local
transactions. Apex query and DML syntax remain M8 work.

### M8 — SOQL, SOSL, and DML

**Status:** Complete

#### Scope

- Dedicated SOQL and SOSL grammars
- Static query validation and bind expressions
- Common filtering, ordering, limiting, aggregation, and relationship queries
- DML statements and common `Database` methods
- Structured query and DML traces

#### Exit criterion

Common repository and service-layer Apex runs against SQLite without source
changes.

### M9 — Triggers and transaction semantics

**Status:** Complete

#### Scope

- Trigger syntax and context variables
- Before/after insert, update, delete, and undelete
- Bulk behavior and recursive trigger execution
- Apex DML/trigger rollback semantics and deterministic execution timelines

#### Exit criterion

Common trigger-handler architectures run locally with realistic bulk and
rollback behavior.

This is the second major enterprise-value checkpoint.

### M10 — Curated platform compatibility

**Status:** Complete

Implements a first common Apex platform profile based on real project usage
rather than attempting every Salesforce API:

- `Date`, `Datetime`, `Time`, `Decimal`, `Id`, `Blob`, and `Object`
- JSON, regex, schema describe, and common `Test` and `Limits` methods
- Deterministic time, IDs, randomness, and user context
- Mockable HTTP callouts

Unsupported APIs must produce structured errors naming the missing API and the
active compatibility profile.

#### Exit criterion

A project can use the supported value types, JSON and regex utilities, schema
describe, deterministic context and limits, and host-mocked HTTP callouts
without source changes. Invalid inputs are catchable runtime failures and APIs
outside the curated surface identify the `m10-common` profile explicitly.

### M11 — Deterministic asynchronous execution

**Status:** Complete

#### Scope

- Checked `Queueable`, `Database.Batchable<T>`, and `Schedulable` contracts
- Checked `@future` methods with serializable primitive collection arguments
- Deterministic `System.enqueueJob`, `Database.executeBatch`, and
  `System.schedule`
- Asynchronous `EventBus.publish` delivery to platform-event triggers
- Salesforce-shaped job IDs and Queueable, batch, and scheduled contexts
- Enqueue-time payload snapshots, FIFO execution, per-job transaction
  checkpoints, bounded draining, and structured lifecycle events
- Explicit draining through `Test.stopTest`; no background threads or implicit
  wall-clock scheduling

#### Exit criterion

An SFDX project can enqueue Queueable, future, batch, scheduled, and platform
event work, drain it deterministically inside an Apex test, observe its database
effects, and reproduce the same job order and IDs on every run.

### M12 — Debugger, REPL, and editor integration

**Status:** Complete

- Persistent REPL state
- Debug Adapter Protocol
- Language Server Protocol
- Breakpoints, stepping, frames, variables, and database inspection
- Go-to-definition, references, rename, and inline diagnostics
- Coverage overlays and transaction timelines

#### Exit criterion

Developers can use Apex Exec as their normal inner loop from a supported editor.

### M13 — Salesforce compatibility oracle

**Status:** Complete

Run identical conformance fixtures locally and in a scratch org, then compare:

- Compile success and diagnostics category
- Values, output, exceptions, and stack behavior
- SOQL results and DML effects
- Trigger order and test outcomes

Every discovered difference becomes a permanent fixture. Compatibility must be
reported as measured coverage, not asserted as a blanket claim.

### M14 — Enterprise CI

**Status:** Complete

- Incremental compilation and impacted-test selection
- Content-addressed caches, sharding, and distributed workers
- Hermetic manifests and deterministic replay
- SARIF, JUnit, and coverage reports
- GitHub, GitLab, and Jenkins integration
- Performance and compatibility policy gates

#### Exit criterion

A large Apex repository can validate a pull request without provisioning an org.

### M15 — Hybrid deployment confidence

**Status:** Complete

- Optional validation-org authentication
- Affected-component and affected-test selection
- Local-versus-org differential results
- Schema and configuration drift detection
- Release-readiness reports

#### Exit criterion

The hermetic M15 example and simulated provider transport verify affected
selection, drift detection, local/org differential normalization, snapshot
replay, and release-readiness decisions. The authenticated transport is
implemented, but this milestone does not claim a completed live-org run.

M14 and M15 are complete as implementation milestones with hermetic examples
and simulated provider transport. Their enterprise-scale and live-org outcome
claims are not yet empirical product evidence; M17 and M22 supply those gates.

## Phase 2 — Compatibility expansion and hardening

Phase 2 converts the implemented M1–M15 foundation into measured real-project
confidence. Its primary language gate is **14 of 14 North Star lexer/parser
indicators passing against the unmodified pinned corpus**. Phase completion
also requires live candidate-bound Salesforce evidence, a representative
enterprise benchmark, broader metadata accounting, selected platform fidelity,
and project-scale incremental performance.

The milestones are deliberately ordered. M16 starts with ternary and
`instanceof`; M17 then opens the real staging-org validation track. M17 cannot
be completed until an authorized validation-org alias is supplied. Before M18
feature implementation, the S0 stabilization gate closes the process-safety,
silent-correctness, instrumentation, and maintainability findings recorded in
`docs/STABILIZATION.md`. M17 remains a hard dependency for the profile,
metadata, and final evidence gates; later local milestones must not be declared
as substitutes for it.

| Milestone | Primary gate | Depends on |
|---|---|---|
| M16 | Ternary and `instanceof` complete slices | M15 |
| M17 | Candidate-bound live validation | M16 and a supplied org |
| S0 | Phase 2 process-safety and maintainability stabilization | M17 |
| M18 | Safe navigation and null coalescing | M16, S0 |
| M19 | Bitwise/shift closure; lexer 7/7 | M18 |
| M20 | Nested declarations, enums, and type literals | M19 |
| M21 | North Star lexer/parser 14/14 | M16, M18–M20 |
| M22 | Frozen representative enterprise baseline | M21 |
| M23 | Enterprise-prioritized SOQL breadth | M22 |
| M24 | Partial DML result fidelity | M20, M22 |
| M25 | API-version compatibility profiles | M17, M22 |
| M26 | Complete metadata accounting | M17, M22, M25 |
| M27 | Sharing and security profiles | M22, M25, M26 |
| M28 | Measured enterprise compatibility closure | M23–M27 |
| M29 | Dependency-scoped semantic work and persistent IR | M21–M28 |
| M30 | Phase 2 evidence and compatibility gate | M16–M29 |

### M16 — Conditional and runtime-type expressions

**Status:** Complete

#### Scope

- Ternary `condition ? whenTrue : whenFalse` tokens, AST, precedence, typing,
  short-circuit execution, diagnostics, and branch coverage
- `value instanceof Type` parsing, checked type relationships, generic/runtime
  behavior, and single evaluation of its value expression
- Focused lexer, parser, semantic, runtime, and differential fixtures
- A refreshed first-diagnostic report for every North Star source

Both operators must be complete language slices. Merely accepting their tokens
or moving a corpus diagnostic is insufficient.

#### Exit criterion

Ternary and `instanceof` execute with checked Apex-shaped behavior, invalid
uses fail in the owning compiler phase, and neither construct remains a first
North Star blocker.

### M17 — Candidate-bound live Salesforce validation

**Status:** Complete

#### Scope

- A versioned validation-evidence schema bound to the exact M14 manifest and CI
  result, changed paths, affected component selectors and digests, selected
  tests, test level, target org, API version, Apex Exec version, Salesforce CLI
  version, capture time, and retrieved-inventory digest
- Replay rejection for any candidate, request, org, API/tool version, or
  maximum-age mismatch
- A real `sf` run against a user-supplied disposable or staging validation org
- A sanitized, reviewable validation snapshot and readiness report containing
  no credentials, auth URLs, or access tokens
- At least one controlled live blocker and repeated clean retrievals to verify
  normalization and drift fidelity

Fake-CLI tests remain the hermetic transport regression suite. They do not
count as live Salesforce evidence, and hybrid evidence alone does not promote
language behavior to **Exact** without matching M13 conformance fixtures.

The version-2 schema, replay enforcement, repeated-retrieval guard, CLI, and
hermetic regression suite are implemented. The reviewed bundle in
`evidence/milestone17/` binds one exact candidate to a user-supplied disposable
Developer Edition. The final authenticated capture and offline replay are
release-ready with two matching retrievals, a passing check-only deployment,
two of two selected methods matching, and no drift. A controlled unchanged
PermissionSet mutation keeps deployment and tests green but produces one drift
finding and blocks release. The org baseline was restored and the clean capture
was repeated afterward.

#### Exit criterion

A reviewed Salesforce evidence bundle for one exact sealed candidate passes
both the authenticated run and offline replay. A controlled drift or deployment
failure blocks release, and altered or expired evidence cannot be replayed.

### S0 — Phase 2 stabilization gate

**Status:** Complete

#### Scope

- Eliminate the recorded interface-cycle and runtime-value process aborts
- Preserve or explicitly reject generic arguments rather than erasing them
- Correct the recorded cast/group, custom-array, and parser-invariant failures
- Make debugger/coverage instrumentation explicit, opt-in, and bounded
- Introduce an execution context and lazy per-class static initialization
- Add CI, dependency, complexity-regression, and public-documentation gates
- Preserve the evidence, work queue, dependencies, and handoff state in
  `docs/STABILIZATION.md` and `docs/stabilization/`

S0 is a bounded stabilization gate, not a rewrite or substitute for the
compatibility milestones.

#### Exit criterion

Every S0 criterion in `docs/STABILIZATION.md` passes on the integrated
stabilization branch, the recorded reproductions cannot abort or silently
approximate behavior, the full verification suite is green, and the owner
approves resuming M18.

### M18 — Null-aware expressions

**Status:** Complete

#### Scope

- Safe-navigation member and method access with single evaluation of receivers
- Null coalescing `left ?? right` with lazy right-hand execution
- Apex precedence, associativity, type joins, chaining, and assignment
  interactions
- Null, side-effect, exception, and coverage fixtures through every compiler
  phase

#### Exit criterion

Safe navigation and null coalescing have checked runtime semantics and no longer
block any North Star source.

### M19 — Bitwise, shift, and compound operators

**Status:** Planned

#### Scope

- Boolean and numeric `&`, `|`, and `^`, unary `~`, `<<`, `>>`, and `>>>`
- Arithmetic and bitwise compound assignments, including indexed/member
  lvalues and evaluate-once behavior
- `Long` values and `L` literals needed for Apex shift width and unsigned-shift
  behavior
- Maximal-munch lexing that still parses adjacent nested-generic `>` closers
- Overflow, sign-extension, precedence, and differential fixtures

#### Exit criterion

`Puff.cls` bit operations and the Boolean non-short-circuit operator in
`fflib_SObjectDomain.cls` are represented correctly, and all seven North Star
lexer indicators pass and are promoted into the ordinary test suite.

### M20 — Nested declarations, enums, and type literals

**Status:** Planned

#### Scope

- Nested classes and interfaces with canonical qualified identities, access,
  inheritance, construction, and runtime dispatch
- Enums with constants, equality, `name`, `ordinal`, `values`, and `valueOf`
- Custom exception subclasses and explicit `this(...)`/`super(...)`
  constructor chaining
- Class/type literals, including qualified, array, and generic type literals
- Arbitrary generic type references such as `Iterable<T>` plus static and
  instance initializer blocks
- Dependency, HIR target, source-map, editor, and runtime support for nested
  declarations

#### Exit criterion

Nested types and enums compile and execute in multi-file projects without
flattening names or weakening access rules, with focused conformance tests for
the forms used by the North Star corpus.

### M21 — North Star grammar closure

**Status:** Planned

#### Scope

- A checked-in, comment-aware census of every remaining grammar form in the
  seven pinned sources
- Non-test annotations and their arguments, `switch on`/`when`, remaining
  modifiers including `final` and `transient`, uninitialized and
  multi-declarator locals, multi-initializer/update `for`, external-ID DML
  syntax, and remaining corpus query forms
- Lossless AST nodes and source spans rather than token skipping, source
  rewriting, or permissive recovery
- A disposition for each accepted construct: executable, checked-only, or an
  explicit semantic unsupported diagnostic

Parser acceptance is intentionally distinct from runtime compatibility. A
14-of-14 result proves complete lexing/parsing of this corpus only.

#### Exit criterion

All 14 North Star lexer/parser indicators pass against byte-identical fixtures,
all `#[ignore]` attributes are removed from those goals, and
`cargo test --test north_star` passes in the ordinary suite.

### M22 — Representative enterprise baseline

**Status:** Planned

#### Scope

- A user-approved project pinned by commit or content digest, with substantial
  production code, at least 100 ordinary Apex test methods, and representative
  schema, SOQL, DML, trigger, async, and platform use
- Every ordinary test method declared in the pinned package roots and
  discovered by Salesforce, frozen as the raw denominator before local results
  are inspected
- Any pre-approved, source-agnostic exclusion rubric produces only a secondary
  filtered view; the Phase 2 percentage always uses the raw denominator, and
  unsupported syntax/APIs, local failures, and mismatches remain in it
- Byte-identical local source with no compatibility patches
- Per-test required-source closures and separate discovery, parse, check,
  execution, and Salesforce-outcome agreement rates plus a machine-readable
  blocker taxonomy
- One strict compatibility numerator: denominator tests whose required source
  closure checks, whose execution reaches a terminal result, and whose
  normalized outcome agrees with the pinned Salesforce outcome; matching
  failures are reported separately from matching passes
- Three deterministic local reruns and cold/warm timing evidence

North Star fixtures and the small milestone examples cannot serve as this
benchmark.

#### Exit criterion

The repository contains a reproducible, candidate/API/tool-bound baseline that
reports every in-scope test, keeps unsupported failures in the denominator, and
orders the remaining compatibility work by measured project impact.

### M23 — Broader SOQL fidelity

**Status:** Planned

#### Scope

- Child subqueries, `HAVING`, date literals, and the relationship/query forms
  selected by the M22 blocker census
- `TYPEOF` and polymorphic relationship behavior where the representative
  schema supplies differential evidence
- Enterprise-prioritized dynamic `Database.query`/`countQuery`, bind, and
  `QueryLocator` behavior
- Checked plans, SQLite execution, query traces, and explicit rejection for
  still-unmodeled clauses
- Local/Salesforce conformance fixtures for cardinality, nulls, ordering,
  aggregation, and relationship hydration

#### Exit criterion

The prioritized enterprise query slice checks and executes without source
changes, has reviewed differential evidence, and introduces no string-parsing
shortcut in the interpreter.

### M24 — Partial DML results and bulk failure fidelity

**Status:** Planned

#### Scope

- `Database.SaveResult`, `UpsertResult`, `DeleteResult`, `UndeleteResult`, and
  structured errors
- `allOrNone=false` per-record success/failure, stable input ordering, IDs, and
  external-ID upsert behavior
- Per-row input index, success ID, upsert-created flag, and error
  status/message/fields; generated IDs reach caller records only on success
- Correct trigger, rollback, limit, and transaction behavior for mixed bulk
  outcomes
- Atomic defaults preserved for statements and `allOrNone=true`

#### Exit criterion

Mixed-success bulk DML returns Salesforce-shaped per-row results and preserves
the documented transaction/trigger boundary, verified locally and through M13
differential fixtures.

### M25 — API-version compatibility profiles

**Status:** Planned

#### Scope

- An explicit profile selected from project/source API metadata
- Project-default versus class/trigger sidecar API-version precedence, including
  mixed-version projects
- Version-gated syntax, conversions, platform APIs, diagnostics, and runtime
  behavior behind compiler/host boundaries rather than scattered conditionals
- Profile identity in cache keys, oracle fixtures, validation evidence, and
  reports
- Explicit failure when a requested version or behavior has no modeled profile

#### Exit criterion

At least two supported API-version profiles demonstrate a reviewed behavioral
difference locally and against Salesforce. A mixed-version project proves
project-default and sidecar precedence, and every checked unit, cached result,
or replayed result is bound to its effective profile.

### M26 — Metadata inventory and org-configuration breadth

**Status:** Planned

#### Scope

- A complete Metadata API catalog for every M25 profile, covering parent/child,
  bundle, folder, decomposed, sidecar, namespace, and multi-part full-name
  conventions
- Every catalog type carries an explicit inventory, retrieve, deploy, drift,
  and local-semantics capability state
- An explicit disposition for every package-root file: recognized metadata,
  intentional non-metadata, or unsupported metadata with a reason
- No silent omission of unknown unchanged metadata from drift accounting
- Separate metrics for file accounting, component inventory,
  retrieve/deploy/drift support, and local runtime semantics
- Profiled org-only discovery with explicit inaccessible/non-retrievable
  findings

#### Exit criterion

The representative project has zero unclassified files and 100% component
accounting. Reports publish separate denominators and percentages for catalog
types, package-root files, component inventory, retrieve/deploy/drift support,
and local runtime semantics. Two clean live inventories have zero unexplained
drift, while controlled org-only addition, removal, and mutation each produce
the expected finding. Unsupported Metadata API types remain measured and
explicit.

### M27 — Sharing and security profiles

**Status:** Planned

#### Scope

- Executable `with sharing`, `without sharing`, and `inherited sharing`
  propagation
- Deterministic users, ownership, roles/groups, record visibility, CRUD, and
  field-level security fixtures
- `Security.stripInaccessible`, security-enforced/user-mode query behavior, and
  user/system DML access levels prioritized by the enterprise benchmark
- Explicit limits for automation, restriction rules, managed packages, and
  other org behavior not modeled locally

#### Exit criterion

Reviewed fixtures match Salesforce for effective sharing propagation across
call boundaries, user/system query and DML modes, object CRUD, field access,
`Security.stripInaccessible`, and the scoped owner/OWD/explicit-share
visibility model. Unsupported security behavior fails explicitly rather than
running as system mode.

### M28 — Measured enterprise compatibility closure

**Status:** Planned

#### Scope

- Close the highest-impact remaining M22 blocker families not already owned by
  M23–M27, without changing benchmark source, denominator, or expected
  Salesforce outcomes
- Implement complete, focused language/platform slices with executable
  regression and differential fixtures; keep unmodeled behavior explicit
- Refresh the per-test compatibility funnel after every slice and publish
  matching-pass, matching-failure, mismatch, and unsupported counts separately
- Preserve the ordinary 14-of-14 North Star gate throughout

#### Exit criterion

At least 60% of the frozen representative denominator satisfies the strict M22
compatibility numerator, with 80% retained as the stretch target. The result is
repeatable across three clean local runs without source changes or exclusions
based on Apex Exec limitations.

### M29 — Incremental typed compilation and persistent IR cache

**Status:** Planned

#### Scope

- Dependency-scoped name resolution, typing, and linking after a changed unit
- A stable, versioned typed or lowered executable IR with persistent
  source-identity remapping inside the existing content-addressed boundary
- Corruption, tool/profile/schema drift, and dependency mismatch rejection
- Cold, no-change, leaf-change, shared-dependency, and restart benchmarks on
  the M22 project

#### Exit criterion

A warm process restart can reuse verified typed/lowered IR without lexing,
parsing, or project-wide semantic linking; a leaf edit rechecks only its
measured invalidation closure. Results and diagnostics remain byte-for-byte
deterministic, with changed-test feedback meeting the documented
single-digit-second target on reference hardware.

### M30 — Phase 2 enterprise compatibility gate

**Status:** Planned

#### Scope

- Refresh live Salesforce, metadata, compatibility-profile, and performance
  evidence for the release candidate
- Reconcile README, status, compatibility, architecture, specifications, and
  public roadmap copy

#### Exit criterion

Phase 2 is complete only when:

- all 14 North Star lexer/parser indicators remain ordinary passing tests;
- at least 60% of the frozen raw representative denominator satisfies the
  strict M22 compatibility numerator, with 80% as the stretch target;
- a current M17 candidate-bound live validation passes;
- current M26 evidence has no unclassified project files, reports every
  catalog/file/component capability denominator, and reproduces the required
  live org-only/drift cases;
- supported API/security claims have reviewed differential evidence;
- the M29 cold/warm/restart/invalidation benchmarks meet their documented
  correctness and latency targets; and
- the complete required verification suite and relevant CLI examples pass.

## Product checkpoint

The decisive goal is not complete emulation of Salesforce. It is:

> A representative enterprise project can run 60–80% of its ordinary Apex unit
> tests locally, quickly, deterministically, and without source changes.

Achieving that threshold changes the cost and speed of Salesforce development;
later milestones increase the percentage and confidence.
