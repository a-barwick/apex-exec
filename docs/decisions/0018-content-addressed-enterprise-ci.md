# ADR 0018: Seal CI runs behind content-addressed manifests

**Status:** Accepted
**Date:** 2026-07-17

## Context

Enterprise CI needs reusable work across machines, deterministic replay,
dependency-based test selection, and auditable gates. Persisting Rust AST or
HIR directly would couple caches to internal layouts and compiler source
identities. Selecting tests from filenames alone would miss transitive Apex
dependencies, while treating metadata and trigger effects as local would
silently under-test changes.

## Decision

Use a versioned JSON manifest as the hermetic boundary. It records the exact
Apex Exec version, project configuration, every package-root file and SHA-256,
changed paths, shard, reports, and policies. The serialized manifest plus the
effective shard is the content address for a whole-run artifact. Cache replay
is allowed only after the live input inventory matches exactly.

Keep selection above compilation but derive it from the compiler-owned
dependency graph. Reverse transitive closure selects tests for known changed
`.cls` files. Metadata, triggers, deleted sources, and unknown paths select all
tests. Stable sorted qualified names are divided by index modulo shard count,
so distributed workers require no coordination.

Cache normalized outcomes and observations rather than internal compiler data.
Standard reports are rendered from the artifact on both execution and replay.
Policy is part of the cache identity, while a separate result digest and atomic
rename protect artifact integrity and publication.

## Consequences

- Cache artifacts are portable across workers running the same Apex Exec
  version and do not freeze private AST/HIR serialization.
- Exact input verification detects drift before a stale result can pass.
- Distributed shards are deterministic and independently replayable.
- Conservative fallback may run more tests, but cannot silently omit work when
  dependency effects are unknown.
- A cache miss still performs whole-project semantic linking. A future
  serializable lowered IR can live behind the same content address.
- Coverage and duration policies describe one artifact/shard; providers remain
  responsible for aggregating standard shard reports when a repository wants a
  whole-suite gate.
