# Milestone 25 Salesforce evidence

This sanitized bundle records the reviewed API-version profile difference for
API 31.0 and API 65.0. Capture ran on 2026-07-19 with Apex Exec 0.1.0 and
Salesforce CLI 2.143.6 against the guarded disposable Developer Edition org
whose verified ID is `00DdL000010oTXlUAM`. No credentials, auth URLs, tokens,
or raw org-display response are stored.

The fixture has a project default of 65.0, explicit 31.0 class and trigger
sidecars, and explicit 65.0 current/test class sidecars. Salesforce and local
execution matched all selected dimensions:

- compile: 1/1;
- canonical effective profiles: 1/1; and
- tests: 1/1.

The total is 3/3 dimensions (**100.00%**). The reviewed behavioral difference
is that a null `instanceof String` expression returns true in the API 31.0
class and false in the API 65.0 class. The API 31.0 trigger also compiles and
executes in the mixed-version transaction.

After loading the ignored root `.env` and verifying the org ID, capture used:

```bash
PATH="/tmp/apex-exec-m25-sf/node_modules/.bin:$PATH" \
cargo +1.88.0 run --locked -- oracle \
  examples/milestone25-oracle/oracle-manifest.json \
  --target-org "$APEX_EXEC_SF_TARGET_ORG" \
  --record-salesforce evidence/milestone25/salesforce.json \
  --report evidence/milestone25/report.json
```

Credential-free replay used:

```bash
cargo +1.88.0 run --locked -- oracle \
  examples/milestone25-oracle/oracle-manifest.json \
  --salesforce-snapshot evidence/milestone25/salesforce.json \
  --report /tmp/apex-exec-m25-replay-report.json
cmp evidence/milestone25/report.json \
  /tmp/apex-exec-m25-replay-report.json
```

Replay matched byte for byte. Cleanup deleted the three fixture classes, one
trigger, and custom object. Metadata API inventory then contained no
`M25VersionProbe__c`; the Tooling API retained one soft-deleted `CustomObject`
record, which is not active metadata.
