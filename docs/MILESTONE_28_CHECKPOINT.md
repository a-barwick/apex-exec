# Milestone 28 checkpoint

Date: 2026-07-19

Branch: `codex/milestone-28-enterprise-compatibility`

Starting commit: `647d8ef1f1146c442046d0d0efdfc8b66f8559d9`

## Milestone contract

M28 closes the highest-impact blockers in the frozen M22 Nebula Logger
denominator without changing benchmark source, expected Salesforce outcomes,
or the 1,159-test denominator. Completion requires at least 60% strict
compatibility, repeatable across three clean local runs. The stretch target is
80%. This checkpoint does not satisfy that exit criterion and must not be
merged to `main` as a completed milestone.

The pre-milestone North Star baseline was lexer 7/7, parser 7/7, total 14/14.
Those counts are syntax indicators, not runtime or Salesforce compatibility
percentages.

## Starting evidence

- `main`, `origin/main`, and the working HEAD all started at
  `647d8ef1f1146c442046d0d0efdfc8b66f8559d9`.
- The worktree was clean before the branch was created.
- The pinned representative project remained at
  `55ba832d1d51680dd5e291d67ffe2104fa48977f`.
- The unchanged pre-M28 replay was 0/1,159 strict compatible and 0/1,159
  parsed. A typed SObject `switch when` pattern affected every test.

## Implemented slices

The current branch contains these bounded language and platform slices:

1. Typed SObject `switch when` patterns, including schema-qualified names,
   arm-scoped bindings, duplicate-pattern rejection, first-match execution,
   null/else behavior, and evaluate-once switching.
2. Lossless `@IsTest(IsParallel=...)` parsing and checking plus deterministic
   serial/parallel test-runner partitioning.
3. Imported `MetadataRelationship` fields with target and controlling-field
   retention through normalized schema and SQLite registry boundaries.
4. Read-only roll-up summary fields for count, sum, minimum, maximum, and
   equality filters. Definitions remain lossless, values are computed before
   query filtering, and multiple summaries over one child object share one
   child scan.
5. Exact equality operators `===` and `!==`, with primitive exact-value
   comparison, reference identity, normal equality precedence, and
   evaluate-once operands.
6. Static SOQL `ALL ROWS`, soft-deleted record visibility, read-only
   `IsDeleted`, and undelete integration.
7. Validated runtime-neutral `@SuppressWarnings(String)` declarations.
8. `@TestVisible` access for annotated private fields, properties, methods,
   constructors, and nested types from lexical test-class contexts only.
9. `System.Comparable` contract validation and HIR-bound custom `List.sort`
   dispatch using a stable bottom-up merge. Comparator exceptions propagate
   without partially rewriting the list.
10. `Database.Stateful` marker handling. Stateful batch receivers retain
    instance state across start, execute chunks, and finish; non-stateful
    receivers are restored from the enqueue-time snapshot for each transaction.

## Enterprise replay progression

All entries below used the unchanged
`benchmarks/milestone22/manifest.json` and
`evidence/milestone22/salesforce.json`.

| Checkpoint | Parse | Check | Strict | Sole blocker | Cold / warm / warm |
|---|---:|---:|---:|---|---|
| Pre-M28 | 0/1,159 | 0/1,159 | 0/1,159 | typed SObject switch pattern | not retained |
| Syntax/schema slices | 1,159/1,159 | 0/1,159 | 0/1,159 | `@SuppressWarnings` | 405,506 / 161 / 161 ms |
| `@SuppressWarnings` | 1,159/1,159 | 0/1,159 | 0/1,159 | `@TestVisible` | 414,698 / 161 / 159 ms |
| `@TestVisible` | 1,159/1,159 | 0/1,159 | 0/1,159 | `System.Comparable` | 414,112 / 161 / 161 ms |

The Comparable and Stateful slices have focused executable tests but have not
received another full 1,159-test replay. The last completed isolated package
check cleared Comparable and identified `Database.Stateful` as the next first
error. Stateful was then implemented and its focused test passed; the
subsequent isolated check was intentionally stopped when this checkpoint was
requested.

## Focused verification completed

- `cargo check --locked --all-targets` passed after the Stateful slice.
- `cargo test --locked --test milestone28 -- --nocapture` passed 9/9 before
  the Stateful test was added.
- The focused Stateful test passed after it was added.
- Metadata importer tests passed 3/3.
- SQLite platform tests passed 7/7.
- The unchanged M21 North Star grammar census passed after the two newly
  supported annotations were classified explicitly.
- `git diff --check` passed before the final Stateful additions.

No full milestone completion verification, coverage run, final North Star run,
live Salesforce differential capture, or post-merge verification has been
performed. Do not infer those results from this checkpoint.

## Resume sequence

1. Confirm the branch and worktree:

   ```bash
   git switch codex/milestone-28-enterprise-compatibility
   git status --short --branch
   ```

2. Run the isolated core check to identify the next first blocker:

   ```bash
   cargo run --locked -- check /tmp/apex-exec-m28-check
   ```

   If the temporary project no longer exists, recreate an SFDX project whose
   sole package directory is the absolute path to
   `benchmarks/milestone22/nebula-logger/nebula-logger/core` at API 65.0.

3. After the next complete slice, refresh the frozen per-test funnel:

   ```bash
   cargo run --locked -- enterprise run \
     benchmarks/milestone22/manifest.json \
     --salesforce evidence/milestone22/salesforce.json \
     --output /tmp/apex-exec-m28-next-report.json
   ```

4. Continue by impacted-test count until at least 696/1,159 tests satisfy the
   strict numerator. Then run three clean repeatability runs, the complete
   verification/coverage/North Star workflow, relevant CLI examples, and live
   differential evidence before marking M28 complete or merging to `main`.

## Explicit remaining limitations

- The strict compatibility numerator is still 0/1,159 at the last completed
  enterprise replay.
- Heterogeneous `List<Object>` sorting and SObject natural ordering are not
  modeled; custom sorting currently requires a statically typed Comparable
  class.
- Roll-up support is limited to imported count/sum/min/max definitions over
  supported Integer/Date/Datetime fields and equality filters.
- Metadata-relationship describe traversal remains outside the implemented
  scalar storage surface.
- Salesforce-exact cross-transaction static isolation is not claimed for
  asynchronous work.
- M28 documentation remains a checkpoint, not release evidence.
