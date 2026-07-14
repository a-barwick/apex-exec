# ADR 0003: Isolate platform behavior behind host interfaces

**Status:** Accepted  
**Date:** 2026-07-11

## Context

The mature runtime must provide database, schema, clock, async, callout, logging,
user-context, and compatibility behavior. Embedding these concerns directly in
AST evaluation would make the runtime difficult to test, configure, or compare
with Salesforce.

## Decision

Platform operations will be invoked through explicit Rust host interfaces. The
language runtime owns Apex evaluation; host implementations own external or
platform-like effects.

The initial interpreter may contain simple debug output internally, but future
platform features must introduce and use the host boundary rather than growing
ad hoc runtime dependencies.

## Consequences

- Tests can provide deterministic clocks, IDs, databases, users, and callouts.
- SQLite is an implementation behind the database/schema contracts rather than
  the language's definition of storage.
- Compatibility profiles can replace or decorate hosts.
- Host APIs require careful value and error boundaries.
- Some plumbing arrives earlier than a tightly coupled prototype would require.
