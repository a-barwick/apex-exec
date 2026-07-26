# Milestone 28 CN4 `Boolean.valueOf` evidence

This sanitized bundle records the focused comparison for static
`Boolean.valueOf(String)` resolution. Captured on 2026-07-24 with Apex Exec
0.1.0, Salesforce API 65.0, and Salesforce CLI 2.144.6, it confirms compile
acceptance and the selected runtime contract.

The guarded disposable Developer Edition org had verified ID
`00DdL000010oTXlUAM`. Local and Salesforce results matched both selected
dimensions:

- compile: 1/1;
- values: 1/1.

The total is 2/2 dimensions (**100.00%**). `Boolean.valueOf` returns `true`
only for a case-insensitive exact `true` string. It returns `false` for the
captured false, arbitrary, and whitespace-padded strings; a null argument
raises `NullPointerException`. After capture, the temporary
`M28CN4BooleanValueOfOracle` Apex class was deleted through the Tooling API and
a guarded query verified no remaining fixture.

Credential-free replay:

```bash
cargo run --locked -- oracle \
  examples/milestone28-cn4-boolean-valueof-oracle/oracle-manifest.json \
  --salesforce-snapshot evidence/milestone28/cn4/salesforce.json \
  --report /tmp/apex-exec-m28-cn4-replay-report.json
cmp evidence/milestone28/cn4/report.json \
  /tmp/apex-exec-m28-cn4-replay-report.json
```
