# ADR 0027: Structure partial DML requests and outcomes

**Status:** Accepted
**Date:** 2026-07-19

## Context

M24 adds `Database.SaveResult`, `UpsertResult`, `DeleteResult`,
`UndeleteResult`, structured row errors, `allOrNone=false`, and external-ID
upsert. The existing host boundary accepts one operation plus a vector of
SObjects and returns a vector of persisted SObjects. Runtime trigger
orchestration assumes that preflight, trigger execution, and persistence all
succeed or throw one `DmlException`.

That contract cannot preserve input positions, distinguish a created upsert
from an updated one, represent multiple typed errors for one row, or return
successful rows beside failed rows. Retrying individual records solely inside
the interpreter would also move schema matching and persistence policy across
the platform boundary and would make trigger and transaction behavior depend
on runtime string errors.

## Decision

### Carry one typed request through the data boundary

The platform DML contract uses a structured request containing:

- the requested insert, update, upsert, delete, or undelete operation;
- explicit `allOrNone` mode;
- an optional schema-indexed external-ID field for upsert; and
- ordered rows containing the original zero-based input index and the
  storage-neutral SObject value.

External-ID identity is resolved against the immutable schema before
execution. New cross-layer facts use the existing typed schema object/field
identities; source spelling remains available for diagnostics. The runtime
does not rediscover a field by parsing rendered names.

### Return one ordered outcome per input row

Every accepted request returns exactly one outcome for every input row, in
stable input order. A successful outcome contains its input index and record
Id. Upsert success additionally records whether the row was created. A failed
outcome contains its input index and one or more structured errors. Each error
has a typed status, deterministic message, and ordered schema field identities.

The public Apex result kind is selected from the requested operation:

- insert and update produce `SaveResult`;
- upsert produces `UpsertResult`;
- delete produces `DeleteResult`; and
- undelete produces `UndeleteResult`.

Runtime platform values retain these typed outcomes. Checked instance calls
implement `isSuccess`, `getId`, `getErrors`, and upsert `isCreated`.
`Database.Error` exposes `getStatusCode`, `getMessage`, and `getFields`.
Unsupported result members fail during semantic checking.

### Preserve atomic and partial transaction rules explicitly

DML statements and `Database` calls with omitted or true `allOrNone` remain
atomic. Any failed row rolls back the complete DML tree, restores caller
SObjects, and raises a catchable `DmlException`; statement syntax never
returns result objects.

For `allOrNone=false`, deterministic preflight failures become row outcomes
without entering trigger contexts. Rows that pass preflight are grouped by
their concrete insert, update, delete, or undelete operation while preserving
relative input order. Before triggers receive the complete valid concrete
group, persistence receives the possibly mutated before images, and after
triggers receive only rows that persisted successfully.

An uncaught trigger or host failure rolls back the affected concrete group and
returns a structured failure for every row in that group. Earlier successful
groups in the same partial request remain committed. The outer Apex
transaction still owns all successful rows: an exception that later escapes
the entry point rolls them back with the rest of the transaction.

Generated Ids are copied to caller SObjects only for successful rows. Failed
rows retain their exact pre-call runtime state. One source-level DML request
consumes one DML-statement limit observation regardless of row count,
insert/update partitioning, or outcome mix.

### Keep matching and validation in the platform database

Id and external-ID matching, duplicate external-ID detection, required-field
validation, generated Id allocation, recycle-bin lookup, and persistence stay
in `platform::database`. External-ID upsert scans the declared field
case-insensitively, rejects null external IDs, rejects multiple stored matches,
updates one match while retaining its Id, and inserts when no match exists.

The initial structured status catalog is closed over the M24-supported failure
families. It can grow without parsing error messages. Validation rules,
`addError`, mixed-SObject lists, access modes, and automation outside Apex
triggers remain explicit unsupported behavior.

## Consequences

- Partial and atomic DML share one request and outcome model instead of
  parallel interpreter implementations.
- Input ordering, result kinds, success Ids, created flags, status values,
  messages, and fields are independently testable below Apex allocation.
- Trigger orchestration remains in the runtime and storage rules remain in the
  platform layer.
- The host API changes incompatibly inside the currently unsupported Rust
  public-API surface. Custom hosts must implement or explicitly reject the
  structured DML capability rather than inherit a successful no-op.
- Snapshot-backed checkpoints remain the current local implementation. M24
  adds deterministic checkpoint/cost assertions, while native long-lived
  SQLite savepoints and the broader host-capability split remain S2-01
  follow-up work rather than changing observable M24 behavior.
- Validation rules, workflow/flow automation, sharing/security access modes,
  `Database.insertImmediate`, and mixed-SObject bulk lists remain outside this
  milestone.
