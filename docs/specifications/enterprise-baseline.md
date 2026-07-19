# Representative enterprise baseline

## Purpose

The enterprise baseline turns one immutable, user-approved Salesforce project
into per-test compatibility evidence. It is an evidence layer above project
discovery, lexing, parsing, semantic checking, isolated test execution, and
platform services; it does not add Salesforce behavior to those layers.

## Candidate manifest

`apex-exec enterprise manifest` records:

- repository URL, 40-character Git commit, release tag, and API version;
- package roots and test roots relative to the SFDX project;
- the Apex Exec version and a minimum raw denominator of 100 tests; and
- every regular package file, sorted by relative path and bound by SHA-256.

Symlinks, paths outside the candidate root, duplicate roots, unsafe relative
paths, an unpinned commit, and a denominator below 100 are rejected. Loading or
using a manifest re-hashes the project and fails on missing, modified, or
unrecorded inputs.

## Salesforce capture

`apex-exec enterprise capture` verifies the candidate, resolves the target org,
and invokes Salesforce `RunLocalTests` at the manifest API version. The
normalized, sealed snapshot contains all returned method-qualified pass/fail
outcomes. It rejects:

- fewer than the manifest minimum number of methods;
- a returned test whose class is outside the pinned test roots;
- duplicate or unsorted test identities;
- outcomes other than normalized `pass` or `fail`; and
- candidate, schema, or snapshot digest tampering.

The capture allowlist excludes credentials and raw CLI responses. A failed
Salesforce test remains a valid denominator member.

## Local measurement

For each frozen Salesforce test:

1. map the test class to a byte-identical file below a pinned test root;
2. compute its case-insensitive, cycle-bounded required Apex source closure,
   including package triggers that DML may dispatch implicitly;
3. lex and parse every closure file;
4. compile that explicit closure against metadata imported from the pinned
   package roots;
5. execute the exact method in an isolated interpreter; and
6. compare normalized terminal pass/fail outcomes.

The report exposes separate discovery, parse, check, execution, agreement, and
strict metrics. Percentages use the unfiltered Salesforce denominator. The
strict numerator requires check success, a terminal local result, and outcome
agreement. Matching passes, matching failures, and mismatches are separate
counts.

Blockers have an explicit `phase`, stable `family`, evidence `detail`, and
optional source path. Summaries are ordered by impacted-test count descending,
then stable phase and family names.

## Determinism and timing

Exactly three local reruns are required. Rerun one is cold with respect to
checked source-closure compilation; reruns two and three reuse that in-memory
cache. Every execution still receives a fresh interpreter and execution store.
Per-test results must be byte-equivalent across reruns after excluding timing,
or the command fails without writing a report. Millisecond timing for each
cold/warm run is retained as observational evidence, not a pass threshold.

## M22 command sequence

```text
apex-exec enterprise manifest <project> ... --output <manifest.json>
apex-exec enterprise capture <manifest.json> \
  --target-org <alias> --sf <pinned-sf> --output <salesforce.json>
apex-exec enterprise run <manifest.json> \
  --salesforce <salesforce.json> --output <report.json>
```

The Salesforce snapshot must be frozen before the final command is run against
the candidate.
