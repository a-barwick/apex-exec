# apex-exec

A deterministic, org-independent Apex compiler and execution runtime,
implemented in Rust.

The long-term goal is to make ordinary Apex development, testing, and
debugging local-first. Salesforce remains the final compatibility oracle, but
developers should not need to deploy to an org to discover routine compiler or
unit-test failures.

The first six milestones support primitive expressions, assignment, lexical
scopes, common control flow, typed collections, single-file user-defined
methods, recursion, overloads, casts, catchable core exceptions, `finally`, and
source-mapped runtime call stacks. M5 adds classes, interfaces, inheritance,
checked instance/static members, and incremental SFDX project compilation. M6
adds annotation-based Apex test discovery, assertions, isolated parallel
execution, filtering, JUnit reports, and line/branch coverage.
Apex identifiers, types, and method names are case-insensitive.

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

The M4 core sample combines recursion, overloads, runtime casts, typed catches,
and `finally`:

```bash
cargo run -- run examples/methods-exceptions.apex
```

The M5 sample is an ordinary three-file SFDX service layer:

```console
$ cargo run -- check examples/milestone5-project
OK (3 classes, 3 source files)
$ cargo run -- invoke examples/milestone5-project Entry.run
Hello, Apex!
```

The M6 sample is an isolated two-test SFDX project:

```console
$ cargo run -- test examples/milestone6-project --jobs 2
PASS CalculatorTest.addsPositiveValues
PASS CalculatorTest.handlesNegativeValues

Coverage:
  force-app/main/default/classes/Calculator.cls: 3/3 lines (100.00%), 2/2 branches (100.00%)
Summary: 2 passed, 0 failed, 2 total; 3/3 lines (100.00%), 2/2 branches (100.00%)
```

Tests can be filtered and exported as JUnit XML:

```bash
cargo run -- test examples/milestone6-project CalculatorTest.addsPositiveValues \
  --junit test-results.xml
```

Compiler stages can be inspected independently:

```console
$ cargo run -- tokens examples/hello.apex
$ cargo run -- ast examples/hello.apex
$ cargo run -- check examples/hello.apex
$ cargo run -- run examples/hello.apex
```

This is an early implementation. Backwards-compatible top-level methods remain
available to anonymous scripts, while project code uses ordinary classes. The
standard-library, test, and exception surfaces are deliberately curated.
SObjects, SOQL, SOSL, and DML are not implemented yet.

## Project documentation

- [Vision](docs/VISION.md) — north star, enterprise value, and product principles
- [Roadmap](ROADMAP.md) — milestones and their exit criteria
- [Current status](docs/STATUS.md) — completed work and immediate next target
- [Architecture](docs/ARCHITECTURE.md) — current and target system design
- [Compatibility](docs/COMPATIBILITY.md) — supported Apex surface and fidelity
- [Development](docs/DEVELOPMENT.md) — commands and contribution workflow
- [Decisions](docs/decisions/README.md) — durable architectural rationale
- [Specifications](docs/specifications/README.md) — intended language behavior
