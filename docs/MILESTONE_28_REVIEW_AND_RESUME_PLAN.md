# Milestone 28 review and resume plan

Date: 2026-07-20

Audited branch: `codex/milestone-28-enterprise-compatibility`

Audited head: `3818999826bbf6e5086c96c4ed79acebafcb3847`

This document records the post-checkpoint repository review and divides the
remaining work into bounded packages suitable for one implementation agent at a
time. It supplements, but does not replace, the frozen evidence and stop
decision in [`MILESTONE_28_CHECKPOINT.md`](MILESTONE_28_CHECKPOINT.md).

## Executive assessment

M1 through M27 are complete as roadmap implementation milestones. M28 remains
active and has not met its product gate. The current branch contains valuable
typed compatibility work, but it is neither merge-ready nor ready for another
feature slice.

The frozen Nebula Logger denominator remains 1,159 Salesforce-passing tests.
The audited replay discovers and parses all 1,159 tests, but no required source
closure passes semantic checking. Strict compatibility is therefore 0/1,159,
against the M28 minimum of 696/1,159.

This zero is a censored compatibility result, not evidence that all 1,159 tests
would fail at runtime. A shared source-closure diagnostic stops each test before
execution, so further checker, runtime, and outcome blockers remain hidden
behind the current first-error families.

The branch is 24 commits ahead of `main` and changes 55 files with 12,523
insertions and 888 deletions. The worktree was clean throughout the review. No
project file was modified while collecting the evidence below.

## Reproduced evidence

### Frozen enterprise funnel

The following command was rerun from the audited head:

```bash
cargo +1.88.0 run --release --locked -- enterprise run \
  benchmarks/milestone22/manifest.json \
  --salesforce evidence/milestone22/salesforce.json \
  --output /tmp/apex-exec-review-m28.json
```

The manifest SHA-256 remained
`c352505e5ade7662919f4f32fea230a72342e8dccfbf2bf4725b31ae4c47cbcd`.
The Salesforce snapshot SHA-256 remained
`1d0972ced93edca0053675229378fd805e4feae5596f60d60737a237df80ada0`.

| Stage | Count | Denominator | Basis points |
|---|---:|---:|---:|
| Discovery | 1,159 | 1,159 | 10,000 |
| Parse | 1,159 | 1,159 | 10,000 |
| Check | 0 | 1,159 | 0 |
| Execution | 0 | 1,159 | 0 |
| Salesforce agreement | 0 | 1,159 | 0 |
| Strict compatible | 0 | 1,159 | 0 |

The three deterministic measurements were 207,153 ms cold, 70 ms warm, and
70 ms warm. The warm result demonstrates in-process complete-build reuse. It
does not satisfy M29's restart-safe persistent-IR or dependency-scoped
rechecking requirements.

### Current first blockers

| Impacted tests | First blocker | Representative source |
|---:|---|---|
| 1,126 | Unsupported `Id.getSObjectType` | `LogEntryHandler.cls` |
| 18 | Unknown `Flow.Interview` | `LogBatchPurger.cls` |
| 15 | Unknown `System` receiver for `System.FeatureManagement.checkPermission` | `LogBatchPurgeController.cls` |

These counts total the raw denominator. They are only the first diagnostics in
each required closure. After any blocker family is integrated, the complete
funnel must be rerun before another feature slice is selected.

### Verification matrix

| Verification | Result |
|---|---|
| `cargo +1.88.0 fmt --check` | Pass |
| North Star syntax suite | Pass, 16/16 tests and 14/14 indicators |
| Focused M28 suite | Pass, 46/46 |
| Complete Rust inventory | 503 tests, eight failures |
| `cargo +1.88.0 clippy --locked --all-targets -- -D warnings` | Fail, 28 diagnostics |
| Pinned Lizard ratchet | Fail, 44 regressions or new hotspots |
| Tooling self-tests | Pass, 20/20 |
| Documentation validation | Pass, 73 Markdown files and 110 local links |
| Website build/test/lint | Pass |
| VS Code smoke test | Pass |
| npm advisory policy | Pass under the documented unexpired allowance |
| Post-M28 full LLVM coverage | Not run |

`cargo-audit` and `cargo-deny` were not installed in the review environment.
Their CI jobs remain required before milestone completion.

### Eight failing Rust tests

The eight failures group into five bounded causes:

1. `method_receivers_resolve_variables_before_static_types` shows that a local
   variable named `String` is incorrectly resolved as the static platform type.
2. `unsupported_platform_apis_name_the_profile` shows that qualified static
   platform diagnostics lose original source spelling (`date.parse` instead of
   `Date.parse`).
3. `legacy_profiles_reject_unmodeled_syntax_and_curated_platform_apis` shows a
   correctness regression: API 31.0 accepts `Date.today()` because one
   qualified-call path bypasses compatibility-profile enforcement.
4. Three M21 tests still assert checked-only behavior and census counts for
   syntax that M28 intentionally implemented, including `@AuraEnabled` and
   `final` locals.
5. Two M27 tests fail because the new standard `User` schema omits fields used
   by the reviewed `System.runAs` fixture, beginning with `Alias`.

The first three failures share the semantic receiver/call boundary and form one
implementation package. The M21 and M27 groups own disjoint files and remain
separate packages.

### Maintainability regression

The pinned ratchet reports 44 violations. It must be restored through
abstraction extraction, not by normalizing a larger baseline. The most
consequential growth includes:

| Function | Recorded cap | Audited value |
|---|---:|---:|
| Runtime `call_platform` | 589 NLOC / CCN 124 | 819 / 151 |
| Semantic `platform_instance_method_type` | 378 / 40 | 584 / 67 |
| Semantic `static_platform_method_type` | 242 / 46 | 353 / 66 |
| Semantic `string_instance_method_type` | 121 / 16 | 213 / 27 |
| Runtime `call_map` | CCN 24 | CCN 32 |

New hotspots also appear across AST type conversion, parser annotations and
switches, metadata import and roll-up resolution, standard schema construction,
typed JSON conversion, value-graph comparison, schema member access, and
coverage traversal. The package sequence below prevents a single agent from
absorbing all 44 findings.

### Documentation and public-contract drift

The documentation checker validates Markdown structure and local links, not
semantic agreement between documents. The review found several stale public
claims:

- `README.md` still says M22 is next even though M22 through M27 are complete
  and M28 is active.
- `README.md` describes M25 validation snapshot schema 3 while current M26
  evidence uses schema 4.
- `COMPATIBILITY.md` still contains pre-M24 statements that Database methods
  return `void` and partial results are unsupported.
- `STATUS.md` retains pre-M27 limitations saying sharing modifiers are rejected.

These claims must be reconciled no later than M30-B. Any Q or C package that
changes an affected behavior must update its compatibility row immediately
rather than waiting for the final sweep.

### Residual stabilization architecture

Feature milestones progressed while several post-S0 stabilization packages
remained blocked. Their underlying work is still real and must be scheduled
through M28 maintainability recovery, M29, or the release gate:

- S1-03: lowered executable/runtime-image targets;
- S1-05: one authoritative intrinsic descriptor and handler catalog;
- S1-06: stable structured diagnostic codes, phases, labels, and locations;
- S2-01: bounded transaction/savepoint and explicit host-capability contracts;
- S2-02: project-scale incremental semantic work and performance;
- S2-03: UTF-16/URI/message-bound LSP and DAP correctness; and
- S2-04: the owner-blocked open-source release gate.

The M28-M4/M5A packages may establish the intrinsic seam needed by S1-05, but
they must not silently claim all S1-05 or S1-06 acceptance criteria. M29-A
must reconcile the remaining S1/S2 dependencies explicitly before
implementation.

## Operating rules for this recovery

- The active M28 branch is the integration branch for this plan. Do not
  implement packages directly on it.
- Create one `codex/m28-*` task branch per package from the current reviewed
  M28 integration head.
- Run packages serially when they touch `src/semantic.rs`, `src/runtime.rs`,
  `src/semantic/intrinsics.rs`, or `src/runtime/platform_intrinsics.rs`.
- An implementation package moves only to **Review**. Integrate it after a
  fresh read-only review and package verification.
- Do not start another compatibility slice while the ordinary suite, Clippy,
  or maintainability ratchet is red.
- Do not change the M22 project, manifest, Salesforce snapshot, expected
  outcomes, raw denominator, or source-closure rules.
- Do not silently widen a package. Record a new finding and schedule another
  bounded package.
- Every behavior change requires executable positive, negative, and
  evaluate-once or bounded-cost coverage proportional to its risk.
- Touching a recorded hotspot requires extracting the owning abstraction.

## Package queue

Statuses are **Ready**, **Blocked**, **Review**, and **Complete**. Only one
package may be **Ready** initially. The integration owner advances the next
package after review and integration.

| ID | Package | Status | Depends on | Primary ownership |
|---|---|---|---|---|
| M28-Q1 | Semantic receiver and profile correctness | Active (`codex/m28-q1-semantic-call-profile`) | Recorded review | `semantic.rs`, focused semantic/M10/M25 tests |
| M28-Q2 | Standard `User` schema restoration | Blocked | Q1 integrated | Standard schema, M27 tests |
| M28-Q3 | M21 expectation and census reconciliation | Blocked | Q2 integrated | M21 tests and census documentation |
| M28-Q4A | AST and SObject error-shape Clippy cleanup | Blocked | Q3 integrated | AST, platform SObject |
| M28-Q4B | SQLite error-shape and database Clippy cleanup | Blocked | Q4A integrated | SQLite, platform database |
| M28-Q4C | Remaining semantic Clippy cleanup | Blocked | Q4B integrated | Semantic and async contract helpers |
| M28-M1 | Frontend and coverage maintainability restoration | Blocked | Clippy green | AST visitor, parser, coverage |
| M28-M2A | Metadata import maintainability restoration | Blocked | M1 integrated | Platform metadata |
| M28-M2B | Database, schema, and SQLite maintainability restoration | Blocked | M2A integrated | Platform database/schema/SQLite |
| M28-M3A | Runtime call/member/construction restoration | Blocked | M2B integrated | Runtime core |
| M28-M3B | Runtime collection dispatch restoration | Blocked | M3A integrated | Runtime List/Set/Map intrinsics |
| M28-M3C | Runtime String and value-graph restoration | Blocked | M3B integrated | Runtime String/value graph |
| M28-M4 | Runtime platform dispatch decomposition | Blocked | M3C integrated | `runtime/platform_intrinsics.rs` |
| M28-M5A | Semantic platform dispatch decomposition | Blocked | M4 integrated | `semantic/intrinsics.rs` |
| M28-M5B | Core semantic hotspot restoration | Blocked | M5A integrated | `semantic.rs` |
| M28-V0 | Integrated quality-gate checkpoint | Blocked | Q1-Q4C, M1-M5B | Verification and documentation |
| M28-C1 | `Id.getSObjectType` compatibility slice | Blocked | V0 complete | Typed ID/schema/runtime boundary |
| M28-CENSUS-1 | Frozen enterprise replay and reprioritization | Blocked | C1 integrated | Enterprise evidence only |
| M28-CN | One next-ranked compatibility family | Blocked | Latest census | Determined by fresh first blockers |
| M28-GATE | M28 completion evidence | Blocked | At least 696 strict tests | Full verification and evidence |
| M29-A | Persistent-IR design and benchmark contract | Blocked | M28-GATE | ADR/specification/benchmarks |
| M29-B | Dependency-scoped semantic work | Blocked | M29-A approved | Project/compiler/HIR |
| M29-C | Versioned persistent executable IR | Blocked | M29-B integrated | HIR/runtime image/cache |
| M29-D | Restart reuse and performance acceptance | Blocked | M29-C integrated | Benchmarks/evidence |
| M30-A | Release-candidate live evidence refresh | Blocked | M28 and M29 complete | Oracle/hybrid/metadata/security |
| M30-B | Public contract and documentation reconciliation | Blocked | M30-A | Public documentation |
| RELEASE | License and supported-public-API gate | Owner-blocked | Owner decisions | License/API/distribution |

## Package definitions

### M28-Q1 — Semantic receiver and profile correctness

Objective: restore one unambiguous receiver-resolution and profile-enforcement
path for local values, user types, and qualified platform owners.

Required behavior:

- A local or lexical value shadows a type or platform owner with the same
  case-insensitive name.
- Qualified platform owners retain original source spelling in diagnostics.
- Every curated platform intrinsic, including qualified forms, passes through
  the effective source-profile gate.
- API 31.0 rejects `Date.today()` with the exact profile identity.
- No hardcoded profile string or rendered-message classification is added.

Acceptance:

- The three named failing tests pass unchanged or are strengthened.
- The full suite passes with only the five explicitly deferred Q2/Q3 failures
  filtered out.
- Formatting and `cargo check --locked --all-targets` pass.
- The touched semantic functions introduce no new Lizard violation or cap
  growth.

Non-scope: standard schema fields, M21 census updates, broad intrinsic-catalog
work, M28 enterprise APIs, and accepting existing complexity debt.

### M28-Q2 — Standard `User` schema restoration

Objective: restore the complete standard `User` field surface required by the
reviewed M27 `System.runAs` and oracle fixtures without weakening schema checks.

Acceptance:

- Both failing M27 tests pass.
- Unknown standard fields remain explicit diagnostics.
- Standard-schema construction remains deterministic and has a bounded cost
  assertion.
- No unrelated enterprise object surface is added.

### M28-Q3 — M21 expectation and census reconciliation

Objective: distinguish intentionally promoted M28 behavior from regressions and
update only the stale M21 assertions and grammar census.

Acceptance:

- The three failing M21 tests pass.
- Byte-identical North Star fixtures remain unchanged and all 14 indicators
  pass.
- Every promoted construct is reflected accurately in compatibility
  documentation.
- No unsupported behavior is relabeled as executable merely to satisfy a test.

### M28-Q4A through M28-Q4C — Clippy recovery

These packages remove the 28 warning-denied Clippy failures without broad
feature changes:

- Q4A owns the large `SwitchLabels` variant and `SObjectError` representation.
- Q4B owns `SqliteError`, the database `entry` rewrite, and affected storage
  tests.
- Q4C owns async-contract initialization, redundant conversion/branching, and
  any remaining warning after Q4A/Q4B integration.

Each package must preserve public diagnostics and avoid hiding warnings with a
blanket `allow`. Q4C exits only when full warning-denied Clippy passes.

### M28-M1 through M28-M5B — Maintainability recovery

Each package removes only the ratchet failures in its ownership zone and leaves
the pinned baseline unchanged:

- M1: AST type conversion, parser annotations/query/switch helpers, and coverage
  traversal.
- M2A: metadata import, field, summary parsing, summary resolution, and focused
  metadata test helpers.
- M2B: standard schema construction, roll-up hydration, SQLite field
  specification, and remaining platform data helpers.
- M3A: runtime member access, method calls, and constructor argument handling.
- M3B: List, Set, and Map runtime dispatch, including the recorded cap growth.
- M3C: String runtime dispatch plus value-graph rendering and comparison.
- M4: split runtime platform execution by typed owner family and extract schema
  and JSON handlers from `call_platform`.
- M5A: split semantic platform and String signature/owner selection into an
  authoritative typed descriptor/helper boundary.
- M5B: reduce the remaining core semantic subtype, cast, switch, construction,
  schema-member, and SObject-method hotspots.

The packages are deliberately serial and narrow because their ownership zones
contain recorded hotspots. Their exit criteria are the existing Lizard caps,
full tests, and no new semantic/runtime source of truth.

### M28-V0 — Integrated quality-gate checkpoint

Run and record:

```bash
cargo +1.88.0 fmt --check
cargo +1.88.0 test --locked
cargo +1.88.0 test --locked --test north_star -- --nocapture
cargo +1.88.0 clippy --locked --all-targets -- -D warnings
python3 -m unittest discover -s tools/tests -p 'test_*.py' -v
python3 tools/docs/check_docs.py
tools/maintainability/check_lizard.sh
```

Also run the website/editor/dependency layers from `.github/workflows/ci.yml`
and collect a fresh full LLVM coverage report. No M28 compatibility feature
package becomes Ready until V0 is complete.

### M28-C1 — `Id.getSObjectType`

Implement a typed vertical slice covering:

- static checking and HIR target identity;
- runtime key-prefix-to-schema resolution;
- null and malformed/unknown ID behavior;
- safe-navigation single evaluation;
- the required catchable `System.SObjectException` behavior;
- focused local and Salesforce differential fixtures;
- deterministic bounded lookup cost.

After integration, do not start `Flow.Interview` automatically. Run
M28-CENSUS-1 and choose the new highest-impact first blocker.

### M28-CENSUS-1 and M28-CN — Measured iteration

Every census package reruns the unchanged enterprise command, records the full
funnel, durations, and blocker taxonomy, then promotes exactly one next-ranked
family into a concrete C-package. `Flow.Interview` and
`System.FeatureManagement.checkPermission` are known candidates, not guaranteed
next packages after C1.

Each C-package must be a complete typed slice with explicit non-scope,
differential evidence where behavior is claimed, and focused regression,
failure, and cost tests.

### M28-GATE

M28 can move to complete only when:

- at least 696/1,159 frozen tests satisfy the strict numerator;
- three clean local runs reproduce the same result;
- full Rust, Clippy, North Star, documentation, maintainability,
  website/editor, dependency, coverage, and relevant CLI gates pass;
- matching passes, matching failures, mismatches, and unsupported outcomes are
  published separately;
- required live Salesforce evidence is reviewed; and
- the milestone documentation is independently reviewed.

## M29, M30, and release work

M29 remains necessary even if M28 reaches 60%. A 207-second cold semantic run
does not meet the product's feedback target. M29 packages must introduce
dependency-scoped semantic work and restart-safe, versioned persistent IR
without serializing session-local source identities.

M30 then refreshes candidate-bound Salesforce, metadata/drift, profile,
security, performance, and coverage evidence before reconciling every public
claim.

An authorized public release additionally requires owner selection of a
license and either a binary-first policy or an explicit supported Rust semver
surface. Implementation agents must not make those decisions.

## Immediate action

Start only M28-Q1 using
[`MILESTONE_28_Q1_KICKOFF_PROMPT.md`](MILESTONE_28_Q1_KICKOFF_PROMPT.md).
All other packages remain blocked until Q1 is reviewed and integrated.
