# Language Specifications

These documents record Apex Exec's intended observable behavior. They are not a
replacement for tests: every implemented rule should be represented by an
executable fixture.

- [Type system](type-system.md)
- [Execution semantics](execution-semantics.md)
- [Diagnostics](diagnostics.md)

Specifications may describe both current and target behavior, but every rule
must be labeled **Implemented**, **Planned**, or **Deferred** when support is not
obvious. `docs/COMPATIBILITY.md` remains the summary of shipped capability.
