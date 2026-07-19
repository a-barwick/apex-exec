# Compatibility

This document states the currently supported Apex surface. It is a product
contract, not a claim of complete Salesforce compatibility.

## Fidelity levels

| Level | Meaning |
|---|---|
| Exact | Differentially verified against Salesforce for the documented cases |
| Compatible | Intended to match common observable Apex behavior |
| Simplified | Preserves the useful shape but intentionally omits semantics |
| Stubbed | Recognized but reports an explicit unimplemented error |
| Unsupported | Rejected explicitly during lexing, parsing, or checking |
| Planned | Not implemented yet |

No behavior is currently labeled **Exact** because no recorded fixture in this
repository has yet been executed against Salesforce. M13 provides the
differential harness; promotion still requires reviewed scratch-org evidence
for the documented case.

## Language surface

| Feature | Parse | Check | Execute | Fidelity | Notes |
|---|---:|---:|---:|---|---|
| `String` | Yes | Yes | Yes | Simplified | Single-quoted literals, common escapes, and the documented M3 method subset |
| `Boolean` | Yes | Yes | Yes | Compatible | `true` and `false` are case-insensitive |
| `Integer` | Yes | Yes | Yes | Simplified | Stored as Rust `i64`; Apex range/overflow pending |
| `Decimal` | Yes | Yes | Yes | Simplified | Decimal literals, mixed Integer arithmetic, comparison, parsing, scale, and fixed-point display |
| `Date` | Yes | Yes | Yes | Simplified | UTC construction/parsing, arithmetic, components, and deterministic formatting |
| `Datetime` | Yes | Yes | Yes | Simplified | UTC construction/parsing, epoch milliseconds, arithmetic, date/time projections, and formatting |
| `Time` | Yes | Yes | Yes | Simplified | Millisecond construction/parsing, wrapping arithmetic, components, and formatting |
| `Id` | Yes | Yes | Yes | Compatible | Validated 15/18-character standalone values with checksum-aware `to15`/`to18` |
| `Blob` | Yes | Yes | Yes | Simplified | UTF-8 value construction, text conversion, size, and Base64 encode/decode |
| `Object` | Yes | Yes | Yes | Simplified | Assignment, overload widening, explicit casts, and `toString()` |
| Explicit initialization | Yes | Yes | Yes | Compatible | Uninitialized declarations are rejected |
| Uninitialized/multi-declarator locals | No | No | No | Unsupported | Valid Apex forms are planned for M21 grammar closure |
| Assignment | Yes | Yes | Yes | Compatible | Invariant supported types or `null`; chained assignment is right-associative |
| Variable references | Yes | Yes | Yes | Compatible | Checked before execution |
| Case-insensitive names | Yes | Yes | Yes | Compatible | Original spelling is preserved |
| Line/block comments | Yes | N/A | N/A | Compatible | Unterminated block comments are errors |
| `System.debug(expression)` | Yes | Yes | Yes | Simplified | Structured platform-host event exposed as plain output by the default host; no Salesforce log metadata |
| Integer arithmetic | Yes | Yes | Yes | Simplified | `+`, `-`, `*`, `/`, `%`, unary signs; checked `i64` runtime behavior |
| Comparison and equality | Yes | Yes | Yes | Compatible | Integer ordering; case-insensitive String `==`; same-type collection and null equality |
| Boolean operators | Yes | Yes | Yes | Compatible | Short-circuit `&&`, <code>&#124;&#124;</code>, and unary `!` |
| String concatenation | Yes | Yes | Yes | Simplified | `+` converts every supported non-Void value; complete String content is preserved and collection text uses deterministic cycle-safe local formatting |
| Increment/decrement | Yes | Yes | Yes | Compatible | Prefix and postfix forms on `Integer` variables and List indexes |
| Ternary expression | Yes | Yes | Yes | Compatible | Right-associative, checked Boolean condition, common result type, lazy selected arm, and branch coverage |
| `instanceof` | Yes | Yes | Yes | Compatible | Viable runtime alternatives over supported types, invariant generic identity, single evaluation, and null-false current-profile behavior |
| Safe navigation | Yes | Yes | Yes | Compatible | Evaluate-once instance member/method access, member/method chain short-circuiting, lazy arguments, typed nulls, and null-aware single-record SOQL; indexed chain continuation is rejected |
| Null coalescing | Yes | Yes | Yes | Compatible | Left-associative Apex precedence, evaluate-once left operand, lazy right operand, checked type joins, and branch coverage |
| Bitwise/shift operators | No | No | No | Unsupported | Includes compound forms and `Long`; planned in M19 |
| Nested blocks and scopes | Yes | Yes | Yes | Compatible | Shadowing and lookup are case-insensitive |
| Conditional statements | Yes | Yes | Yes | Compatible | `if` and `if`/`else` |
| `switch on` / `when` | No | No | No | Unsupported | Required by M21 North Star grammar closure |
| Loops and loop control | Yes | Yes | Yes | Compatible | Traditional and enhanced `for`, `while`, `do`/`while`, `break`, and `continue` |
| Anonymous `return` | Yes | Yes | Yes | Simplified | Value-less return terminates anonymous execution; declared methods have checked values |
| `null` | Yes | Yes | Yes | Simplified | Assignable to every supported value type; selected runtime null behavior implemented |
| `List<T>` | Yes | Yes | Yes | Compatible | Recursive invariant type; ordered, indexed, mutable reference value |
| `Set<T>` | Yes | Yes | Yes | Simplified | Unique mutable reference value with deterministic local insertion order |
| `Map<K,V>` | Yes | Yes | Yes | Simplified | Deterministic local insertion order; `keySet()` is a snapshot |
| Array syntax | Yes | Yes | Yes | Simplified | One-dimensional `T[]` alias for `List<T>`; sized construction validates and supports primitive, `Object`, known custom-class, and core-exception elements |
| Collection literals | Yes | Yes | Yes | Compatible | List/Set elements and Map `key => value` entries |
| Collection indexing | Yes | Yes | Yes | Compatible | List/array reads and writes; Set/Map indexing is rejected |
| Built-in method calls | Yes | Yes | Yes | Compatible | Fixed case-insensitive collection, String, Math, System, and core-exception surface; checked calls carry typed intrinsic IDs |
| User-defined methods | Yes | Yes | Yes | Simplified | Class instance/static methods plus backwards-compatible top-level declarations; typed returns, overloads, recursion, and checked targets |
| Explicit casts | Yes | Yes | Yes | Simplified | Same-type, Object, core-exception, and related user-class/interface casts, structurally disambiguated from grouped member/index/postfix expressions; invalid runtime casts throw `TypeException` |
| Exception control flow | Yes | Yes | Yes | Simplified | `try`, typed `catch`, `finally`, `throw`, rethrow, and core exception construction |
| Runtime exception promotion | N/A | N/A | Yes | Compatible | Null dereference, bounds, arithmetic, String-range, and cast faults are catchable typed exceptions |
| Runtime source stacks | N/A | N/A | Yes | Simplified | Method failures retain deterministic innermost-to-outermost source call frames, including independently mapped cross-file callers |
| Classes/interfaces | Yes | Yes | Yes | Simplified | Top-level classes/interfaces, construction, object identity, member calls, `interface extends`, cycle-checked hierarchy edges, and visited iterative subtype traversal |
| Nested types and enums | No | No | No | Unsupported | Qualified nested identities, enums, and type literals are planned in M20 |
| Typed custom SObjects | Yes | Yes | Yes | Simplified | Metadata-aware project compilation, construction, case-insensitive checked field access, and in-memory identity |
| Dynamic `SObject` | Yes | Yes | Yes | Simplified | `new SObject(apiName)`, `get(String)`, and `put(String,Object)`; unknown runtime names raise `IllegalArgumentException` |
| Static SOQL | Yes | Yes | Yes | Simplified | Checked direct/parent fields, binds, filters, ordering, limits, aggregates, and SQLite execution |
| Static SOSL | Yes | Yes | Yes | Simplified | Checked returning clauses with deterministic local String-field matching |
| DML statements | Yes | Yes | Yes | Simplified | Scalar/bulk insert, update, upsert, delete, and undelete with atomic trigger execution |
| `Database` DML methods | Yes | Yes | Yes | Simplified | Common methods are atomic and return void; partial result APIs are unsupported |
| Apex triggers | Yes | Yes | Yes | Simplified | Schema-checked `.trigger` units, typed contexts, eight before/after DML events, bulk handlers, and bounded recursion |
| Transaction rollback | N/A | N/A | Yes | Compatible | Caught failures roll back one DML tree; uncaught failures roll back the entry-point transaction |
| Recycle bin / undelete | Yes | Yes | Yes | Simplified | Deleted local records retain fields and IDs for deterministic undelete |
| `AggregateResult` | Yes | Yes | Yes | Simplified | Grouped query results with `get(String)` |
| Static/instance members | Yes | Yes | Yes | Simplified | Fields, methods, lazy per-class initialization with cached success/failure, checked cycles/depth, overloads, checked dispatch, and static entry-point invocation |
| Inheritance/access modifiers | Yes | Yes | Yes | Simplified | Single class inheritance, interfaces, access checks, abstract/virtual/override, and virtual dispatch |
| Properties | Yes | Yes | Yes | Simplified | Auto and custom get/set accessors with accessor-specific visibility |
| Test annotations | Yes | Yes | Via runner | Simplified | Case-insensitive `@IsTest`, optional `SeeAllData=false`, method-only `@TestSetup`, and correct `Test.isRunningTest()` mode in tests and their queued work; `SeeAllData=true` is explicit |
| `@future` | Yes | Yes | Via drain | Simplified | Public/global static void methods; primitive and primitive List/Set arguments are snapshotted at enqueue |
| Queueable Apex | Yes | Yes | Via drain | Simplified | Checked interface contract, deterministic `System.enqueueJob`, context job ID, payload snapshot, and FIFO execution |
| Batch Apex | Yes | Yes | Via drain | Simplified | Checked single-argument `Database.Batchable<T>` contract whose declared `T` binds the List-returning `start` and `execute` scope types, plus deterministic chunking, context job ID, and `finish` |
| Scheduled Apex | Yes | Yes | Via drain | Simplified | Checked Schedulable contract, seven-field cron shape validation, deterministic submission, and trigger ID |
| Platform events | Yes | Yes | Via drain | Simplified | `EventBus.publish` queues imported `__e` records for after-insert trigger delivery; no retention or replay |
| JSON | Yes | Yes | Yes | Simplified | Ordered primitive/List/Set/String-keyed Map serialization, catchable cycle/limit failures, and recursive untyped deserialization |
| Regex | Yes | Yes | Yes | Compatible | `Pattern.compile`/`quote` and stateful `Matcher` match/find/group/start/end |
| Schema describe | Yes | Yes | Yes | Simplified | Imported-object global describe, name, key prefix, and custom flag; qualified and unqualified Schema type spellings are accepted |
| HTTP callouts | Yes | Yes | Via host | Simplified | Stateful request/response APIs, queued mock responses, captured requests, and no live network |
| Persistent REPL | Yes | Yes | Yes | Simplified | Accepted snippets share deterministic replayed state; failed snippets do not commit |
| Statement debugger | N/A | N/A | Yes | Simplified | Opt-in breakpoints, step in/over/out, frames, cycle-safe variables, database events, and transaction timelines over bounded immutable snapshots |
| Editor navigation | N/A | Via HIR | N/A | Simplified | Project class/member definitions, references, rename edits, and source-mapped inline diagnostics |
| Coverage overlays | N/A | N/A | Via runner | Compatible | Every executable production line is exposed as covered or uncovered to editor clients |

## M3 built-in method surface

Method names are case-insensitive. Supported overloads still receive static
arity and argument-type checking.

- `List<T>`: `add`, `addAll`, `clear`, `clone`, `contains`, `get`, `indexOf`,
  `isEmpty`, `remove`, `set`, `size`, and scalar `sort`. `add` accepts either a
  value or an index and value. `sort` places null before non-null values.
- `Set<T>`: `add`, `addAll`, `clear`, `clone`, `contains`, `containsAll`,
  `isEmpty`, `remove`, `removeAll`, `retainAll`, and `size`.
- `Map<K,V>`: `clear`, `clone`, `containsKey`, `get`, `isEmpty`, `keySet`,
  `put`, `putAll`, `remove`, `size`, and `values`.
- Static `String`: `valueOf`, `join`, `isBlank`, `isNotBlank`, `isEmpty`, and
  `isNotEmpty`.
- Instance `String`: `length`, `contains`, `startsWith`, `endsWith`, `equals`,
  `equalsIgnoreCase`, `indexOf`, one- and two-argument `substring`, `trim`,
  `toLowerCase`, `toUpperCase`, and literal `replace`.
- Integer-backed `Math`: `abs`, `max`, `min`, and `mod`.
- `System`: `debug`.

String `length`, `indexOf`, and `substring` use UTF-16 code-unit positions for
ordinary Unicode scalar strings. A substring boundary that would split a
surrogate pair is rejected explicitly because Rust strings cannot contain the
resulting unpaired surrogate. This limitation, along with Rust-backed Unicode
case and whitespace behavior, keeps the String surface at **Simplified**
fidelity.

## Collection runtime fidelity

Collection assignment aliases the same mutable reference. Copy constructors
and `clone()` create independent shallow copies. List order is preserved. Set
iteration and Map-derived order are deterministic insertion order locally for
repeatability; this does not attempt to reproduce Salesforce's deterministic
internal ordering. `Map.keySet()` returns a snapshot rather than a backed view.
Direct enhanced iteration over a Map is rejected; callers iterate `keySet()` or
`values()` instead.

## M4 methods, casts, and exceptions

Methods are collected before any body is checked, so forward calls and
recursion are supported. Names are case-insensitive. A method overload is
identified by its name and exact parameter-type sequence; return type alone
cannot distinguish overloads. Applicable candidates are compared
parameter-by-parameter: one wins only when every parameter is identical to or
more specific than the corresponding parameter on the others, with at least
one strict improvement. The supported subtype relationships are concrete core
exceptions to `Exception` and every value type to `Object`. Crossing or
unrelated candidates remain ambiguous, including for `null`. The selected
call target is recorded during checking rather than rediscovered from runtime
values. M5 moves that target and every checked expression type out of parsed
syntax into typed HIR side tables.

Backwards-compatible top-level declarations remain available to anonymous
scripts. Each invocation has an isolated local scope and cannot read the
caller's locals. Non-`void` methods must return or throw on every statically
reachable path. `finally` executes during normal completion, return, loop
control, and exception unwinding; an abrupt completion in `finally` replaces
the pending result.

The implemented exception types are `Exception`, `NullPointerException`,
`ListException`, `MathException`, `TypeException`, `StringException`,
`IllegalArgumentException`, `FinalException`, `AssertException`,
`QueryException`, `DmlException`, and `AsyncException`. They support zero- or one-String-argument
construction and `getMessage()`, `getTypeName()`, and
`getStackTraceString()`. Catch matching recognizes each concrete type and the
`Exception` root. Custom exception classes, causes, a broader built-in
hierarchy, and Salesforce-exact message and stack formatting are not yet
claimed.

`Object` is a checked widening, overload, runtime-cast, and `toString` carrier.
Casts include identical types, `Object` up/downcasts, the core exception root,
and related user classes/interfaces. Unsupported unrelated casts are compile
errors, while a permitted downcast with the wrong runtime value throws
`TypeException`. Equality/hash and the broader inherited Object API are not
implemented.

## M5 classes and project compilation

Top-level class and interface names are case-insensitive and participate in
cross-file resolution. Supported class members are constructors, fields,
properties, and methods. Fields receive typed null before explicit
initialization; static state belongs to the interpreter and initializes lazily
per class, while instance state uses object identity. Static fields and auto
properties are all allocated to typed null before source-order field
initializers run. Base classes initialize first; successful and failed outcomes
are cached. Cross-class dependency cycles and chains deeper than 64 active
classes raise catchable `TypeException` values. Auto properties use
interpreter-owned backing storage, while custom accessors execute checked
bodies. `this`, `super`, static access, and bare member access resolve at
compile time.

Classes may extend one virtual/abstract class and implement interfaces.
Abstract, virtual, and override declarations are validated, interface
obligations are enforced, every superclass/interface edge participates in
iterative cycle validation, and instance calls use virtual dispatch. Interfaces
extend interfaces; an `implements` clause on an interface is rejected as
invalid syntax before contract traversal. User types participate in assignment,
overload ranking, and related up/downcasts through a visited iterative subtype
walk. Access checks cover public, private, protected, and global members,
including accessor-specific property visibility. Nested types, enums, explicit
superclass-constructor calls, custom exception classes, and the full Apex
conversion system remain unsupported.
Sharing modifiers parse so class declarations remain structurally inspectable,
but semantic checking rejects them because sharing/security behavior is
deferred rather than silently ignored.

SFDX project compilation finds `sfdx-project.json`, loads package-directory
paths, recursively discovers `.cls` units, and requires one top-level type whose
name matches each filename. Compilation produces cross-file dependency edges
and a merged checked HIR with file-aware diagnostics. Cached paths retain stable
source identities and file-local offsets, so merging does not rewrite AST
spans. A persistent compiler reuses unchanged parsed units, calculates
reverse-dependent invalidation, and reuses the complete checked build when all
inputs are unchanged; semantic linking currently reruns across the project
after any source change.

The CLI accepts `check <project-or-package-directory>` and an `invoke` form with
`<project> <Class.method>`. Invocation is deliberately limited to a
public/global static zero-argument method and prints a non-void return after any
`System.debug` output.

## M6 Apex test runner

An `@IsTest` class may contain `@IsTest` and `@TestSetup` methods. Both method
kinds must be static, return void, accept no parameters, and have a body.
Private top-level test classes are accepted. `SeeAllData=false` is recognized;
`SeeAllData=true` is rejected because no org data host exists. Other
annotations remain explicit parse errors.

`System.assert(Boolean[, message])`, `System.assertEquals(expected, actual[,
message])`, and `System.assertNotEquals(expected, actual[, message])` raise a
catchable `AssertException` on failure. The legacy `testMethod` modifier, newer
`Assert` class, and broader `Test` APIs are not implemented.

Project tests are discovered and sorted case-insensitively by `Class.method`.
Filters accept a class, method, exact qualified name, or `*` glob. Every test
receives a new interpreter; setup methods execute before that test in the same
interpreter. Its explicit execution context makes `Test.isRunningTest()` true
for setup/test methods and deterministic async work they submit; ordinary and
debugger entry points remain false. A bounded worker pool can therefore run
tests in parallel without sharing static fields, object/collection/SObject
identity, default recording-host output, runtime stacks, or database records.
M8 makes setup DML visible inside that test interpreter. One-time setup
snapshot optimization remains future work.

Console output and JUnit XML report pass/failure results. Coverage includes
executable statement lines in non-test classes and the true/false outcomes of
`if`, `while`, `do`/`while`, and condition-bearing `for` branches. These counts
are deterministic local coverage, not Salesforce-exact coverage accounting.

## M7 SObject schema and SQLite

Project compilation imports custom-object metadata from SFDX package
directories. Decomposed `.object-meta.xml`/`.field-meta.xml` layouts and
monolithic `.object` files normalize into one case-insensitive catalog.
Supported field kinds are Checkbox, zero-scale Number, common String-shaped
types, Id, Lookup, and MasterDetail. Every imported object receives an Id field;
custom name-field metadata adds Name. Unsupported types and nonzero Number
scales fail explicitly.

Imported custom-object API names are valid Apex types in project compilation.
They support zero-argument construction and statically checked field reads,
writes, and integer increment/decrement. A typed custom object widens to the
dynamic `SObject` root, whose `get` and `put` methods validate object and field
names at runtime. SObjects have mutable reference identity and deterministic
debug formatting. Id and relationship fields currently surface as `String` in
Apex; the storage layer retains validated `RecordId` values.

`RecordId` validates 15- or 18-character ASCII-alphanumeric Salesforce ID
shapes, verifies 18-character case checksums, and deterministically generates
18-character IDs from an object key prefix and sequence.

The SQLite adapter creates one physical table per normalized object plus schema
registry tables. Migrations add new objects and fields while preserving data;
incompatible changes to existing prefixes, types, nullability, or relationship
targets fail explicitly. Storage supports unconditional create/update, read,
delete, commit, rollback, named savepoints, fixture replacement, and fast
record reset. M8 implements DML validation and query semantics above this
boundary; M9 adds trigger, recycle-bin, and transaction-checkpoint semantics.

## M8 SOQL, SOSL, and DML

SOQL and SOSL are dedicated grammar nodes rather than ordinary Apex expressions
or runtime query strings. Static queries validate object names, selected and
filtered fields, aggregate arguments, grouping, ordering, parent relationship
paths, and bind types against imported metadata. Checked HIR stores
schema-indexed plans; bind values are evaluated once at runtime.

SOQL supports direct fields and one custom parent relationship level through
`__r`, comparison operators, `LIKE`, literal or collection-bound `IN`/`NOT IN`,
`AND`/`OR`/`NOT`, `ORDER BY` with direction and null placement, `LIMIT`,
`OFFSET`, list results, contextual single-record results, and scalar
`COUNT()`. Aggregate results support grouped direct fields plus `COUNT`,
`SUM`, `MIN`, and `MAX`; aliases are read through `AggregateResult.get`.
Child subqueries, `HAVING`, `TYPEOF`, date literals, polymorphic relationships,
and the broader SOQL surface remain unsupported.

SOSL supports a String literal or bind after `FIND`, `IN ALL FIELDS` and
`IN NAME FIELDS`, and one or more checked `RETURNING` clauses with fields,
filters, ordering, and limits. Matching is deterministic case-insensitive
substring search over stored String fields. It does not claim Salesforce
tokenization, stemming, wildcard, snippet, division, or relevance behavior.

Scalar and List values support `insert`, `update`, `upsert`, `delete`, and
`undelete` statements plus the corresponding `Database` methods. Inserts generate
deterministic Salesforce-shaped Ids and copy them into the original runtime
SObjects. Each DML call is atomic. Delete retains the stored image in a local
recycle bin, and undelete restores its Id and fields. The current `Database`
methods return `void`; SaveResult/DeleteResult APIs and `allOrNone=false`
partial results are rejected explicitly. External-ID upsert, validation rules,
workflows, sharing, and limits are not yet implemented.

The default recording host owns one lazy in-memory SQLite database per
interpreter and publishes structured SOQL, SOSL, and DML events. Test setup DML
is visible to its test, while separate test interpreters remain isolated.
One-time Salesforce test-setup snapshot optimization is not reproduced.

## M9 triggers and transaction semantics

SFDX discovery loads `.trigger` files alongside classes. Trigger declarations
validate their schema object and unique event set, then check their bodies
against typed context values. Supported contexts are `Trigger.new`,
`Trigger.old`, `Trigger.newMap`, `Trigger.oldMap`, `Trigger.isExecuting`,
`isBefore`, `isAfter`, `isInsert`, `isUpdate`, `isDelete`, `isUndelete`, and
`size`. Context spelling is case-insensitive. Lists and maps use the concrete
SObject type, so common trigger-handler signatures compile without casts.

Before and after insert, update, delete, and undelete run over bulk groups.
Mixed upserts split into concrete insert and update groups. Before new records
alias the caller's runtime SObjects and may change persisted fields; old
records, after new records, and all context collections are read-only and raise
`FinalException` on mutation. Recursive DML executes triggers synchronously and
is capped at 16 active trigger levels with an explicit `DmlException`.

Every DML tree owns a nested database checkpoint. A trigger failure caught by
Apex rolls back that DML and recursive work it started while preserving earlier
successful DML. An exception escaping anonymous execution, static invocation,
or a test rolls back the whole entry-point transaction. Checkpoints include
active records, recycled records, and deterministic Id sequences.

The recording host exposes trigger enter/exit events and DML events in one
ordered transaction timeline. This is deterministic local ordering, not a
claim of Salesforce-exact order-of-execution automation. Multiple triggers for
one object run in deterministic project source order; Salesforce does not
guarantee an equivalent order. `addError`, merge, validation/workflow/flow
automation, mixed-SObject bulk lists, and partial DML results remain
unsupported.

## M10 curated platform compatibility

The first compatibility profile is named `m10-common`. Supported platform
calls are statically selected HIR intrinsics; calls outside the recognized
surface produce a compile diagnostic naming the API and profile.

- `Date`: `newInstance`, `valueOf`, `today`, `addDays`, `addMonths`, `addYears`,
  `daysBetween`, `format`, `year`, `month`, and `day`.
- `Datetime`: `newInstance`, `now`, `valueOf`, `valueOfGmt`, `getTime`,
  `date`/`dateGmt`, `time`/`timeGmt`, arithmetic through seconds, and `format`.
- `Time`: `newInstance`, `valueOf`, arithmetic through milliseconds,
  components, and `format`.
- `Decimal`: literals, mixed Integer operators, `valueOf`, `setScale`, `abs`,
  `scale`, and deterministic text conversion.
- `Id`: `valueOf`, `to15`, and `to18`; `Blob`: `valueOf`, `toString`, and
  `size`; `EncodingUtil`: Base64 encode/decode.
- `JSON`: `serialize`, `serializePretty`, and `deserializeUntyped`; structural
  serialization rejects cycles or exhausted traversal budgets with catchable
  `IllegalArgumentException`.
- Regex: `Pattern.compile`/`quote` and
  `Matcher.matches`/`find`/`group`/`start`/`end`.
- Describe: `Schema.getGlobalDescribe`, `SObjectType.getDescribe`, and
  object-level `getName`, `getKeyPrefix`, and `isCustom`.
- Deterministic services: `System.now`/`today`/`currentTimeMillis`,
  `Datetime.now`, `Date.today`, `Math.random`, and common `UserInfo` methods.
- Tests/limits: `Test.startTest`, `stopTest`, `isRunningTest`, and current/limit
  query, DML statement, and callout counters. `isRunningTest` reads the
  explicit execution context rather than a hardcoded value.
- Callouts: common `HttpRequest`/`HttpResponse` state plus `Http.send`.
  `PlatformHost` supplies responses; `RecordingHost` queues mocks and captures
  requests. An unmocked callout throws `CalloutException` and no live network
  transport exists.

Date/time formatting is fixed UTC rather than locale/time-zone aware. Decimal
does not expose every Apex rounding mode. Typed JSON deserialization, arbitrary
user-object reflection, field describe, `HttpCalloutMock`/`Test.setMock`, named
credentials, and namespace-qualified `System.JSON` syntax remain unsupported.
Runtime user objects therefore keep their existing identity-string JSON
surface rather than recursively exposing fields.
SObject Id/reference fields retain their earlier String surface; standalone
`Id` values are validated but full field-level integration is future work.

## M11 deterministic asynchronous execution

Classes may implement the checked built-in `Queueable`,
`Database.Batchable<T>`, and `Schedulable` contracts. The checker records their
execute/start/finish targets in HIR, validates context and scope types, and
rejects incomplete contracts before runtime. `Database.Batchable` requires
exactly one retained type argument, and that declared type determines the
`List<T>` returned by `start` and the `List<T>` passed to `execute`. Generic
arguments on `Queueable`, `Schedulable`, and user-defined interfaces are
rejected explicitly. `@future` methods must be public/global static void methods
and accept only supported primitive values or Lists/Sets of those primitives.

`System.enqueueJob`, future calls, `Database.executeBatch`, `System.schedule`,
and `EventBus.publish` append work to one interpreter-owned FIFO. Submission
returns deterministic Salesforce-shaped `707` IDs where Apex exposes an ID.
Class, collection, and SObject payloads are deep-snapshotted at enqueue so later
synchronous mutation is not visible to the job. Queueable, batch, and scheduled
contexts expose their job/trigger ID, and `System.isFuture`, `isQueueable`,
`isBatch`, and `isScheduled` identify the active job.

Queued work never runs on a background thread. `Test.stopTest` is the explicit
drain point and processes work in submission order, including work chained by a
running job, up to a deterministic 100-job bound. Each job uses a nested
database checkpoint and publishes structured queued, started, completed, or
failed lifecycle events. It captures the submitting test/debug mode, installs
that context for the job, and restores its caller after success or failure. A
failure rolls back that job and fails the drain.

Batch `start` currently returns `List<T>`; `QueryLocator`, `Iterable`, stateful
serialization between chunks, flex-queue policy, monitoring, abort, and
rescheduling are not implemented. Scheduled cron text is checked for seven
fields but is not evaluated against a clock. Platform-event delivery supports
imported `__e` records and after-insert triggers without retention, replay IDs,
publish result objects, or subscriber retry policy. Async work shares its
interpreter's static store, so Salesforce-exact cross-transaction static
isolation is not claimed.

## M12 debugger, REPL, and editor integration

`apex-exec repl` retains accepted declarations, variables, collections, and
database effects by checking and deterministically replaying accumulated source.
A failing snippet leaves the accepted source and observable state unchanged.
`:source`, `:reset`, and `:quit` manage the session.

`apex-exec dap` implements stdio Debug Adapter Protocol framing and the
initialize/launch/configuration, breakpoint, thread, stack, scope, variable,
continue, step-in, step-over, step-out, terminate, and disconnect workflows.
Project launches use a public static zero-argument `Class.method`; scripts run
their anonymous statements. Custom `apex/database` and
`apex/transactionTimeline` requests expose source-stop-aware platform events.
Debugging navigates deterministic pre-statement snapshots, not a suspended live
runtime. Debugger launches alone opt into snapshot allocation and retain the
earliest 4,096 snapshots under an estimated 16 MiB snapshot-memory ceiling,
with at most 256 variables and 128 frames per snapshot and 16 KiB per rendered
value. `DebugExecution::trace_status` reports when any bound truncates the
trace. Rendered values also use a 64-level, 4,096-node, and 4,096-element graph
budget, mark cycles as `<cycle>`, and mark exhausted display budgets as `…`.
That 16 KiB limit applies only to debug and debugger presentation; semantic
String conversion and ordinary invocation preserve complete scalar and nested
String content while retaining the structural graph budgets. Ordinary
execute/invoke paths select no instrumentation, while the test runner records
coverage facts without debugger snapshots. Expression evaluation and value
mutation from the debug console are not supported.

`apex-exec lsp [project]` implements stdio Language Server Protocol
initialization, full-document synchronization, inline diagnostic publication,
go-to-definition, references, project symbol rename, and the custom
`apex/coverage` request. Navigation and rename use checked HIR class/member
targets case-insensitively. Local-variable rename, completion, hover, formatting,
semantic tokens, unsaved cross-file semantic linking, and code actions remain
future work.

## M13 Salesforce compatibility oracle

`apex-exec oracle` loads a versioned JSON conformance manifest and runs every
fixture through Apex Exec plus either a live Salesforce scratch org or a
recorded Salesforce snapshot. A fixture selects a compile-only, public static
zero-argument invocation, or individual Apex test entry point and explicitly
lists the dimensions it measures.

Normalized observations cover compile success and broad diagnostic category,
named JSON values, ordinary debug output, exception type/message/stack,
SOQL/SOSL row effects, DML effects, trigger enter/exit order, and Apex test
outcomes. Named values use
`APEX_EXEC_ORACLE_VALUE|name|json-value` debug markers, which are removed from
ordinary output before comparison. Provider-specific diagnostic wording and
test failure rendering remain in snapshots as evidence but are not treated as
equivalent behavior.

Live runs use the authenticated Salesforce CLI to deploy each fixture project
to the selected org, execute anonymous Apex or one test, and normalize JSON and
debug logs. `--record-salesforce` writes the normalized org result for durable
review and offline replay through `--salesforce-snapshot`. Reports publish
matched/total compatibility coverage overall and per selected dimension and
return a failing exit status for any difference.

The harness does not create, authenticate, or delete scratch orgs. It also does
not make unmeasured behavior exact: an **Exact** claim is scoped to reviewed
fixture evidence, the selected dimensions, API version, and Salesforce
environment represented by its recorded snapshot.

## M14 enterprise CI

`apex-exec ci manifest` seals an SFDX project into a versioned, portable input
manifest. It records the project configuration and every regular file below
the package roots with SHA-256. `apex-exec ci run` rejects modified, missing,
unrecorded, escaping, and symlinked inputs, then uses the full manifest and
effective shard as a content-addressed cache key. `--replay` requires that exact
artifact and never falls back to execution.

Changed `.cls` files use the checked project dependency graph to select test
classes transitively. A change to metadata, a trigger, a deleted source, or an
unknown path selects all tests conservatively. Sorted qualified test names are
partitioned deterministically with zero-based `--shard INDEX/TOTAL`; each shard
can run on an independent worker with the existing per-test thread pool.
`--changed-list` accepts newline-delimited paths produced by CI provider diff
commands.

Every executed or replayed artifact can produce JUnit test results, Cobertura
line coverage, and SARIF 2.1 compile/test/policy diagnostics. Policies can gate
test failures, line and branch coverage, total compile/test duration, and the
measured percentage in an M13 compatibility report. The compatibility report
itself becomes a sealed input. `ci integrations` emits pull-request templates
for GitHub Actions, GitLab CI, and Jenkins with two distributed shards. The
templates invoke an `apex-exec` binary provisioned by the enterprise runner.

This is deterministic local CI selection, not a claim that implicit Salesforce
dependencies can always be inferred. The conservative fallback is intentional,
and coverage/performance gates on individual shards describe that shard unless
the surrounding provider aggregates their standard reports.

## M15 hybrid deployment confidence

`apex-exec hybrid` consumes an M14 hermetic CI manifest as the release
candidate. Known changed Apex classes select deployable classes and affected
tests through reverse dependency closure. Metadata, triggers, deletions, and
unknown paths select every project component and test conservatively.

SFDX source and sidecars normalize into Metadata API component identities with
SHA-256 content digests. Schema and configuration drift is reported for
project-owned components that are not directly changed by the release.
Directly changed metadata is treated as intended deployment payload; code
differences are validated by the deployment and test differential instead.
Supported inventory categories include Apex classes/triggers, custom objects
and fields, common object children, permission sets, profiles, settings, flows,
layouts, labels, custom metadata, roles, groups, queues, tabs, applications,
workflows, named credentials, and remote-site settings.

The classifier has 28 static metadata-type mappings. Unrecognized unchanged
paths do not enter inventory or drift accounting, and multi-part Custom
Metadata filenames are currently truncated at the first dot. M26 replaces this
curated classifier with explicit, API-versioned file accounting.

With `--target-org`, Apex Exec verifies an existing authenticated alias,
retrieves the scoped org metadata into isolated project-local `.apex-exec`
directories, and runs `sf project deploy start --dry-run`. Each retrieve
directory is prepared with the legacy `main/default` output shape before the
command and removed after inventory capture; this works with both the older
and current Salesforce CLI output contracts exercised by M17. Method-qualified
affected tests remain sealed in the evidence, while the Metadata API transport
deduplicates them to class names for `RunSpecifiedTests`; readiness compares
only the exact selected methods. A no-test selection uses `NoTestRun`.
Salesforce may execute and report other methods in a selected class, but those
extra observations do not expand the bound request. Auth inspection does not
request verbose output or persist access tokens/auth URLs. The tool does not
create, authenticate, reset, or delete orgs.

Version-1 validation snapshots are rejected. M17 schema-version-2 snapshots
bind the provider-neutral inventory and validation observations to the exact
serialized M14 manifest, cache key, CI-result digest, changed paths, affected
component selectors/digests, selected tests, test level, target alias and org
ID, API version, Apex Exec/Salesforce CLI versions, capture time, exact age
policy, and full snapshot digest. Authenticated capture requires two identical
normalized retrieval digests before check-only deployment.

`--validation-snapshot` replay is credential-free but requires the exact M14
cache artifact, expected target and org ID, matching tool/API versions, and
unexpired evidence. A release is ready only when those identity checks pass,
hermetic local CI and policy pass, the Salesforce dry run passes, unaffected
schema/configuration has no drift, and every affected local test outcome
matches Salesforce. JSON and console reports include the evidence identity,
affected components/tests, coverage, drift, differential percentage, and
explicit blockers. The reviewed `evidence/milestone17/` bundle records a clean
authenticated capture and exact offline replay for one sealed candidate, plus
a controlled unchanged-PermissionSet drift that blocks release even though the
check-only deployment and selected tests pass. The org baseline was restored
and recaptured clean. This is release-readiness evidence, not an **Exact**
language or Salesforce compatibility claim.

## Platform surface

| Feature | Status | Target milestone |
|---|---|---|
| SFDX project loading | Implemented (simplified) | M5 |
| Apex unit tests | Implemented (simplified) | M6 |
| SObject schema | Implemented (simplified metadata importer and catalog) | M7 |
| Typed/dynamic SObjects | Implemented (schema-checked fields and DML identity) | M7 |
| Salesforce-shaped IDs | Implemented (validation, checksum, deterministic generation) | M7 |
| SQLite storage | Implemented (additive migration, CRUD, transactions, savepoints, fixtures/reset) | M7 |
| DML | Implemented (simplified atomic scalar/bulk operations) | M8 |
| Partial DML results | Planned (`allOrNone=false` and result/error objects) | M24 |
| SOQL | Implemented (simplified checked static queries) | M8 |
| Broader SOQL | Planned (enterprise-prioritized relationships, aggregation, literals, and polymorphism) | M23 |
| SOSL | Implemented (simplified deterministic local search) | M8 |
| Structured query/DML/trigger timelines | Implemented | M9 |
| Triggers | Implemented (simplified) | M9 |
| Transaction-wide rollback | Implemented (snapshot-backed) | M9 |
| Recycle bin / undelete | Implemented (simplified) | M9 |
| Common platform APIs | Implemented (`m10-common` profile) | M10 |
| Async Apex | Implemented (simplified deterministic drain profile) | M11 |
| Persistent REPL | Implemented (deterministic replay profile) | M12 |
| Debug Adapter Protocol | Implemented (snapshot-backed stdio server) | M12 |
| Language Server Protocol | Implemented (diagnostics/navigation/rename/coverage) | M12 |
| VS Code integration | Implemented (thin LSP/DAP client and coverage overlays) | M12 |
| Salesforce differential oracle | Implemented (live CLI adapter, recorded snapshots, normalized comparison, and measured coverage) | M13 |
| Enterprise CI | Implemented (hermetic manifests/replay, content cache, impacted tests, shards, standard reports, provider templates, and policy gates) | M14 |
| Hybrid deployment confidence | Implemented (affected components/tests, optional validation org, drift, test differential, readiness reports, and snapshot replay) | M15 |
| Governor limits | Deferred | Post-core compatibility profile |
| Candidate-bound live validation evidence | Implemented (schema v2, exact replay, repeated retrieval, and reviewed clean/blocked live bundle) | M17 |
| Broad metadata accounting and org-only drift | Planned | M26 |
| Sharing/security behavior | Planned (profile-scoped) | M27 |
| API-version differences | Planned | M25 |
| Runtime isolation for untrusted code | Out of scope | None |

## Compiler behavior

- Unknown characters and invalid strings fail lexing.
- Invalid or unsupported syntax fails parsing.
- Public raw-token parser construction requires exactly one terminal EOF, one
  source identity, and ordered non-overlapping spans; malformed streams return
  structured `TokenStreamError` values before parsing.
- Unknown variables, generic mismatches, invalid iteration/indexing, and
  invalid built-in or user-defined calls fail semantic checking.
- Unknown element types in constructed collections and sized arrays fail
  semantic checking before execution.
- Duplicate method signatures, ambiguous/no-match overloads, invalid return
  paths, invalid catches, and unsupported casts fail semantic checking.
- Duplicate/unknown classes, inheritance cycles, inaccessible members, invalid
  static/instance access, bad overrides, and missing abstract/interface
  implementations fail semantic checking.
- Invalid interface hierarchy syntax and unsupported hierarchy type arguments
  fail explicitly; `Database.Batchable<T>` retains and checks its declared
  element type rather than inferring it from methods.
- Supported runtime language faults are typed, catchable exceptions. Internal
  checked-state violations remain distinct diagnostics.
- Unsupported built-in methods are rejected explicitly rather than silently
  approximated.
- Unsupported annotations and invalid test/setup signatures are rejected
  before discovery; runtime and assertion failures become structured test
  failures without aborting the remaining suite.
- Invalid async annotations, interface contracts, submission types, cron
  shapes, batch sizes, non-serializable payloads, and drain overflow fail
  explicitly at the checker or runtime boundary.
- Unknown metadata objects/fields, incompatible custom-field assignments,
  unsupported metadata types, invalid dynamic SObject access, invalid IDs, and
  incompatible SQLite migrations fail explicitly at their owning boundary.
- Unknown query objects/fields/relationships, incompatible binds and
  aggregates, invalid DML operands, cardinality failures, invalid trigger
  contexts, recursion overflow, and unsupported partial DML semantics fail
  explicitly.
- Invalid REPL snippets do not commit; unsupported LSP/DAP requests return
  structured protocol errors, and breakpoints outside executable lines remain
  unverified.
- Invalid oracle schemas, escaping fixture paths, duplicate fixture names or
  dimensions, missing compile comparisons, provider mismatches, malformed
  Salesforce JSON, and unselected or absent providers fail explicitly.
- Invalid CI schemas, tool-version drift, unsafe or changed input inventories,
  invalid shards, corrupt cache artifacts, malformed changed-file lists, and
  unmet test/coverage/performance/compatibility policies fail explicitly.
- Invalid or tampered validation snapshots, unsafe/duplicate/noncanonical
  request fields, candidate/request/org/API/tool/age mismatches, unstable
  repeated retrievals, failed validation-org authentication/retrieval,
  malformed Salesforce JSON, drift, local policy failures, dry-run failures,
  and differential test mismatches fail explicitly or block release readiness.
- Diagnostics are generated by Apex Exec and are not required to reproduce
  Salesforce's exact wording.
- `tests/north_star/` contains pinned real-world complexity indicators. Their
  lexer/parser goal tests measure progress only; they are not compatibility or
  execution claims until promoted into the supported surface above.

M18 reproduces 5 of 14 passing goals (35.71%): 5 of 7 lexer goals and 0 of 7
parser goals, up from the 1-of-14 Phase 2 baseline. Ternary and `instanceof` are
no longer first blockers, and no current first diagnostic is a null-aware
operator. The current first diagnostics are bitwise syntax, unsupported
annotations, and nested declarations. M21 requires 14 of 14
passing against the unchanged corpus and removes every goal's `#[ignore]`.
These are syntax indicators only, not runtime or Salesforce compatibility
percentages.

## Updating this document

Any pull request or task that changes observable language or platform support
must update the relevant row. Promote behavior to **Exact** only when a fixture
has been run against Salesforce and the supported cases are recorded.
