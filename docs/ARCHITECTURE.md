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

The CLI is a thin adapter over those functions.

M6 discovers tests from checked annotation metadata and executes each test in
its own interpreter. Setup methods share that test's interpreter and run before
the test method. Each test receives a fresh execution store, default recording
host, call stack, and coverage trace, so the bounded worker pool does not share
observable runtime state. Results are sorted by case-insensitive qualified test
name after execution so parallel scheduling is never observable in reports.

The interpreter records executed statement spans and true/false conditional
outcomes. The test runner maps those observations through the project source
map, excludes `@IsTest` classes from the production denominator, and owns
console/JUnit rendering. Test policy and report formats do not leak into parser,
semantic, or ordinary execution entry points.

Checked built-in calls carry a typed `IntrinsicId` in HIR, just like
user-defined calls carry a selected declaration target. Runtime dispatch
therefore never repeats case-insensitive built-in lookup. An interpreter
borrows immutable checked code through a `RuntimeImage`; its execution scopes
and traces remain isolated, while an `ExecutionStore` owns its collection
arena, object arena, and static slots. `System.debug` crosses a structured
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
| `semantic` | Compiler façade with declaration/body checking, shared overload ordering, and intrinsic validation |
| `runtime` | Execution façade, borrowed runtime image, mutable execution store, platform host, intrinsic execution, environments, and values |
| `project` | Compilation façade over discovery, source-unit caching, dependency graphs, and diagnostic source mapping |
| `platform` | Storage-independent normalized schema and transactional record-storage contracts |
| `platform::metadata` | SFDX custom-object and field metadata import |
| `platform::sobject` | Schema-validated typed/dynamic SObject values at the platform boundary |
| `platform::sqlite` | Schema migration and transactional SQLite record persistence |
| `platform::database` | Query execution, relationship hydration, aggregates, SOSL search, and atomic DML semantics |
| `runtime::database` | Bind evaluation and conversion between HIR plans, runtime values, and platform requests/results |
| `test_runner` | Test discovery, isolated scheduling, filtering, reporting, and coverage aggregation |
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

The parsed `Program` stores classes, backwards-compatible top-level method
declarations, and executable anonymous statements separately. Signature and
class collection are early semantic passes, so cross-file lookup, forward
calls, and recursion work without source-order dependence. Runtime invocations
replace the caller's lexical-scope stack with a new parameter scope and restore
it on every completion path. Collections, class/static state, and object arenas
remain in the interpreter's execution store. Debug events flow through the
configured platform host, whose state may be owned by the interpreter or
intentionally shared by reference.

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

The implemented host surface owns structured debug, query, and DML output.
`platform::schema` provides a case-insensitive normalized catalog and
`SchemaProvider`, while `platform::storage` defines storage-neutral records and
transaction traits. M7 adds metadata import, an additive SQLite adapter, and
schema-backed interpreter SObjects.

M8 extends that boundary with checked SOQL/SOSL requests and atomic DML
operations. The default recording host lazily owns one in-memory SQLite
database per interpreter and records structured query/DML events. In-memory
SObject field mutation still does not silently persist a record; only an
explicit DML statement or `Database` method crosses the persistence boundary.

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
isolation substrate that M8 can connect to Apex tests.

The transaction contract deals in storage-neutral records rather than AST or
runtime `Value` nodes. Interpreter SObjects likewise use checked schema indices
and in-memory values; DML lowering will perform the explicit conversion at the
platform boundary.

## Compatibility architecture

Compatibility has three layers:

1. **Declared surface:** `docs/COMPATIBILITY.md` states what is intended.
2. **Executable fixtures:** tests define observable local behavior.
3. **Differential oracle:** later milestones execute fixtures against Salesforce
   and record mismatches.

API-version, sharing, limits, and security behavior should eventually be
explicit runtime profiles. They must not appear as scattered conditionals.

## Performance direction

Correctness and phase boundaries take priority during the language milestones.
The current foundation already includes class dependency graphs, parsed-unit
reuse, isolated parallel test execution, and per-file coverage aggregation.
Further project-scale performance work will rely on:

- Interned names and types
- Dependency-scoped incremental semantic analysis
- Cached typed or lowered IR
- Content-addressed CI artifacts

These optimizations must preserve deterministic results and source diagnostics.
