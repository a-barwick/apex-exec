# Milestone 28 CN5 `Integer.valueOf` evidence

This sanitized bundle records the focused comparison for static
`Integer.valueOf` resolution. Captured on 2026-07-24 with Apex Exec 0.1.0,
Salesforce API 65.0, and Salesforce CLI 2.144.6, it confirms compile acceptance
and the selected String and Integer runtime contracts.

The guarded disposable Developer Edition org had verified ID
`00DdL000010oTXlUAM`. Local and Salesforce results matched both selected
dimensions:

- compile: 1/1;
- values: 1/1.

The total is 2/2 dimensions (**100.00%**). Signed base-10 String values convert
to checked 32-bit Integers. Invalid String content raises `TypeException`, a
String null raises `NullPointerException`, and a typed Integer null remains
null. The package intentionally does not claim Salesforce's broader
`Integer.valueOf(Object)` conversion surface.

After capture, the temporary `M28CN5IntegerValueOfOracle` Apex class was
deleted through the Tooling API and a guarded query verified no remaining
fixture.

Credential-free replay:

```bash
cargo run --locked -- oracle \
  examples/milestone28-cn5-integer-valueof-oracle/oracle-manifest.json \
  --salesforce-snapshot evidence/milestone28/cn5/salesforce.json \
  --report /tmp/apex-exec-m28-cn5-replay-report.json
cmp evidence/milestone28/cn5/report.json \
  /tmp/apex-exec-m28-cn5-replay-report.json
```
