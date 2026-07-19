# Architecture

## Current pipeline

```text
Apex source
    │
    ▼
  Lexer ──► tokens with file-aware byte spans
    │
    ▼
  Parser ─► immutable untyped AST
    │
    ▼
Semantic checker ─► checked HIR (syntax plus typed resolution side tables)
    │
    ▼
Tree-walking interpreter ─► values, objects, and debug output
    │
    ▼
Isolated test runner ─► deterministic results, JUnit, and coverage
    │
    ▼
Platform kernel ─► normalized schema, checked queries/DML, and SQLite transactions
    │
    ▼
REPL / DAP / LSP ─► persistent inner loop and source-mapped editor services
    │
    ▼
Compatibility oracle ─► normalized local/Salesforce snapshots and measured diffs
    │
    ▼
Hybrid readiness ─► affected validation, drift findings, and release decision
    │
    ▼
Enterprise baseline ─► frozen Salesforce denominator and per-test impact census
```

The public library entry points in `src/lib.rs` deliberately expose each phase:

- `tokenize`
- `parse`
- `check`
- `execute`
- `project::discover`
- `project::compile` / `ProjectCompiler::compile`
- `test_runner::run`

M5 keeps parsed syntax immutable and introduces a checked HIR program. Semantic
analysis records expression types and selected top-level, constructor, static,
instance, super, field, and property targets in side tables keyed by source
span. The runtime executes those targets directly. Dynamic values never repeat
or change compiler overload/member resolution.

Project compilation discovers SFDX package directories, assigns each cached
path a stable `SourceId`, and keeps every parsed span local to its own file.
The project façade delegates discovery, dependency collection, and diagnostic
mapping to focused modules. Dependency collection uses the shared AST visitor,
and the source map resolves diagnostics, coverage, and individual runtime stack
frames by source identity. An unchanged input set reuses the complete checked
program; after a change, unchanged parsed units are retained and reverse
dependents are identified before project-wide semantic linking.

M7 imports custom-object metadata from those package directories before
semantic linking. The normalized catalog is attached to checked HIR, so custom
object types, field targets, and dynamic SObject calls execute without exposing
SQLite types to the checker or interpreter. Metadata-only changes invalidate
the complete checked-build cache even when every `.cls` fingerprint is stable.

M8 adds dedicated SOQL/SOSL AST nodes and schema-indexed checked query plans.
Runtime evaluation supplies concrete bind values and converts between
interpreter SObjects and storage-neutral platform requests. Filtering,
ordering, aggregates, relationship hydration, deterministic SOSL matching, and
atomic DML validation live above SQLite in the platform database service.

M9 adds trigger declarations as project source units and typed trigger-context
member targets in HIR. The interpreter owns trigger dispatch because it owns
Apex values, handler calls, and recursive control flow. The platform database
preflights old/new record images, owns the recycle bin, and snapshots active
records, recycled records, and ID sequences. Nested host checkpoints make one
DML tree atomic while an outer entry-point checkpoint provides uncaught
transaction rollback. Trigger enter/exit and DML events share one deterministic
host timeline.

M10 extends the checked intrinsic boundary with a closed curated platform API
set. Scalar date/time/decimal/ID values live directly in interpreter values;
stateful regex, Blob, describe, and HTTP objects live in the execution store by
identity. The host supplies deterministic clock, pseudo-random, user, limits,
and callout services. A default host never performs network I/O: tests enqueue
responses and can inspect captured requests. Unknown platform calls fail during
checking with the active `m10-common` profile rather than reaching dynamic
runtime lookup.

M11 adds checked async interface contracts and future annotations to HIR. The
interpreter owns a bounded FIFO because it already owns serializable Apex value
identity, call execution, coverage, and trigger dispatch. Submission deep-copies
the reachable payload graph into enqueue-time snapshots and emits a structured
queued event. `Test.stopTest` explicitly drains jobs in deterministic order;
each job runs inside a nested platform transaction checkpoint and emits
started/completed/failed events. No worker thread or wall-clock scheduler exists.

S0-04 adds an explicit private execution context beside, not inside, runtime
instrumentation. Ordinary and debugger entry points select non-test mode; the
test runner selects test mode. Queued work captures the submitting context,
installs it around its transaction, and restores the caller's context on every
result. The existing deterministic async profile still shares its
`ExecutionStore`; the context seam does not claim Salesforce cross-transaction
static isolation.

Static storage is now lazy per class. First active use initializes base classes
first, allocates every static field and auto-property slot for one class to
typed null, and evaluates that class's field initializers in source order.
Explicit `Uninitialized`, `Initializing`, `Initialized`, and
`Failed(Diagnostic)` states cache both success and failure. An active class
stack permits legal same-class helper/default-order access, detects cross-class
reentry, and enforces a 64-class dependency-depth budget.

M12 adds deterministic statement-boundary debug snapshots above the runtime.
An explicit runtime instrumentation policy keeps ordinary execution at
`None`, test execution at `Coverage`, and debugger launches at `Debugger`.
Only debugger launches render scopes and retain snapshots. Their immutable
trace keeps the earliest pre-statement observations and is bounded to 4,096
snapshots, an estimated 16 MiB of retained snapshot structures and text, 256
variables and 128 frames per snapshot, and 16 KiB per rendered value.
`DebugExecution::trace_status` reports retained bytes and any truncation.
Semantic String conversion, bounded presentation rendering, JSON
serialization, equality, and debugger capture share runtime value-graph
traversal state. Debug and debugger presentation use fixed depth, node,
element, and UTF-8 output budgets with deterministic `<cycle>` and `…`
markers. Semantic conversion preserves complete scalar and nested String
content while retaining the structural budgets and cycle marker. JSON uses
the same identity path and structural budgets but turns cycles or exhausted
budgets into catchable `IllegalArgumentException` values. Collection equality
uses an explicit work stack and a rollback-aware visited-pair trail, so cyclic
and deeply nested graphs do not consume the host call stack.
Debug Adapter Protocol clients navigate the completed immutable trace, so
editor response timing never changes Apex execution. The Language Server
Protocol adapter uses checked HIR targets and the project source map for
definitions, references, rename, diagnostics, and coverage overlays. The
persistent REPL commits a snippet only after the accumulated source checks and
executes successfully, then reconstructs state by deterministic replay.

M13 adds a provider-neutral differential boundary above project compilation,
runtime hosting, and the test runner. Versioned fixture manifests select a
compile, static invocation, or test entry point plus the dimensions meaningful
for that fixture. Local structured host events and Salesforce CLI JSON/logs
normalize into the same snapshot model. Recorded Salesforce snapshots are
durable conformance evidence and allow deterministic offline comparison; live
scratch-org transport remains isolated in the oracle adapter.

M15 composes the M14 hermetic manifest, compiler dependency graph, isolated
test runner, and Salesforce transport above their existing boundaries. It
normalizes SFDX source files into metadata components, selects changed classes
and their reverse dependents, retrieves only affected code plus project-owned
schema/configuration, and performs a check-only Salesforce deployment.
Versioned validation snapshots preserve the same provider-neutral inventory,
test, and deployment observations for deterministic offline replay.

M17 replaces the portable-but-unbound M15 snapshot with a sealed evidence
envelope. The hybrid layer hashes the serialized M14 manifest and exact cached
CI result, canonicalizes the affected request, records org/API/tool/time
provenance, and binds the retrieved inventory and full snapshot by SHA-256.
Authenticated capture retrieves the same scope twice and stops before deploy
when normalized inventory digests differ. Replay remains above compiler and
runtime phases, requires the exact M14 cache artifact, and rejects identity or
age mismatches before readiness evaluation. The reviewed M17 bundle preserves
the clean authenticated, exact replay, and controlled-drift blocker outcomes
for one sealed candidate without adding org behavior to language phases.

M22 adds a representative-project evidence layer above the same boundaries.
An immutable third-party candidate manifest is captured before local
compatibility inspection, and Salesforce `RunLocalTests` supplies the raw,
method-qualified denominator. The enterprise runner computes bounded per-test
Apex source closures, invokes the ordinary parser, semantic checker, and
isolated test runner, then records separate stage rates and terminal-outcome
agreement. It never moves org transport into the lexer, parser, checker, or
runtime. Unsupported behavior stays in the denominator and is classified by
the stage that rejected it. See
`docs/specifications/enterprise-baseline.md` and ADR 0025.

M16 adds dedicated conditional and runtime-type AST nodes without moving
semantic state into parsed syntax. The checker records each expression's
result type in the existing HIR side table, computes ternary joins from the
supported assignment/subtype relation, and validates `instanceof` with a
separate runtime-type relation so numeric assignment promotion cannot become
runtime identity. The interpreter evaluates only the chosen ternary arm and
evaluates an `instanceof` value once against execution-store type identity.

M18 adds maximal-munch `?.` and `??` tokens plus explicit null-coalescing AST
nodes and navigation metadata on member/method nodes. Safe navigation remains
checked against the ordinary typed HIR member/call target; static targets and
mutation are rejected, while a null receiver short-circuits the remaining
navigation chain without evaluating arguments. Null coalescing is
left-associative, reuses the checked type-join relation, evaluates its left
operand once, and evaluates its right operand only for null. HIR marks only
single-record SOQL used by a null-aware expression as empty-result tolerant, so
ordinary single-record queries preserve their `QueryException` behavior.
Coverage records null and present outcomes for both operators.

The CLI is a thin adapter over those functions.

M6 discovers tests from checked annotation metadata and executes each test in
its own interpreter. Setup methods share that test's interpreter and run before
the test method. Each test receives a fresh execution store, default recording
host, call stack, and coverage-only instrumentation trace; debugger snapshots
are never allocated by the test runner. The bounded worker pool therefore does
not share observable runtime state. Results are sorted by case-insensitive
qualified test name after execution so parallel scheduling is never observable
in reports.

The interpreter records executed statement spans and true/false statement or
ternary conditional outcomes. The test runner discovers ternary conditions
through the shared immutable AST visitor, maps observations through the
project source map, excludes `@IsTest` classes from the production denominator,
and owns console/JUnit rendering. Test policy and report formats do not leak
into parser, semantic, or ordinary execution entry points.

Checked built-in calls carry a typed `IntrinsicId` in HIR, just like
user-defined calls carry a selected declaration target. Runtime dispatch
therefore never repeats case-insensitive built-in lookup. An interpreter
borrows immutable checked code through a `RuntimeImage`; its execution scopes
and traces remain isolated, while an `ExecutionStore` owns its collection
arena, object arena, lazy class states, and static slots. `System.debug` crosses a structured
`PlatformHost` boundary whose default owned host records output for the
existing convenience APIs. A custom host may intentionally share external
state between interpreters.

## Current modules

| Module | Responsibility |
|---|---|
| `span` | File-aware source identities and local byte ranges |
| `token` | Token kinds and lexical spelling |
| `lexer` | Source-to-token conversion and lexical errors |
| `ast` | Parsed program representation and shared immutable visitor |
| `hir` | Checked expression types, declaration targets, and intrinsic IDs |
| `parser` | Grammar façade with declaration, statement, expression, type/lookahead, and test modules |
| `parser::queries` | Dedicated SOQL/SOSL grammar and DML statement parsing |
| `parser::declarations` | Class, interface, member, annotation, and trigger declaration grammar |
| `semantic` | Compiler façade with declaration/body checking, shared overload ordering, and intrinsic validation |
| `runtime` | Execution façade, borrowed runtime image, mutable execution store, platform host, intrinsic execution, environments, and values |
| `runtime::context` | Ordinary, test, and debugger execution mode plus deterministic async inheritance |
| `runtime::class_initialization` | Lazy per-class state model and bounded dependency depth |
| `runtime::instrumentation` | Explicit none/coverage/debugger policy, coverage facts, and bounded debugger snapshot retention |
| `runtime::value_graph` | Cycle-aware, budgeted value rendering and JSON traversal plus iterative collection equality |
| `project` | Compilation façade over discovery, source-unit caching, dependency graphs, and diagnostic source mapping |
| `platform` | Storage-independent normalized schema and transactional record-storage contracts |
| `platform::metadata` | SFDX custom-object and field metadata import |
| `platform::sobject` | Schema-validated typed/dynamic SObject values at the platform boundary |
| `platform::sqlite` | Schema migration and transactional SQLite record persistence |
| `platform::database` | Query execution, aggregates, DML preflight, recycle-bin state, and transaction snapshots |
| `runtime::database` | Query/DML conversion plus typed bulk trigger context construction and recursive dispatch |
| `runtime::asynchronous` | Enqueue-time payload snapshots, deterministic FIFO jobs, explicit draining, batch chunking, and platform-event delivery |
| `test_runner` | Test discovery, isolated scheduling, filtering, reporting, and coverage aggregation |
| `repl` | Transactional accumulated-source REPL state and deterministic replay |
| `debugger` | Breakpoint, stepping, frame, variable, database, and timeline navigation over runtime snapshots |
| `editor` | Checked project symbol indexing, rename edits, inline diagnostics, and line coverage overlays |
| `protocol` | Shared LSP/DAP `Content-Length` JSON message framing |
| `lsp` | Stdio Language Server Protocol requests and document diagnostics |
| `dap` | Stdio Debug Adapter Protocol launch and inspection workflows |
| `oracle` | Versioned conformance manifests, local/Salesforce adapters, normalized snapshots, differential reports, and measured compatibility coverage |
| `ci` | Hermetic input manifests, content-addressed result artifacts, impacted-test selection, deterministic sharding, standard reports, and policy gates |
| `hybrid` | Metadata-component inventory, affected deployment selection, validation-org transport, schema/configuration drift, test differential, and release readiness |
| `enterprise` | Frozen representative-project capture, per-test source closures, stage metrics, and impact-ordered blocker census |
| `diagnostic` | User-facing source diagnostics |
| `main` | CLI argument and filesystem handling |

## Invariants

### Names

Apex names are case-insensitive. Every source identifier carries:

- `spelling`: the exact source text used in diagnostics and AST inspection
- `canonical`: a case-normalized lookup key
- `span`: its original source range

Only the lookup key is canonicalized. Diagnostics must preserve source spelling.

### Phase ownership

- The lexer does not decide program meaning.
- The parser does not resolve names or execute code.
- Semantic analysis rejects invalid names and types before execution.
- The runtime does not compensate for missing compile-time validation.
- The CLI does not contain language semantics.

### Errors

Invalid syntax, unsupported syntax, semantic errors, unsupported platform APIs,
and runtime exceptions are distinct concepts. They may share the diagnostic
renderer, but must remain distinguishable as the error model grows.

Runtime language faults now carry a core Apex exception type, message, origin
span, and method-call frames. Compile diagnostics leave the runtime fields
empty. `try`/`catch` handles only typed runtime exceptions; internal
checked-state diagnostics are never silently converted into catchable Apex
behavior.

### Calls and scopes

The parsed `Program` stores classes, triggers, backwards-compatible top-level
method declarations, and executable anonymous statements separately. Signature
and class collection are early semantic passes, so cross-file lookup, forward
calls, and recursion work without source-order dependence. Runtime invocations
replace the caller's lexical-scope stack with a new parameter scope and restore
it on every completion path. Collections, class/static state, and object arenas
remain in the interpreter's execution store. Debug and transaction-timeline
events flow through the configured platform host, whose state may be owned by
the interpreter or intentionally shared by reference.

Class member targets pair a class index with a member index. Instance calls may
perform virtual dispatch only within the checked signature selected by the HIR;
`super` calls execute their checked base target directly. Fields and automatic
properties use typed slots keyed by the same target. Static slots live on the
interpreter, while instance slots live on arena-backed object identities.

Returns, loop control, and exceptions use the same statement-flow boundary.
This lets `finally` observe and, when it completes abruptly, replace every kind
of pending completion. The interpreter tracks active calls; when an exception
first reaches a handler or escapes a method, it snapshots frames that pair the
leaf method with the origin and each caller method with its nested call site.

Built-in calls use the same rule: semantic analysis records a typed intrinsic
target, and runtime execution matches that closed ID rather than method
spelling. Adding a built-in requires an explicit checker mapping and runtime
implementation, so unsupported platform surface cannot silently fall through
to dynamic dispatch.

## Compiler pipeline evolution

Direct AST walking remains appropriate for the current language slice.
Inheritance, overload resolution, and project compilation now use typed HIR
side tables so execution does not repeat compiler decisions. The next structural
evolution, when scale or additional language semantics require it, is a lowered
executable representation:

```text
Source
  → Tokens
  → Parsed AST
  → Name resolution
  → Typed AST or HIR
  → Lowered executable IR
  → Interpreter/runtime
```

The typed representation should make conversions, selected overloads, member
resolution, loop targets, and runtime operations explicit. Execution should not
repeat compiler reasoning.

The current typed representation is immutable syntax plus HIR side tables. A
lowered executable IR can replace this layout without moving semantic state
back into parsed nodes.

## Target runtime boundaries

The runtime should contain a language engine and a replaceable platform host.
Platform behavior must not be hard-coded throughout expression evaluation.

Conceptual Rust interfaces:

```rust
trait DatabaseHost {
    fn query(&mut self, query: &CheckedQuery) -> RuntimeResult<Vec<SObject>>;
    fn insert(&mut self, records: &[SObject]) -> RuntimeResult<DmlResult>;
}

trait SchemaHost {
    fn object(&self, name: &str) -> Option<&ObjectSchema>;
}

trait ClockHost {
    fn now(&self) -> ApexDatetime;
}

trait AsyncHost {
    fn enqueue(&mut self, job: AsyncJob) -> RuntimeResult<JobId>;
}

trait CalloutHost {
    fn send(&mut self, request: HttpRequest) -> RuntimeResult<HttpResponse>;
}
```

Additional hosts can own logging, user context, randomness, IDs, limits, and
filesystem-independent fixture data.

The implemented host surface owns structured debug, query, DML, trigger and
async lifecycle timelines, deterministic context, limits, and mock HTTP callout
behavior.
`platform::schema` provides a case-insensitive normalized catalog and
`SchemaProvider`, while `platform::storage` defines storage-neutral records and
transaction traits. M7 adds metadata import, an additive SQLite adapter, and
schema-backed interpreter SObjects.

M8 extends that boundary with checked SOQL/SOSL requests and atomic DML
operations. M9 adds DML preflight, nested transaction checkpoints, and trigger
timeline events. The default recording host lazily owns one in-memory SQLite
database per interpreter. In-memory SObject field mutation still does not
silently persist a record; only explicit DML crosses the persistence boundary,
including a before-trigger mutation made while that DML is active.

## Local data architecture

SQLite provides local org state below DML/query semantics.
The logical model supports fast isolated transactions for tests:

```text
SFDX metadata
    → normalized object schema
    → SQLite migrations
    → transaction/savepoint per test
    → SOQL/DML adapter
    → trigger dispatcher
```

Schema normalization remains separate from SQLite DDL so alternate storage or
in-memory implementations remain possible. The SQLite adapter owns physical
tables and an explicit schema registry. Additive migrations preserve records;
incompatible changes fail rather than rebuilding or coercing data silently.
Named savepoints, rollback, fixture replacement, and reset implement the
storage isolation substrate. M9's default host layers snapshot checkpoints over
that substrate so nested recursive DML and the outer Apex entry point can roll
back independently.

The transaction contract deals in storage-neutral records rather than AST or
runtime `Value` nodes. Interpreter SObjects likewise use checked schema indices
and in-memory values; DML execution performs explicit conversion at the
platform boundary before and after trigger dispatch.

## Compatibility architecture

Compatibility has three layers:

1. **Declared surface:** `docs/COMPATIBILITY.md` states what is intended.
2. **Executable fixtures:** tests define observable local behavior.
3. **Differential oracle:** versioned fixtures execute locally and against
   Salesforce, normalize observations, and record mismatches.

M13 implements the third layer with durable local and Salesforce snapshots.
Each report measures matched fixture dimensions overall and by category.
Recorded matches can support a narrowly documented **Exact** claim only for the
fixture cases and environment actually observed; no unmeasured surface is
promoted implicitly.

API-version, sharing, limits, and security behavior should eventually be
explicit runtime profiles. They must not appear as scattered conditionals.

## Enterprise CI architecture

M14 keeps orchestration above the compiler and runtime. A versioned CI manifest
records the Apex Exec version, every file below the SFDX package roots, the
project configuration, SHA-256 digests, changed paths, shard topology, report
destinations, and policy. A run refuses modified, missing, unrecorded, unsafe,
or symlinked inputs before consulting its cache. The normalized manifest,
sealed inputs, policy, and effective shard form the content address.

On a cache miss, project compilation remains the sole owner of parsing,
semantic analysis, and its dependency graph. Changed `.cls` files select test
classes through the transitive reverse dependency closure. Metadata, triggers,
deleted files, and paths absent from the current graph deliberately select all
tests because their effects cannot be proven local. Qualified test names are
sorted and partitioned by stable index modulo shard count; workers therefore
need no shared scheduler and can publish independent artifacts.

The cache artifact contains the compile outcome, exact test selection,
deterministic test/coverage observations, measured durations, and policy
outcome. Artifacts carry a result digest and are published by atomic rename.
Replay first verifies current inputs and then requires an exact cache key and
result digest. JUnit, Cobertura, and SARIF are regenerated from either an
executed or replayed artifact. GitHub Actions, GitLab CI, and Jenkins templates
obtain changed paths from their pull-request base and run the same manifest on
two independent shards.

## Hybrid deployment architecture

M15 treats the sealed M14 manifest as the local release candidate and does not
add org behavior to compiler or runtime phases. Changed Apex classes select
their reverse-dependent deployable classes through the compiler graph.
Metadata, triggers, deleted files, and unknown paths conservatively select the
complete project because their implicit Salesforce dependencies cannot be
proven locally.

The provider-neutral inventory groups source and sidecar files into stable
Metadata API identities and content digests. Drift comparison covers
project-owned schema and configuration components that are not directly
changed by the release; changed components are intentional deployment payload,
not drift. Code differences are handled by the dry-run deployment and affected
test differential instead of being mislabeled as configuration drift.

An authenticated adapter first verifies an existing org alias without verbose
output, retrieves the scoped metadata into isolated project-local
`.apex-exec` directories, and invokes `sf project deploy start --dry-run`.
Each retrieve directory is precreated with `main/default` so both legacy and
current Salesforce CLI output contracts have a valid conversion target, then
removed after inventory capture. It never creates an org, authenticates
interactively, requests an auth URL, or persists credentials. M17 records the
resulting observations in strict schema-version-2 evidence. Live capture
requires a cacheable M14 result, pins the project API version on both retrieves
and deployment, and compares two independently retrieved normalized
inventories. The bound request retains method-qualified selected tests, while
the Metadata API transport deduplicates them to test-class flags; differential
evaluation remains limited to the exact bound methods even if Salesforce
reports additional class methods. Offline replay asserts the expected alias
and org ID, checks the installed Salesforce CLI and Apex Exec versions,
enforces the exact recorded age policy, then reproduces the M14 cache
key/result and affected request before release readiness is evaluated. The raw
authentication response is never serialized. Release readiness still requires
the hermetic local CI policy, check-only deployment, unaffected
schema/configuration drift, and every selected test outcome to agree.

## M19 checked place and numeric architecture

M19 keeps lexing, parsing, checking, and execution separate while closing the
operator surface. The lexer applies maximal munch to every arithmetic,
bitwise, and shift compound token. Type parsing then consumes a logical `>` at
a time from `>`, `>>`, or `>>>`, so the same token stream preserves shift
expressions and adjacent nested-generic closers. Declaration, cast, and
enhanced-for lookahead clone the parser cursor and invoke the real type grammar
instead of maintaining a second positional grammar.

Semantic analysis records the selected operation and assignable target in HIR.
Numeric operations carry their checked Integer, Long, or Decimal family;
Boolean bitwise, integral bitwise, shifts, and String concatenation are
separate closed variants. Assignable SObject schema positions cross the
checker/runtime boundary as typed `ObjectTypeId` and `FieldId` values rather
than raw positions.

At runtime, simple assignment, compound assignment, and prefix/postfix mutation
all resolve an ephemeral `Place`. A Place evaluates a receiver and List index
once, retains the resulting storage identity, and then provides one read/write
path for locals, class members, List elements, and SObject fields. Compound
operations read before evaluating the right operand and write only after the
checked operation succeeds. The numeric module owns checked 32-bit Integer,
64-bit Long, Decimal arithmetic, divide/remainder faults, bitwise operations,
shift-distance masking, and signed/unsigned shift behavior; runtime does not
reselect operand families from source syntax.

## M20 qualified type and initialization architecture

M20 replaces split type lookahead with one lossless `TypeRef` grammar that
retains every qualified segment, recursive generic argument, array suffix, and
source span. Semantic lowering canonicalizes lookup separately from source
spelling. Nested declarations retain their enclosing and fully qualified
identities throughout project dependencies, editor indexes, HIR targets, and
runtime dispatch; typed `ClassId` values carry new type identities across the
checker/runtime boundary.

The checked program precomputes class lineage, field/property slots, and
source-ordered static and instance initialization steps. Runtime execution
allocates typed-null slots first, then consumes those steps, including
initializer blocks, while preserving lazy class initialization and its bounded
cycle/depth failures. Constructor targets record checked `this(...)` and
`super(...)` delegation; semantic analysis resolves overloads, access, and
cycles before runtime executes a constructor chain.

Enums have qualified runtime identity and checked constants/method targets for
`name`, `ordinal`, `values`, and `valueOf`. Class literals carry canonical
qualified, array, or generic type facts in HIR. Custom exception subclasses
participate in typed throw/catch/subtyping and inherit the supported zero- and
one-String construction surface.

## M21 grammar-closure architecture

M21 separates syntax ownership from execution claims. Arbitrary annotations
and arguments, switch arms, external-ID DML fields, multi-declarator fields,
and remaining modifiers have dedicated immutable AST structure with original
spelling and file-aware spans. Semantic analysis either consumes a supported
form or emits an explicit unsupported diagnostic; runtime retains defensive
guards but never interprets checked-only syntax.

Uninitialized and multi-declarator locals are executable. The checker and
runtime process declarators left to right in one lexical scope, using typed
null for omitted initializers. Comma-separated traditional-`for` expressions
use a structural sequence node that visits and executes each child in source
order without creating a scope, coverage line, or debugger snapshot of its own.
The AST-derived census in `docs/NORTH_STAR_GRAMMAR_CENSUS.md` is an executable
guard over the unchanged corpus rather than a text search that can count
comments.

## Phase 2 architecture constraints

The Phase 2 roadmap expands compatibility without weakening the existing phase
boundaries:

- New syntax receives explicit tokens and lossless AST nodes. Parser acceptance
  for a North Star source does not authorize the runtime to approximate an
  unsupported construct; the checker must either record a typed target or emit
  an explicit unsupported diagnostic.
- Nested declarations need canonical qualified identities owned by their
  enclosing type. Dependency graphs, HIR targets, editor indexes, source maps,
  and runtime dispatch must use that identity rather than flattening names.
- API version and sharing/security behavior belong in explicit compiler/runtime
  profiles. The effective profile must participate in cache, oracle, and hybrid
  evidence identity.
- Hybrid validation evidence must bind to the exact sealed candidate, affected
  request, target, API/tool versions, and capture age before replay can approve
  a release.
- Metadata breadth begins with complete accounting. Every package-root file is
  recognized, intentionally excluded, or reported unsupported; an unknown
  unchanged path cannot silently disappear from drift analysis.
- Persistent typed/lowered IR cannot serialize session-local `SourceId` values
  directly. A stable path/content identity and verified remapping layer is
  required, with clean-build-equivalent diagnostics after load.

Consequential representation, profile, evidence-schema, and persistent-cache
choices require ADRs in their implementation milestones.

## Performance direction

Correctness and phase boundaries take priority during the language milestones.
The current foundation includes class dependency graphs, parsed-unit reuse,
isolated parallel test execution, per-file coverage aggregation, hermetic CI
manifests, content-addressed whole-run reuse, deterministic distributed shards,
and affected hybrid validation. Further project-scale performance work can
rely on:

- Interned names and types
- Dependency-scoped incremental semantic analysis
- Cached typed or lowered IR within the existing content-addressed boundary

These optimizations must preserve deterministic results and source diagnostics.
