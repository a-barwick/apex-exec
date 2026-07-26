# Milestone 28 CN11 dynamic-query single-record cast evidence

This sanitized bundle records the focused comparison for casting a
`Database.query` result directly to its concrete SObject type. Captured on
2026-07-24 with Apex Exec 0.1.0, Salesforce API 65.0, and Salesforce CLI
2.144.6, it confirms compile acceptance and the one-row runtime contract.

The guarded disposable Developer Edition org had verified ID
`00DdL000010oTXlUAM`. Local and Salesforce results matched both selected
dimensions:

- compile: 1/1;
- values: 1/1.

The total is 2/2 dimensions (**100.00%**). A one-row dynamic query casts or
assigns directly to the concrete record, while zero and multiple rows each
raise a catchable `QueryException`. The checker keeps this behavior specific
to typed single-record dynamic queries and continues to reject casting an
arbitrary `List<SObject>` to one SObject.

The comparison created and deleted two temporary Account rows inside a
`finally` block. A guarded query verified no rows remained. The temporary
`M28CN11DynamicQuerySingleCastOracle` Apex class was then deleted through the
Tooling API and a guarded query verified no remaining fixture.

Credential-free replay:

```bash
cargo run --locked -- oracle \
  examples/milestone28-cn11-dynamic-query-single-cast-oracle/oracle-manifest.json \
  --salesforce-snapshot evidence/milestone28/cn11/salesforce.json \
  --report /tmp/apex-exec-m28-cn11-replay-report.json
cmp evidence/milestone28/cn11/report.json \
  /tmp/apex-exec-m28-cn11-replay-report.json
```
