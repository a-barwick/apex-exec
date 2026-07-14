# apex-exec

A deterministic, org-independent Apex compiler and execution runtime,
implemented in Rust.

The long-term goal is to make ordinary Apex development, testing, and
debugging local-first. Salesforce remains the final compatibility oracle, but
developers should not need to deploy to an org to discover routine compiler or
unit-test failures.

The first three milestones support primitive expressions, assignment, lexical
scopes, common control flow, and typed `List`, `Set`, `Map`, and one-dimensional
array syntax over `String`, `Boolean`, `Integer`, `null`, and nested collection
types. Apex identifiers and built-in method names are case-insensitive.

```console
$ cargo run -- run examples/hello.apex
Hello, world!
$ cargo run -- run examples/control-flow.apex
45
```

The unchanged M3 collection acceptance program is also executable:

```bash
cargo run -- run examples/collections.apex
```

Compiler stages can be inspected independently:

```console
$ cargo run -- tokens examples/hello.apex
$ cargo run -- ast examples/hello.apex
$ cargo run -- check examples/hello.apex
$ cargo run -- run examples/hello.apex
```

This is an early implementation. The available method calls are a fixed core
standard-library subset; user-defined methods, exceptions, classes, SOQL,
SOSL, and DML are not implemented yet.

## Project documentation

- [Vision](docs/VISION.md) — north star, enterprise value, and product principles
- [Roadmap](ROADMAP.md) — milestones and their exit criteria
- [Current status](docs/STATUS.md) — completed work and immediate next target
- [Architecture](docs/ARCHITECTURE.md) — current and target system design
- [Compatibility](docs/COMPATIBILITY.md) — supported Apex surface and fidelity
- [Development](docs/DEVELOPMENT.md) — commands and contribution workflow
- [Decisions](docs/decisions/README.md) — durable architectural rationale
- [Specifications](docs/specifications/README.md) — intended language behavior
