# ADR 0025: Freeze enterprise compatibility by Salesforce test

**Status:** Accepted
**Date:** 2026-07-19

## Context

Small examples and syntax corpora can prove individual language behavior, but
they do not show how missing capabilities affect a representative Salesforce
codebase. A project-wide parse percentage is also misleading: one unsupported
production class may block hundreds of tests, while many isolated files may
have no effect on an ordinary test.

M22 needs a stable denominator before Apex Exec results are inspected. It also
needs to preserve compiler/runtime boundaries and avoid compatibility patches,
test exclusions, or optimistic treatment of unsupported behavior.

## Decision

- Use user-approved Nebula Logger Core v4.18.4, pinned at Git commit
  `55ba832d1d51680dd5e291d67ffe2104fa48977f`, as the representative project.
  The Git submodule and a SHA-256 manifest bind every package byte, package
  root, test root, repository, release tag, and API version.
- Freeze every ordinary test method returned by Salesforce
  `RunLocalTests` after deploying byte-identical source to a disposable scratch
  org. Reject captures with fewer than 100 results or results owned by classes
  outside the pinned test roots.
- Persist only allowlisted org and test evidence: target alias, org ID,
  API/tool versions, capture time, test level, source test classes, normalized
  outcomes, and evidence digests. Do not persist access tokens, auth URLs,
  refresh tokens, instance URLs, or raw CLI responses.
- Compute a conservative per-test Apex source closure from case-insensitive
  class ownership and identifier references. Include every package trigger
  because trigger dispatch can be reached through DML without a source-level
  trigger-name reference. Traversal uses a visited set and is bounded by the
  pinned source inventory.
- Measure discovery, parse, check, terminal execution, and normalized
  Salesforce-outcome agreement separately. Parser success never implies
  semantic or runtime compatibility.
- Define the strict numerator as tests whose complete required source closure
  checks, whose local execution reaches a terminal pass/fail result, and whose
  pass/fail outcome agrees with Salesforce. Keep matching failures in the
  numerator but report them separately from matching passes.
- Keep every unsupported construct, platform gap, local failure, and outcome
  mismatch in the raw denominator. M22 defines no exclusions.
- Run the local measurement exactly three times. The first run compiles source
  closures into a bounded in-memory cache; the two warm runs reuse checked
  closures but execute each test in a fresh interpreter. Reject any difference
  in per-test results across reruns.
- Classify blockers from explicit stage boundaries. Rendered diagnostic text is
  evidence detail, not the source of phase or family classification.

## Consequences

- The baseline orders future compatibility work by tests affected rather than
  by file count or anecdotal priority.
- A release, commit, package byte, API version, Salesforce CLI version, org
  result, or local result change requires new evidence.
- Conservative lexical closures can include more source than a perfect
  semantic dependency analysis would require. This may understate
  compatibility, but it cannot turn a known blocker into an unsupported
  success.
- Matching Salesforce failures satisfy outcome agreement but are visibly
  separated from matching passes; consumers cannot present the strict numerator
  as an all-passing test count.
- Scratch-org credentials remain local and ignored. The checked-in capture is
  reproducible evidence, not reusable authentication.
- The report is an Apex Exec compatibility measurement for one frozen project.
  It is not a general Salesforce compatibility percentage.
