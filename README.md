# apex-exec

A deterministic, org-independent Apex compiler and execution runtime,
implemented in Rust.

The long-term goal is to make ordinary Apex development, testing, and
debugging local-first. Salesforce remains the final compatibility oracle, but
developers should not need to deploy to an org to discover routine compiler or
unit-test failures.

The first fifteen milestones provide the core language, typed collections,
exceptions, classes and inheritance, SFDX project compilation, isolated Apex
tests with coverage, metadata-backed SObjects, SQLite transactions, checked
SOQL/SOSL and DML, triggers with rollback, and a curated platform API profile.
That profile includes date/time/decimal/ID/Blob values, JSON, regex, schema
describe, deterministic context and limits, and host-mocked HTTP callouts.
Deterministic async execution, editor/REPL debugging, a measured Salesforce
differential oracle, hermetic content-addressed enterprise CI, and targeted
check-only Salesforce validation complete the current local feedback loop and
release-confidence gate.
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

The M7 sample imports `Invoice__c` metadata and executes both typed and dynamic
field access:

```console
$ cargo run -- invoke examples/milestone7-project InvoiceDemo.run
Approved
125
```

The M10 sample combines value types, JSON, regex, describe, deterministic
services, limits, and four isolated Apex tests:

```console
$ cargo run -- invoke examples/milestone10-project PlatformDemo.run
2026-07-18 | 2026-07-17 10:00:00 | 10 | 12.25 | bWlsZXN0b25lLTEw | 10 | true | BYg
$ cargo run -- test examples/milestone10-project
Summary: 4 passed, 0 failed, 4 total; 13/13 lines (100.00%), 0/0 branches (100.00%)
```

The M13 oracle manifest runs identical project entry points locally and in an
authenticated scratch org, records normalized Salesforce evidence, and reports
measured compatibility by selected dimension:

```bash
cargo run -- oracle examples/milestone13-oracle/oracle-manifest.json \
  --target-org my-scratch \
  --record-salesforce milestone13-salesforce.json \
  --report milestone13-report.json

# Re-run the local side against reviewed, versioned Salesforce evidence.
cargo run -- oracle examples/milestone13-oracle/oracle-manifest.json \
  --salesforce-snapshot milestone13-salesforce.json
```

The M14 CI manifest seals project inputs, selects impacted tests, runs
independent deterministic shards, caches exact results, and emits
JUnit/Cobertura/SARIF:

```bash
cargo run -- ci run \
  examples/milestone14-project/apex-exec-ci.json \
  --shard 0/2

# Require the exact verified artifact without falling back to execution.
cargo run -- ci run \
  examples/milestone14-project/apex-exec-ci.json \
  --shard 0/2 --replay
```

The M15 hybrid gate reuses that hermetic manifest, selects affected deployment
components and tests, checks schema/configuration drift, compares local and org
test outcomes, and emits one release-readiness decision. An authenticated run
uses a Salesforce check-only deployment; reviewed snapshots support offline
replay:

```bash
cargo run -- hybrid \
  examples/milestone15-project/apex-exec-ci.json \
  --target-org staging \
  --record-validation milestone15-validation.json \
  --report milestone15-readiness.json

# Repeat the decision without credentials or Salesforce compute.
cargo run -- hybrid \
  examples/milestone15-project/apex-exec-ci.json \
  --validation-snapshot milestone15-validation.json
```

Repository pull requests, pushes to `main`, merge-queue candidates, and weekday
scheduled builds run the same guarantees in GitHub Actions. The required gate
combines Rust formatting, linting, documentation, and cross-platform tests with
16 end-to-end Apex program/project cases, both M14 enterprise-CI shards, and
the M15 release-gate project. A `v*` tag whose version matches `Cargo.toml`
builds verified Linux, macOS, and Windows archives, adds SHA-256 checksums, and
publishes a GitHub release.

Run the realistic Apex regression layer locally after building the binary:

```bash
cargo build
APEX_EXEC_BIN=target/debug/apex-exec .github/scripts/run-apex-regression.sh
```

Compiler stages can be inspected independently:

```console
$ cargo run -- tokens examples/hello.apex
$ cargo run -- ast examples/hello.apex
$ cargo run -- check examples/hello.apex
$ cargo run -- run examples/hello.apex
```

This remains an early compatibility implementation. Backwards-compatible
top-level methods remain available to anonymous scripts, while project code
uses ordinary classes. Platform, test, and exception surfaces are deliberately
curated; unsupported behavior is rejected explicitly.

## Project documentation

- [Vision](docs/VISION.md) — north star, enterprise value, and product principles
- [Roadmap](ROADMAP.md) — milestones and their exit criteria
- [Current status](docs/STATUS.md) — completed work and immediate next target
- [Architecture](docs/ARCHITECTURE.md) — current and target system design
- [Compatibility](docs/COMPATIBILITY.md) — supported Apex surface and fidelity
- [Development](docs/DEVELOPMENT.md) — commands and contribution workflow
- [Decisions](docs/decisions/README.md) — durable architectural rationale
- [Specifications](docs/specifications/README.md) — intended language behavior
