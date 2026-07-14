# Type System

## Status

Primitive expression checking and null assignment are implemented. General
conversions, generic types, classes, and overload resolution are planned.

## Names

**Implemented.** Type and variable names are case-insensitive. Source spelling
is preserved for diagnostics.

## Supported primitive types

### `String`

**Implemented, partial fidelity.** String literals use single quotes. Double
quotes are rejected. Common backslash escapes are interpreted by the lexer.

### `Boolean`

**Implemented, partial fidelity.** The literals `true` and `false` are
case-insensitive.

### `Integer`

**Implemented, simplified.** Values currently use Rust `i64`. Arithmetic is
checked and produces a runtime diagnostic on overflow, but Apex-compatible
range and overflow behavior are planned.

### `null`

**Implemented, simplified.** `null` can initialize or be assigned to each
currently supported primitive type. Equality and string concatenation handle
null values; operations requiring a concrete Integer or Boolean report a
runtime diagnostic when a nullable variable contains `null`.

## Declarations

**Implemented.** Local declarations require explicit initialization:

```apex
String name = 'Ada';
```

Uninitialized declarations are rejected in the current milestone.

## Assignment

**Implemented.** The initializer or assigned expression must have the declared
primitive type or be `null`. Variables must be declared before use. Duplicate
declarations in the same lexical scope are rejected, while nested scopes may
shadow an outer name.

## Operators

**Implemented for M2.** Integer arithmetic and ordering require Integer
operands. Equality accepts matching primitive types or `null`. Boolean
operators require Boolean operands and short-circuit at runtime. `+` performs
Integer addition unless either operand is a String, in which case supported
primitive and null values are converted for concatenation. Increment and
decrement require a mutable Integer variable.

## Planned rules

- `null` and reference-type nullability
- Numeric operations and conversions
- `Decimal`, `Double`, `Long`, and other platform primitives
- Generic collection types
- Class, interface, inheritance, and assignment compatibility
- Static/instance member resolution
- Method overload selection
- Casts and runtime type checks
