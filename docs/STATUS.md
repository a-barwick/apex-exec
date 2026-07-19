# Current Status

**Last updated:** 2026-07-18

## Active program gate

S0 — Phase 2 stabilization

The pre-open-source audit and execution strategy are now captured in
[`docs/STABILIZATION.md`](STABILIZATION.md). M18 — null-aware expressions
remains the next feature milestone, but feature implementation is gated until
the bounded S0 process-safety and correctness criteria pass.

Initial S0 work may begin in three disjoint packages:

- S0-01 — frontend process safety and correctness (Active maintainability
  remediation on `codex/stab-frontend-safety` from `c7d4ac7`)
- S0-02 — opt-in runtime instrumentation
- S0-05 — CI, maintainability ratchet, and release-document gates

Runtime graph safety (S0-03) follows instrumentation, and execution context/
lazy class initialization (S0-04) follows both. Package status, dependencies,
acceptance criteria, branch rules, and the coordinator prompt live under
`docs/stabilization/`.

## Completed

- Rust binary and library crate
- Separate lexer, parser, AST, semantic-analysis, diagnostic, and runtime modules
- `String`, `Boolean`, simplified `Integer`, and `null` values
- Explicit initialization, right-associative assignment, and variable references
- Case-insensitive identifiers with original spelling retained in the AST
- Single-quoted strings, comments, and common string escapes
- Precedence-based arithmetic, comparison, equality, and Boolean expressions
- String concatenation and prefix/postfix increment and decrement
- Right-associative ternary expressions with checked Boolean conditions,
  common result typing, lazy arm execution, and production branch coverage
- Checked `instanceof` expressions with viable-alternative diagnostics,
  generic collection identity, class/interface runtime relationships,
  null-false behavior, and single evaluation of the value expression
- Blocks with nested lexical scopes
- `if`/`else`, `while`, `do`/`while`, and traditional `for` execution
- `break`, `continue`, and value-less anonymous `return`
- Recursive `List<T>`, `Set<T>`, and `Map<K,V>` types, including nested
  collections
- One-dimensional `T[]` syntax as an alias for `List<T>`
- Empty, copy, literal, and sized-array construction, including custom,
  `Object`, and core-exception element types, with complete constructed-element
  type validation
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
- Structural cast/group disambiguation for parenthesized member access,
  indexing, postfix mutation, signed operators, and supported genuine casts
- Catchable `NullPointerException`, `ListException`, `MathException`,
  `TypeException`, `StringException`, `IllegalArgumentException`, and
  `FinalException` behavior
- Source-mapped runtime call stacks and exception type/message/accessor support
- `tokens`, `ast`, `check`, and `run` CLI commands
- Source-span compile and runtime diagnostics
- Public raw-token parser construction that rejects empty, malformed-EOF,
  mixed-source, reversed, overlapping, and non-monotonic streams before
  lookahead
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
  assignment, iterative cycle validation across every hierarchy edge, and
  visited iterative subtype and interface-contract traversal
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
  `Schedulable`, including preserved declared batch element types, matching
  start/execute signatures, context parameter types, and required methods
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
  hierarchy generic-argument definitions, references, case-insensitive rename
  edits, and source-mapped inline diagnostics
- Per-executable-line coverage data and an LSP `apex/coverage` request that
  preserves covered/uncovered state rather than aggregate counts alone
- A stdio Language Server Protocol server covering initialization,
  full-document diagnostics, definition, references, rename, saved-project
  refresh, and coverage overlays
- A VS Code thin client registering Apex files, the LSP/DAP servers, launch
  configurations, and in-editor covered/uncovered line decorations
- A complete M12 debug example plus protocol, editor, debugger, REPL, project
  timeline, CLI, and coverage integration tests
- Versioned Salesforce conformance manifests with validated fixture names,
  project containment, compile/invoke/test entry points, and explicit measured
  dimensions
- Provider-neutral snapshots covering compile outcome/category, named JSON
  values, output, exceptions and stack frames, SOQL/SOSL, DML, trigger order,
  and Apex test outcomes
- A local oracle adapter over project compilation, structured runtime host
  events, source mapping, and the isolated test runner
- An authenticated Salesforce CLI adapter for metadata deployment, anonymous
  Apex, and individual Apex tests, including normalized deployment/test JSON
  and debug-log query, DML, trigger, output, exception, and stack observations
- Durable Salesforce snapshot recording and offline replay plus JSON/console
  differential reports with overall and per-dimension compatibility coverage
- A deployable M13 SFDX example and 13 focused unit/integration tests covering
  every comparison dimension, unsafe manifests, phase categories, live
  transport shapes, snapshot round trips, regression detection, and CLI status
- Versioned hermetic CI manifests sealing project configuration and every
  package-root input with SHA-256, tool version, changes, shard topology,
  reports, and policy
- Exact input-inventory verification that rejects modified, missing,
  unrecorded, unsafe, and symlinked inputs before cache lookup or execution
- Content-addressed whole-run artifacts with verified cache hits and an
  explicit replay-only mode that never falls back to execution
- Transitive dependency-based impacted-test selection for changed Apex classes,
  with conservative all-test fallback for metadata, triggers, deletions, and
  unknown paths
- Stable qualified-test sharding for independent distributed workers plus the
  existing bounded per-test parallel execution
- SARIF 2.1 diagnostics, JUnit test results, and Cobertura line-coverage
  reports regenerated identically from executed or replayed artifacts
- Enforceable test-failure, line/branch coverage, duration, and M13 measured
  compatibility gates
- Generated GitHub Actions, GitLab CI, and Jenkins pull-request integrations
  that collect changed paths and execute two deterministic shards
- A complete M14 six-class SFDX example with four passing Apex tests and 100%
  production line/branch coverage, plus five focused unit tests and seven
  integration tests covering manifests, drift, selection, fallback, shards,
  cache/replay, reports, policies, compile failures, provider templates, and
  the CLI
- Provider-neutral SFDX metadata-component inventories that group source and
  sidecars into stable Metadata API identities, categories, and SHA-256 digests
- Affected deployment-component selection through the compiler dependency
  graph, with conservative complete-project fallback for metadata, triggers,
  deletions, and unknown paths
- Optional validation-org authentication checks that never request verbose
  credentials, scoped metadata retrieval into an isolated temporary directory,
  and targeted `sf project deploy start --dry-run` execution
- Unaffected schema/configuration drift detection that distinguishes intended
  release changes from environmental drift
- Normalized local-versus-org affected-test outcomes, versioned validation
  snapshots for credential-free replay, and JSON/console release-readiness
  reports with explicit blockers
- A complete M15 four-class SFDX example with three passing Apex tests and 100%
  full-suite production line/branch coverage, plus four focused unit tests and
  seven integration tests covering inventory, safety, selection, fallback,
  drift, differential failures, snapshots, authenticated CLI transport, report
  output, and readiness exit status
- A complete M16 project and oracle-ready manifest exercise ternary and
  `instanceof` through project checking, static invocation, isolated Apex
  tests, CLI workflows, branch coverage, and normalized differential dimensions
- Strict M17 validation-evidence schema version 2 with SHA-256 binding for the
  serialized M14 manifest, exact cached CI result, changed paths, affected
  selectors/digests, selected tests/test level, normalized inventory, and full
  snapshot
- Target alias/org ID, project API version, Apex Exec/Salesforce CLI versions,
  UTC capture time, exact maximum-age policy, expected-target replay assertions,
  future/expired evidence checks, and credential-free M14 replay-only enforcement
- Two isolated, API-version-pinned Salesforce metadata retrievals per live
  capture, with deployment prevented when their normalized inventories differ
- Sanitized snapshot/report serialization that retains only allowlisted org
  evidence and never records the raw authentication response, access token,
  auth URL, or instance URL
- Five focused M17 integration tests plus expanded M15 and hybrid unit coverage
  for positive capture/replay, candidate/request/target/API/tool/age mismatch,
  controlled deployment blockers, drift, retrieval stability, tampering,
  error-phase ordering, CLI exit status, and output side effects
- Reviewed M17 live evidence in `evidence/milestone17/` for exact candidate
  `083fa8e…`: two stable Salesforce retrievals, passing check-only deployment,
  two of two selected test methods matching, zero clean drift, and successful
  credential-free replay
- Controlled live PermissionSet drift for that same candidate with a passing
  deployment and matching tests but one unchanged-configuration finding that
  blocks release; the disposable org baseline was restored and recaptured clean
- Cross-version Salesforce retrieve handling that uses a project-local isolated
  output directory, prepares the legacy `main/default` shape, and collapses
  method-qualified local selections to unique Metadata API test-class flags
- 320 ordinary tests pass with no failures (14 separate North Star goal tests
  remain intentionally ignored); LLVM source-line coverage is 84.33% overall
  and 83.57% across the three changed production modules (`ci`, `hybrid`, and
  the CLI)

## Immediate target

Implement M18 safe-navigation and null-coalescing expressions as complete
lexer/parser/semantic/runtime slices, including precedence, chaining, lazy
evaluation, side-effect, diagnostic-phase, and integration coverage. The
complete Phase 2 sequence and its evidence baseline are in `ROADMAP.md` and
`docs/PHASE_2_BASELINE.md`.

## North Star indicators

M16 reproduces 5 of 14 passing indicators (**35.71%**): lexer 5 of 7
(**71.43%**) and parser 0 of 7 (**0%**), a gain of four lexer indicators from
the Phase 2 baseline. `SOQL.cls`, `Logger.cls`, `Rollup.cls`,
`RollupService.cls`, and `JSONParse.cls` now lex completely. Their first parser
diagnostics are unsupported annotations or nested declarations; the two
remaining lexer blockers are bitwise/compound operators in
`fflib_SObjectDomain.cls` and `Puff.cls`. Ternary and `instanceof` no longer
appear as first diagnostics.

Later reachable blockers include arbitrary annotations, nested declarations,
enums, class literals, static initializer blocks, `switch`/`when`,
uninitialized and multi-declarator locals, constructor delegation, arbitrary
generic type references, and additional modifiers/literals. These are
syntax-progress indicators, not semantic, execution, or Salesforce
compatibility percentages. M21 requires all 14 original fixtures to pass
without `#[ignore]` or corpus changes.

M17 intentionally changes no Apex syntax, so its North Star movement is zero:
lexer 5/7, parser 0/7, total 5/14.

## Phase 2 evidence baseline

- At the M17 branch point, refreshed `main` and `origin/main` both resolved to
  the M16 merge commit `35563e5`.
- The reviewed live bundle in `evidence/milestone17/` records clean
  authenticated capture, offline replay, controlled drift blocking, and final
  baseline restoration for the same sealed candidate.
- Fake-CLI tests remain the hermetic transport regression suite and are not
  counted as the live Salesforce evidence.
- No representative enterprise project or frozen Salesforce test denominator
  has been measured against the 60–80% vision target.
- The M15 path classifier recognizes 28 static metadata types. Unknown
  unchanged metadata is omitted from drift accounting, and multi-part Custom
  Metadata full names are currently truncated.
- Version-1 validation snapshots remain rejected. Schema-version-2 evidence
  binds candidate, request, org, API/tool versions, capture age, inventory, and
  the complete snapshot; altered, expired, or mismatched evidence fails before
  readiness evaluation.

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
- Ternary result typing covers identical types, null with a concrete type,
  supported subtype widening, Integer-to-Decimal promotion, and `Object` as the
  common carrier for otherwise unrelated supported values. It does not add
  broader Apex conversion rules.
- `instanceof` uses the current compatibility profile: null is false, invariant
  generic collection identity is preserved, and statically always-true or
  impossible tests fail during checking. Historical pre-API-32 null behavior,
  String-to-Id quirks, and unsupported platform generic interfaces await
  versioned profiles and broader type support.
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
- Nested types, enums, annotations other than
  `@IsTest`/`@TestSetup`/`@future`, explicit superclass-constructor calls,
  custom exception classes, and Salesforce-exact object string formatting are
  not implemented.
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
  `if`, ternary, `while`, `do`/`while`, and condition-bearing `for` branches.
  It is not a claim of Salesforce-exact coverage accounting.
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
- The M13 oracle shells out to an already-installed, authenticated Salesforce
  CLI and deploys fixtures to a caller-selected org. It does not create,
  authenticate, reset, or delete scratch orgs.
- Salesforce debug-log parsing currently measures the supported SOQL, DML, and
  Apex-trigger event shapes. Other automation, namespaced log variants,
  provider log truncation, and platform features outside the local
  compatibility surface require additional permanent fixtures.
- Recorded oracle matches are scoped to their selected dimensions and
  Salesforce environment. They do not make unmeasured behavior exact, and no
  behavior is promoted to Exact until reviewed org evidence is committed.
- M14 dependency selection follows explicit Apex class references. Metadata,
  triggers, deleted files, and unknown paths select all tests; dynamic
  Salesforce dependencies are never guessed. Coverage and duration policies
  are evaluated per shard artifact unless the CI provider aggregates the
  emitted standard reports.
- Content-addressed artifacts cache normalized compile/test/report observations,
  not serialized AST/HIR. A cache miss still performs project-wide semantic
  linking after parsing reuse; persistent lowered-IR caching remains M29 work.
- M15 inventories 28 curated common SFDX metadata types rather than the entire
  Metadata API. Unknown changed paths conservatively validate the complete
  project, while unknown unchanged metadata is outside drift accounting.
  Multi-part Custom Metadata filenames are currently reduced to their first
  dot-separated segment.
- Drift compares project-owned schema/configuration content retrieved for the
  release and excludes directly changed components as intended payload. It
  does not discover org configuration that has no corresponding local
  component.
- Version-1 M15 snapshots are rejected. Version-2 evidence detects candidate,
  request, org, API/tool, age-policy, inventory, and snapshot mismatches, but
  its SHA-256 seal is an integrity check rather than a digital signature.
- Offline hybrid replay makes no org request but requires the recorded
  Salesforce CLI version to remain installed and the exact M14 cache artifact
  to be available.
- Authenticated hybrid validation requires an already-installed Salesforce CLI
  and previously authenticated target alias. It does not create, authenticate,
  reset, or delete orgs, and dry-run results remain subject to Salesforce
  availability and org compute limits.

## Handoff checklist

After meaningful implementation work:

- Update the completed and limitation lists above.
- Update the active milestone if its exit criterion passes.
- Update `docs/COMPATIBILITY.md` for changed behavior.
- Add or update conformance tests.
- Add an ADR if an architectural boundary or expensive choice changed.
- Run the verification commands in `AGENTS.md`.
