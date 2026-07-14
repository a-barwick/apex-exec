# Development

## Requirements

- Rust 1.88 or newer
- Cargo

The current crate has no third-party dependencies.

## Build and run

```bash
cargo build
cargo run -- run examples/hello.apex
```

Inspect individual compiler phases:

```bash
cargo run -- tokens examples/hello.apex
cargo run -- ast examples/hello.apex
cargo run -- check examples/hello.apex
cargo run -- run examples/hello.apex
```

## Required verification

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

Run the relevant CLI example after changing command behavior or execution.

## Change workflow

1. Read `docs/STATUS.md` and the active milestone in `ROADMAP.md`.
2. Identify the affected compiler phase and compatibility rows.
3. Add tests that demonstrate the desired behavior and important failures.
4. Implement the smallest complete language slice across all required phases.
5. Run the required verification commands.
6. Update `docs/STATUS.md` and `docs/COMPATIBILITY.md`.
7. Add an ADR when the change makes a consequential or expensive-to-reverse
   design decision.

## Testing strategy

Behavior should be exercised at the narrowest useful layer:

- Lexer tests for token boundaries, trivia, and invalid characters
- Parser tests for syntax shape, precedence, and recovery diagnostics
- Semantic tests for names, scopes, types, and conversions
- Runtime tests for values, control flow, calls, and exceptions
- CLI tests or examples for file handling, output, and rendered diagnostics
- Conformance fixtures for observable Apex behavior

As coverage grows, place feature-focused integration tests under a
`tests/conformance/` module tree while retaining top-level Cargo test entry
points.

## Documentation maintenance

Documentation is part of the implementation:

- Product intent belongs in `VISION.md`.
- Milestone scope belongs in `ROADMAP.md`.
- Immediate handoff information belongs in `STATUS.md`.
- Observable support belongs in `COMPATIBILITY.md` and tests.
- Architectural rationale belongs in an ADR.
- Recurring instructions belong in `AGENTS.md`.

Do not duplicate detailed requirements across multiple files. Link to the
authoritative document instead.
