# Vision

## Mission

Apex Exec is a deterministic, org-independent Apex development platform that
provides fast compile, test, and debug feedback without requiring Salesforce
deployment for the normal development loop.

Salesforce remains the final compatibility oracle. The goal is to move routine
language validation, unit testing, data setup, and debugging onto developer
machines and ordinary CI workers, then use an org only for targeted final
validation and deployment.

## Dream-state experience

A developer clones an SFDX project and runs:

```bash
apex-exec test
```

The tool discovers project metadata, compiles Apex, creates an isolated
SQLite-backed org simulation, runs tests, and reports failures, stack traces,
coverage, queries, DML, triggers, and resource use in seconds.

The local inner loop is:

```bash
apex-exec check force-app
apex-exec test --changed
apex-exec test AccountServiceTest.should_merge_accounts
apex-exec debug AccountServiceTest.should_merge_accounts
```

The final compatibility gate is:

```bash
apex-exec verify --target-org staging
```

It runs only compatibility-sensitive tests in Salesforce and reports meaningful
differences between the local runtime and the platform.

## Enterprise problems

### Slow feedback

Developers should not deploy source merely to discover syntax, type, or ordinary
unit-test failures. Local compilation and execution should reduce feedback from
minutes to seconds or milliseconds.

### Environment scarcity and contention

Routine work should not require a sandbox, scratch org, credential, Dev Hub, or
shared test environment. Local and CI runs should be hermetic.

### Configuration and data drift

The relevant schema, settings, and fixtures should be version-controlled and
repeatable. Tests must not depend on mutable shared data.

### Large and flaky test suites

Tests should run in isolated transactions, in parallel where safe, with stable
time, randomness, IDs, and async scheduling. Dependency analysis should select
affected tests without sacrificing correctness.

### Weak observability

Developers need breakpoints, source stacks, local variables, query plans, DML
history, trigger timelines, database snapshots, and deterministic replay.

### Release risk

API-version and platform differences should be surfaced through explicit
compatibility profiles and local-versus-org differential tests before a
deployment starts.

## Product principles

1. **Compatibility is measured.** Supported behavior is backed by executable
   conformance fixtures and, eventually, differential runs against Salesforce.
2. **Unsupported behavior is explicit.** Never ignore syntax or silently invent
   platform behavior.
3. **Local execution is deterministic.** Time, IDs, randomness, async work, and
   persistent state must be controllable.
4. **The common path wins.** Prioritize APIs and semantics used by real projects
   rather than chasing the entire Salesforce surface.
5. **Compiler phases stay independent.** Lexing, parsing, resolution, typing,
   lowering, execution, and platform services remain testable boundaries.
6. **Source compatibility matters.** Common Apex should run without a local-only
   dialect or invasive conditional code.
7. **Hybrid validation is honest.** Apex Exec reduces org dependence; it does
   not claim to eliminate the need for final Salesforce verification.

## Success measures

- The original anonymous collection-and-loop program runs unchanged.
- A real multi-file SFDX project compiles locally.
- A representative enterprise project runs 60–80% of ordinary Apex unit tests
  locally without source changes.
- Changed-code test feedback is typically sub-second or single-digit seconds.
- Supported behavior has a published fidelity level and conformance coverage.
- Local failures contain actionable source diagnostics and runtime traces.
- Final org validation runs substantially fewer tests and deployments than the
  existing workflow.

## Non-goals

- Reimplement every Salesforce product or niche managed API.
- Promise perfect compatibility without conformance evidence.
- Reproduce Salesforce's internal implementation when observable behavior can
  be provided by a simpler deterministic model.
- Add governor limits, sharing, or API-version differences before the core
  language and data runtime are useful. These may become compatibility profiles
  later.
- Execute untrusted Apex as a security sandbox in the initial product.
