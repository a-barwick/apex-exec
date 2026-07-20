# Milestone 27 Salesforce evidence

This sanitized bundle records the reviewed sharing and security profile for
Salesforce API 66.0. Capture ran on 2026-07-19 with Apex Exec 0.1.0 and
Salesforce CLI 2.143.6 against the guarded disposable Developer Edition org
whose verified ID is `00DdL000010oTXlUAM`. No credentials, auth URLs, tokens,
or raw org-display response are stored.

The same source fixture runs locally and on Salesforce to compare compile and
test outcomes for explicit with/without/inherited sharing propagation,
owner/private visibility, user/system query and DML modes, CRUD/FLS denial, and
`Security.stripInaccessible`. Salesforce and Apex Exec match both selected
dimensions:

- compile: 1/1;
- tests: 1/1.

The total is 2/2 dimensions (**100.00%**). Two initial live mismatches were
fixed rather than hidden: system-mode operations retain effective class
sharing, and object/field permission DML denials use
`CANNOT_INSERT_UPDATE_ACTIVATE_ENTITY` while record-sharing denials retain
`INSUFFICIENT_ACCESS_OR_READONLY`.

After loading the ignored root `.env` and verifying the org ID, capture used:

```bash
PATH="/tmp/apex-exec-m25-sf/node_modules/.bin:$PATH" \
cargo +1.88.0 run --locked -- oracle \
  examples/milestone27-oracle/oracle-manifest.json \
  --target-org "$APEX_EXEC_SF_TARGET_ORG" \
  --record-salesforce evidence/milestone27/salesforce.json \
  --report evidence/milestone27/report.json
```

Credential-free replay used:

```bash
cargo +1.88.0 run --locked -- oracle \
  examples/milestone27-oracle/oracle-manifest.json \
  --salesforce-snapshot evidence/milestone27/salesforce.json \
  --report /tmp/apex-exec-m27-replay-report.json
cmp evidence/milestone27/report.json \
  /tmp/apex-exec-m27-replay-report.json
```

Replay matched byte for byte. Cleanup deleted the four fixture classes and
custom object from a temporary source copy. Guarded post-cleanup checks found
zero active `M27*` Apex classes and zero Metadata API `M27SecureRow__c`
components. The Tooling API retains one soft-deleted `CustomObject` row, which
is not active metadata.
