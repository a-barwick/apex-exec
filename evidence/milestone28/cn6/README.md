# Milestone 28 CN6 generated custom-share evidence

This sanitized bundle records the focused comparison for generated custom
object share types. Captured on 2026-07-24 with Apex Exec 0.1.0, Salesforce API
65.0, and Salesforce CLI 2.144.6, it confirms that the imported
`M28CN6Share__c` metadata exposes the generated `M28CN6Share__Share` type,
custom row-cause constant, and access-level describe values used by the
enterprise blocker.

The guarded disposable Developer Edition org had verified ID
`00DdL000010oTXlUAM`. Local and Salesforce results matched both selected
dimensions:

- compile: 1/1;
- values: 1/1.

The total is 2/2 dimensions (**100.00%**). A guarded `EntityParticle` query
also recorded the generated share's eight Salesforce fields in `schema.json`.
The local schema reproduces that field set and nullability while rejecting an
undeclared sharing-reason constant. This package does not claim
Salesforce-exact share-row visibility propagation.

After capture, the temporary Apex class and custom object were deleted through
the Tooling and Metadata APIs. Guarded queries verified that neither the class
nor the live schema entity remained.

Credential-free replay:

```bash
rustup run 1.88.0 cargo run --locked -- oracle \
  examples/milestone28-cn6-generated-share-oracle/oracle-manifest.json \
  --salesforce-snapshot evidence/milestone28/cn6/salesforce.json \
  --report /tmp/apex-exec-m28-cn6-replay-report.json
cmp evidence/milestone28/cn6/report.json \
  /tmp/apex-exec-m28-cn6-replay-report.json
```
