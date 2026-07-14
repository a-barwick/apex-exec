# Type System

## Status

Primitive expression checking, recursive generic collections, array aliases,
and the fixed M3 built-in method surface are implemented. User-defined overload
resolution is active M4 work; general conversions and user-defined types remain
later work.

## Names

**Implemented.** Type and variable names are case-insensitive. Source spelling
is preserved for diagnostics.

## Supported primitive types

### `String`

**Implemented, partial fidelity.** String literals use single quotes. Double
quotes are rejected. Common backslash escapes are interpreted by the lexer.
The supported method subset is statically checked. Lengths and indexes use
UTF-16 code units for ordinary Unicode scalar strings; a substring boundary
that would split a surrogate pair is rejected because Rust cannot represent
the resulting unpaired surrogate.

### `Boolean`

**Implemented, partial fidelity.** The literals `true` and `false` are
case-insensitive.

### `Integer`

**Implemented, simplified.** Values currently use Rust `i64`. Arithmetic is
checked and produces a runtime diagnostic on overflow, but Apex-compatible
range and overflow behavior are planned.

### `null`

**Implemented, simplified.** `null` can initialize or be assigned to every
supported primitive or collection type. Equality and string concatenation
handle null values; operations requiring a concrete Integer, Boolean,
collection, or index report a runtime diagnostic when a nullable value is used.

## Collection types

**Implemented for M3.** `List<T>`, `Set<T>`, and `Map<K,V>` may be nested
recursively over any supported value type. Generic types are invariant, so
`List<Integer>` is not assignable to `List<String>`. Collection-valued Set
elements and Map keys or values are accepted; the runtime does not impose Rust
hashability restrictions on Apex types.

One-dimensional `T[]` syntax is an alias for `List<T>` in both directions:

```apex
String[] names = new List<String>();
List<String> aliases = new String[2];
```

Only one explicit array suffix is supported. Use nested generic Lists for a
nested collection; `T[][]` is rejected explicitly.

## Collection construction

The checked constructors are:

- Empty `List<T>`, `Set<T>`, and `Map<K,V>` constructors.
- List and Set copy construction from a List or Set with the same element type.
- Map copy construction from a Map with identical key and value types.
- List and Set element literals, and Map `key => value` literals.
- `new T[size]`, where `size` is an Integer, producing `List<T>`.

Every constructor argument, literal element, Map key, Map value, and array size
is checked before execution. Collection copy constructors and `clone()` produce
independent shallow copies; ordinary assignment preserves reference identity.

## Declarations

**Implemented.** Local declarations require explicit initialization:

```apex
String name = 'Ada';
```

Uninitialized declarations are rejected in the current supported surface.

## Assignment

**Implemented.** The initializer or assigned expression must have the declared
invariant type or be `null`. Variables must be declared before use. Duplicate
declarations in the same lexical scope are rejected, while nested scopes may
shadow an outer name. List indexes are assignable lvalues when the index is an
Integer; Set and Map indexing is rejected.

## Built-in method calls

**Implemented for the documented M3 subset.** The checker resolves supported
static and instance built-ins case-insensitively, validates overload arity and
argument types, and assigns a return type. Calls such as List mutation and
`System.debug` return `Void`; a Void result cannot initialize a value, be
assigned, or participate in another value expression. Unknown methods and
unsupported overloads are compile-time errors.

Bare call receivers are resolved as variables before supported static type
names. A local variable named `String`, `Math`, or `System` therefore retains
normal variable precedence.

## Operators

**Implemented.** Integer arithmetic and ordering require Integer operands.
Equality accepts matching supported types or `null`; String `==` and `!=` are
case-insensitive, while collection membership uses case-sensitive String
equality. Boolean operators require Boolean operands and short-circuit at
runtime. `+` performs Integer addition unless either operand is a String, in
which case every supported non-Void value can be converted for concatenation.
Increment and decrement require a mutable Integer variable or Integer-valued
List index.

## Planned rules

- Numeric operations and conversions
- `Decimal`, `Double`, `Long`, and other platform primitives
- Class, interface, inheritance, and assignment compatibility
- User-defined static and instance member resolution
- User-defined method overload selection
- Casts and runtime type checks
