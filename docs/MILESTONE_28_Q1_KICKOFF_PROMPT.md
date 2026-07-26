# Milestone 28 Q1 kickoff prompt

Copy the prompt below into a fresh Codex thread. It is intentionally limited to
one semantic correctness package that a single implementation agent can
complete and hand off for review.

```text
You are the implementation owner for Apex Exec work package M28-Q1:
Semantic receiver and profile correctness. Assume no prior conversation
context. Work as one persistent implementation agent; do not delegate or spawn
subagents.

First read, in order:

1. AGENTS.md
2. docs/VISION.md
3. ROADMAP.md
4. docs/STATUS.md
5. docs/ARCHITECTURE.md
6. docs/COMPATIBILITY.md
7. docs/STABILIZATION.md
8. docs/stabilization/FINDINGS.md
9. docs/stabilization/WORK_PACKAGES.md
10. docs/stabilization/OPERATIONS.md
11. docs/MILESTONE_28_CHECKPOINT.md
12. docs/MILESTONE_28_REVIEW_AND_RESUME_PLAN.md
13. docs/decisions/0028-bind-compatibility-profiles-per-source.md
14. docs/decisions/0031-close-enterprise-compatibility-with-measured-typed-slices.md
15. docs/specifications/type-system.md

Then inspect the repository, current branch, git status, recent history, and
the Q1 implementation boundary. Confirm that the M28 integration branch
contains the recorded review plan and that the worktree is clean.

Objective:

Fix only the three related semantic receiver/call regressions recorded under
M28-Q1:

1. A local value named `String` must shadow the static platform type, so
   `String String = 'value'; String.valueOf(1);` resolves the receiver as the
   local String value and reports the instance-method error.
2. Qualified static platform diagnostics must preserve source spelling, so
   `Date.parse` is not rendered as `date.parse`.
3. Every qualified curated platform call must pass through effective
   compatibility-profile enforcement. An API 31.0 source must reject
   `Date.today()` and identify `salesforce-api-31.0`.

Branch and status:

- Do not implement directly on main or on the M28 integration branch.
- Create `codex/m28-q1-semantic-call-profile` from the current
  `codex/milestone-28-enterprise-compatibility` integration head.
- Update M28-Q1 in `docs/MILESTONE_28_REVIEW_AND_RESUME_PLAN.md` to Active with
  the branch before implementation.
- At handoff, move it to Review and record the commit and verification.

Reproduce before editing:

cargo +1.88.0 test --locked --lib \
  semantic::tests::method_receivers_resolve_variables_before_static_types
cargo +1.88.0 test --locked --test milestone10 \
  unsupported_platform_apis_name_the_profile
cargo +1.88.0 test --locked --test milestone25 \
  legacy_profiles_reject_unmodeled_syntax_and_curated_platform_apis

Required design:

- Local/lexical value lookup takes precedence over type or platform-owner
  interpretation for a variable-shaped receiver.
- Static platform-owner recognition retains the original source identifier or
  qualified spelling separately from its canonical lookup key.
- All static platform calls, including qualified forms such as
  `System.Database` and `System.Request`, converge on the same post-selection
  compatibility-profile enforcement boundary.
- Keep parsed syntax immutable and record the existing typed HIR intrinsic
  target.
- Do not add hardcoded profile strings, rendered-message classification,
  duplicate signature tables, or runtime fallback resolution.
- Preserve case-insensitive lookup and exact source spans.

Likely files:

- src/semantic.rs
- src/semantic/intrinsics.rs only if the shared boundary requires it
- src/semantic/tests.rs
- tests/milestone10.rs
- tests/milestone25.rs
- the Q1 status row in docs/MILESTONE_28_REVIEW_AND_RESUME_PLAN.md

Non-scope:

- Standard User schema fields and the two M27 failures
- M21 grammar-census or expectation updates
- Id.getSObjectType, Flow.Interview, or FeatureManagement
- Broad intrinsic-catalog or platform-dispatch refactoring
- Accepting or regenerating the Lizard baseline
- Fixing unrelated Clippy or maintainability findings

Tests:

- Keep the three existing reproductions and strengthen them if needed.
- Add a qualified System-owner regression if the convergence rule is not
  already covered.
- Add a negative shadowing case for a platform owner if needed to prove the
  rule is general rather than special-cased to String.
- Verify exact diagnostic spelling, profile identity, and source span.

Required verification:

cargo +1.88.0 fmt --check
cargo +1.88.0 check --locked --all-targets
cargo +1.88.0 test --locked --lib \
  semantic::tests::method_receivers_resolve_variables_before_static_types
cargo +1.88.0 test --locked --test milestone10 \
  unsupported_platform_apis_name_the_profile
cargo +1.88.0 test --locked --test milestone25 \
  legacy_profiles_reject_unmodeled_syntax_and_curated_platform_apis
cargo +1.88.0 test --locked --all-targets -- \
  --skip annotations_switch_and_external_id_dml_preserve_lossless_syntax_and_phase_boundaries \
  --skip remaining_modifiers_and_multi_fields_fail_in_the_semantic_phase \
  --skip north_star_grammar_census_is_comment_aware_and_stable \
  --skip system_run_as_switches_the_deterministic_owner_visibility_context \
  --skip milestone27_oracle_fixture_is_locally_reproducible

Run the pinned Lizard gate and record the complete expected branch failure, but
prove that every Q1-touched function introduces no new violation and does not
grow an existing NLOC or CCN cap. Do not broaden Q1 to repair unrelated debt.

Commit a coherent, buildable checkpoint with a descriptive message. Do not
push, open a pull request, merge into the M28 integration branch, or start Q2.

Return the handoff format from docs/stabilization/OPERATIONS.md, including:

- fail-before and pass-after evidence for all three regressions;
- exact files changed and public API impact;
- focused and filtered-full-suite results;
- touched-function complexity comparison;
- remaining known five Q2/Q3 test failures;
- branch and final commit SHA;
- final status Review.
```
