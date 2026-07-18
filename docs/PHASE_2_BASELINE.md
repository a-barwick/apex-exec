# Phase 2 Baseline Audit

**Audited:** 2026-07-17

**Phase 1 commit:** `bc92a9e` (`Merge milestone 15 hybrid deployment confidence`)

This audit records the evidence used to define Phase 2 in `ROADMAP.md`. It
reviews the shipped compiler/runtime, the executable tests, the M15 hybrid
adapter, and the project documentation. `docs/COMPATIBILITY.md` remains the
contract for shipped behavior; this document explains why the next milestones
exist.

## Executive conclusion

The proposed post-roadmap summary is confirmed, with two qualifications:

1. The North Star number is exactly **1 of 14 passing**, but those indicators
   measure only lexing and parsing.
2. M15 is implementation-complete and its branch is merged and pushed, but its
   current recorded-snapshot format is not strong enough to serve as durable
   evidence for a different release candidate.

After refreshing `origin`, `main` and `origin/main` both resolve to
`bc92a9ee2e56016d7232274ce108cfa0db6373be`. The required ordinary test suite
passes 276 tests; the 14 North Star goals remain ignored in that run.

## Claim-by-claim review

| Claim | Repository evidence | Conclusion |
|---|---|---|
| 13 of 14 North Star indicators fail | `cargo test --test north_star -- --ignored` reports 1 pass and 13 failures against the pinned corpus | Confirmed |
| The leading blockers are safe navigation, null coalescing, ternary, bitwise operators, and `instanceof` | Those five feature families account for all seven current first diagnostics | Confirmed |
| The authenticated M15 path is simulated but has not run against a supplied org | The authenticated integration creates a shell-script `sf` and returns fixed JSON; no live Salesforce or validation snapshot is tracked | Confirmed |
| The 60–80% enterprise target is unmeasured | The repository has small milestone examples and a syntax-only North Star corpus, but no representative project, frozen test denominator, or benchmark report | Confirmed |
| M15 metadata breadth is curated | `src/hybrid.rs` has 28 static component-type mappings and omits paths its classifier does not recognize | Confirmed |
| Important language/platform gaps remain | The AST/checker/runtime and compatibility docs lack nested types, enums, broader SOQL, partial DML results, sharing/security, and API-version profiles | Confirmed |
| Changed projects still relink semantically as a whole | `ProjectCompiler` reuses parsed units and computes invalidation, then calls `check_with_schema` on the merged project after any change | Confirmed |
| Persistent typed/lowered-IR caching is future work | CI caches normalized outcomes, not AST/HIR/IR, and `SourceId` is session-local | Confirmed |

## North Star reproduction

The corpus pin test passes at seven files, 14,740 lines, and 614,536 bytes.

| Fixture | Highest completed stage | Current first diagnostic |
|---|---|---|
| `SOQL.cls` | Source only | unexpected `?` at line 1149 (`?.`) |
| `Logger.cls` | Source only | unexpected `?` at line 22 (`?.`) |
| `Rollup.cls` | Source only | unexpected `?` at line 61 (`??`) |
| `RollupService.cls` | Source only | unexpected `?` at line 119 (ternary) |
| `fflib_SObjectDomain.cls` | Source only | unexpected `&` at line 66 |
| `Puff.cls` | Source only | unexpected `|` at line 63 (`|=`) |
| `JSONParse.cls` | Lexed | expected `;` at line 208 (`instanceof`) |

The five first-blocker families are not the complete grammar backlog. Once they
are bypassed, the corpus exposes arbitrary annotations, nested classes and
interfaces, enums, static initializer blocks, class literals, `switch`/`when`,
uninitialized and multi-declarator locals, constructor delegation, arbitrary
generic type references, `transient`, `Long` literals, and compound
assignments. Every fixture contains or reaches a nested declaration.

The Phase 2 roadmap therefore uses three distinct checkpoints:

- complete expression slices with semantic/runtime behavior;
- seven of seven lexer indicators passing; and
- fourteen of fourteen lexer/parser indicators passing without altering the
  pinned sources.

Passing the final checkpoint is 100% of this fixed syntax indicator set. It is
not 100% Apex language, runtime, platform, or Salesforce compatibility.

## Salesforce evidence review

The M15 adapter implements the expected non-interactive sequence: inspect an
existing alias without verbose credentials, retrieve scoped metadata into a
temporary directory, run `sf project deploy start --dry-run`, normalize tests
and deployment failures, and produce a readiness decision. Unit and integration
tests cover success, drift, differential failure, replay, output, exit status,
and command construction.

The remaining evidence gap is real transport and org behavior:

- the authenticated integration test copies the local `force-app` tree into
  the fake retrieval directory;
- no tracked snapshot or readiness report records a real validation run;
- actual retrieve formatting/default injection and org availability have not
  been observed; and
- `docs/COMPATIBILITY.md` correctly makes no **Exact** claim.

The version-1 `ValidationSnapshot` also lacks a manifest/candidate digest,
affected request digest, capture time, API version, and tool/CLI provenance.
Replay recomputes local CI but excludes code and directly changed components
from drift. A stale snapshot can therefore approve changed code when its
selected test names and outcomes happen to match. M17 closes this before live
evidence is treated as durable release evidence.

## Enterprise evidence review

The North Star sources are intentionally not an SFDX execution project or a
conformance suite. The milestone examples prove focused features, not
enterprise representativeness. There is currently no:

- approved representative-project selection rubric;
- immutable real-project candidate;
- Salesforce-derived ordinary-test denominator;
- per-stage compatibility funnel;
- deterministic rerun evidence; or
- measured local execution/outcome-agreement percentage.

M22 first freezes and reports an honest baseline. M28 improves that same
denominator to at least 60%, with 80% as the stretch target; M30 revalidates it
as part of the final evidence gate.

## Metadata review

The M15 classifier recognizes 28 hard-coded Metadata API identities: two code
types, three schema types, and 23 configuration types. This is a useful common
SFDX inventory, not broad Metadata API accounting. Unknown changed files cause
conservative all-component validation, but unknown unchanged metadata is absent
from inventory and drift reports.

A local, environment-specific comparison found 449 unique parent/child type
names in the installed `@salesforce/source-deploy-retrieve` 10.4.0 registry.
That number is not a durable product denominator; Phase 2 must pin an
API-versioned registry before publishing coverage.

The audit also found that `metadata_member_name` truncates multi-part Custom
Metadata filenames. For example, `Feature.Flag.md-meta.xml` is classified with
member name `Feature` rather than `Feature.Flag`. M26 covers both this
correctness issue and the larger requirement that every package-root file
receive an explicit inventory disposition.

## Documentation review

The authoritative vision, roadmap, status, architecture, and compatibility
documents consistently describe the major limitations. The audit found these
additional maintenance issues:

- the type-system specification still listed implemented `Decimal` support as
  planned;
- the execution-semantics specification still described query/DML/trigger
  integration as future M8–M9 work;
- the development guide used an untracked validation-snapshot filename without
  explaining how it is produced; and
- the public website still describes M7 as active and six milestones complete.

The first three are corrected with the Phase 2 roadmap update. The public
website remains historical rather than being treated as current evidence;
reconciliation is an explicit M30 release-gate item.

## Reproduction commands

```bash
git fetch origin --prune
git rev-parse main origin/main
cargo test
cargo test --test north_star reports_current_north_star_progress -- --nocapture
cargo test --test north_star -- --ignored
```

The ignored North Star command is expected to fail at this baseline. A staging
org was not supplied, so no live Salesforce command was attempted.
