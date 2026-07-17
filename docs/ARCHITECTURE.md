# Architecture

## Current pipeline

```text
Apex source
    │
    ▼
  Lexer ──► tokens with byte spans
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

The CLI is a thin adapter over those functions.

M6 discovers tests from checked annotation metadata and executes each test in
its own interpreter. Setup methods share that test's interpreter and run before
the test method. Independent interpreters make static fields, object and
collection arenas, output, call stacks, and coverage state safe to execute in a
bounded worker pool. Results are sorted by case-insensitive qualified test name
after execution so parallel scheduling is never observable in reports.

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
| `semantic` | Compiler façade with declaration/body checking, shared overload ordering, and intrinsic validation |
| `runtime` | Execution façade, borrowed runtime image, mutable execution store, platform host, intrinsic execution, environments, and values |
| `project` | Compilation façade over discovery, source-unit caching, dependency graphs, and diagnostic source mapping |
| `platform` | Storage-independent normalized schema and transactional record-storage contracts |
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
it on every completion path. Collections, class/static state, object arenas,
and output remain interpreter-owned shared runtime state.

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

## Target compiler pipeline

Direct AST walking is appropriate for the current language slice. Before class
inheritance, overload resolution, and project-scale compilation become large,
the pipeline should evolve to:

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

The first typed representation is now implemented as immutable syntax plus HIR
side tables. A lowered executable IR remains a future evolution; it can replace
this layout without moving semantic state back into parsed nodes.

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

The implemented host surface currently owns structured debug output.
`platform::schema` provides a case-insensitive normalized catalog and
`SchemaProvider`, while `platform::storage` defines storage-neutral records and
transaction traits. They are deliberately not wired into Apex expressions yet;
metadata import, SQLite adaptation, and SObject runtime values are the next M7
layers.

## Local data architecture

SQLite will eventually provide persistent local org state. The logical model
should still support fast isolated transactions for tests:

```text
SFDX metadata
    → normalized object schema
    → SQLite migrations
    → transaction/savepoint per test
    → SOQL/DML adapter
    → trigger dispatcher
```

Schema normalization must remain separate from SQLite DDL so alternate storage
or in-memory implementations remain possible.

That separation now exists in code: normalized schema types have no SQLite
dependency, and the transaction contract deals in storage-neutral records
rather than AST or runtime `Value` nodes.

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
Project-scale performance will later rely on:

- Interned names and types
- Module/class dependency graphs
- Incremental parsing and semantic analysis
- Cached typed IR
- Isolated parallel test execution
- Per-file statement-line and conditional-outcome coverage aggregation
- Content-addressed CI artifacts

These optimizations must preserve deterministic results and source diagnostics.
