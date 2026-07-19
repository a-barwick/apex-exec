# ADR 0023: Stage typed compiler and runtime substrate before M19

**Status:** Accepted
**Date:** 2026-07-19

## Context

M19 adds `Long`, bitwise and shift operators, and compound assignment across
locals, indexes, class members, and SObject fields. The current implementation
cannot add that slice safely without multiplying existing representation and
runtime debt:

- parsed type syntax is converted directly into semantic `TypeName` values,
  while declaration lookahead separately reimplements part of the type grammar;
- executable HIR targets use raw `usize` positions for top-level methods,
  classes, members, schema objects, and fields;
- assignment and increment/decrement each evaluate and mutate assignable
  expressions through separate target-specific paths;
- numeric coercion, arithmetic, overflow, comparison, and mutation policy are
  distributed across semantic checking and runtime matches;
- runtime exception names and compatibility-profile names are carried as
  strings at boundaries that later milestones must extend; and
- intrinsic signatures and runtime handlers are selected through multiple
  manually synchronized tables.

ADR 0022 therefore makes the relevant S1 substrate a prerequisite for M19 and
M20. The substrate must be introduced in reviewable vertical slices, preserve
the current public compiler phases, and avoid pulling M29's persistent cache
format and restart reuse into M19.

## Decision

### Separate source type syntax from resolved type identity

The parser will produce a lossless `TypeRef` syntax tree. A `TypeRef` preserves:

- every qualified name segment with source spelling and span;
- every generic argument recursively;
- each array suffix and its exact span; and
- the full type-reference span.

Parser lookahead will use the same non-mutating type-reference grammar and a
checkpointed cursor. It will not maintain a second `type_end_at` grammar.

Semantic analysis will resolve `TypeRef` into interned `TypeId` values owned by
the checked program. `TypeId` is semantic identity; it never appears in the
parsed AST. The type arena records primitive, collection, user-declaration,
schema-object, platform, exception, null, and void shapes needed by the active
migration slice. Existing public `TypeName` views may remain temporarily as
derived compatibility adapters, but new cross-layer facts use `TypeId`.

### Introduce typed definition identities with explicit ownership

Raw executable positions will be replaced incrementally by private newtypes:

- `UnitId` identifies a parsed project source unit;
- `DefId` identifies a user declaration within its owning checked program;
- `ClassId` and `MemberId` are typed projections used by class/runtime code;
- `ObjectTypeId` and `FieldId` identify normalized schema definitions; and
- runtime heap identities such as collection and object instances remain
  distinct from compiler identities.

Each ID is valid only for the immutable checked program or schema arena that
created it. APIs accept the typed ID and its owner together, and constructors
remain private to the owning module. No ID is serialized or reconstructed from
source order in S1.

Project compilation keeps stable file-aware `SourceId` values. When cached
syntax units are assembled, their `TypeRef` spans retain that source identity;
semantic arenas are rebuilt for a changed checked program. Persistent ID
remapping, cache schema versioning, corruption rejection, and restart reuse
remain M29 work.

### Lower executable targets in vertical slices

HIR will continue to own immutable parsed syntax plus checked side tables during
the migration, but every migrated executable fact will carry typed IDs and the
resolved operation needed by runtime. Calls, references, members, casts, and
assignable places will be migrated independently behind typed enums.

The runtime image will precompute class lineage and dispatch metadata as those
targets migrate. Runtime execution consumes the checked target directly and
must not repeat source-name lookup, overload selection, type resolution, or
assignable-target classification.

This ADR does not require a wholesale expression IR before M19. A later slice
may replace span-keyed side tables with per-expression lowered nodes once the
typed identities are established and measured.

### Resolve assignable expressions once into checked places

Semantic analysis will record a `PlaceTarget` for every checked assignable
expression:

- local variable;
- List index;
- instance or static field/property;
- schema-backed SObject field; or
- another explicitly supported mutable target added later.

The target records typed compiler identities and the assignment value type.
Runtime resolves it once into an ephemeral `Place` containing already-evaluated
receiver/index identity. `Place::read` and `Place::write` own access checks,
typed-null handling, collection mutability, and target-specific storage.
Prefix/postfix mutation, simple assignment, and M19 compound assignment share
this path.

Evaluation order is receiver, then index where present, then right-hand value.
Each receiver/index expression is evaluated exactly once. A failing read,
operation, or write cannot re-evaluate any subexpression.

### Centralize numeric and bitwise policy

A closed numeric operation module will own:

- compile-time operand/result rules for `Integer`, `Long`, and `Decimal`;
- assignment conversion and compound-assignment result conversion;
- checked arithmetic and overflow behavior;
- divide/remainder-by-zero behavior;
- Boolean and integral bitwise operations;
- shift-distance masking, sign extension, and unsigned right shift; and
- runtime exception construction at the operation span.

The checker records a typed `NumericOperation` or `BitwiseOperation` in HIR.
Runtime applies that operation to evaluated values without re-selecting a
numeric family. String concatenation remains an explicitly separate operation.
Boolean `&`, `|`, and `^` evaluate both operands; `&&` and `||` retain
short-circuit behavior.

`Integer` is modeled as a signed 32-bit Apex value and `Long` as a signed
64-bit Apex value at observable numeric boundaries. Internal host
representations may be wider only while validating a result before conversion.
Overflow produces the documented catchable arithmetic failure rather than host
wrapping or a process panic.

### Stage exception, context, and intrinsic migration

Runtime faults will migrate from string exception names to a typed
`ExceptionKind` carried with source span and typed frames. Public diagnostics
remain the boundary rendering. The existing execution context becomes a typed
value carried by runtime entry points and async snapshots; compatibility
profiles become a typed ID selected by compilation/runtime configuration rather
than a hardcoded string.

An intrinsic descriptor catalog will become the authoritative mapping from
intrinsic ID to owner, name, call style, signature, profile disposition,
effects, and runtime handler family. It will be introduced after typed runtime
image targets, not coupled to M19's numeric implementation.

These migrations are required before milestones that consume them, but M19
does not need to complete unrelated intrinsic families, structured diagnostic
rendering, or compatibility-profile behavior.

### Required migration order

1. Add lossless `TypeRef`, typed IDs, and duplicate/conflicting hierarchy
   validation without changing supported behavior.
2. Migrate the executable identity slices required by assignable expressions
   and numeric types.
3. Add `PlaceTarget`, ephemeral runtime `Place`, and centralized existing
   `Integer`/`Decimal` arithmetic and increment/decrement behavior.
4. Prove behavior parity and single evaluation with focused negative and
   stress tests, then mark S1-04 complete.
5. Implement M19 exclusively through the new type, place, and numeric
   operation boundaries.

Every slice must remain buildable, pass the complete verification suite, and
include deterministic cost assertions where it touches a compiler/runtime hot
path. Complexity must be reduced at the existing decision-tree hotspots rather
than moved unchanged.

## Consequences

- M19 gains one semantic and runtime source of truth for numeric and mutation
  behavior, including indexed/member evaluate-once semantics.
- M20 can build qualified nested declaration identity on lossless source
  syntax and typed definitions rather than extending positional indexes.
- Compiler and runtime code becomes temporarily transitional: public
  `TypeName` adapters and some unmigrated raw facts may coexist with typed
  arenas, but new facts cannot use raw identities.
- Changed project builds still rebuild semantic arenas until M29 introduces a
  versioned persistent representation and remapping.
- The migration adds up-front work before visible M19 syntax, but prevents
  compound assignment and `Long` from deepening the current duplicated
  mutation and numeric matches.
- Owner approval and a fresh architecture review are required before the first
  behavior-changing S1-02 implementation slice.
