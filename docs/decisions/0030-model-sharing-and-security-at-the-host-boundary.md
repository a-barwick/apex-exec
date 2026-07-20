# ADR 0030: Model sharing and security at the typed host boundary

## Status

Accepted. Guarded API 66.0 Salesforce evidence matches 2/2 selected dimensions,
with byte-identical credential-free replay.

## Context

Sharing, CRUD/FLS, query access modes, DML access modes, and field sanitization
all affect database outcomes, but they are not parser conveniences and cannot
be inferred from rendered diagnostics. The compiler must preserve explicit
source choices, the runtime must propagate an effective mode across calls, and
the platform host must own users, permissions, record visibility, and stored
records.

An omitted sharing declaration and explicit `inherited sharing` also have
different entry behavior in the modeled API 60.0–66.0 family. Collapsing them
in HIR would make that difference unrecoverable. Conversely, embedding profiles
or permission sets into semantic analysis would couple org configuration to
type checking and make deterministic local fixtures difficult to audit.

## Decision

1. Preserve query and DML access syntax in AST and lower it to closed typed
   enums in HIR/platform requests.
2. Preserve four class-sharing states in runtime metadata: with, without,
   inherited, and omitted. Resolve entry behavior separately from nested-call
   inheritance.
3. Keep the effective sharing mode and compatibility profile in the execution
   context. Checked call targets, not runtime names, select access behavior.
4. Put normalized users, roles, groups, object/field permissions, ownership,
   and record grants in `SecurityPolicy`, owned by `PlatformHost`.
5. Filter visible record IDs in the database request before query windowing and
   aggregation. Keep permission failures typed at the query/DML boundary.
6. Make sanitization clone and traverse runtime SObject graphs with memoization
   and explicit depth/node limits.
7. Load deterministic project fixtures from schema-versioned
   `.apex-exec/security.json`, with optional `.apex-exec/schema` definitions
   for local standard-object fixtures. Missing configuration is default-deny,
   not implicit system access.
8. Keep API 67.0 and later outside the modeled catalog. M27 implements the
   already-modeled API 60.0–66.0 defaults only.

## Consequences

- Lexer/parser, semantic, runtime, and database responsibilities remain
  separated.
- Custom hosts can supply a different security service without changing
  checked programs.
- Local tests can deterministically reproduce newly created users by using
  wildcard fixture permissions, while explicit identities still override the
  fallback.
- Unconfigured CRUD/FLS checks fail loudly, and controlled-by-parent or other
  unmodeled visibility cannot accidentally run with elevated access.
- Security fixtures and local schema become compilation/cache inputs and must
  be included in evidence review even though local schema is not deployed to
  Salesforce.
- API 67.0 default changes require a future compatibility-profile decision;
  this ADR does not authorize them.

## Rejected alternatives

- **Treat sharing modifiers as documentation only.** This recreates the silent
  system-mode approximation M27 exists to remove.
- **Store only an effective Boolean on each class.** This loses call-boundary
  inheritance and the inherited-versus-omitted entry distinction.
- **Perform permission checks in semantic analysis.** Permissions are runtime
  user/org state and would invalidate compiler phase boundaries.
- **Filter after query execution.** Limits, aggregation, ordering, and child
  results would observe inaccessible rows.
- **Return original records from `stripInaccessible`.** This leaks fields
  through aliases and mutates caller-visible state.
