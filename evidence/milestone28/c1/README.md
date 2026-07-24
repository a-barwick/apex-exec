# Milestone 28 C1 Salesforce evidence

This sanitized bundle records the focused `Id.getSObjectType()` comparison
for Salesforce API 65.0. The fixture compares a known Account ID, safe
navigation on a null ID, and the catchable failure for an unknown key prefix.
No credentials, auth URLs, tokens, or raw CLI responses are stored.

The fixture was captured against the guarded disposable Developer Edition org
whose verified ID is `00DdL000010oTXlUAM` on 2026-07-24. The live run used
Apex Exec 0.1.0 and Salesforce CLI 2.30.8. Local and Salesforce results
matched both selected dimensions:

- compile: 1/1;
- values: 1/1.

The total is 2/2 dimensions (**100.00%**). After capture, the temporary
`M28C1IdOracle` Apex class was deleted from the org and the deletion was
verified.

Credential-free replay used:

```bash
cargo +1.88.0 run --locked -- oracle \
  examples/milestone28-c1-oracle/oracle-manifest.json \
  --salesforce-snapshot evidence/milestone28/c1/salesforce.json \
  --report /tmp/apex-exec-m28-c1-replay-report.json
cmp evidence/milestone28/c1/report.json \
  /tmp/apex-exec-m28-c1-replay-report.json
```

The replay matched byte for byte. A guarded post-capture check found zero
active `M28C1IdOracle` Apex classes in the validation org.
