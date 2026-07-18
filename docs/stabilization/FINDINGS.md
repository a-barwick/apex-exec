# Stabilization Findings and Evidence

This register preserves the pre-Phase-2 audit evidence. It distinguishes
confirmed failures from structural risks so future agents do not treat an
architectural concern as a reproduced compatibility defect, or vice versa.

## Quantitative baseline

The audited repository contained approximately 40,300 lines of Rust under
`src` and `tests`.

Largest production modules:

| File | Lines |
|---|---:|
| `src/runtime.rs` | 3,598 |
| `src/semantic.rs` | 3,506 |
| `src/hybrid.rs` | 2,578 |
| `src/semantic/intrinsics.rs` | 1,729 |
| `src/oracle.rs` | 1,543 |
| `src/ci.rs` | 1,384 |
| `src/runtime/intrinsics.rs` | 1,147 |
| `src/runtime/platform_intrinsics.rs` | 1,135 |
| `src/runtime/database.rs` | 1,029 |

The audit found no literal 1,000-line function. Using Lizard thresholds of
80 NLOC or cyclomatic complexity 15, it found 74 warnings that covered roughly
one-third of production NLOC. The highest-risk functions included:

| Function | NLOC | CC |
|---|---:|---:|
| `call_platform` | 589 | 124 |
| `platform_instance_method_type` | 378 | 40 |
| `static_platform_method_type` | 242 | 46 |
| `member_access_type` | 229 | 37 |
| `validate_class_member_declarations` | 185 | 37 |
| CLI `run_ci` | 203 | 67 |

The production parser was healthier: 2,054 lines across six focused files, a
largest file of 458 lines, and a largest function of 87 lines. Its primary
risks are duplicated type grammar, growing branch complexity, lossless-span
gaps, and lack of error recovery.

## Confirmed P0 failures

### F-P0-01 — Interface cycles abort the compiler

The semantic hierarchy validator checks the `superclass` chain but not cycles
through `interfaces`. `collect_interface_methods` recursively follows both
without a visited set.

Evidence:

- [`validate_class_hierarchy`](../../src/semantic.rs#L220)
- [`collect_interface_methods`](../../src/semantic.rs#L1035)

Reproduction:

```apex
public interface A implements B { void a(); }
public interface B implements A { void b(); }
public class C implements A {
    public void a() {}
    public void b() {}
}
```

Observed result:

```text
thread 'main' has overflowed its stack
fatal runtime error: stack overflow, aborting
```

Even if the declaration syntax is invalid Apex, it must be rejected with a
diagnostic rather than terminate the compiler.

### F-P0-02 — Cyclic runtime values abort execution

Collections can contain themselves through `Object`. Structural equality,
display, JSON serialization, and debugger snapshot rendering recursively walk
the graph without tracking visited identities.

Evidence:

- [`collections_equal` and `display_value`](../../src/runtime.rs#L3038)
- [`value_to_json`](../../src/runtime/platform_intrinsics.rs#L905)
- Async cloning already demonstrates graph memoization in
  [`asynchronous.rs`](../../src/runtime/asynchronous.rs#L444)

Reproduction:

```apex
List<Object> values = new List<Object>();
values.add(values);
Integer done = 1;
```

Observed result: process stack overflow during the next statement's automatic
debug snapshot.

### F-P0-03 — Debugger capture is always on

Every non-block statement calls `capture_debug_snapshot`, including ordinary
execution and test runs that discard snapshots. Snapshot capture walks scopes,
clones names and frames, and recursively renders visible values.

Evidence:

- [`execute_statement`](../../src/runtime.rs#L676)
- [`capture_debug_snapshot`](../../src/runtime.rs#L3262)

The resulting cost grows with executed statements, visible state, and rendered
graph size. It is both a correctness amplifier for F-P0-02 and a likely M22
memory/performance blocker.

### F-P0-04 — Generic interface arguments are silently erased

`parse_named_type` parses `<T>` only to extend the span, then returns a
`NamedType` without the argument.

Evidence:

- [`parse_named_type`](../../src/parser/types.rs#L91)
- [`NamedType`](../../src/ast.rs#L802)

Confirmed reproduction: a class declaring
`implements Database.Batchable<Integer>` with `start` and `execute` methods
using `String` passed `check` with `OK`.

This violates the project invariant that unsupported behavior must be rejected
explicitly rather than approximated silently.

### F-P0-05 — Valid parser forms are misclassified

Confirmed cases:

- `(foo) + bar` is classified as a cast to type `foo` by
  [`is_cast_start`](../../src/parser/types.rs#L186), producing
  `cannot cast Integer to foo`.
- `Foo[] values = new Foo[3];` is rejected with
  `expected '(' after class name` because
  [`parse_new_expression`](../../src/parser/expressions.rs#L324) classifies
  construction semantically before handling array syntax.
- Qualified `Schema.SObjectType` and `Schema.DescribeSObjectResult` spellings
  drift between parsing and `TypeName` recognition.
- Public `Parser::new(vec![])` can panic because
  [`token_at`](../../src/parser.rs#L77) assumes a nonempty EOF-terminated token
  stream.

### F-P0-06 — Execution context is hardcoded or globally eager

- [`Test.isRunningTest`](../../src/runtime/platform_intrinsics.rs#L433) always
  returns `true`; an ordinary CLI run confirmed the wrong result.
- [`initialize_static_fields`](../../src/runtime.rs#L616) initializes every
  static field in every class at every entry point. An unused class containing
  `static Integer broken = 1 / 0` prevents unrelated code from running.

M20 static blocks and M27 execution/security context will compound these
problems if an explicit execution context and lazy class state are not
introduced first.

## Structural P1 findings

### F-P1-01 — The current HIR is not a stable lowered representation

[`hir::Program`](../../src/hir.rs#L20) owns a cloned AST plus fact tables keyed
by `Span`. Calls, class members, schema objects, and fields frequently use raw
positional `usize` identities.

This impedes nested-type identity, durable incremental compilation, profile
binding, typed exceptions, and M29 persistence. Introduce typed `UnitId`,
`DefId`, `TypeId`, `ObjectId`, and `FieldId`, then lower executable operations
incrementally.

### F-P1-02 — Incremental compilation currently reuses parsing, not semantic work

[`ProjectCompiler::compile`](../../src/project.rs#L105) clones the previous
`Compilation` on unchanged builds, clones cached ASTs into a merged program on
changed builds, and still runs project-wide semantic checking. The checker
clones the program into HIR and clones classes again.

The dependency invalidation closure is currently reporting information, not
limiting semantic work. Per-unit immutable arenas and dependency-scoped
resolution should be introduced before the M22 project makes this cost part of
every feedback loop.

### F-P1-03 — Platform APIs have multiple sources of truth

At least three coupled surfaces must change for every platform API:

- Intrinsic ID and static/instance classification in
  [`hir/intrinsic.rs`](../../src/hir/intrinsic.rs#L42)
- Semantic name/signature/type selection in
  [`semantic/intrinsics.rs`](../../src/semantic/intrinsics.rs#L821)
- Runtime implementation in
  [`runtime/platform_intrinsics.rs`](../../src/runtime/platform_intrinsics.rs#L23)

`call_platform` is already 589 NLOC and CC 124. A declarative descriptor catalog
should own identity, owner, name, call style, signature, profile disposition,
effects, handler, and documentation coverage.

### F-P1-04 — Runtime mutation has no lvalue abstraction

Assignment and increment/decrement independently resolve locals, indexes,
instance fields, static fields, and SObject fields:

- [`evaluate_assignment`](../../src/runtime.rs#L2165)
- [`mutate_integer`](../../src/runtime.rs#L2765)

M19 compound assignment and `Long` would multiply this code. Introduce a
resolved `Place` with shared read/write/mutate operations and central numeric
coercion/arithmetic policy first.

### F-P1-05 — Runtime state has too many unrestricted concerns

`Interpreter` owns evaluator scopes, stores, class context, receiver state,
call stack, execution traces, debugger snapshots, host state, coverage, and
async scheduling. Extension `impl Interpreter` blocks in separate files provide
visual separation but not enforced boundaries.

Extract or narrow evaluator state, transaction coordination, class/runtime
image state, scheduler context, and platform services. File splitting alone is
not the goal.

### F-P1-06 — Database checkpoints copy the complete local database

[`LocalDatabase::snapshot`](../../src/platform/database.rs#L187) clones schema,
active records, recycle records, and sequences. Runtime unit boundaries call
this even though the storage layer exposes native savepoints.

This is likely a primary M22/M24 scaling bottleneck. Use an outer transaction
with nested savepoints or small journals.

### F-P1-07 — `PlatformHost` defaults can silently weaken correctness

[`PlatformHost`](../../src/runtime/host.rs#L160) combines database,
transactions, triggers, async, time, randomness, user, HTTP, debugging, and
limits. Correctness-critical transaction defaults succeed as no-ops, while the
default DML preparation cannot reconstruct complete old-record semantics.

Split capability interfaces or make unsupported behavior explicit. Add a
shared host/storage conformance suite.

### F-P1-08 — Current DML contracts cannot represent M24 cleanly

The host DML boundary returns `Vec<SObject>` and runtime transaction handling
assumes all-or-nothing success. M24 needs a structured request carrying
all-or-none mode, original row indexes, profile/access context, and a per-row
outcome with typed errors.

This design requires an ADR before M24 implementation.

### F-P1-09 — Diagnostics are machine-readable only at a broad level

[`Diagnostic`](../../src/diagnostic.rs#L11) stores a message, span, optional
string exception type, and stack. `ProjectErrorKind::Diagnostic` combines
lexical, syntax, semantic, and runtime phases. Oracle classification parses
message substrings in [`oracle.rs`](../../src/oracle.rs#L1202), while CI
recovers locations by reparsing rendered output.

Add stable diagnostic codes, phase, severity, labels/help, typed source
locations, unsupported-capability IDs, and typed runtime exceptions.

### F-P1-10 — Parser syntax and semantic classification are coupled

Type grammar is implemented independently by `parse_type_name`,
`parse_named_type`, and `type_end_at`. `parse_new_expression` chooses object,
exception, or collection AST variants before semantic resolution. Annotations
use a closed semantic enum and reject unknown syntax immediately.

M19–M21 need one lossless syntax `TypeRef`, syntax-directed construction,
preserved annotation syntax, exact spans, and transactional lookahead built
from the same grammar.

### F-P1-11 — Recursive user-controlled input has no uniform budget

Parser expression/type/block recursion, semantic hierarchy traversal, and
runtime graph traversal do not share depth or node limits. Every recursive
user-controlled traversal must either prove acyclicity or enforce cycle/depth
budgets.

### F-P1-12 — LSP and protocol handling have correctness gaps

- [`line_column` and `offset_at`](../../src/editor.rs#L416) count Unicode scalar
  values, while LSP character positions are UTF-16 code units.
- [`path_uri`](../../src/lsp.rs#L290) escapes spaces only.
- Saved-project refresh at [`lsp.rs`](../../src/lsp.rs#L226) discards compile
  errors with `.ok()`.
- [`read_message`](../../src/protocol.rs#L4) trusts arbitrary
  `Content-Length`.

These should be fixed after the structured diagnostic model to avoid another
temporary adapter.

## Scale and compatibility risks to benchmark

- Deterministic Set and Map storage is vector-backed; lookup and structural
  comparison can be linear or quadratic.
- SOQL scans and hydrates broadly before filtering, ordering, offset, and
  limit; relationship hydration can issue reads per row/reference.
- Async jobs reuse interpreter/store state, including static and heap identity,
  across transaction-like units.
- SQLite additive migration can add a required field without a deterministic
  backfill or rejection for populated tables.
- Metadata XML parsing uses literal tag search and manual entity replacement
  rather than a namespace-aware XML parser.
- `Value` has a wide match surface; adding `Long` without central numeric
  operations will create cross-runtime churn.
- Compatibility profile names such as `m10-common` are embedded as strings in
  semantic and runtime code rather than carried as a typed context.

## Open-source release findings

- No repository license is present.
- No GitHub Actions workflows, contribution guide, security policy, code of
  conduct, or release/changelog process are present.
- `Cargo.toml` lacks `rust-version`, license, repository, documentation,
  keywords, and category metadata.
- `lib.rs` exposes nearly every internal module publicly; the supported semver
  surface has not been selected.
- README and website milestone copy are stale relative to `docs/STATUS.md`.
- The VS Code extension has no lockfile or dedicated tests.
- Website dependency audit reports two moderate transitive PostCSS advisories.

License selection and the initial supported public API are owner decisions.

## Strengths to preserve

- The intended lexer/parser/semantic/HIR/runtime/storage boundaries are sound.
- Apex case-insensitive lookup consistently preserves source spelling and
  spans.
- SOQL and SOSL use checked syntax/plans rather than interpreter string
  shortcuts.
- Unsupported behavior is usually explicit.
- Runtime image/store, storage-neutral record values, deterministic ordering,
  trigger cleanup, and memoized async cloning are useful seams.
- The milestone integration suite is substantial and currently green.

The stabilization program is not a rewrite. It closes process-safety defects
and strengthens the existing seams before additional milestones put more
weight on them.
