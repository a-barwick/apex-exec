# ADR 0004: Store collections by interpreter-owned identity

**Status:** Accepted
**Date:** 2026-07-13

## Context

Apex collections are mutable reference values. Assigning a list, set, or map to
another variable aliases the same collection, while a collection copy
constructor creates a distinct shallow copy. Storing a Rust `Vec` directly in a
runtime value would copy or move collection contents and lose this observable
identity.

Sets and maps can also contain collection values. Requiring Rust `Hash` keys
would impose host-language restrictions that Apex does not have, and hash-table
iteration would make local execution less predictable.

## Decision

The interpreter owns an arena of collections. Runtime values carry a small
collection identifier, and copying a value preserves that identifier. Copy
constructors and `clone()` allocate a new arena entry containing shallow copies
of the source elements or entries.

Lists, sets, and maps use vector-backed storage. Sets enforce uniqueness by
structural Apex-value equality, and maps find and replace keys with the same
equality operation. Their local iteration order is deterministic insertion
order; it is not presented as Salesforce's internal hash order.

The arena tracks active list and set iteration so mutation through any alias can
be rejected explicitly. `Map.keySet()` initially returns a deterministic
snapshot rather than a backed view; this fidelity difference is declared in the
compatibility contract.

## Consequences

- Collection assignment has Apex-shaped reference semantics without pervasive
  `Rc<RefCell<_>>` borrowing concerns.
- Self-copying operations such as `values.addAll(values)` can snapshot their
  input before mutating safely.
- Nested collections and collection-valued map keys do not need artificial
  hashability restrictions.
- Set and map lookup is linear. This is acceptable for the current language
  milestones but may need an indexed representation at project scale.
- The interpreter owns structural equality, display, and collection lifecycle
  behavior. A later VM or lowered runtime must preserve the same identities.
- A fully backed `Map.keySet()` view remains compatibility work rather than an
  accidental promise of snapshot equivalence.
