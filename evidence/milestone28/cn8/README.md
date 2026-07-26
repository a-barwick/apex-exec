# Milestone 28 CN8 `User.IsActive` evidence

This sanitized bundle records the focused comparison for the standard
`User.IsActive` field and its use in an `ApexEmailNotification.User`
relationship filter. Captured on 2026-07-24 with Apex Exec 0.1.0, Salesforce
API 65.0, and Salesforce CLI 2.144.6, it confirms compile acceptance and the
selected field, describe, and query contracts.

The guarded disposable Developer Edition org had verified ID
`00DdL000010oTXlUAM`. Local and Salesforce results matched both selected
dimensions:

- compile: 1/1;
- values: 1/1.

The total is 2/2 dimensions (**100.00%**). `User.IsActive` is a Boolean field,
its describe name and display type are `IsActive` and `BOOLEAN`, and the
enterprise-shaped relationship filter compiles and executes. The local schema
continues to reject assigning a String to the field, and its fixed
construction-cost assertion now covers 151 curated fields.

After capture, the temporary `M28CN8UserIsActiveOracle` Apex class was deleted
through the Tooling API and a guarded query verified no remaining fixture.

Credential-free replay:

```bash
cargo run --locked -- oracle \
  examples/milestone28-cn8-user-is-active-oracle/oracle-manifest.json \
  --salesforce-snapshot evidence/milestone28/cn8/salesforce.json \
  --report /tmp/apex-exec-m28-cn8-replay-report.json
cmp evidence/milestone28/cn8/report.json \
  /tmp/apex-exec-m28-cn8-replay-report.json
```
