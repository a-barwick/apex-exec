# Current Status

**Last updated:** 2026-07-16

## Active milestone

M7 — SObject schema and SQLite

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

## Immediate target

Implement M7 SObject schema and SQLite on the isolated project/test runtime
established in M6.

The normalized schema, storage transaction, and platform host seams are now in
place. The next slice should populate them from real SFDX metadata and provide
the first SQLite adapter rather than adding storage behavior to the compiler or
interpreter directly.

Recommended implementation order:

1. Import custom object and field metadata from ordinary SFDX projects.
2. Normalize metadata independently from physical storage and generate a local
   SQLite schema.
3. Add typed and dynamic SObject values, Salesforce-shaped IDs, and field
   access.
4. Add isolated transactions, savepoints, rollback, and fast test reset.
5. Add executable schema/data fixtures without coupling semantic analysis to
   SQLite.

## North Star indicators

At M6 completion, the pinned real-world lexer/parser goals pass 1 of 14
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
- `Object` is currently a typed assignment and cast carrier, not the full Apex
  `Object` API planned for the curated platform milestone.
- The core exception subset supports construction, catching, rethrowing,
  messages, type names, and deterministic stack text. Custom exception classes,
  causes, and Salesforce-exact stack formatting require later compatibility and
  differential work.
- SFDX discovery reads package-directory paths but does not yet interpret the
  full Salesforce DX configuration or metadata surface. Each `.cls` file must
  contain exactly one matching top-level class or interface.
- Parsed source units and unchanged complete builds are cached with stable
  source identities. A changed unit computes dependency invalidation and reuses
  unchanged parsed ASTs, while the cross-file semantic link currently reruns
  project-wide.
- The normalized schema catalog and transaction traits are architectural M7
  foundations only. Custom metadata import, SQLite persistence, SObject runtime
  values, Salesforce-shaped ID validation/generation, and DML remain pending.
- Nested types, enums, annotations other than `@IsTest`/`@TestSetup`, explicit
  superclass-constructor calls, custom exception classes, and Salesforce-exact
  object string formatting are not implemented.
- Sharing modifiers are parsed for structural progress but rejected during
  checking because sharing/security semantics remain deferred.
- Test setup methods run before every isolated test interpreter. This provides
  deterministic state today; Salesforce's one-time setup transaction followed
  by database snapshots requires the M7 data host.
- Test discovery supports annotation-based static void methods only. Legacy
  `testMethod`, the newer `Assert` class, `Test.startTest`/`stopTest`, and
  org-backed `SeeAllData=true` remain unsupported.
- Coverage counts executable production statement lines and both outcomes of
  `if`, `while`, `do`/`while`, and condition-bearing `for` branches. It is not a
  claim of Salesforce-exact coverage accounting.
- SOQL, SOSL, DML, and SObjects are not implemented.

## Handoff checklist

After meaningful implementation work:

- Update the completed and limitation lists above.
- Update the active milestone if its exit criterion passes.
- Update `docs/COMPATIBILITY.md` for changed behavior.
- Add or update conformance tests.
- Add an ADR if an architectural boundary or expensive choice changed.
- Run the verification commands in `AGENTS.md`.
