# M24 partial-DML differential evidence

## Candidate and environment

- Implementation checkpoint:
  `47e4bea631a538b1697d262e7203ef1c9dfa97c1`
- Fixture tree:
  `examples/milestone24-oracle`
- Ordered fixture-file digest:
  `c1b73bcd6253573d8a6410c40fa88ad2c85a9995a424148239d0620636f78a68`
- Salesforce API: `65.0`
- Salesforce CLI: `@salesforce/cli/2.143.6`
- Rust: `rustc 1.88.0`; Cargo: `1.88.0`
- Target alias: `apex-exec-m17`
- Guarded org ID: `00DdL000010oTXlUAM`

The ignored root `.env` was loaded without printing values. Before every live
mutation, `sf org display --json` was reduced to the org ID and required to
equal the guarded ID above. CLI 2.143.6 ran from an isolated pinned `/tmp`
install because the machine's legacy 2.30.8 build could not decode the current
Metadata API `Finalizing` state.

## Measured slice

The one unchanged Apex test covers:

- stable three-row `Database.insert(..., false)` result ordering;
- successful caller/result Ids and failed-row null Id;
- typed `REQUIRED_FIELD_MISSING`, nonempty message, and `Amount__c` field;
- one source DML-statement limit charge;
- the Salesforce partial-save retry boundary: first trigger attempt over three
  rows and second attempt over two survivors, producing five before-trigger
  and four after-trigger row observations; and
- case-insensitive external-ID upsert update/insert behavior, success Ids, and
  `isCreated` flags.

The normalized report matches 2/2 selected dimensions: compile 1/1 and test
outcome 1/1. This is exact evidence for this fixture and environment, not a
general Salesforce compatibility percentage.

## Reproduction

After applying the org-ID guard and placing CLI 2.143.6 first on `PATH`, the
live capture used:

```text
cargo +1.88.0 run --locked -- oracle \
  examples/milestone24-oracle/oracle-manifest.json \
  --target-org apex-exec-m17 \
  --record-salesforce evidence/milestone24/salesforce.json \
  --report evidence/milestone24/report.json
```

Offline replay used:

```text
cargo +1.88.0 run --locked -- oracle \
  examples/milestone24-oracle/oracle-manifest.json \
  --salesforce-snapshot evidence/milestone24/salesforce.json \
  --report /tmp/apex-exec-m24-replay-report.json
cmp evidence/milestone24/report.json /tmp/apex-exec-m24-replay-report.json
```

The replay matched both dimensions and reproduced the checked-in report
byte-for-byte.

## Release verification

The full Rust suite reports 424 passed, zero failed, and zero ignored tests.
Formatting, Clippy with warnings denied, documentation validation, and the
pinned Lizard maintainability ratchet pass. The established coverage command
was:

```text
cargo +1.88.0 llvm-cov --locked --all-targets --json \
  --output-path /tmp/apex-exec-m24-final-coverage.json
```

LLVM source-line coverage is 28,410/33,480 (**84.86%**) overall and
12,388/14,785 (**83.79%**) across the 12 instrumented production modules
changed from the M24 starting commit. The coverage tool's standard source view
excludes test/harness files. `src/platform/mod.rs` changed but contains no
instrumentable lines, so it is not in the changed-module denominator; no other
feature-code exclusions were applied.

The unchanged North Star corpus remains 7/7 lexer and 7/7 parser indicators,
14/14 total, a change of zero from the pre-M24 baseline. These are syntax
indicators, not runtime or Salesforce compatibility percentages.

## Baseline restoration

After capture, only `M24PartialDmlTest`, `M24DmlAudit`,
`M24ResultTrigger`, and `M24Result__c` were deleted from the guarded disposable
org. The deletion succeeded with seven components and zero component errors.
Metadata-list verification against the same guarded org ID reported zero
remaining `M24*` Apex classes, triggers, and custom objects.
