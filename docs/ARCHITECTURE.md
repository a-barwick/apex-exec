# Architecture

## Current pipeline

```text
Apex source
    │
    ▼
  Lexer ──► tokens with byte spans
    │
    ▼
  Parser ─► untyped AST
    │
    ▼
Semantic checker ─► validated AST
    │
    ▼
Tree-walking interpreter ─► values and debug output
```

The public library entry points in `src/lib.rs` deliberately expose each phase:

- `tokenize`
- `parse`
- `check`
- `execute`

The CLI is a thin adapter over those functions.

## Current modules

| Module | Responsibility |
|---|---|
| `span` | Source byte ranges |
| `token` | Token kinds and lexical spelling |
| `lexer` | Source-to-token conversion and lexical errors |
| `ast` | Parsed program representation |
| `parser` | Grammar and syntax diagnostics |
| `semantic` | Name lookup and primitive type validation |
| `runtime` | AST execution, environments, and values |
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
- Content-addressed CI artifacts

These optimizations must preserve deterministic results and source diagnostics.
