# ADR 0011: Use an additive SQLite schema registry

**Status:** Accepted
**Date:** 2026-07-16

## Context

M7 must turn normalized SFDX metadata into durable SQLite tables without making
SQLite the source of truth for compiler types. Rebuilding tables on every
metadata change risks silent coercion or data loss, while relying only on
SQLite introspection loses relationship targets and normalized nullability.
Tests also need fast record reset without paying schema creation cost.

## Decision

SQLite owns two internal registry tables for normalized object and field
definitions and one physical record table per object. Migrations compare the
requested catalog with the registry and add new objects or fields in a single
transaction. Existing key prefixes and field definitions must match exactly;
incompatible changes fail explicitly instead of rebuilding or coercing data.

Physical record operations remain unconditional storage primitives. Required
field checks protect stored records, but insert-versus-update policy, DML
results, triggers, and Apex transaction behavior remain above this adapter.
Named savepoints, fixture replacement, and record-only reset reuse the migrated
schema.

## Consequences

- Metadata-only additions preserve existing local data.
- The registry retains normalized details that SQLite column introspection
  cannot represent faithfully.
- Unsupported destructive schema changes are visible and deterministic.
- Renames, removals, type conversions, and table compaction require a future
  explicit migration policy.
- Test isolation can delete records or roll back a savepoint without rebuilding
  schema.
