# M23 query-fidelity differential evidence

## Candidate and environment

- Implementation checkpoint:
  `7dde7cb4b7dbf79ccc93916e73ef160502c19dc9`
- Fixture tree:
  `examples/milestone23-oracle`
- Ordered fixture-file digest:
  `df2fc246340aebe4f9b0f865ae184a77ab1c44377687179a38a610e3bd07855d`
- Salesforce API: `65.0`
- Salesforce CLI: `@salesforce/cli/2.134.1`
- Rust: `rustc 1.88.0`; Cargo: `1.88.0`
- Target alias: `apex-exec-m17`
- Guarded org ID: `00DdL000010oTXlUAM`

The ignored root `.env` was loaded without printing values. Before each live
mutation, `sf org display --json` was reduced to the org ID and required to
equal the guarded ID above.

## Measured slice

The one unchanged Apex test covers:

- `CreatedDate = TODAY`;
- a correlated custom child subquery with ordering and limit;
- a two-level custom parent relationship path;
- grouped `HAVING COUNT(Id)`;
- `Database.query` with a simple named bind;
- `Database.countQuery`; and
- `Database.getQueryLocator` driving one deterministic batch chunk.

The normalized result matches on 2/2 selected dimensions: compile 1/1 and test
outcome 1/1. This is exact evidence for this fixture and environment, not a
general Salesforce compatibility percentage.

Nebula Logger Core v4.18.4 contains no `TYPEOF` use in its pinned package
source. M23 therefore retains an explicit parser rejection for `TYPEOF` instead
of claiming unmeasured polymorphic behavior.

## Reproduction

After applying the org-ID guard, the live capture used:

```text
cargo +1.88.0 run --locked -- oracle \
  examples/milestone23-oracle/oracle-manifest.json \
  --target-org apex-exec-m17 \
  --record-salesforce evidence/milestone23/salesforce.json \
  --report evidence/milestone23/report.json
```

Offline replay used:

```text
cargo +1.88.0 run --locked -- oracle \
  examples/milestone23-oracle/oracle-manifest.json \
  --salesforce-snapshot evidence/milestone23/salesforce.json \
  --report /tmp/apex-exec-m23-replay-report.json
cmp evidence/milestone23/report.json /tmp/apex-exec-m23-replay-report.json
```

The replay matched both dimensions and reproduced the checked-in report
byte-for-byte.

## Release verification

The full Rust suite reports 416 passed, zero failed, and zero ignored tests.
Formatting, Clippy with warnings denied, documentation validation, and the
pinned Lizard maintainability ratchet pass. The established coverage command
was:

```text
cargo +1.88.0 llvm-cov --locked --all-targets --json \
  --output-path /tmp/apex-exec-m23-final-coverage.json
```

LLVM source-line coverage is 27,363/32,259 (**84.82%**) overall and
13,580/16,135 (**84.16%**) across the 20 instrumented production modules
changed from the M23 starting commit. The coverage tool's standard source view
excludes test/harness files. `src/platform/mod.rs` changed but contains no
instrumentable lines, so it is not in the changed-module denominator; no other
feature-code exclusions were applied.

The unchanged North Star corpus remains 7/7 lexer and 7/7 parser indicators,
14/14 total, a change of 0 from the pre-M23 baseline. These are syntax
indicators, not runtime or Salesforce compatibility percentages.

## Baseline restoration

After capture, only `M23QueryFidelityTest`, `M23InvoiceBatch`,
`M23Invoice__c`, `M23Customer__c`, and `M23Region__c` were deleted from the
guarded disposable org. The deletion succeeded with nine components and zero
component errors. A metadata-list verification against the same guarded org ID
reported zero remaining `M23*` Apex classes and zero remaining `M23*` custom
objects.
