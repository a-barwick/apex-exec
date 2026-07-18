# Current Status

**Last updated:** 2026-07-17

## Active milestone

M13 — Salesforce compatibility oracle

## Completed

- Rust binary and library crate
- Separate lexer, parser, AST, semantic-analysis, diagnostic, and runtime modules
- `String`, `Boolean`, simplified `Integer`, and `null` values
- Explicit initialization, right-associative assignment, and variable references
- Case-insensitive identifiers with original spelling retained in the AST
- Single-quoted strings, comments, and common string escapes
- Precedence-based arithmetic, comparison, equality, and Boolean expressions
- String concatenation and prefix/postfix increment and decrement
- Blocks with nested lexical scopes
- `if`/`else`, `while`, `do`/`while`, and traditional `for` execution
- `break`, `continue`, and value-less anonymous `return`
- Recursive `List<T>`, `Set<T>`, and `Map<K,V>` types, including nested
  collections
- One-dimensional `T[]` syntax as an alias for `List<T>`
- Empty, copy, literal, and sized-array construction
- List indexing, indexed assignment, and indexed increment/decrement
- Collection reference aliasing with independent shallow copies from copy
  constructors and `clone()`
- Enhanced `for` over List and Set values with loop control and scoped loop
  variables
- Common List, Set, and Map access, mutation, copy, membership, and size
  methods
- Core static and instance `String` methods, Integer-backed `Math` methods,
  and `System.debug(expression)`
- Case-insensitive built-in method dispatch and method-call expressions
- Top-level method declarations with typed parameters, forward calls, typed and
  `void` returns, case-insensitive overloads, and recursion
- Two-pass method signature collection with statically recorded overload
  selections and isolated runtime call scopes
- `try`, typed `catch`, `finally`, `throw`, and catchable core exception values
- Minimal `Object` assignment and explicit casts, including catchable invalid
  downcasts
- Catchable `NullPointerException`, `ListException`, `MathException`,
  `TypeException`, `StringException`, `IllegalArgumentException`, and
  `FinalException` behavior
- Source-mapped runtime call stacks and exception type/message/accessor support
- `tokens`, `ast`, `check`, and `run` CLI commands
- Source-span compile and runtime diagnostics
- Focused compiler/runtime unit tests and public-pipeline integration tests
- Disk-backed scenarios run through every compiler stage and the CLI
- The unchanged M3 acceptance program executes from both the library and CLI
- The M4 methods-and-exceptions core sample executes from both the library and
  CLI
- Typed HIR side tables own checked expression types and selected call/member
  targets; parsed syntax no longer carries semantic mutation
- Built-in calls resolve to typed HIR intrinsic IDs during checking; runtime
  execution does not repeat method-name or receiver-family resolution
- Top-level, constructor, and class-member overloads share one most-specific
  candidate algorithm and the supported user-type subtype relation
- Class and interface declarations with case-insensitive user-defined types
- Constructors, default field initialization, instance/static fields, and
  automatic or custom properties
- Instance/static methods, member overload selection, `this`, `super`, and
  runtime virtual dispatch
- Public, private, protected, and global member access checks, including
  accessor-specific visibility
- Class inheritance, abstract/virtual methods, overrides, interfaces, subtype
  assignment, and contract validation
- Object identity, inherited storage, class casts, and source-mapped class call
  execution
- SFDX `packageDirectories` discovery, recursive `.cls` loading, filename/type
  validation, and cross-file semantic resolution
- Cross-file dependency graphs, cached parsed units, dependent invalidation,
  and complete-build reuse when project inputs are unchanged
- Stable per-file source identities keep cached AST spans local, eliminate
  project-wide span rebasing, and map every runtime stack frame independently
- A shared AST visitor owns syntax traversal; project dependency collection no
  longer maintains a duplicate recursive walker
- Parser, project, semantic intrinsic, runtime intrinsic, and test-runner
  responsibilities are organized behind stable module façades
- Project-aware `check` and public static zero-argument `invoke` CLI workflows
- A three-file SFDX service-layer example that compiles and runs locally
- A pinned seven-file, 14,740-line open-source Apex North Star corpus with
  executable lexer/parser milestone indicators
- Case-insensitive `@IsTest` and `@TestSetup` annotations with checked test and
  setup method signatures; unsupported annotations and org-backed
  `SeeAllData=true` are rejected explicitly
- Catchable `AssertException` behavior for `System.assert`,
  `System.assertEquals`, and `System.assertNotEquals`
- Project test discovery, class/method/glob filtering, deterministic result
  ordering, and independent interpreter state per test
- Setup execution before each isolated test and bounded parallel execution with
  a configurable worker count
- Expected runtime and assertion failures captured as test results without
  stopping the remaining suite
- Console and JUnit test reports plus production line and conditional-branch
  coverage
- A two-file SFDX test project that passes through `apex-exec test`, including
  parallel execution, setup isolation, assertions, and full sample coverage
- Runtime preparation borrows immutable checked code through a `RuntimeImage`
  instead of cloning the complete program, methods, and classes
- An `ExecutionStore` isolates collection/object arenas and static member slots
  from checked code and interpreter control-flow state
- Structured debug events cross a replaceable platform host while preserving
  the existing convenience execution APIs
- A case-insensitive normalized SObject schema catalog and storage-neutral
  transactional record interfaces establish the M7 schema/storage boundary
- Project discovery, dependency analysis, and diagnostic mapping are isolated
  modules; callers can inspect typed project error categories
- Decomposed and monolithic SFDX custom-object metadata import into the
  case-insensitive normalized schema catalog, including standard Id/Name,
  Boolean, integer Number, String-shaped, and relationship fields
- Schema-backed custom object names, constructors, statically typed field
  access, dynamic `SObject` construction, and case-insensitive `get`/`put`
  execution in ordinary SFDX projects
- Deterministic Salesforce-shaped 18-character record ID generation plus
  15/18-character validation and checksum verification
- SQLite schema generation and additive migration with explicit rejection of
  incompatible key-prefix or field-definition changes
- SQLite-backed create/read/update/delete, transactions, named savepoints,
  rollback, fixture replacement, and fast data reset behind the
  storage-neutral platform contract
- A complete M7 example project whose imported `Invoice__c` metadata compiles
  and executes through both typed and dynamic SObject access
- Dedicated SOQL and SOSL AST grammars with immutable checked HIR plans rather
  than string parsing or SQLite concerns in expression evaluation
- Static SObject/field, aggregate/grouping, parent relationship, and bind
  validation against the normalized M7 schema
- SOQL comparison, `LIKE`, `IN`/`NOT IN`, Boolean filters, scalar and collection
  binds, ordering/null placement, `LIMIT`, and `OFFSET`
- Single-record and list SOQL results, scalar `COUNT()`, grouped
  `COUNT`/`SUM`/`MIN`/`MAX`, and `AggregateResult.get`
- One-level custom parent relationship selection and runtime traversal through
  `__r` names backed by lookup/master-detail IDs
- SOSL `FIND` with String binds, `IN ALL FIELDS`/`IN NAME FIELDS`, and typed
  `RETURNING` clauses with filters, ordering, and limits
- Bulk and scalar `insert`, `update`, `upsert`, and `delete` statements plus
  the corresponding common `Database` methods
- Catchable `QueryException` and `DmlException` failures for query cardinality,
  bind, persistence, and unsupported DML semantics
- A lazy per-execution in-memory SQLite database host with deterministic IDs,
  atomic DML calls, test/setup visibility, and independent state per test
- Public structured SOQL, SOSL, and DML trace events on the recording host
- A two-class M8 repository/service example that persists, queries, follows a
  parent relationship, and executes through the CLI without source changes
- Dedicated trigger declarations discovered from `.trigger` project files,
  with schema-checked objects, event lists, bodies, and source mapping
- Typed `Trigger.new`, `old`, `newMap`, `oldMap`, phase/operation flags, and
  `size` contexts that pass directly into common static handler methods
- Before/after insert, update, delete, and undelete dispatch over bulk groups,
  including concrete insert/update partitioning for mixed upserts
- Mutable before-trigger new records plus read-only context collections, old
  images, and after images
- Deterministic recursive trigger execution with an explicit depth bound and
  structured enter/exit events interleaved with DML in one transaction timeline
- Nested database checkpoints for caught per-DML rollback and uncaught
  entry-point rollback, including recursive trigger work
- Recycle-bin persistence and undelete restoration with stable record IDs
- Trigger statement and branch observations included in Apex test coverage
- A handler-oriented M9 example that checks, invokes, and runs three Apex tests
  with 100% production line and branch coverage
- Decimal literals and checked mixed Integer/Decimal arithmetic, comparison,
  scale, parsing, formatting, and overflow/division failures
- Immutable `Date`, `Datetime`, and `Time` values with deterministic UTC
  construction, parsing, component access, formatting, and arithmetic
- Dedicated validated `Id` values, UTF-8 `Blob` values, and Base64 utilities
- JSON serialization/pretty-printing and recursive untyped deserialization over
  primitives, Lists, Sets, and String-keyed Maps
- Regex `Pattern`/`Matcher` compilation, full matching, repeated search, capture
  groups, and source positions
- Schema global describe maps plus SObject name, key-prefix, and custom-object
  describe access
- Deterministic clock, pseudo-randomness, and user context supplied through the
  replaceable platform host
- Common `Test.startTest`/`stopTest`/`isRunningTest` and query, DML, and callout
  `Limits` counters
- Stateful `HttpRequest`/`HttpResponse` objects and host-mocked `Http.send`,
  including captured requests and explicit missing-mock failures
- Profile-aware compile errors for unsupported curated platform calls
- A complete M10 example whose four Apex tests pass with 100% production line
  coverage, plus ten Rust integration tests covering success, failure,
  determinism, mocking, checking, and runtime boundaries
- Checked platform contracts for `Queueable`, `Database.Batchable<T>`, and
  `Schedulable`, including their context parameter types and required methods
- Checked `@future` methods with public/global static void signatures and
  serializable primitive or primitive-collection parameters
- Deterministic `System.enqueueJob`, `Database.executeBatch`, and
  `System.schedule` submission with Salesforce-shaped `707` job IDs
- Enqueue-time deep snapshots for class, collection, and SObject payloads,
  deterministic FIFO draining, explicit bounds, and catchable `AsyncException`
  failures
- Queueable, batch, future, and scheduled execution contexts plus
  `System.isQueueable`, `isBatch`, `isFuture`, and `isScheduled`
- Asynchronous `EventBus.publish` delivery to checked after-insert platform
  event triggers without persisting event records as ordinary SObjects
- Per-job transaction checkpoints and public queued/started/completed/failed
  lifecycle events, with parent IDs for work chained by an async job
- Explicit Apex-test draining through `Test.stopTest`; ordinary execution never
  starts a background scheduler or implicitly drains queued work
- A complete M11 example whose two Apex tests exercise all five async forms
  with 100% production line coverage (26/26), plus seven Rust integration tests
  covering order, snapshots, explicit draining, lifecycle failures, contracts,
  limits, and runtime boundaries
- Persistent transactional REPL sessions with accumulated declarations and
  variables, deterministic replay, incremental output, rollback on rejected
  snippets, and reset/source/quit commands
- Statement-boundary debugger snapshots with verified breakpoints, entry stops,
  step in/over/out, source-mapped Apex frames, visible typed variables, runtime
  exceptions, debug output, database DML inspection, and transaction timelines
- A stdio Debug Adapter Protocol server covering launch/configuration, threads,
  frames, scopes, variables, stepping, continue, terminate, and disconnect for
  both anonymous scripts and project `Class.method` entry points
- Checked project symbol indexing for class/member go-to-definition,
  references, case-insensitive rename edits, and source-mapped inline
  diagnostics
- Per-executable-line coverage data and an LSP `apex/coverage` request that
  preserves covered/uncovered state rather than aggregate counts alone
- A stdio Language Server Protocol server covering initialization,
  full-document diagnostics, definition, references, rename, saved-project
  refresh, and coverage overlays
- A VS Code thin client registering Apex files, the LSP/DAP servers, launch
  configurations, and in-editor covered/uncovered line decorations
- A complete M12 debug example plus protocol, editor, debugger, REPL, project
  timeline, CLI, and coverage integration tests
- 240 ordinary tests pass with no failures (14 separate North Star goal tests
  remain intentionally ignored); LLVM source-line coverage is 83.96% overall,
  including 100.00% for REPL, 94.21% for debugger, 93.80% for LSP, 92.91% for
  DAP, 88.37% for protocol framing, and 82.68% for editor indexing

## Immediate target

Begin M13 by defining a Salesforce differential fixture manifest and result
model before adding scratch-org authentication or transport.

## North Star indicators

At M12 completion, the pinned real-world lexer/parser goals pass 1 of 14
indicators (**7.14%**): lexer 1 of 7 (**14.29%**) and parser 0 of 7 (**0%**).
`JSONParse.cls` now parses through its class and ordinary members before
stopping at unsupported `instanceof` syntax. Annotation tokenization moved the
other first lexer blockers forward to safe navigation, null coalescing,
ternary syntax, and bitwise operators. These
are syntax-progress indicators, not semantic, execution, or Salesforce
compatibility percentages.

## Known limitations

- `Integer` uses simplified internal `i64` semantics rather than complete Apex
  range and overflow behavior.
- Anonymous `return` remains value-less; declared methods support checked
  return values.
- Array notation supports one suffix and is normalized to `List<T>`; explicit
  multidimensional array suffixes are rejected. Nested generic Lists remain
  supported.
- Set and Map observations use deterministic insertion order locally. This is
  a reproducibility choice, not a reproduction of Salesforce's internal
  iteration order.
- `Map.keySet()` returns a snapshot rather than a backed view.
- String length, index, and substring operations use UTF-16 code-unit offsets
  for ordinary Unicode scalar strings. A substring boundary that would split a
  surrogate pair is rejected because Rust strings cannot represent the result.
- Top-level method declarations remain as a backwards-compatible anonymous
  script surface. Ordinary project code uses class-contained methods.
- Overload resolution supports exact matches, `Exception` and `Object`
  widening, and checked user-class/interface subtyping. Numeric and broader
  platform conversions remain future work.
- `Object` supports assignment, overload widening, casts, and `toString()`;
  equality/hash and the broader inherited Apex Object surface remain
  unsupported.
- The core exception subset supports construction, catching, rethrowing,
  messages, type names, and deterministic stack text. Custom exception classes,
  causes, and Salesforce-exact stack formatting require later compatibility and
  differential work.
- SFDX discovery reads package-directory paths but does not yet interpret the
  full Salesforce DX configuration or metadata surface. Each `.cls` or
  `.trigger` file must contain exactly one matching top-level declaration.
- Parsed source units and unchanged complete builds are cached with stable
  source identities. A changed unit computes dependency invalidation and reuses
  unchanged parsed ASTs, while the cross-file semantic link currently reruns
  project-wide.
- Custom metadata import supports Checkbox, zero-scale Number, common
  String-shaped fields, Id, Lookup, and MasterDetail. Decimal Number, date/time,
  geolocation, address, calculated fields, and the broader metadata surface are
  rejected explicitly.
- SQLite migrations are additive. Removing/renaming physical columns and
  changing an existing field type, nullability, relationship target, or key
  prefix require an explicit future migration policy.
- Typed custom-object field access is available in metadata-aware project
  compilation. Id and relationship fields still appear as `String` in SObject
  execution for backwards compatibility, while standalone `Id` values validate
  15/18-character IDs. Full field-level `Id` integration remains future work.
- Static SOQL supports direct fields and one custom parent `__r` relationship
  level. Child subqueries, `HAVING`, `TYPEOF`, polymorphic relationships, date
  literals, and the broader SOQL grammar remain unsupported.
- Aggregate queries support grouped direct fields plus `COUNT`, `SUM`, `MIN`,
  and `MAX` over the current scalar types. `AggregateResult` exposes only
  `get(String)`.
- SOSL uses deterministic case-insensitive substring matching over stored
  String fields. Salesforce tokenization, stemming, wildcards, snippets,
  division clauses, and relevance ranking are not reproduced.
- DML calls are atomic and support Id-based insert/update/upsert/delete/
  undelete with triggers, but not validation rules, workflows, sharing, limits,
  external-ID upsert, mixed-SObject bulk lists, or partial results. `Database`
  methods currently return `void`; result APIs and `allOrNone=false` are
  rejected explicitly.
- Trigger dispatch supports the eight planned before/after event combinations,
  bulk contexts, handler calls, recursion, and read-only images. It does not
  model Salesforce automation beyond Apex triggers, `addError`, merge events,
  or platform-exact multi-trigger ordering.
- Transaction checkpoints copy the local active and recycled record sets.
  Persistent or high-volume hosts need native transactions or savepoints behind
  the same host boundary.
- The default recording host owns an in-memory SQLite database for one
  interpreter. A persistent project database configuration and fixture CLI
  remain future work.
- Nested types, enums, annotations other than `@IsTest`/`@TestSetup`/`@future`, explicit
  superclass-constructor calls, custom exception classes, and Salesforce-exact
  object string formatting are not implemented.
- Sharing modifiers are parsed for structural progress but rejected during
  checking because sharing/security semantics remain deferred.
- Test setup methods run before every isolated test interpreter, and their DML
  is visible to that test while remaining isolated from parallel tests.
  Salesforce's one-time setup transaction/snapshot optimization is not yet
  reproduced.
- Test discovery supports annotation-based static void methods only. Legacy
  `testMethod`, the newer `Assert` class, and org-backed `SeeAllData=true`
  remain unsupported.
- Async execution is a deterministic local profile, not a wall-clock scheduler:
  only `Test.stopTest` drains queued work, jobs run FIFO with a 100-job drain
  bound, batch `start` returns `List<T>` rather than `QueryLocator`/`Iterable`,
  and batch scope sizes are limited to 1–2000. Async jobs share one interpreter
  execution store, so Salesforce's fully serialized cross-transaction static
  isolation is not yet claimed. Cron expressions are shape-checked but not
  calendar-evaluated; job monitoring, abort/reschedule APIs, flex queue
  behavior, finalizers, future callout options, and platform-event replay or
  retention are unsupported.
- Coverage counts executable production statement lines and both outcomes of
  `if`, `while`, `do`/`while`, and condition-bearing `for` branches. It is not a
  claim of Salesforce-exact coverage accounting.
- Date/time formatting is fixed deterministic UTC formatting rather than
  locale/time-zone aware Salesforce formatting. Decimal uses 96-bit
  `rust_decimal` semantics and does not yet implement every Apex rounding mode.
- JSON typed deserialization, arbitrary user-object field reflection, and
  namespace-qualified `System.JSON` syntax remain unsupported; untyped JSON
  preserves ordered String-keyed Maps.
- Schema describe currently covers imported SObjects and object-level name,
  key-prefix, and custom flags; field describe and broader metadata are
  unsupported.
- HTTP callouts are synchronous and must be mocked through `PlatformHost`.
  Apex `HttpCalloutMock`/`Test.setMock`, named credentials, TLS, and live
  network transport are intentionally not implemented.
- The REPL reconstructs accepted state by deterministic whole-session replay.
  It does not preserve host effects that fall outside the deterministic local
  platform profile.
- Debugger stops are immutable pre-statement snapshots. Debug-console
  expression evaluation, mutation, conditional breakpoints, data breakpoints,
  exception filters, and reverse execution are not implemented.
- LSP navigation and rename cover checked project classes and members.
  Local-variable rename, completion, hover, formatting, semantic tokens, code
  actions, and unsaved multi-file semantic linking remain future work.

## Handoff checklist

After meaningful implementation work:

- Update the completed and limitation lists above.
- Update the active milestone if its exit criterion passes.
- Update `docs/COMPATIBILITY.md` for changed behavior.
- Add or update conformance tests.
- Add an ADR if an architectural boundary or expensive choice changed.
- Run the verification commands in `AGENTS.md`.
