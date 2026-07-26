# Milestone 28 CN10 `FlowDefinitionView.IsActive` evidence

This sanitized bundle records the focused comparison for the standard
`FlowDefinitionView.IsActive` field. Captured on 2026-07-24 with Apex Exec
0.1.0, Salesforce API 65.0, and Salesforce CLI 2.144.6, it confirms compile
acceptance, describe metadata, typed assignment, and filtered query execution.

The guarded disposable Developer Edition org had verified ID
`00DdL000010oTXlUAM`. Local and Salesforce results matched both selected
dimensions:

- compile: 1/1;
- values: 1/1.

The total is 2/2 dimensions (**100.00%**). The field describe name is
`IsActive`, its display type is `BOOLEAN`, and a query filtered by
`IsActive = TRUE` assigns the selected value to an Apex `Boolean`. The local
schema continues to reject assigning a String to the field, and its fixed
construction-cost assertion now covers 153 curated fields.

After capture, the temporary `M28CN10FlowDefinitionActiveOracle` Apex class
was deleted through the Tooling API and a guarded query verified no remaining
fixture.

Credential-free replay:

```bash
cargo run --locked -- oracle \
  examples/milestone28-cn10-flow-definition-active-oracle/oracle-manifest.json \
  --salesforce-snapshot evidence/milestone28/cn10/salesforce.json \
  --report /tmp/apex-exec-m28-cn10-replay-report.json
cmp evidence/milestone28/cn10/report.json \
  /tmp/apex-exec-m28-cn10-replay-report.json
```
