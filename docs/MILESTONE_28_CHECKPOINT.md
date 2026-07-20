# Milestone 28 checkpoint

Date: 2026-07-20

Branch: `codex/milestone-28-enterprise-compatibility`

Starting commit: `647d8ef1f1146c442046d0d0efdfc8b66f8559d9`

Bounded implementation head: `ae5b52d9b38dc36bc628a09e7ab344660716da1d`

## Milestone contract and stop decision

M28 closes the highest-impact blockers in the frozen M22 Nebula Logger
denominator without changing benchmark source, expected Salesforce outcomes,
or the 1,159-test denominator. Completion requires at least 60% strict
compatibility, repeatable across three clean local runs. The stretch target is
80%.

This checkpoint does not satisfy that exit criterion. Work stopped
deliberately after the already-open
`Map.putAll(List<SObject>)` slice was implemented, verified, and committed.
The newly exposed blocker families listed below were not started. This branch
must not be merged to `main` as a completed milestone.

A subsequent read-only integrated review found ordinary-suite, Clippy, and
maintainability regressions that must be corrected before another compatibility
slice. The evidence, bounded recovery queue, and first-package prompt are in
[`MILESTONE_28_REVIEW_AND_RESUME_PLAN.md`](MILESTONE_28_REVIEW_AND_RESUME_PLAN.md).

The pre-milestone North Star baseline was lexer 7/7, parser 7/7, total 14/14.
Those counts are syntax indicators, not runtime or Salesforce compatibility
percentages.

## Frozen inputs

- The branch started from
  `647d8ef1f1146c442046d0d0efdfc8b66f8559d9`.
- The representative project remains pinned at
  `55ba832d1d51680dd5e291d67ffe2104fa48977f`.
- Manifest SHA-256:
  `c352505e5ade7662919f4f32fea230a72342e8dccfbf2bf4725b31ae4c47cbcd`.
- Salesforce snapshot SHA-256:
  `1d0972ced93edca0053675229378fd805e4feae5596f60d60737a237df80ada0`.
- The raw denominator remains 1,159 tests.
- No benchmark source, expected Salesforce outcome, pinned project revision,
  manifest, snapshot, or denominator was changed.

## Commit and slice ledger

The original checkpoint commit was:

- `94771dd` — typed SObject `switch when`; lossless
  `@IsTest(IsParallel=...)`; imported metadata relationships; bounded roll-up
  summaries; exact equality operators; SOQL `ALL ROWS` and soft deletion;
  validated `@SuppressWarnings`; lexical `@TestVisible`; stable custom
  `System.Comparable` sorting; and `Database.Stateful` batch state.

Every implementation commit after that checkpoint, through the deliberate
stop point, is recorded here:

| Commit | Bounded slice |
|---|---|
| `cfaf5c6` | Lexical-family private access; UTF-16-aware String helpers; `Database.AllowsCallouts`; `Database.BatchableContext`; typed async context/mocks and request values; transient fields and JSON omission; `@AuraEnabled` options; `HttpCalloutMock`; `Callable` and type-reflection dispatch; deterministic platform-cache unavailability; scalar and enum switch dispatch. |
| `8dedceb` | Typed `LoggingLevel`; final field assignment rules; lazy static final properties; finite `Double` values; typed JSON deserialization; VisualEditor dynamic picklists; custom-metadata accessors and bounded deep clone; schema tokens, describe fields, and field sets. |
| `9fb0b5a` | `TriggerOperation` enum semantics. |
| `d2a1e95` | Canonical nested collection type identity. |
| `a34935a` | Typed `Id` access on dynamic SObjects. |
| `bf20478` | `Database.DMLOptions` modeling and all-or-none behavior. |
| `c05f1f7` | Checked covariant SObject list downcasts. |
| `9b9a87f` | Exactly-once assignment enforcement for final locals. |
| `64e2e90` | Named SObject constructor-field initialization. |
| `5536e6e` | SObject-ID extraction for SOQL collection binds. |
| `f60d5a6` | Assignment-compatible widening from `Id` to `String`. |
| `497f9e3` | Typed `Queueable` interface values. |
| `8d96b81` | `Long.valueOf` and epoch Datetime conversion. |
| `87066cc` | Standard platform-event `EventUuid` metadata. |
| `9a9e7ab` | Unqualified SObject static type tokens. |
| `cf57e38` | Validated implicit `String`-to-`Id` conversion. |
| `a046bea` | `String instanceof Id` validation. |
| `4c62e4b` | Explicit Network context plus measured governor counters and Salesforce-aligned maxima. |
| `9aad967` | `Map<Id/String, SObject>(List<SObject>)` indexing by record ID with Salesforce-shaped duplicate/null failures. |
| `ca4c9a9` | `FlowVersionView` runtime fields and `FlowDefinitionView` relationship metadata. |
| `0284f5d` | Mixed lexical static/instance method-overload resolution. |
| `ae5b52d` | `Map.putAll(List<SObject>)` for compatible `Id`/`String` keyed SObject maps, including replacement, full pre-mutation validation, and exact duplicate/null row failures. |

The final `Map.putAll` slice reuses the constructor's bounded list-to-map
validation. A failing list does not partially mutate the target map.

## Latest frozen enterprise funnel

The post-`ae5b52d` replay used the unchanged frozen inputs:

```bash
cargo +1.88.0 run --release --locked -- enterprise run \
  benchmarks/milestone22/manifest.json \
  --salesforce evidence/milestone22/salesforce.json \
  --output /tmp/apex-exec-m28-after-sobject-put-all.json
```

| Stage | Count | Denominator | Basis points |
|---|---:|---:|---:|
| Discovery | 1,159 | 1,159 | 10,000 |
| Parse | 1,159 | 1,159 | 10,000 |
| Check | 0 | 1,159 | 0 |
| Execution | 0 | 1,159 | 0 |
| Salesforce agreement | 0 | 1,159 | 0 |
| Strict compatible | 0 | 1,159 | 0 |

Matching passes, matching failures, and outcome mismatches were all zero
because no test passed semantic checking. Measured replay durations were
206,042 ms cold, 70 ms warm, and 69 ms warm.

The latest replay therefore remains 0/1,159 strict compatible. It is a
checkpoint measurement, not completion evidence.

## Exact remaining first blockers

The latest report assigned the full denominator to these three first-error
families:

1. 1,126 tests — check / `semantic.unsupported`:
   ``unsupported API `Id.getSObjectType` in compatibility profile
   `salesforce-api-65.0` `` in
   `nebula-logger/core/main/log-management/classes/LogEntryHandler.cls`.
2. 18 tests — check / `semantic.unsupported`:
   ``unknown type `Flow.Interview` `` in
   `nebula-logger/core/main/log-management/classes/LogBatchPurger.cls`.
3. 15 tests — check / `semantic.unsupported`:
   ``unknown variable `System` `` in
   `nebula-logger/core/main/log-management/classes/LogBatchPurgeController.cls`.

These impacted-test counts total 1,159. None of these blocker families was
implemented after the replay.

## Verification at the bounded stop point

The following checks passed after the `Map.putAll(List<SObject>)` changes:

- Focused SObject list-map constructor/`putAll` regression: 1/1.
- `cargo +1.88.0 test --locked --test milestone28`: 46/46.
- Existing typed map-method regression in `tests/milestone3.rs`: 1/1.
- `cargo +1.88.0 check --locked --all-targets`.
- `cargo +1.88.0 fmt --check`.
- `git diff --check`.
- The frozen enterprise cold/warm/warm replay described above.

No post-slice full `cargo test`, Clippy gate, coverage run, final North Star
run, three-clean-run acceptance sequence, or post-merge verification was
performed. Those remain milestone-completion work and must not be inferred
from this checkpoint.

Live Salesforce probes used during the post-checkpoint work were guarded
against disposable org ID `00DdL000010oTXlUAM`. They bound Network/Limits
behavior, SObject-list map duplicate/null failures, and `FlowVersionView`
describe fields at API 67.0 with Salesforce CLI 2.30.8. They were development
differentials, not final M28 release evidence.

## Clean resume command

Resume only from the published milestone branch:

```bash
git fetch origin
git switch codex/milestone-28-enterprise-compatibility
git pull --ff-only origin codex/milestone-28-enterprise-compatibility
git status --short --branch
cargo +1.88.0 run --release --locked -- enterprise run \
  benchmarks/milestone22/manifest.json \
  --salesforce evidence/milestone22/salesforce.json \
  --output /tmp/apex-exec-m28-resume.json
```

Before implementing anything, confirm the two SHA-256 values, denominator,
funnel, and three blocker entries above. The next dominant family is
`Id.getSObjectType`; it has not been claimed or designed in this checkpoint.
The later review requires completing the quality-recovery packages before
claiming that feature slice.

M28 can be marked complete only after at least 696/1,159 tests satisfy the
strict numerator, three clean runs reproduce the result, all repository gates
and relevant CLI examples pass, required coverage/North Star/live
differential evidence is captured, and the completion documentation is
reviewed. Do not merge this checkpoint to `main`.
