# ADR 0015: Drain asynchronous Apex explicitly and deterministically

**Status:** Accepted  
**Date:** 2026-07-17

## Context

M11 introduces Queueable, future, batch, scheduled, and platform-event work.
A wall-clock scheduler or background worker would make local tests race-prone,
hide pending work at process exit, and break the project's deterministic
execution principle. Async payloads also contain interpreter-owned object and
collection identities, while database effects need the existing platform
transaction boundary.

## Decision

- Semantic analysis validates built-in async interface contracts and records
  their method targets in HIR.
- One interpreter-owned FIFO stores pending jobs. Submission allocates a stable
  Salesforce-shaped ID, snapshots the reachable payload graph, and records its
  parent job when chained.
- Queued work runs only at an explicit drain point. M11 uses `Test.stopTest`;
  ordinary execution never creates worker threads or drains implicitly.
- Every job runs under a nested platform transaction checkpoint. Failure rolls
  back the job, emits a failed lifecycle event, and stops the drain.
- Batch execution expands one queued job into deterministic `start`, ordered
  List chunks, `execute`, and `finish` calls. Platform events deliver directly
  to checked after-insert triggers without ordinary record persistence.
- A fixed drain bound rejects runaway chaining explicitly.

## Consequences

- Async Apex tests are repeatable and cannot race an invisible scheduler.
- Payload mutation after enqueue does not alter submitted work.
- The interpreter can reuse checked targets, trigger dispatch, coverage, and
  the platform database without leaking runtime values into the host API.
- This is a simplified local profile: cron is not clock-evaluated, platform
  events are not retained, and jobs do not yet receive a fully separate static
  execution store.
