# Apex Exec Project Guidance

## Project intent

Apex Exec is a local Apex compiler and runtime focused on high compatibility
with common Salesforce Apex. It should give developers fast, deterministic
feedback without requiring an org for the normal edit-compile-test loop.

Before changing behavior, read:

1. `docs/VISION.md`
2. `ROADMAP.md`
3. `docs/STATUS.md`
4. `docs/ARCHITECTURE.md`
5. `docs/COMPATIBILITY.md`

## Working rules

- Create an appropriately named `codex/` branch before making changes; never
  implement directly on `main`.
- Commit coherent, verified checkpoints while work is in progress. Keep each
  checkpoint buildable and give it a descriptive message.
- When a roadmap milestone is complete, run all required verification, update
  its documentation, and merge the milestone branch into `main`.
- Work within the active roadmap milestone unless the user explicitly changes
  scope.
- Keep lexing, parsing, semantic analysis, and execution separate.
- Apex names are case-insensitive. Preserve source spelling and spans for
  diagnostics; canonicalize only for lookup.
- Reject invalid syntax and unsupported behavior explicitly. Never silently
  approximate Salesforce behavior.
- Add an executable test for every observable behavior and every bug fix.
- Record expensive or consequential design choices in `docs/decisions/`.
- Update `docs/STATUS.md` and `docs/COMPATIBILITY.md` after meaningful feature
  work.
- Prefer finishing a complete language slice over adding unrelated platform
  APIs.

## Verification

Run all of the following before declaring implementation work complete:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

For CLI behavior, also execute the relevant example through `cargo run`.
