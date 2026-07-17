# ADR 0010: Keep normalized schema independent from record storage

**Status:** Accepted
**Date:** 2026-07-16

## Context

M7 must import SFDX metadata, type-check SObject access, generate SQLite
migrations, and isolate test data. A SQLite-shaped schema model would force the
compiler and metadata importer to understand physical storage details.
Conversely, persisting interpreter `Value` nodes would make a storage adapter
depend on execution internals and blur the later DML/trigger boundary.

## Decision

The platform layer has two independent contracts:

- `schema` owns a case-insensitive, normalized `SchemaCatalog` plus the
  read-only `SchemaProvider` interface.
- `storage` owns storage-neutral record IDs, field values, records, and
  transaction interfaces.

SQLite will adapt these contracts rather than define them. Apex DML validation,
trigger dispatch, and result semantics will sit above unconditional
transactional record operations. Neither contract depends on parser AST nodes,
HIR tables, runtime values, or a database crate. The borrowing transaction
contract uses static dispatch; a later runtime host may add a dynamically
erased adapter rather than forcing object safety into the storage core.

## Consequences

- Metadata normalization can be tested without SQLite and reused by the
  checker, runtime, and migration generator.
- In-memory and connection-backed stores can implement the same transaction
  boundary, including isolated test fixtures.
- Record values must be converted explicitly at the runtime/platform boundary.
- The first contracts intentionally expose only the value kinds needed by the
  current roadmap; new SObject field kinds require deliberate schema and
  storage extensions.
- Savepoints, Salesforce-shaped ID generation, DML errors, and trigger
  ordering remain higher-level M7–M9 work.
