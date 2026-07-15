# ADR 0008: Isolate test execution by interpreter

**Status:** Accepted
**Date:** 2026-07-14

## Context

M6 needs deterministic per-test state, continued execution after failures,
parallel scheduling, and coverage without coupling test policy to ordinary Apex
execution. The current interpreter owns static fields, object and collection
arenas, output, scopes, and runtime stacks. Sharing one interpreter between
tests would make order observable and require pervasive reset logic before the
M7 database transaction model exists.

## Decision

Discover test and setup methods from semantically validated annotations. Create
a new interpreter for every test, run that class's setup methods and test method
in the same interpreter, and discard the interpreter afterward. Schedule these
independent executions through a bounded worker pool, then sort results by
case-insensitive qualified test name.

The interpreter records only execution observations: statement spans and
true/false condition outcomes. The test runner owns production/test
classification, source mapping, aggregation, console output, and JUnit XML.
Test classes are excluded from the production coverage denominator.

## Consequences

- Static fields, reference identity, output, and exceptions cannot leak between
  tests, and scheduling does not affect report order.
- Expected failures remain ordinary structured results, so the suite continues.
- Parallel execution needs no shared mutable runtime state.
- Setup code currently reruns for every test. M7 can replace data setup work
  with a one-time transaction snapshot while retaining interpreter isolation.
- Coverage is deterministic and source-mapped, but its statement-line and
  conditional-outcome model is deliberately not labeled Salesforce-exact.
- Interpreter construction and static initialization are repeated per test;
  snapshotting or immutable runtime images remain future optimizations.
