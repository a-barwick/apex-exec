# Milestone 28 CN3 typed-SObject `Map` cast evidence

This sanitized bundle records the focused comparison for narrowing a
`Map<Id,SObject>` to a typed SObject map. Captured on 2026-07-24 with Apex
Exec 0.1.0, Salesforce API 65.0, and Salesforce CLI 2.144.6, it confirms both
compile acceptance and the runtime identity rule used by Salesforce.

The guarded disposable Developer Edition org had verified ID
`00DdL000010oTXlUAM`. Local and Salesforce results matched both selected
dimensions:

- compile: 1/1;
- values: 1/1.

The total is 2/2 dimensions (**100.00%**). A manually constructed
`Map<Id,SObject>` rejects narrowing to `Map<Id,Account>` with `TypeException`,
while the typed `Trigger.oldMap` narrows successfully. After capture, the
temporary `M28CN3MapCastOracle` class and `M28CN3MapCastTrigger` trigger were
deleted and a Tooling API query verified zero remaining active fixtures.

Credential-free replay:

```bash
cargo run --locked -- oracle \
  examples/milestone28-cn3-sobject-map-cast-oracle/oracle-manifest.json \
  --salesforce-snapshot evidence/milestone28/cn3/salesforce.json \
  --report /tmp/apex-exec-m28-cn3-replay-report.json
cmp evidence/milestone28/cn3/report.json \
  /tmp/apex-exec-m28-cn3-replay-report.json
```
