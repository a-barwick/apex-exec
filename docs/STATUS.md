# Current Status

**Last updated:** 2026-07-13

## Active milestone

M5 — Classes and project compilation

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
- A pinned seven-file, 14,740-line open-source Apex North Star corpus with
  executable lexer/parser milestone indicators

## Immediate target

Implement M5 classes and project compilation while preserving the checked call
and runtime-exception boundaries established in M4.

Recommended implementation order:

1. Introduce the typed intermediate representation described in
   `docs/ARCHITECTURE.md` and move resolved call targets out of the parsed AST.
2. Add SFDX project discovery and `.cls` compilation-unit loading.
3. Implement classes, constructors, fields, properties, and static/instance
   member resolution.
4. Add interfaces, inheritance, abstract/virtual methods, and overrides.
5. Build cross-file dependency graphs and incremental project compilation.

## North Star indicators

At M4 completion, the pinned real-world lexer/parser goals pass 1 of 14
indicators (**7.14%**): lexer 1 of 7 (**14.29%**) and parser 0 of 7 (**0%**).
`JSONParse.cls` lexes and stops at its M5 class declaration. The other first
blockers are annotations, ternary syntax, and compound bitwise operators. These
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
- Top-level method declarations are an interim single-file M4 compilation
  model. Ordinary class-contained Apex methods and cross-file lookup arrive in
  M5.
- Overload resolution supports exact matches plus the documented `Exception`
  and minimal `Object` widening relationships. General Apex conversions and
  inheritance-aware overload ranking remain future work.
- `Object` is currently a typed assignment and cast carrier, not the full Apex
  `Object` API planned for the curated platform milestone.
- The core exception subset supports construction, catching, rethrowing,
  messages, type names, and deterministic stack text. Custom exception classes,
  causes, and Salesforce-exact stack formatting require later class/project and
  differential work.
- Only the documented built-in method subset and single-file user-defined
  methods are callable. Classes, SOQL, SOSL, DML, and SObjects are not
  implemented.

## Handoff checklist

After meaningful implementation work:

- Update the completed and limitation lists above.
- Update the active milestone if its exit criterion passes.
- Update `docs/COMPATIBILITY.md` for changed behavior.
- Add or update conformance tests.
- Add an ADR if an architectural boundary or expensive choice changed.
- Run the verification commands in `AGENTS.md`.
