# Compatibility

This document states the currently supported Apex surface. It is a product
contract, not a claim of complete Salesforce compatibility.

## Fidelity levels

| Level | Meaning |
|---|---|
| Exact | Differentially verified against Salesforce for the documented cases |
| Compatible | Intended to match common observable Apex behavior |
| Simplified | Preserves the useful shape but intentionally omits semantics |
| Stubbed | Recognized but reports an explicit unimplemented error |
| Unsupported | Rejected explicitly during lexing, parsing, or checking |
| Planned | Not implemented yet |

No behavior is currently labeled **Exact** because the Salesforce differential
conformance harness is a later milestone.

## Language surface

| Feature | Parse | Check | Execute | Fidelity | Notes |
|---|---:|---:|---:|---|---|
| `String` | Yes | Yes | Yes | Simplified | Single-quoted literals, common escapes, and the documented M3 method subset |
| `Boolean` | Yes | Yes | Yes | Compatible | `true` and `false` are case-insensitive |
| `Integer` | Yes | Yes | Yes | Simplified | Stored as Rust `i64`; Apex range/overflow pending |
| Explicit initialization | Yes | Yes | Yes | Compatible | Uninitialized declarations are rejected |
| Assignment | Yes | Yes | Yes | Compatible | Invariant supported types or `null`; chained assignment is right-associative |
| Variable references | Yes | Yes | Yes | Compatible | Checked before execution |
| Case-insensitive names | Yes | Yes | Yes | Compatible | Original spelling is preserved |
| Line/block comments | Yes | N/A | N/A | Compatible | Unterminated block comments are errors |
| `System.debug(expression)` | Yes | Yes | Yes | Simplified | Built-in method call with plain stdout and no Salesforce log metadata |
| Integer arithmetic | Yes | Yes | Yes | Simplified | `+`, `-`, `*`, `/`, `%`, unary signs; checked `i64` runtime behavior |
| Comparison and equality | Yes | Yes | Yes | Compatible | Integer ordering; case-insensitive String `==`; same-type collection and null equality |
| Boolean operators | Yes | Yes | Yes | Compatible | Short-circuit `&&`, <code>&#124;&#124;</code>, and unary `!` |
| String concatenation | Yes | Yes | Yes | Simplified | `+` converts every supported non-Void value; collection text uses deterministic local formatting |
| Increment/decrement | Yes | Yes | Yes | Compatible | Prefix and postfix forms on `Integer` variables and List indexes |
| Nested blocks and scopes | Yes | Yes | Yes | Compatible | Shadowing and lookup are case-insensitive |
| Conditional statements | Yes | Yes | Yes | Compatible | `if` and `if`/`else` |
| Loops and loop control | Yes | Yes | Yes | Compatible | Traditional and enhanced `for`, `while`, `do`/`while`, `break`, and `continue` |
| Anonymous `return` | Yes | Yes | Yes | Simplified | Value-less return terminates anonymous execution |
| `null` | Yes | Yes | Yes | Simplified | Assignable to every supported value type; selected runtime null behavior implemented |
| `List<T>` | Yes | Yes | Yes | Compatible | Recursive invariant type; ordered, indexed, mutable reference value |
| `Set<T>` | Yes | Yes | Yes | Simplified | Unique mutable reference value with deterministic local insertion order |
| `Map<K,V>` | Yes | Yes | Yes | Simplified | Deterministic local insertion order; `keySet()` is a snapshot |
| Array syntax | Yes | Yes | Yes | Simplified | One-dimensional `T[]` alias for `List<T>`; sized construction supported |
| Collection literals | Yes | Yes | Yes | Compatible | List/Set elements and Map `key => value` entries |
| Collection indexing | Yes | Yes | Yes | Compatible | List/array reads and writes; Set/Map indexing is rejected |
| Built-in method calls | Yes | Yes | Yes | Compatible | Fixed case-insensitive M3 collection, String, Math, and System surface |
| User-defined methods | No | No | No | Planned | M4 |
| Exceptions | No | No | No | Planned | M4 |
| Classes/interfaces | No | No | No | Planned | M5 |
| Inheritance/access modifiers | No | No | No | Planned | M5 |
| Properties/annotations | No | No | No | Planned | M5–M6 |

## M3 built-in method surface

Method names are case-insensitive. Supported overloads still receive static
arity and argument-type checking.

- `List<T>`: `add`, `addAll`, `clear`, `clone`, `contains`, `get`, `indexOf`,
  `isEmpty`, `remove`, `set`, `size`, and scalar `sort`. `add` accepts either a
  value or an index and value. `sort` places null before non-null values.
- `Set<T>`: `add`, `addAll`, `clear`, `clone`, `contains`, `containsAll`,
  `isEmpty`, `remove`, `removeAll`, `retainAll`, and `size`.
- `Map<K,V>`: `clear`, `clone`, `containsKey`, `get`, `isEmpty`, `keySet`,
  `put`, `putAll`, `remove`, `size`, and `values`.
- Static `String`: `valueOf`, `join`, `isBlank`, `isNotBlank`, `isEmpty`, and
  `isNotEmpty`.
- Instance `String`: `length`, `contains`, `startsWith`, `endsWith`, `equals`,
  `equalsIgnoreCase`, `indexOf`, one- and two-argument `substring`, `trim`,
  `toLowerCase`, `toUpperCase`, and literal `replace`.
- Integer-backed `Math`: `abs`, `max`, `min`, and `mod`.
- `System`: `debug`.

String `length`, `indexOf`, and `substring` use UTF-16 code-unit positions for
ordinary Unicode scalar strings. A substring boundary that would split a
surrogate pair is rejected explicitly because Rust strings cannot contain the
resulting unpaired surrogate. This limitation, along with Rust-backed Unicode
case and whitespace behavior, keeps the String surface at **Simplified**
fidelity.

## Collection runtime fidelity

Collection assignment aliases the same mutable reference. Copy constructors
and `clone()` create independent shallow copies. List order is preserved. Set
iteration and Map-derived order are deterministic insertion order locally for
repeatability; this does not attempt to reproduce Salesforce's deterministic
internal ordering. `Map.keySet()` returns a snapshot rather than a backed view.
Direct enhanced iteration over a Map is rejected; callers iterate `keySet()` or
`values()` instead.

## Platform surface

| Feature | Status | Target milestone |
|---|---|---|
| SFDX project loading | Planned | M5 |
| Apex unit tests | Planned | M6 |
| SObject schema | Planned | M7 |
| SQLite storage | Planned | M7 |
| DML | Planned | M8 |
| SOQL | Planned | M8 |
| SOSL | Planned | M8 |
| Triggers | Planned | M9 |
| Common platform APIs | Planned | M10 |
| Async Apex | Deferred | M11 |
| Governor limits | Deferred | Post-core compatibility profile |
| Sharing/security behavior | Deferred | Post-core compatibility profile |
| API-version differences | Deferred | Post-core compatibility profile |
| Runtime isolation for untrusted code | Out of scope | None |

## Compiler behavior

- Unknown characters and invalid strings fail lexing.
- Invalid or unsupported syntax fails parsing.
- Unknown variables, generic mismatches, invalid iteration/indexing, and
  invalid built-in calls fail semantic checking.
- Unsupported built-in methods are rejected explicitly rather than silently
  approximated.
- Diagnostics are generated by Apex Exec and are not required to reproduce
  Salesforce's exact wording.
- `tests/north_star/` contains pinned real-world complexity indicators. Their
  lexer/parser goal tests measure progress only; they are not compatibility or
  execution claims until promoted into the supported surface above.

## Updating this document

Any pull request or task that changes observable language or platform support
must update the relevant row. Promote behavior to **Exact** only when a fixture
has been run against Salesforce and the supported cases are recorded.
