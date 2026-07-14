# Type System

## Status

Primitive exact-type checking is implemented. Conversions, nullability,
generics, classes, and overload resolution are planned.

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

**Implemented, simplified.** Values currently use Rust `i64`. Apex-compatible
range, overflow, and arithmetic behavior are planned.

## Declarations

**Implemented.** Local declarations require explicit initialization:

```apex
String name = 'Ada';
```

Uninitialized declarations are rejected in the current milestone.

## Assignment

**Implemented, limited.** The initializer or assigned expression must currently
have exactly the declared primitive type. Variables must be declared before
use. Duplicate declarations in the same current environment are rejected.

## Planned rules

- `null` and reference-type nullability
- Numeric operations and conversions
- `Decimal`, `Double`, `Long`, and other platform primitives
- Generic collection types
- Class, interface, inheritance, and assignment compatibility
- Static/instance member resolution
- Method overload selection
- Casts and runtime type checks
