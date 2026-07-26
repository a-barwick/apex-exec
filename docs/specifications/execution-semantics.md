# Execution Semantics

## Status

Primitive expressions through M19, lexical scopes, control flow, mutable
collections, checked calls/exceptions, classes, cross-file execution, isolated
tests, schema-backed SObjects, SOQL/SOSL/DML, triggers, curated platform
services, and deterministic async execution are implemented at the fidelity
summarized in `docs/COMPATIBILITY.md`. M12–M15 add debug/editor observations,
provider comparison, hermetic CI, and hybrid validation above the same runtime
boundaries. Remaining Phase 2 declarations, sharing/security, and API-version
profiles remain planned.

## Program execution

**Implemented.** Statements execute from first to last. Execution begins only
after parsing and semantic validation succeed.

## Variables

**Implemented.** A declaration evaluates each initializer from left to right
and stores the value under the identifier's canonical case-insensitive key.
An omitted initializer stores typed null. Each declarator enters the same scope
before the next initializer. Assignment replaces the stored value. Reads of
unknown variables are compile-time errors and are also guarded defensively by
the runtime. Primitive values copy by value. Collection values carry
execution-store-owned identity, so ordinary assignment aliases the same mutable
collection. Class instances use the same per-interpreter store; static fields
and properties belong to one interpreter run rather than process-global state.

## Debug output

**Implemented, simplified.** `System.debug(expression)` converts every
supported value to deterministic text and emits one structured debug event.
Lists, Sets, and Maps render recursively in their deterministic local order.
The default `RecordingHost` buffers each message as one returned output line,
and the CLI prints those lines without Salesforce log metadata. Custom runtime
hosts can consume or stream events without changing language execution.

## Expressions

**Implemented.** Arithmetic uses checked signed 32-bit Integer, signed 64-bit
Long, and Decimal operations and reports division, remainder, and overflow
failures. Integral `&`, `|`, `^`, `~`, `<<`, `>>`, and `>>>` use Integer or
Long width; shift distances are masked to 31 or 63 and unsigned right shift
zero-fills. Boolean `&`, `|`, and `^` evaluate both operands, while `&&` and
`||` short-circuit.

Assignment is right-associative. Simple and compound assignments plus prefix
and postfix increment/decrement resolve locals, List indexes, class members,
and SObject fields through one checked place. Receiver and index expressions
are evaluated once before the right operand. A failed read or numeric operation
does not write a partial value.

Ternary evaluates its Boolean condition once, records the condition outcome for
coverage, and evaluates exactly one arm. A runtime null Boolean raises
`NullPointerException`; errors and side effects in the unselected arm do not
occur. `instanceof` evaluates its value once and compares the resulting
non-null runtime identity with the checked target type. Null is false in the
current profile. Historical API-version behavior is deferred to M25 rather
than selected dynamically inside expression evaluation.

Method-call receivers are evaluated once, followed by arguments from left to
right, each exactly once. Static built-ins do not evaluate a runtime receiver.
Unsupported dispatch does not fall back to runtime approximation because calls
are validated before execution. Every supported built-in call is recorded as a
typed HIR intrinsic ID; runtime dispatch does not repeat receiver-family or
case-insensitive method-name resolution.

User-defined call arguments are also evaluated left to right exactly once.
Each invocation replaces the caller's lexical scopes with an isolated parameter
scope while sharing that execution's store and configured platform host. The
statically selected HIR target is executed directly, so runtime values do not
repeat overload or member resolution. Return values unwind blocks and loops to
the caller; recursive calls use the same isolation rules.

Interpreter preparation borrows the immutable checked program through a
runtime image. It does not clone the full AST, method list, class list, or HIR
tables. A per-interpreter execution store owns collection/object identity and
static slots; scopes, call state, traces, and host state remain isolated with
that interpreter when the host is owned. A custom host reference may
intentionally share external state.

## Classes and objects

**Implemented for M5, simplified.** Construction evaluates arguments left to
right, allocates object identity and inherited typed slots, initializes base
state before derived state, and then executes the selected constructor.
Instance methods receive the checked object; static methods have no instance
receiver. Class static fields initialize once per interpreter run.

Fields and automatic properties use checked typed slots. Custom getters and
setters execute isolated accessor frames; a setter receives the implicit
`value` parameter. Member access, assignment, and increment/decrement use HIR
targets selected during checking. Null object receivers raise
`NullPointerException`.

Virtual calls start from the checked signature and select the most-derived
concrete override on the runtime object's class. `super` calls bypass virtual
dispatch and execute the checked base target. User-object equality is identity
equality. Deterministic debug text uses a local `ClassName@id` shape and is not
claimed to match Salesforce formatting.

## SObjects

**Implemented for M7, simplified.** Metadata-aware project compilation treats
imported custom-object API names as concrete SObject types. Construction
allocates mutable reference identity. Direct fields resolve to checked schema
indices and retain Boolean, Integer, or String-shaped static types; reads of
unset fields return a typed null. Typed SObjects widen to the `SObject` root.

`new SObject(apiName)` resolves a dynamic type at runtime. `get(String)` and
`put(String,Object)` perform case-insensitive schema lookup and raise
`IllegalArgumentException` for unknown objects or fields. Dynamic writes
validate the runtime value against the normalized field kind. SObject equality
is identity equality. Deterministic debug text contains the object API name and
assigned fields. Ordinary field mutation remains in memory; explicit M8 DML
crosses the storage boundary and M9 trigger dispatch may mutate before images
inside that DML tree.

## Collections

**Implemented for M3.** Runtime collections are mutable reference values stored
in the interpreter's execution store.

- Assignment copies collection identity and therefore aliases mutations.
- Copy constructors and `clone()` allocate an independent shallow copy.
- List and Set copy constructors accept either a List or Set with the same
  element type. Map copy construction requires the identical Map type.
- List and Set literals evaluate elements from left to right. Set literals
  remove duplicates.
- Map literals evaluate keys and values from left to right. A later duplicate
  key replaces the value without moving the key's deterministic local position.
- Sized arrays are Lists initialized with the requested number of typed-null
  slots. They remain elastic through List methods; indexed assignment cannot
  grow them.
- A missing Map `get`, `put`, or `remove` result is a null carrying the Map value
  type.
- Self-copying operations such as `values.addAll(values)` and
  `mapping.putAll(mapping)` snapshot their source before mutating.

Lists preserve element order. Sets and Maps use deterministic insertion order
locally so repeated runs remain stable. That order is intentionally documented
as simplified and is not a claim about Salesforce hash iteration. Map
`keySet()` returns a deterministic snapshot, not a backed view.

The supported collection methods are:

- List: `add`, `addAll`, `clear`, `clone`, `contains`, `get`, `indexOf`,
  `isEmpty`, `remove`, `set`, `size`, and scalar `sort`. `add` accepts either a
  value or an index and value. Sorting accepts String, Integer, and Long
  elements and orders null before non-null values.
- Set: `add`, `addAll`, `clear`, `clone`, `contains`, `containsAll`, `isEmpty`,
  `remove`, `removeAll`, `retainAll`, and `size`. Mutating Set methods report
  whether membership changed, except `clear`, which returns Void.
- Map: `clear`, `clone`, `containsKey`, `get`, `isEmpty`, `keySet`, `put`,
  `putAll`, `remove`, `size`, and `values`. `put` and `remove` return the prior
  typed value or typed null.

List indexes are zero-based. Indexed reads, writes, and mutation require an
in-range concrete Integer. Indexed `add` also accepts the position immediately
after the final element. Set and Map indexing is rejected during checking.

## Core String, Math, and System calls

**Implemented for the fixed M3 subset plus M6 assertions.** Method names are
case-insensitive.

- Static String: `valueOf`, `join`, `isBlank`, `isNotBlank`, `isEmpty`, and
  `isNotEmpty`.
- Instance String: `length`, `contains`, `startsWith`, `endsWith`, `equals`,
  `equalsIgnoreCase`, `indexOf`, one- and two-argument `substring`, `trim`,
  `toLowerCase`, `toUpperCase`, and literal `replace`.
- Integer-backed Math: `abs`, `max`, `min`, and `mod`.
- System: `debug`, `assert`, `assertEquals`, and `assertNotEquals`. Failed
  assertions raise a catchable `AssertException`.

String `length`, `indexOf`, and `substring` use UTF-16 code-unit positions for
ordinary Unicode scalar strings. Rust strings cannot contain an unpaired
surrogate, so a substring boundary that would split a surrogate pair produces
an explicit runtime diagnostic. Unicode case conversion and whitespace
classification otherwise follow the Rust standard library and remain
simplified compatibility behavior.

The String `==` and `!=` operators compare case-insensitively, while
`String.equals` and the equality used by collection membership remain
case-sensitive. `String.equalsIgnoreCase` provides explicit case-insensitive
method comparison. Both equality methods return `false` for a null argument.

## Scope and control flow

**Implemented.** Blocks introduce lexical scopes. Loop-local variables do not
escape their declaring scope. `if`/`else`, traditional `for`, `while`, and
`do`/`while` execute directly over checked Boolean conditions. Traditional
`for` accepts comma-separated initializer and update expressions, executes
them left to right, and does not count its structural sequence container as an
extra coverage line or debugger stop. Enhanced `for` evaluates a List or Set
expression once, snapshots its elements for traversal,
and gives the iteration variable its own non-escaping scope. Direct Map
iteration is rejected; callers iterate `keySet()` or `values()`.

Structural mutation of a List or Set during enhanced iteration is rejected
through every alias, including diagnostic unwinding. `break` and `continue`
target the nearest enclosing loop. A value-less `return` terminates anonymous
execution. Declared methods support typed values and `void` completion.

Typed SObject `switch on`/`when` patterns resolve schema identities during
checking. A matching arm introduces its binding only within that arm, the
switch expression executes once, the first matching arm wins, and `when null`
and `when else` retain their Apex ordering behavior. Duplicate typed patterns
are rejected. Other unimplemented pattern families remain explicit semantic
errors rather than runtime approximations.

## Exceptions

**Implemented for M4, simplified.** `try` may have typed `catch` clauses, a
`finally` block, or both. `throw` accepts a core exception value or `null`;
throwing null raises `NullPointerException`. A concrete catch matches its own
type, while `Exception` catches every implemented concrete exception. Catch
variables preserve the original type, message, and accumulated frames so they
can be inspected or rethrown.

`finally` executes after normal completion, caught or uncaught exceptions,
method returns, and loop `break`/`continue`. If `finally` itself returns, throws,
breaks, or continues, that abrupt completion replaces the pending outcome.

Null dereferences raise `NullPointerException`; invalid List positions and
sizes raise `ListException`; division, remainder, and integer overflow raise
`MathException`; invalid permitted downcasts raise `TypeException`; and String
range/representation failures raise `StringException`. Structural mutation of
an actively iterated List or Set raises `FinalException`. Runtime exceptions
carry their origin span, and the active call chain is snapshotted when the
exception first reaches a handler or escapes a method. The leaf frame uses the
origin span; each caller frame uses the nested call site, producing an
innermost-to-outermost source stack. Single-source rendering maps against its
supplied source. Project rendering resolves the origin and every frame
independently by `SourceId`, then maps each file-local byte span to line and
column.

Core exception values support zero- or one-String-argument construction plus
`getMessage()`, `getTypeName()`, and deterministic `getStackTraceString()`.
Exact Salesforce wording, stack formatting, causes, and custom exception
classes are not yet claimed.

## Platform effects

**Implemented for the curated profile, simplified.** Debug, query, DML,
trigger, async lifecycle, deterministic clock/random/user context, limits, and
mock HTTP observations cross the replaceable `PlatformHost`. Normalized schema
is wired into checked HIR and in-memory SObject execution, while query/DML
requests cross storage-neutral platform contracts into SQLite. Async work is
queued explicitly and drains deterministically at `Test.stopTest`; no
background scheduler or live callout transport exists.

Batch receivers preserve instance state across `start`, every `execute` chunk,
and `finish` only when the checked class implements `Database.Stateful`.
Non-stateful stages each restore the enqueue-time receiver snapshot. Static
state is deterministic locally, but Salesforce-exact transaction isolation is
not claimed.

M23 dynamic SOQL calls evaluate their String argument once, parse it through
the dedicated SOQL grammar, and semantically recheck it against the immutable
program schema plus visible simple-name bind types. Successful calls use the
same checked plan, platform request, SQLite executor, and structured query
trace as static SOQL. Dynamic parse, check, result-shape, and database failures
surface as catchable `QueryException` values at the call site.

Date literals are evaluated in UTC against the host clock. Inserted records
receive `CreatedDate`; inserts and mutations receive `LastModifiedDate`.
Child-query correlation is batched by reference Id after the parent window is
selected. Parent traversal is bounded to five levels, nested child subqueries
are rejected, and `Database.QueryLocator` is an opaque checked record snapshot
consumed by the deterministic batch pipeline.

Every source executes under its exact typed `salesforce-api-X.0` profile.
M25 models API 31.0 and the current API 60.0–66.0 family, including the
reviewed null-`instanceof` difference. Null-aware syntax and the curated M10
platform surface require a current profile. Unmodeled versions fail during
project discovery, and profile selection remains outside expression syntax.
M27 adds the separate sharing/security context at the compiler/host boundary.
The context preserves four declaration states long enough to distinguish
explicit inherited sharing from an omitted declaration at an entry point.
Query and DML requests also carry an operation access mode and current user;
the host applies CRUD/FLS and visible-record decisions before database
windowing or mutation preparation. Detailed behavior and limits are specified
in `sharing-security.md`.

## Transactions

**Implemented through M9, simplified.** `platform::storage` defines
storage-neutral begin/read/write/delete/commit/rollback plus named-savepoint
contracts. `SqliteStorage` migrates normalized object tables, performs CRUD,
replaces fixtures transactionally, and resets records without rebuilding
schema.

Every DML tree owns a nested checkpoint that includes active records, recycled
records, and deterministic ID sequences. A caught DML/trigger failure rolls
back that tree, while an exception escaping an invocation or test rolls back
the outer entry point.

M24 adds structured partial saves. False `allOrNone` returns exactly one
ordered result per input, copies generated Ids only to successful caller
records, and retries the subset that survived a failed attempt. Attempts are
bounded at three. Triggers refire on each subset; trigger/static observations
remain visible, while retry-scoped query and callout counters reset to their
pre-first-attempt values. A third-attempt row error raises a catchable
`DmlException`. Statements and omitted/true `allOrNone` remain atomic.
