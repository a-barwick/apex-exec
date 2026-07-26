# Milestone 28 transient-property evidence

This sanitized bundle records the focused `transient` property comparison for
Salesforce API 65.0. Captured on 2026-07-24 with Apex Exec 0.1.0 and
Salesforce CLI 2.30.8, it checks that a transient property can be declared,
read, and written, and that JSON serialization omits it while retaining an
ordinary property.

The guarded disposable Developer Edition org had verified ID
`00DdL000010oTXlUAM`. Local and Salesforce results matched both selected
dimensions:

- compile: 1/1;
- values: 1/1.

The total is 2/2 dimensions (**100.00%**). The values confirm that both
properties are usable and that JSON contains only `visible`. After capture,
the temporary `M28CNTransientOracle` Apex class was deleted and verified
absent from the org.

Credential-free replay:

```bash
cargo +1.88.0 run --locked -- oracle \
  examples/milestone28-cn-transient-oracle/oracle-manifest.json \
  --salesforce-snapshot evidence/milestone28/cn-transient/salesforce.json \
  --report /tmp/apex-exec-m28-cn-transient-replay-report.json
cmp evidence/milestone28/cn-transient/report.json \
  /tmp/apex-exec-m28-cn-transient-replay-report.json
```
