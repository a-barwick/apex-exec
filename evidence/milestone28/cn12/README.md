# Milestone 28 CN12 single-email-message evidence

This sanitized bundle records the focused comparison for the
`Messaging.SingleEmailMessage` surface used by the frozen enterprise
project. Captured on 2026-07-24 with Apex Exec 0.1.0, Salesforce API 65.0,
and Salesforce CLI 2.144.6, it confirms constructor, setter, getter, static
capacity, send-result, and error-result compile compatibility.

The guarded disposable Developer Edition org had verified ID
`00DdL000010oTXlUAM`. Local and Salesforce results matched both selected
dimensions:

- compile: 1/1;
- values: 1/1.

The total is 2/2 dimensions (**100.00%**). The Salesforce entrypoint
exercised only deterministic message setters and getters; it did not send an
email. A separate local executable test covers capacity reservation and
`Messaging.sendEmail`, records exactly one host invocation, and validates the
typed result members.

No Salesforce data rows were created. The temporary
`M28CN12SingleEmailMessageOracle` Apex class was deleted through the Tooling
API, and a guarded query verified no remaining fixture.

`candidate-census.json` records the unchanged frozen 1,159-test denominator
against the CN12 candidate. Discovery and parsing remain 1,159/1,159 while
strict compatibility remains 0/1,159. The former 1,117-test email blocker is
gone; 1,005 tests now first stop at missing `Approval.LockResult`. This
candidate measurement is not the formal post-integration census.

Credential-free replay:

```bash
cargo run --locked -- oracle \
  examples/milestone28-cn12-single-email-message-oracle/oracle-manifest.json \
  --salesforce-snapshot evidence/milestone28/cn12/salesforce.json \
  --report /tmp/apex-exec-m28-cn12-replay-report.json
cmp evidence/milestone28/cn12/report.json \
  /tmp/apex-exec-m28-cn12-replay-report.json
```
