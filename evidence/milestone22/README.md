# M22 Nebula Logger enterprise baseline

## Frozen identities

- Candidate: Nebula Logger Core v4.18.4
- Repository: `https://github.com/jongpie/NebulaLogger.git`
- Commit: `55ba832d1d51680dd5e291d67ffe2104fa48977f`
- Source API: 65.0
- Salesforce CLI: `@salesforce/cli/2.134.1`
- Candidate manifest: 1,055 files
- Source test classes: 45
- Raw Salesforce denominator: 1,159 methods
- Salesforce outcomes: 1,159 pass, 0 fail
- Exclusions or compatibility patches: none

The capture was produced in a one-day Enterprise scratch org using Nebula's
checked-in scratch definition and deployment substitution workflow. The
scratch org is disposable; authentication is intentionally not checked in.

## Results

| Stage | Count | Raw denominator |
|---|---:|---:|
| Salesforce pass | 1,159 | 1,159 |
| Local discovery | 1,159 | 1,159 |
| Local parse | 0 | 1,159 |
| Local check | 0 | 1,159 |
| Terminal execution | 0 | 1,159 |
| Salesforce agreement | 0 | 1,159 |
| Strict compatibility | 0 | 1,159 |

Matching passes, matching failures, and outcome mismatches are zero because no
required source closure reaches execution. Three reruns produced identical
per-test results. Cold/warm timing is recorded in `report.json`.

The report retains 43 exact parse blocker locations and sorts them by affected
test count. The leading production blockers are:

1. typed `switch when` pattern in `LogEntryEventHandler.cls`: 1,159 tests;
2. SOQL `TODAY` date literal in `LogManagementDataSelector.cls`: 1,159 tests;
3. `System.runAs` block in `LoggerMockDataCreator.cls`: 953 tests; and
4. `@IsTest(IsParallel=true)` in `Logger_Tests.cls`: 496 tests.

Unsupported results remain in the raw denominator. The strict 0/1,159 is the
honest starting point for this candidate, not a general Salesforce
compatibility percentage.

## Reproduction

Initialize the pinned submodule and install Nebula's locked dependencies:

```text
git submodule update --init benchmarks/milestone22/nebula-logger
cd benchmarks/milestone22/nebula-logger
npm ci --ignore-scripts
```

Create an Enterprise scratch org from
`config/scratch-orgs/base-scratch-def.json`, set
`EMAIL_SERVICE_RUN_AS_USER` using Nebula's
`env:set:email-service-run-as-user` script, and deploy
`nebula-logger/core` at API 65.0 with `RunLocalTests`. Then, from the Apex Exec
root:

```text
cargo run -- enterprise capture benchmarks/milestone22/manifest.json \
  --target-org <alias> \
  --sf benchmarks/milestone22/nebula-logger/node_modules/.bin/sf \
  --wait 60 \
  --output evidence/milestone22/salesforce.json

cargo run -- enterprise run benchmarks/milestone22/manifest.json \
  --salesforce evidence/milestone22/salesforce.json \
  --output evidence/milestone22/report.json
```

Freeze `salesforce.json` before running the local measurement. The capture and
report bind the candidate and each other by SHA-256. Neither evidence file
contains authentication secrets or raw Salesforce CLI output.
