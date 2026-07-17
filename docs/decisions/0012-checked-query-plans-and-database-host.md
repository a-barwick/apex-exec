# 0012 — Checked query plans and database host

## Status

Accepted

## Context

M8 adds SOQL, SOSL, and DML to a runtime that already separates immutable
checked code, interpreter-owned values, normalized schema, and SQLite storage.
Treating a query as an ordinary method call or passing source text directly to
SQLite would make the runtime repeat compiler work, weaken schema diagnostics,
and couple Apex syntax to one storage adapter.

Query binds also require a boundary between static validation and runtime
values. DML must update interpreter SObject identities, especially generated
Ids, without making ordinary field assignment persist implicitly.

## Decision

- SOQL and SOSL use dedicated AST nodes and grammar routines.
- Semantic analysis resolves object, field, aggregate, grouping, ordering, and
  parent-relationship references into schema-indexed HIR plans. Bind
  expressions remain immutable Apex expressions but carry checked types.
- Runtime evaluation converts HIR indices to storage-neutral request names,
  evaluates binds once, and converts query/DML values at the platform boundary.
- `platform::database` owns filtering, ordering, limits, aggregates,
  relationship hydration, deterministic SOSL matching, generated record Ids,
  and insert/update/upsert/delete validation above the unconditional M7 storage
  contract.
- `RecordingHost` lazily owns an in-memory `SqliteStorage` per interpreter and
  records structured query and DML events. Custom hosts may reject database
  operations or provide an alternate implementation.
- Explicit DML is the only persistence operation. Successful inserts copy
  generated Ids back into the original interpreter SObject identities.
- Unsupported partial-result and recycle-bin behavior fails explicitly rather
  than approximating Salesforce semantics.

## Consequences

The parser, checker, runtime, and SQLite layers remain independently testable,
and invalid schema references or bind types fail before execution. Tests gain
isolated real SQLite behavior without sharing state across workers, while
repository/service code can run unchanged through the common query/DML path.

The initial executor intentionally supports one custom parent relationship
level and a curated query grammar. Child subqueries, full-text relevance,
result APIs, external-ID upsert, triggers, recycle-bin state, and
transaction-wide trigger rollback require later extensions at the same checked
plan and database-host boundaries.
