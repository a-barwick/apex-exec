# ADR 0014: Keep curated platform APIs checked and host-backed

**Status:** Accepted  
**Date:** 2026-07-17

## Context

M10 adds value types, utility APIs, deterministic context, limits, and HTTP
callouts. Implementing those calls through runtime name lookup would weaken the
existing HIR invariant and make unsupported Salesforce surface ambiguous.
Embedding wall-clock, user, random, or network behavior in the interpreter
would also make tests nondeterministic and couple language execution to one
environment.

## Decision

- Every supported platform call resolves during semantic analysis to a closed
  `PlatformIntrinsic` HIR target with statically checked arity, argument types,
  and return type.
- Immutable scalar values live directly in runtime `Value`; stateful platform
  objects use execution-store identities so test isolation matches collections
  and user objects.
- Clock, pseudo-randomness, user context, limit observations, and HTTP transport
  cross `PlatformHost`.
- `RecordingHost` uses fixed deterministic defaults, a stable pseudo-random
  sequence, queued HTTP responses, and captured HTTP requests. It never performs
  a live callout.
- Unsupported calls on recognized platform owners are compile diagnostics that
  name both the API and the `m10-common` compatibility profile.

## Consequences

- The interpreter never repeats platform overload or owner resolution.
- Tests can configure time, users, and responses without local-only Apex
  syntax, shared global state, or network access.
- Additional profiles can change host policy or expose new checked intrinsics
  without scattering name checks through execution.
- The curated surface is intentionally finite. Typed JSON reflection,
  `HttpCalloutMock` source-level registration, full describe metadata,
  locale/time-zone formatting, and live HTTP remain explicit future work.
