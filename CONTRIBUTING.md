# Contributing to Apex Exec

Thank you for helping make local Apex development faster and more reliable.
Apex Exec is in an active pre-release stabilization program, so correctness,
explicit unsupported behavior, and reproducible evidence take priority over
feature count.

## Current contribution status

The repository owner has not yet selected a project license or the supported
Rust public API policy. Those are explicit release blockers in
[`docs/STABILIZATION.md`](docs/STABILIZATION.md), not implicit permissions.
Until both decisions are recorded, external contributors may discuss changes
and share reproductions, but maintainers must not merge externally authored
code that would require unselected contribution or redistribution terms.

This guide documents the technical workflow now so it is ready when the owner
opens normal external contribution intake. It does not grant a license to the
project or impose contributor licensing terms.

## Start with project context

Read:

1. [`AGENTS.md`](AGENTS.md)
2. [`docs/VISION.md`](docs/VISION.md)
3. [`ROADMAP.md`](ROADMAP.md)
4. [`docs/STATUS.md`](docs/STATUS.md)
5. [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)
6. [`docs/COMPATIBILITY.md`](docs/COMPATIBILITY.md)

During stabilization, also read
[`docs/STABILIZATION.md`](docs/STABILIZATION.md) and the documents under
[`docs/stabilization/`](docs/stabilization/). Do not claim a work package or
expand its scope without following those operating rules.

## Development setup

The Rust toolchain requires Rust 1.88 or newer. The website requires the Node
version declared in `website/package.json`.

```bash
cargo build
cargo run -- run examples/hello.apex
```

For website work:

```bash
cd website
npm ci
npm run build
npm test
npm run lint
```

For the VS Code thin client:

```bash
cd editors/vscode
npm ci
npm test
```

## Change expectations

- Work from a focused branch; automation-created branches use the `codex/`
  prefix.
- Keep lexer, parser, semantic, HIR, runtime, and platform responsibilities in
  their documented layers.
- Preserve original spelling and source spans while keeping Apex lookup
  case-insensitive.
- Reject unsupported behavior explicitly.
- Add an executable regression for each observable behavior or defect.
- Record consequential architecture choices in `docs/decisions/`.
- Do not mix opportunistic cleanup into a scoped work package.
- Never commit credentials, org authentication material, generated site output,
  caches, `node_modules`, or local CI artifacts.

## Required verification

Every Rust change runs:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

Repository-level checks also include:

```bash
python3 -m unittest discover -s tools/tests -p 'test_*.py' -v
python3 tools/docs/check_docs.py
tools/maintainability/check_lizard.sh
python3 tools/dependencies/check_npm_audit.py
```

Dependency checks and their exception rules are documented in
[`docs/DEPENDENCY_POLICY.md`](docs/DEPENDENCY_POLICY.md). A failed or skipped
check must be reported as such; do not describe it as passing.

## Pull requests

Keep commits coherent and buildable. In the pull request:

- explain the user-visible outcome and architectural boundary;
- link tests or exact reproductions;
- list every verification command and result;
- identify new public APIs and complexity changes;
- call out known risks, owner decisions, and deliberately excluded scope.

The stable required check is `Required CI gate`. Review and CI do not
replace the additional integration procedure for active stabilization work.

## Reporting problems

Use a normal issue for non-sensitive defects and feature discussion. Follow
[`SECURITY.md`](SECURITY.md) for vulnerabilities and
[`CODE_OF_CONDUCT.md`](CODE_OF_CONDUCT.md) for community conduct concerns.
