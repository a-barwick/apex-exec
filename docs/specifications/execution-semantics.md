# Execution Semantics

## Status

Primitive expressions, lexical scopes, and control flow are implemented. Calls,
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

## Expressions

**Implemented.** Arithmetic uses checked `i64` operations and reports division,
remainder, and overflow failures. `&&` and `||` short-circuit. Assignment is
right-associative. Prefix and postfix increment/decrement mutate `Integer`
variables while returning the Apex-shaped new or prior value respectively.

## Scope and control flow

**Implemented.** Blocks introduce lexical scopes. Loop-local variables do not
escape their declaring scope. `if`/`else`, traditional `for`, `while`, and
`do`/`while` execute directly over checked Boolean conditions. `break` and
`continue` target the nearest enclosing loop. A value-less `return` terminates
anonymous execution; method return values are planned for M4.

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
