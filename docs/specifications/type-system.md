# Type System

## Status

Primitive expression checking, recursive generic collections, array aliases,
the fixed M3 built-in method surface, M4 methods/exceptions, M5
class/interface project types, and M6 System assertions are implemented.
General numeric and platform conversions remain later work.

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
checked and produces a catchable `MathException` on overflow, but
Apex-compatible range and overflow behavior are planned.

### `Object`

**Implemented, simplified.** Every supported non-Void value is
assignable to `Object`. The original runtime value and collection identity are
preserved so a later explicit downcast can validate its concrete type. M5 class
instances retain object identity through `Object`; general Object methods and
platform-type behavior are not part of this surface.

### `null`

**Implemented, simplified.** `null` can initialize or be assigned to every
supported value type. Equality and string concatenation handle null values;
operations requiring a concrete Integer, Boolean, collection, or index raise a
typed runtime exception when a nullable value is used.

## Core exception types

**Implemented for M4, simplified.** `Exception` is the common catch type for
`NullPointerException`, `ListException`, `MathException`, `TypeException`,
`StringException`, `IllegalArgumentException`, and `FinalException`. Concrete
exceptions widen to `Exception` or `Object`. Downcasts from `Exception` or
`Object` are explicit and checked at runtime. Custom exception classes and the
broader Apex hierarchy require later compatibility work.

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

Uninitialized local declarations are rejected in the current supported
surface. Class fields and automatic properties receive typed null before any
explicit initializer executes.

## Assignment

**Implemented.** The initializer or assigned expression must have the declared
invariant type or be `null`. Variables must be declared before use. Duplicate
declarations in the same lexical scope are rejected, while nested scopes may
shadow an outer name. Checked fields and writable properties are assignable;
static/instance and access rules apply. List indexes are assignable lvalues
when the index is an Integer; Set and Map indexing is rejected.

## Built-in method calls

**Implemented for the documented M3 subset and M6 System assertions.** The
checker resolves supported static and instance built-ins case-insensitively,
validates overload arity and argument types, assigns a return type, and records
a typed intrinsic target in HIR. Calls such as List mutation, `System.debug`,
and the `System.assert`, `System.assertEquals`, and `System.assertNotEquals`
overloads return `Void`; a Void result cannot initialize a value, be assigned,
or participate in another value expression. Unknown methods and unsupported
overloads are compile-time errors.

Bare call receivers are resolved as variables before supported static type
names. A local variable named `String`, `Math`, or `System` therefore retains
normal variable precedence.

## User-defined methods and overloads

**Implemented through M5, simplified.** Class instance/static methods and the
backwards-compatible top-level script grammar accept typed parameters and
either a value type or `void` return. All signatures are collected before
bodies are checked, enabling cross-file calls, forward calls, and recursion.
Method and parameter lookup is case-insensitive, and each method body has an
isolated local scope plus its checked class receiver where applicable.

An overload key is its canonical method name plus parameter types. Return type
does not participate. Resolution considers only statically checked argument
types and records the selected target in typed HIR rather than on the parsed
call node. Applicable
candidates are compared parameter-by-parameter. A candidate is more specific
only if every parameter is identical to or a supported subtype of the other
candidate and at least one is a strict subtype. Concrete core exceptions are
subtypes of `Exception`, user classes/interfaces use checked inheritance, and
all supported value types widen to `Object`. Crossing or unrelated candidates
remain ambiguous, including for `null`. Numeric and broader platform
conversions are not attempted.

Value-returning methods must return a compatible value or throw on every path
recognized by the conservative control-flow check. `void` methods may complete
normally or use a value-less `return`; anonymous execution remains limited to a
value-less `return`.

## User-defined types

**Implemented for M5, simplified.** A class may extend one class and implement
interfaces. User types are invariant by name but participate in assignment and
overload selection through their checked superclass/interface relationships.
Abstract/virtual/override rules and interface obligations are validated before
execution. Member lookup covers constructors, fields, properties, and methods,
with public/private/protected/global and static/instance checks.

Nested types, enums, explicit superclass-constructor calls, and generic user
types are not yet supported.

## Explicit casts

**Implemented for M4, simplified.** Casts are accepted between identical
types, to or from the minimal `Object` carrier, between a concrete core
exception and the `Exception` root, and between related user
classes/interfaces. Unrelated concrete casts are rejected during checking. A
permitted downcast whose runtime value has another type raises `TypeException`;
casting `null` yields a null carrying the target static type.

## Operators

**Implemented.** Integer arithmetic and ordering require Integer operands.
Equality accepts matching supported types or `null`; String `==` and `!=` are
case-insensitive, while collection membership uses case-sensitive String
equality. Boolean operators require Boolean operands and short-circuit at
runtime. `+` performs Integer addition unless either operand is a String, in
which case every supported non-Void value can be converted for concatenation.
Increment and decrement require a mutable Integer variable, field/property, or
Integer-valued List index.

## Planned rules

- Numeric operations and conversions
- `Decimal`, `Double`, `Long`, and other platform primitives
- Nested/generic user types, enums, and custom exception classes
- General conversions beyond the implemented inheritance relationships
- Full Object and platform-type behavior
