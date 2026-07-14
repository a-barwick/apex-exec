# ADR 0005: Record checked calls and unify abrupt runtime flow

**Status:** Accepted
**Date:** 2026-07-13

The M4 abrupt-flow decision remains active. The transitional parsed-AST call
cell was superseded by the typed HIR side tables in ADR 0006.

## Context

M4 adds overloaded user-defined methods, recursion, exceptions, and `finally`
before the typed HIR planned for project and class compilation. Re-resolving an
overload from runtime values would duplicate semantic reasoning and could select
a different method after a value widened to `Object`. Simply pushing a method
scope on the current environment would also let a callee read caller locals,
which Apex does not allow.

Exception handling creates a second control-flow pressure. `finally` must run
for normal completion, method returns, loop control, and exceptions, and an
abrupt completion inside `finally` must replace the pending outcome. Runtime
faults must be catchable without turning compiler errors or impossible checked
states into Apex behavior.

## Decision

The semantic checker performs two method passes. It first collects every
case-insensitive signature, then checks bodies and writes the chosen method
index into an interior-mutable cell on each bare call AST node. The interpreter
executes that checked index directly. Top-level method declarations are an
interim single-file compilation model until M5 provides ordinary class units
and a typed HIR.

Every method invocation temporarily replaces the lexical-scope stack with an
isolated parameter scope. The interpreter-owned collection arena and output
remain shared, and the caller's scopes are restored on every result.

Statement execution represents normal, break, continue, and return completion
explicitly; typed runtime exceptions use the error path. `try`/`catch` and
`finally` combine those outcomes at one boundary. Runtime language faults carry
an exception type, message, origin span, and call frames in the public
diagnostic envelope. Compile diagnostics have no exception type and therefore
cannot be caught. The interpreter tracks active method calls and snapshots the
chain when an exception first reaches a handler or escapes a method. The leaf
frame uses the origin; each caller frame uses the checked nested call span.

## Consequences

- Forward calls and recursion do not depend on declaration order.
- Runtime dispatch cannot diverge from static overload resolution.
- Callees cannot accidentally observe anonymous or caller-local variables.
- One unwinding model gives `finally` consistent behavior for returns, loop
  control, and exceptions.
- Unhandled failures render useful source stacks while caught values retain
  their type, message, and existing frames.
- The parsed AST currently contains semantic state through a call-ID cell.
  M5 must move that state into the planned typed HIR instead of extending this
  transitional pattern.
- Top-level methods and the minimal `Object` cast carrier are declared
  compatibility limitations, not permanent source-surface promises.
