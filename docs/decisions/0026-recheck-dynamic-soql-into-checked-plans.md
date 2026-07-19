# ADR 0026: Recheck dynamic SOQL into checked plans

**Status:** Accepted
**Date:** 2026-07-19

## Context

M23 adds `Database.query`, `Database.countQuery`, and
`Database.getQueryLocator`. Their query text is available only at runtime, but
ADR 0012 requires query meaning to remain in the parser, semantic checker, and
platform database rather than becoming an interpreter-owned string evaluator.
Dynamic bind expressions must also preserve Apex evaluation order and produce
catchable `QueryException` failures.

## Decision

- Parse runtime query text with the same dedicated SOQL parser used by static
  query expressions. The interpreter may request parsing, but it does not
  tokenize or interpret clauses itself.
- Recheck the resulting SOQL AST against the checked program's immutable schema
  and a snapshot of visible variable types. Dynamic binds are limited to
  case-insensitive simple variable names and are recorded explicitly in HIR.
- Lower successful dynamic queries into the same schema-indexed
  `CheckedSoqlQuery` and storage-neutral `SoqlRequest` used by static SOQL.
  SQLite execution, relationship hydration, date evaluation, ordering,
  aggregation, limits, and query tracing therefore have one implementation.
- Evaluate the query-text Apex expression exactly once. Convert parse,
  semantic, result-shape, and platform failures at the call boundary into
  catchable `QueryException` values.
- Represent `Database.QueryLocator` as an opaque runtime platform value that
  owns a checked record-list snapshot. Batch execution consumes that snapshot
  through the existing deterministic async scope pipeline.
- Keep recursive query shapes bounded: parent paths are limited to five
  relationship levels, nested child subqueries are rejected, and child
  hydration scans once per selected child query rather than once per parent.

## Consequences

- Static and dynamic SOQL share syntax, type rules, checked plans, execution,
  traces, and explicit unsupported behavior; there is no interpreter string
  shortcut.
- Runtime query failures are later-phase `QueryException` results even when the
  equivalent static query would fail compilation.
- Only simple named binds are supported in dynamic text. `queryWithBinds`,
  access-level arguments, aggregate-returning `Database.query`, nested child
  subqueries, and polymorphic `TYPEOF` remain explicit future work.
- Query text is parsed on every dynamic call. Plan caching can be added later
  above the checked-plan boundary if profiling justifies a bounded,
  schema-keyed cache.
