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
6. `docs/STABILIZATION.md`

While the Phase 2 stabilization gate is active, also read:

1. `docs/stabilization/FINDINGS.md`
2. `docs/stabilization/WORK_PACKAGES.md`
3. `docs/stabilization/OPERATIONS.md`

## Active stabilization program

- M18 feature implementation is gated until the S0 exit criteria in
  `docs/STABILIZATION.md` pass.
- Claim only a work package marked **Ready**, record its branch/status before
  implementation, and respect its dependencies, file ownership, scope, and
  non-scope.
- Run at most three implementation workstreams concurrently. Never allow two
  active workstreams to substantially edit the same runtime, semantic, AST, or
  HIR hotspot.
- Implementation work moves a package to **Review**. Only the integration owner
  marks it **Complete** after independent review, integration, reproductions,
  and full verification.
- New findings belong in `docs/stabilization/FINDINGS.md` and the tracker; do
  not silently expand an active package.
- The repository owner must select the license, supported public API policy,
  approve S1 architecture ADRs, and approve the final stabilization merge to
  `main`.

## Local Salesforce validation org

- A disposable Salesforce Developer Edition is authenticated in the local
  Salesforce CLI under alias `apex-exec-m17`. Despite earlier shorthand, this
  is not a scratch org.
- Its expected org ID is `00DdL000010oTXlUAM`. Before any deployment, deletion,
  metadata mutation, or evidence capture, load the ignored root `.env` and
  verify that `sf org display --target-org "$APEX_EXEC_SF_TARGET_ORG" --json`
  reports that exact org ID. Do not use `--verbose` or print token-bearing
  fields.
- The root `.env` contains local-only connection and login values under the
  `APEX_EXEC_SF_*` names. It is ignored by Git and must never be staged,
  committed, copied into evidence, echoed, or included in command output.
  Prefer the existing CLI alias and token store; use the login values only if
  reauthentication is genuinely required.
- The owner authorized this org as disposable Apex Exec validation
  infrastructure: agents may deploy, mutate, and delete project fixtures as
  needed after the org-ID guard passes. Keep mutations scoped to Apex Exec
  fixtures, record the intended baseline, and restore it after controlled
  blocker/drift scenarios.
- M17 left the exact milestone-15 fixture baseline restored in the org. Its
  reviewed evidence used API `65.0`; current work must bind evidence to the
  API and Salesforce CLI versions it actually uses rather than assuming the
  old versions remain current.
- The prior task's permission to retrieve verification codes from email was
  explicitly limited to that task. Future agents do not have that permission.
  If CLI authorization is disconnected and interactive verification is
  required, stop and ask the user for help.
- The authenticated org supplies Salesforce transport and outcome evidence; it
  does not by itself choose or approve the representative M22 enterprise
  project or its raw test denominator.

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
- Do not add raw positional identities across compiler/runtime boundaries,
  hardcoded compatibility-profile strings, rendered-message classification, or
  anonymous zero-span sentinels.
- Every recursive traversal over user-controlled syntax or runtime graphs must
  prove acyclicity or enforce visited/depth/node limits.
- Keep debugging, coverage, and tracing opt-in and bounded.
- Touching a recorded complexity hotspot requires extracting the relevant
  abstraction; moving the same decision tree into another file is not a
  maintainability improvement.
- Add a reproducible benchmark or deterministic cost assertion for compiler,
  runtime, collection, query, transaction, or async hot-path changes.

## Verification

Run all of the following before declaring implementation work complete:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

For CLI behavior, also execute the relevant example through `cargo run`.
