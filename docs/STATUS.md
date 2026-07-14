# Current Status

**Last updated:** 2026-07-13

## Active milestone

M3 — Collections and core standard library

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
- `System.debug(expression)` to plain stdout
- `tokens`, `ast`, `check`, and `run` CLI commands
- Source-span compile and runtime diagnostics
- Sixteen focused compiler/runtime unit tests
- Thirty integration and full-scenario tests, including disk-backed Apex run
  through every compiler stage and the CLI

## Immediate target

Execute the M3 acceptance program unchanged:

```apex
List<String> strs = new List<String>();

for (Integer i = 0; i < 100; i++) {
    String s = String.valueOf(i);
    strs.add(s);
}

System.debug(String.join(strs, ''));
```

Recommended implementation order:

1. Add generic type and array syntax to the lexer, parser, and AST.
2. Add typed `List`, `Set`, and `Map` construction and literals.
3. Add indexing and method-call expressions.
4. Implement collection mutation, access, size, and iteration.
5. Add the essential `String`, `Math`, and `System` methods needed by common code.
6. Add CLI acceptance examples and collection conformance tests.

## Known limitations

- `Integer` uses simplified internal `i64` semantics rather than complete Apex
  range and overflow behavior.
- Anonymous `return` is value-less; method return values arrive in M4.
- Enhanced `for` loops depend on iterable collection types and arrive in M3.
- Collections, generic and array types, method calls, and the broader standard
  library are not implemented.
- User-defined methods, classes, exceptions, SOQL, SOSL, DML, and SObjects are
  not implemented.

## Handoff checklist

After meaningful implementation work:

- Update the completed and limitation lists above.
- Update the active milestone if its exit criterion passes.
- Update `docs/COMPATIBILITY.md` for changed behavior.
- Add or update conformance tests.
- Add an ADR if an architectural boundary or expensive choice changed.
- Run the verification commands in `AGENTS.md`.
