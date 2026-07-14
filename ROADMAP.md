# Roadmap

This roadmap works backward from the product vision in `docs/VISION.md`.
Milestones describe coherent, demonstrable capabilities rather than release
dates. A milestone is complete only when its exit criteria and verification
requirements pass.

Status values: **Complete**, **Active**, **Planned**, and **Deferred**.

## M1 — Primitive anonymous execution

**Status:** Complete

### Scope

- Lexer, parser, AST, semantic analysis, and tree-walking execution
- Explicitly initialized `String`, `Boolean`, and `Integer` variables
- Assignment and variable references
- Case-insensitive name lookup
- `System.debug(variable)` with plain stdout output
- `tokens`, `ast`, `check`, and `run` commands
- Source-span diagnostics

### Exit criterion

```apex
String message = 'Hello, world!';
System.debug(message);
```

prints `Hello, world!` and all compiler stages can inspect the program.

## M2 — Expressions and control flow

**Status:** Complete

### Scope

- Arithmetic, comparison, equality, and Boolean operators
- Apex operator precedence and associativity
- String concatenation
- Prefix and postfix increment/decrement
- `null`
- Blocks and nested lexical scopes
- `if`/`else`, `for`, `while`, and `do` statements
- `break`, `continue`, and `return`
- Apex-compatible assignment checks for the supported primitive types

### Non-scope

- Collections and generic types
- User-defined methods and classes
- SOQL, SOSL, and DML

### Exit criterion

```apex
Integer total = 0;
for (Integer i = 0; i < 10; i++) {
    total = total + i;
}
System.debug(total);
```

prints `45`, with tests covering precedence, scopes, loop control, and invalid
operand types.

## M3 — Collections and core standard library

**Status:** Active

### Scope

- `List<T>`, `Set<T>`, `Map<K,V>`, and array syntax
- Generic type checking, construction, access, mutation, and iteration
- Common collection methods
- Essential `String`, `Math`, and `System` methods
- Method-call expressions

### Exit criterion

The original project acceptance program runs unchanged:

```apex
List<String> strs = new List<String>();

for (Integer i = 0; i < 100; i++) {
    String s = String.valueOf(i);
    strs.add(s);
}

System.debug(String.join(strs, ''));
```

## M4 — Methods, exceptions, and runtime correctness

**Status:** Planned

### Scope

- Method declarations, parameters, calls, and return values
- Overload resolution
- Recursion
- `try`, `catch`, `finally`, and `throw`
- Core Apex exception types
- Null dereference, invalid cast, bounds, and arithmetic failures
- Source-mapped runtime stack traces

### Exit criterion

Multi-method programs compile with static type checks and report runtime
failures with useful Apex source stacks.

## M5 — Classes and project compilation

**Status:** Planned

### Scope

- SFDX project discovery and `.cls` loading
- Classes, constructors, fields, properties, and methods
- Static and instance members
- Access modifiers
- Interfaces, inheritance, abstract/virtual methods, and overrides
- Cross-file resolution and dependency graphs
- Incremental project compilation

### Exit criterion

An ordinary multi-file Apex service layer compiles and can be invoked locally.

Before this milestone grows substantially, introduce a typed intermediate
representation as described in `docs/ARCHITECTURE.md`.

## M6 — Apex test runner

**Status:** Planned

### Scope

- `@isTest`, test discovery, test methods, and setup methods
- Assertions and expected failures
- Per-test isolation
- Filtering and deterministic execution
- JUnit output and line/branch coverage
- Parallel execution where semantic isolation permits it

### Exit criterion

`apex-exec test force-app` runs a useful subset of a real project's unit tests
without an org.

This is the first major enterprise-value checkpoint.

## M7 — SObject schema and SQLite

**Status:** Planned

### Scope

- Import SFDX custom object and field metadata
- Generate and migrate a local SQLite schema
- Typed and dynamic SObjects
- Salesforce-style IDs and field access
- Relationships, transactions, savepoints, rollback, and fixtures
- Fast database reset for test isolation

### Exit criterion

Apex can create, retrieve, update, and delete custom SObjects in an isolated
local transaction.

## M8 — SOQL, SOSL, and DML

**Status:** Planned

### Scope

- Dedicated SOQL and SOSL grammars
- Static query validation and bind expressions
- Common filtering, ordering, limiting, aggregation, and relationship queries
- DML statements and common `Database` methods
- Structured query and DML traces

### Exit criterion

Common repository and service-layer Apex runs against SQLite without source
changes.

## M9 — Triggers and transaction semantics

**Status:** Planned

### Scope

- Trigger syntax and context variables
- Before/after insert, update, delete, and undelete
- Bulk behavior and recursive trigger execution
- Transaction rollback and deterministic execution timelines

### Exit criterion

Common trigger-handler architectures run locally with realistic bulk and
rollback behavior.

This is the second major enterprise-value checkpoint.

## M10 — Curated platform compatibility

**Status:** Planned

Implement the common Apex platform surface based on real project usage rather
than attempting every Salesforce API. Initial candidates include:

- `Date`, `Datetime`, `Time`, `Decimal`, `Id`, `Blob`, and `Object`
- JSON, regex, schema describe, and common `Test` and `Limits` methods
- Deterministic time, IDs, randomness, and user context
- Mockable HTTP callouts

Unsupported APIs must produce structured errors naming the missing API and the
active compatibility profile.

## M11 — Deterministic asynchronous execution

**Status:** Deferred

Add Queueable, future, batch, scheduled, and event-driven execution only after
the synchronous platform kernel is stable. Tests must explicitly drain async
work; background scheduling must not introduce nondeterminism.

## M12 — Debugger, REPL, and editor integration

**Status:** Planned

- Persistent REPL state
- Debug Adapter Protocol
- Language Server Protocol
- Breakpoints, stepping, frames, variables, and database inspection
- Go-to-definition, references, rename, and inline diagnostics
- Coverage overlays and transaction timelines

### Exit criterion

Developers can use Apex Exec as their normal inner loop from a supported editor.

## M13 — Salesforce compatibility oracle

**Status:** Planned

Run identical conformance fixtures locally and in a scratch org, then compare:

- Compile success and diagnostics category
- Values, output, exceptions, and stack behavior
- SOQL results and DML effects
- Trigger order and test outcomes

Every discovered difference becomes a permanent fixture. Compatibility must be
reported as measured coverage, not asserted as a blanket claim.

## M14 — Enterprise CI

**Status:** Planned

- Incremental compilation and impacted-test selection
- Content-addressed caches, sharding, and distributed workers
- Hermetic manifests and deterministic replay
- SARIF, JUnit, and coverage reports
- GitHub, GitLab, and Jenkins integration
- Performance and compatibility policy gates

### Exit criterion

A large Apex repository can validate a pull request without provisioning an org.

## M15 — Hybrid deployment confidence

**Status:** Planned

- Optional validation-org authentication
- Affected-component and affected-test selection
- Local-versus-org differential results
- Schema and configuration drift detection
- Release-readiness reports

### Exit criterion

Enterprises reserve Salesforce compute for targeted final validation while
performing routine compilation and testing locally.

## Product checkpoint

The decisive goal is not complete emulation of Salesforce. It is:

> A representative enterprise project can run 60–80% of its ordinary Apex unit
> tests locally, quickly, deterministically, and without source changes.

Achieving that threshold changes the cost and speed of Salesforce development;
later milestones increase the percentage and confidence.
