# Milestone 28 CN2 nested-enum equality evidence

This sanitized bundle records the focused comparison for equality between a
nested enum property whose declared type retains the short source spelling and
the same enum constant referenced through its fully qualified type name.
Captured on 2026-07-24 with Apex Exec 0.1.0, Salesforce API 65.0, and
Salesforce CLI 2.144.6, it confirms that both equality and inequality compile
and evaluate as expected.

The guarded disposable Developer Edition org had verified ID
`00DdL000010oTXlUAM`. Local and Salesforce results matched both selected
dimensions:

- compile: 1/1;
- values: 1/1.

The total is 2/2 dimensions (**100.00%**). Both `same` and `different` are
`true`. After capture, the temporary `M28CN2EnumContainer` and
`M28CN2EqualityOracle` Apex classes were deleted and a Tooling API query
verified zero remaining active fixture classes.

Credential-free replay:

```bash
cargo run --locked -- oracle \
  examples/milestone28-cn2-equality-oracle/oracle-manifest.json \
  --salesforce-snapshot evidence/milestone28/cn2/salesforce.json \
  --report /tmp/apex-exec-m28-cn2-replay-report.json
cmp evidence/milestone28/cn2/report.json \
  /tmp/apex-exec-m28-cn2-replay-report.json
```
