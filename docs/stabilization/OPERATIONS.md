# Stabilization Operations

This document defines how independent threads execute the work packages without
losing status, duplicating work, or creating unreviewable merge conflicts.

## Roles

### Program/integration thread

- Owns `docs/STABILIZATION.md`.
- Creates or coordinates the `codex/stabilization` integration branch.
- Selects only **Ready** packages.
- Enforces dependencies and file ownership.
- Integrates reviewed branches one at a time.
- Runs full verification after every integration.
- Reports owner decisions and final gate readiness.
- Does not use the integration branch for opportunistic feature work.

### Implementation thread

- Claims exactly one work package.
- Creates the package's `codex/` branch from the declared baseline.
- Reads all required project and stabilization documentation.
- Implements only the stated scope.
- Adds executable regression tests for every behavior change.
- Commits coherent, buildable checkpoints.
- Updates the tracker and produces the handoff report below.

### Review thread

- Begins from the work-package scope and diff, not the implementer's narrative.
- Performs a read-only correctness, architecture, test, and scope review.
- Distinguishes blocking findings from follow-up suggestions.
- Does not implement changes unless explicitly reassigned.

## Concurrency rules

- Run at most three implementation threads concurrently.
- The first allowed parallel wave is S0-01, S0-02, and S0-05.
- Never run two branches that both substantially modify `src/runtime.rs`,
  `src/semantic.rs`, `src/ast.rs`, or `src/hir.rs`.
- S0-03 follows S0-02; S0-04 follows both.
- Cross-cutting diagnostic work runs exclusively.
- Architecture ADRs are reviewed before dependent implementation begins.
- A dependency is satisfied only after integration, not when another branch
  merely claims completion.

## Claim procedure

Before implementation:

1. Confirm the package is **Ready** in `docs/STABILIZATION.md`.
2. Confirm no active thread owns overlapping hotspot files.
3. Update the package to **Active** with branch and owner/thread identifier.
4. Create the declared `codex/` branch from the current integration baseline.
5. Re-run or inspect the package's recorded reproduction before changing code.
6. State scope, non-scope, assumptions, and expected files in the thread.

If the tracker cannot be updated without conflicting with another branch, the
integration thread records claims centrally and task branches include a
handoff-status patch for later integration.

## Implementation guardrails

- Keep lexing, parsing, semantic analysis, lowering, and execution separate.
- Do not silently erase syntax or approximate unsupported behavior.
- Do not introduce new raw positional IDs across compiler/runtime boundaries.
- Do not introduce new hardcoded profile strings or message-text
  classification.
- Do not use anonymous zero spans as sentinel error state.
- Every recursive traversal over user-controlled syntax or runtime values must
  prove acyclicity or enforce visited/depth/node limits.
- Observability must be opt-in and bounded.
- Correctness-critical host capabilities must not default to successful no-ops.
- Touching a complexity hotspot requires extracting the relevant abstraction;
  moving the same decision tree to another file is not improvement.
- Hot-path changes require a reproducible benchmark or deterministic cost
  assertion.

## Required verification

Every implementation package runs:

```bash
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
```

It also runs:

- Every CLI reproduction named in the package.
- Focused new tests.
- Website/editor checks if those artifacts changed.
- Complexity non-regression checks once S0-05 supplies them.
- Relevant performance assertions for runtime/compiler/data hot paths.

### Verification drift guards

Review and integration verification must not rely on a clean worktree alone:

- Pin the expected branch and full candidate SHA before every verification
  group, require an empty tracked/staged/untracked status, and recheck both
  after the group.
- Prove the declared integration baseline and documentation checkpoint remain
  ancestors of the candidate.
- Use a new candidate-SHA-specific `CARGO_TARGET_DIR` for authoritative Rust
  review and gate runs. Do not reuse a task or reviewer target directory.
- Reconcile test totals against the checked-in harness inventory. If a binary
  exposes a test absent from the candidate source, stop, quarantine the shared
  target, and rerun from a new isolated target before recording evidence.
- Keep implementation branches immutable while read-only review is active.
  Any blocking fix creates a new Review SHA and invalidates the earlier
  verification result.

## Handoff report

Every implementation thread returns this exact structure:

```markdown
## Work package

- ID:
- Branch:
- Commits:
- Final status: Review | Blocked

## Outcome

What behavior or architecture changed.

## Acceptance evidence

- Requirement:
- Test or reproduction:
- Result:

## Verification

- `cargo fmt --check`:
- `cargo test`:
- `cargo clippy --all-targets -- -D warnings`:
- Additional commands:

## Files and boundaries

- Files changed:
- New public API:
- Complexity delta:
- Performance evidence:

## Review notes

- Known risks:
- Follow-up IDs proposed:
- Documentation updated:
- Scope deliberately not addressed:
```

A package cannot be marked **Complete** directly by its implementation thread.
It moves to **Review**. The integration thread marks it **Complete** after
review, integration, and full-suite verification.

## Review checklist

Reviewers must answer:

1. Does the change satisfy every acceptance criterion?
2. Can any user-controlled input still panic, abort, loop indefinitely, or
   allocate without a bound?
3. Did the change preserve source spelling, spans, and case-insensitive lookup?
4. Did syntax, semantics, and execution stay in their owning layers?
5. Was complexity reduced, or merely moved?
6. Is there now more than one source of truth?
7. Are unsupported cases explicit?
8. Are evaluation order and single evaluation tested?
9. Are error types/locations machine-readable rather than inferred from text?
10. Are all new public APIs intentional and documented?
11. Are regression, negative, and stress tests proportional to risk?
12. Did the branch remain within its declared scope?

## Integration procedure

1. Verify the task branch is clean and its commits are coherent.
2. Read the independent review.
3. Require blocking findings to be resolved on the task branch.
4. Merge one task branch into `codex/stabilization`.
5. Run the complete verification suite.
6. Run affected reproductions again on the integrated state.
7. Update `docs/STABILIZATION.md` with status, branch, commits, and evidence.
8. Commit the integration checkpoint.
9. Only then unblock dependent packages.

When S0-GATE passes, request owner approval before merging to `main`.

## Handling new findings

Do not expand an active package just because a nearby problem was discovered.
Instead:

1. Record a new finding in `FINDINGS.md`.
2. Add a proposed work-package ID and priority.
3. State whether it blocks the active package's acceptance criteria.
4. Let the integration thread schedule it.

An active package may fix a new issue only when it is required to satisfy the
declared acceptance criteria and remains inside the same architectural
boundary.

## Recovery and stale work

- If a branch is stale, rebase or merge the current integration baseline only
  after committing or preserving its coherent work.
- Never use destructive reset/checkout operations on another thread's changes.
- If two completed branches conflict in a hotspot, integrate the architectural
  prerequisite first and return the other branch to its owner for adaptation.
- If verification exposes a cross-package regression, mark the later package
  **Blocked**, preserve the failing evidence, and do not continue the wave.
