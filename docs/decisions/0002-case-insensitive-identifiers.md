# ADR 0002: Canonicalize identifiers for case-insensitive lookup

**Status:** Accepted  
**Date:** 2026-07-10

## Context

Apex identifiers are case-insensitive, while useful diagnostics and AST output
must retain the spelling written by the developer.

## Decision

Every identifier stores its original spelling, a canonical lowercase lookup
key, and its source span. Symbol tables and runtime environments use the
canonical key. Diagnostics and source-oriented output use the original spelling.

Keywords and built-in names are compared case-insensitively.

## Consequences

- `message`, `Message`, and `MESSAGE` resolve to the same declaration.
- Diagnostics can quote exactly what appeared in source.
- Every new symbol table must use the canonical key consistently.
- Unicode case semantics are currently deferred; supported identifiers are
  ASCII-shaped and canonicalized with ASCII lowercase conversion.
