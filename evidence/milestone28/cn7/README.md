# Milestone 28 CN7 organization-limits evidence

This sanitized bundle records the focused comparison for
`System.OrgLimits.getMap()` and the `System.OrgLimit` accessors. Captured on
2026-07-24 with Apex Exec 0.1.0, Salesforce API 65.0, and Salesforce CLI
2.144.6, it confirms compile acceptance and the selected map, name, usage, and
capacity contracts.

The guarded disposable Developer Edition org had verified ID
`00DdL000010oTXlUAM`. Local and Salesforce results matched both selected
dimensions:

- compile: 1/1;
- values: 1/1.

The total is 2/2 dimensions (**100.00%**). Both qualified and unqualified
`OrgLimits` calls expose a map containing `SingleEmail`; its value reports the
same name as the map key, nonnegative usage, and available capacity. Local
organization-limit values come from the explicit platform-host snapshot. The
default recording host supplies a deterministic `SingleEmail` value of 0/15,
and custom hosts must either supply a snapshot or report the capability as
unavailable.

After capture, the temporary `M28CN7OrgLimitsOracle` Apex class was deleted
through the Tooling API and a guarded query verified no remaining fixture.

Credential-free replay:

```bash
cargo run --locked -- oracle \
  examples/milestone28-cn7-org-limits-oracle/oracle-manifest.json \
  --salesforce-snapshot evidence/milestone28/cn7/salesforce.json \
  --report /tmp/apex-exec-m28-cn7-replay-report.json
cmp evidence/milestone28/cn7/report.json \
  /tmp/apex-exec-m28-cn7-replay-report.json
```
