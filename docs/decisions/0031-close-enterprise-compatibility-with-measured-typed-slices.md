# ADR 0031: Close enterprise compatibility with measured typed slices

Status: Accepted

Date: 2026-07-19

## Context

M28 must raise the strict compatibility numerator of the frozen 1,159-test M22
project without modifying its source, denominator, or sealed Salesforce
outcomes. A single common parse or check failure can affect every test, while
accepting syntax as an unchecked no-op would violate the compiler's explicit
compatibility contract.

Several high-impact blockers also cross compiler and runtime boundaries:
SObject switch patterns need schema identity, Comparable sorting needs checked
method dispatch, roll-up summaries need lossless metadata plus computed query
values, and Stateful batches need transaction-boundary object snapshots.

## Decision

M28 closes blockers as bounded typed slices in descending impacted-test order.
Each slice:

- preserves source spelling and spans in the AST;
- records resolved identities or contracts in HIR instead of repeating
  resolution at runtime;
- keeps schema normalization independent from SQLite storage;
- adds positive, negative, phase, side-effect, and integration tests as
  applicable; and
- refreshes the unchanged per-test enterprise funnel before the next slice is
  selected.

Computed summary fields retain their complete normalized definitions, are
read-only at assignment and persistence boundaries, and share one child-object
scan per query. Exact equality uses constant-time value or arena identity
checks rather than recursive equality. Comparable dispatch is checked once and
uses stable iterative merge sorting. Stateful batch receivers persist only
when the marker contract is present; non-stateful stages clone the original
enqueue snapshot.

No M28 slice may change the benchmark manifest, Salesforce capture, source
files, exclusions, or strict numerator definition.

## Consequences

Compatibility movement remains attributable to concrete behavior rather than
parser permissiveness. Runtime execution consumes checked identities and
contracts, and expensive query/sort paths have deterministic cost assertions.

The approach requires repeated full-corpus runs, which become slower as global
blockers are removed. Some Salesforce surfaces remain explicit limitations,
including heterogeneous Object sorting, broad metadata describe behavior, and
full asynchronous transaction isolation. M28 cannot be declared complete
until the frozen strict threshold and all release verification criteria pass.
