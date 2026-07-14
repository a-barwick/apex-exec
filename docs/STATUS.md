# Current Status

**Last updated:** 2026-07-11

## Active milestone

M2 — Expressions and control flow

## Completed

- Rust binary and library crate
- Separate lexer, parser, AST, semantic-analysis, diagnostic, and runtime modules
- `String`, `Boolean`, and simplified `Integer` values
- Explicit initialization, assignment, and variable references
- Case-insensitive identifiers with original spelling retained in the AST
- Single-quoted strings, comments, and common string escapes
- `System.debug(variable)` to plain stdout
- `tokens`, `ast`, `check`, and `run` CLI commands
- Source-span diagnostics
- Eight compiler/runtime integration tests

## Immediate target

Execute this program and print `45`:

```apex
Integer total = 0;
for (Integer i = 0; i < 10; i++) {
    total = total + i;
}
System.debug(total);
```

Recommended implementation order:

1. Expand tokens for operators, braces, and control-flow keywords.
2. Add precedence-based expression parsing.
3. Add typed expression validation.
4. Add blocks and lexical scope stacks.
5. Add `if` and `while` execution.
6. Add `for`, increment/decrement, `break`, and `continue`.
7. Add CLI acceptance examples and conformance tests.

## Known limitations

- Only primitive literals or variables can initialize and assign values.
- `System.debug` accepts a variable, not a general expression.
- There are no binary or unary operators.
- There are no nested scopes or control-flow statements.
- `Integer` uses simplified internal `i64` semantics rather than complete Apex
  numeric behavior.
- Collections, methods, classes, exceptions, SOQL, SOSL, DML, and SObjects are
  not implemented.
- The project directory is not currently a Git repository.

## Handoff checklist

After meaningful implementation work:

- Update the completed and limitation lists above.
- Update the active milestone if its exit criterion passes.
- Update `docs/COMPATIBILITY.md` for changed behavior.
- Add or update conformance tests.
- Add an ADR if an architectural boundary or expensive choice changed.
- Run the verification commands in `AGENTS.md`.
