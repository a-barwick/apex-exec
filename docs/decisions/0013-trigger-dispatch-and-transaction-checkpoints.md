# ADR 0013: Orchestrate triggers with nested database checkpoints

**Status:** Accepted  
**Date:** 2026-07-17

## Context

M9 must run typed Apex trigger bodies around bulk DML, allow triggers to issue
recursive DML, restore deleted records, and roll back the correct database
scope when a trigger fails. The runtime owns Apex values and control flow,
while the platform database owns stored and recycled records. Moving either
responsibility across that boundary would couple SQLite to Apex execution or
make the interpreter duplicate persistence rules.

A failed DML statement that is caught by Apex rolls back that DML call,
including recursive work it started, without undoing earlier successful DML.
An exception that escapes the entry point rolls back the complete Apex
transaction.

## Decision

- The platform database preflights each bulk request into ordered records with
  the concrete insert, update, delete, or undelete event plus old and new
  SObject images. Mixed upserts are partitioned by their concrete event.
- The interpreter creates typed `Trigger.new`, `Trigger.old`, map, Boolean, and
  size contexts from that preflight, executes matching trigger bodies in
  deterministic project source order, and sends the possibly mutated before
  image back through the ordinary DML host boundary.
- Old images and every after-trigger new image are runtime read-only values.
- The host exposes nested snapshot checkpoints. Every DML call owns one
  checkpoint, and every public execution/test entry point owns an outer
  checkpoint. Committing discards the latest snapshot; rollback restores active
  records, recycled records, and deterministic ID sequences.
- Delete moves the full stored record into a database-owned recycle bin.
  Undelete restores the same identity and lets supported before-undelete
  trigger changes participate in the restored record.
- Trigger enter/exit and DML events share one ordered host timeline. Recursive
  trigger depth is explicit and bounded.

## Consequences

- Caught trigger failures are DML-atomic, while uncaught failures restore the
  complete local transaction.
- Recursive DML naturally nests checkpoints and timeline events without
  teaching SQLite about Apex trigger declarations.
- Before-trigger field mutations use the same interpreter SObject identities
  as the caller and therefore reach persistence without copying semantics.
- Snapshots currently copy all active records plus recycle-bin state. This is
  deterministic and sufficient for the local milestone, but a persistent or
  large-data host should replace it with native long-lived transactions or
  savepoints behind the same host contract.
- Validation rules, workflow/process automation, partial DML results, and
  Salesforce-exact trigger ordering beyond the supported trigger phase remain
  explicit future compatibility work.
