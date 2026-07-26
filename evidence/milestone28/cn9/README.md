# Milestone 28 CN9 `FlowDefinitionView.VersionNumber` evidence

This sanitized bundle records the focused comparison for the standard
`FlowDefinitionView.VersionNumber` field. Captured on 2026-07-24 with Apex
Exec 0.1.0, Salesforce API 65.0, and Salesforce CLI 2.144.6, it confirms
compile acceptance, describe metadata, typed assignment, and query execution.

The guarded disposable Developer Edition org had verified ID
`00DdL000010oTXlUAM`. Local and Salesforce results matched both selected
dimensions:

- compile: 1/1;
- values: 1/1.

The total is 2/2 dimensions (**100.00%**). The field describe name is
`VersionNumber`, its display type is `INTEGER`, and a query result assigns to
an Apex `Integer`. The local schema continues to reject assigning a String to
the field, and its fixed construction-cost assertion now covers 152 curated
fields.

After capture, the temporary `M28CN9FlowDefinitionVersionOracle` Apex class
was deleted through the Tooling API and a guarded query verified no remaining
fixture.

Credential-free replay:

```bash
cargo run --locked -- oracle \
  examples/milestone28-cn9-flow-definition-version-oracle/oracle-manifest.json \
  --salesforce-snapshot evidence/milestone28/cn9/salesforce.json \
  --report /tmp/apex-exec-m28-cn9-replay-report.json
cmp evidence/milestone28/cn9/report.json \
  /tmp/apex-exec-m28-cn9-replay-report.json
```
