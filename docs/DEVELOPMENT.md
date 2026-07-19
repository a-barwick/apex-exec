# Development

## Requirements

- Rust 1.88 or newer
- Cargo
- Node.js and npm only when developing the optional VS Code client

## Build and run

```bash
cargo build
cargo run -- run examples/hello.apex
cargo run -- run examples/control-flow.apex
cargo run -- check examples/milestone5-project
cargo run -- invoke examples/milestone5-project Entry.run
cargo run -- test examples/milestone6-project --jobs 2
cargo run -- ci run examples/milestone14-project/apex-exec-ci.json --shard 0/2
cargo run -- hybrid examples/milestone15-project/apex-exec-ci.json \
  --validation-snapshot /path/to/reviewed-milestone17-validation.json \
  --expected-target-org staging \
  --expected-org-id 00D000000000001 \
  --replay
cargo run -- repl
cargo run -- lsp .
cargo run -- dap
```

The reviewed M17 clean and controlled-drift artifacts are in
`evidence/milestone17/`. Produce a refresh from an authorized staging alias with
`--target-org`, `--record-validation`, and `--report`; do not commit credentials
or auth URLs. Authenticated capture requires a cacheable M14 result and performs
two scoped retrievals. Replay requires `--replay`, the recorded alias and org
ID, the same maximum-age policy, and the recorded Salesforce CLI version. See
`docs/specifications/hybrid-validation-evidence.md` for the exact contract.

The VS Code thin client is under `editors/vscode`; see its README for local
extension-host instructions.

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
python3 -m unittest discover -s tools/tests -p 'test_*.py' -v
python3 tools/docs/check_docs.py
tools/maintainability/check_lizard.sh
```

Run the relevant CLI example after changing command behavior or execution.
Website changes also require `npm ci`, `npm run build`, `npm test`, and
`npm run lint` from `website/`. VS Code thin-client changes require `npm ci`
and `npm test` from `editors/vscode/`. Dependency verification and the exact
advisory exception policy are documented in
[`DEPENDENCY_POLICY.md`](DEPENDENCY_POLICY.md).

## Git workflow

- Create an appropriately scoped branch before changing files. Agent-created
  branches use the `codex/` prefix, such as `codex/m3-collections`.
- Do not implement directly on `main`.
- Commit at coherent checkpoints after the relevant tests pass. Each checkpoint
  should build independently and describe one reviewable unit of work.
- Before completing a roadmap milestone, satisfy its exit criterion, run the
  full required verification suite, update the project documentation, and
  merge the milestone branch into `main`.
- Keep unrelated or user-owned working-tree changes out of commits.

## Change workflow

1. Create an appropriately named branch from the active integration base.
2. Read `docs/STATUS.md`, the active milestone in `ROADMAP.md`, and any active
   stabilization control documents.
3. Identify the affected compiler phase and compatibility rows.
4. Add tests that demonstrate the desired behavior and important failures.
5. Implement the smallest complete language slice across all required phases.
6. Commit coherent checkpoints as they become independently verified.
7. Run the required verification commands.
8. Update `docs/STATUS.md` and `docs/COMPATIBILITY.md`.
9. Add an ADR when the change makes a consequential or expensive-to-reverse
   design decision.
10. Merge through the active integration process after its exit criterion
    passes.

## Testing strategy

Behavior should be exercised at the narrowest useful layer:

- Lexer tests for token boundaries, trivia, and invalid characters
- Parser tests for syntax shape, precedence, and recovery diagnostics
- Semantic tests for names, scopes, types, and conversions
- Runtime tests for values, control flow, calls, and exceptions
- CLI tests or examples for file handling, output, and rendered diagnostics
- Project tests for SFDX discovery, cross-file resolution, dependency edges,
  cache reuse, and invalidation
- CI tests for hermetic input drift, impacted selection, shards, cache/replay,
  standard reports, and policy boundaries
- Conformance fixtures for observable Apex behavior

As coverage grows, place feature-focused integration tests under a
`tests/conformance/` module tree while retaining top-level Cargo test entry
points.

Full-program scenarios live as ordinary `.apex` files under `tests/scenarios/`.
Each scenario should combine multiple supported language features, assert its
observable output through the public compiler pipeline, and execute the same
file through the built `apex-exec` CLI. Keep narrow grammar, type, scope, and
runtime edge cases as unit tests in the owning module so failures remain easy to
localize.

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
