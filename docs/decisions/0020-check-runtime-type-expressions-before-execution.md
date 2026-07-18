# ADR 0020: Check runtime-type expressions before execution

**Status:** Accepted
**Date:** 2026-07-18

## Context

M16 adds two expression forms with different kinds of static reasoning.
Ternary needs a common result type even though only one arm executes.
`instanceof` has a type, rather than a value expression, on its right and Apex
rejects both impossible alternatives and tests that are statically always
true. Reusing assignment compatibility for runtime identity would incorrectly
turn conversions such as Integer-to-Decimal promotion into instance
relationships. Re-resolving relationships from runtime values would also
duplicate the checker and weaken the immutable AST/HIR boundary.

Coverage adds a related ownership question: ternary is an expression nested
inside arbitrary statements, while the existing coverage denominator was
collected from statement conditions.

## Decision

Represent ternary and `instanceof` as dedicated immutable AST nodes.
Semantic checking records their Boolean or joined result type in the existing
HIR expression-type table. Ternary joins use the supported assignment/subtype
relation, while `instanceof` uses a separate runtime-subtype and overlap
relation over supported concrete, collection, SObject, exception, class, and
interface types.

Execution evaluates the ternary condition once and only the selected arm.
`instanceof` evaluates its value once, treats null as false in the current
profile, and compares the resulting execution-store identity with the already
checked target. The runtime does not repeat viability checking.

Collect ternary branch candidates through the shared immutable AST visitor and
record their true/false outcomes through the existing execution trace. Lexing
recognizes the `?` punctuation required by ternary; safe navigation and null
coalescing remain parser errors until M18.

## Consequences

- Parsed syntax remains independent of semantic analysis and reusable by the
  incremental project compiler.
- Runtime behavior cannot reinterpret numeric conversions or choose a
  different type relationship from the checker.
- Both ternary arms receive static diagnostics while the unselected arm has no
  runtime side effects or exceptions.
- Generic collection runtime checks remain invariant and preserve their stored
  element/key/value types.
- Ternary contributes two production coverage branches wherever it is nested.
- Current-profile null behavior is explicit. Historical pre-API-32 behavior
  and later platform generic relationships require M25 version profiles or
  broader type support rather than conditionals scattered through execution.
