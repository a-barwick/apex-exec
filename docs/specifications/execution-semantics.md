# Execution Semantics

## Status

Straight-line primitive execution is implemented. Control flow, calls,
exceptions, transactions, and platform effects are planned.

## Program execution

**Implemented.** Statements execute from first to last. Execution begins only
after parsing and semantic validation succeed.

## Variables

**Implemented.** A declaration evaluates its initializer and stores the value
under the identifier's canonical case-insensitive key. Assignment replaces the
stored value. Reads of unknown variables are compile-time errors and are also
guarded defensively by the runtime.

## Debug output

**Implemented, simplified.** `System.debug(variable)` converts the primitive
value to plain text and appends one output line. The CLI prints each line to
stdout without Salesforce log metadata.

## Scope and control flow

**Planned for M2.** Blocks introduce lexical scopes. Loop-local variables must
not escape their declaring scope. `break` and `continue` target the nearest
enclosing loop. `return` unwinds the current callable once methods exist.

## Exceptions

**Planned for M4.** Runtime failures will carry an Apex exception value and a
source-mapped call stack. `finally` must execute during normal and exceptional
unwinding.

## Platform effects

**Planned.** Database access, time, IDs, randomness, async scheduling, callouts,
and user context will be provided by deterministic host interfaces. Tests must
be able to replace or control each effect.

## Transactions

**Planned for M7–M9.** DML and triggers execute inside a transaction. Unhandled
exceptions roll back transactional changes. Test methods receive isolated state.
