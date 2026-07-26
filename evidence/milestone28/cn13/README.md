# Milestone 28 CN13 approval-lock-result evidence

This sanitized bundle records the focused comparison for the
`Approval.LockResult` surface selected by the sealed thirteenth census.
Captured on 2026-07-26 with Apex Exec 0.1.0, Salesforce API 65.0, and
Salesforce CLI 2.144.6, it covers type/list identity, deterministic
`JSON.deserialize` construction, `JSON.serialize` and `JSON.serializePretty`,
and typed result/error accessors.

The guarded disposable Developer Edition org had verified ID
`00DdL000010oTXlUAM`. Local and Salesforce results matched both selected
dimensions:

- compile: 1/1;
- values: 1/1.

The total is 2/2 dimensions (**100.00%**). Both success and failure values were
constructed from JSON with a fixed synthetic Account ID. The failure carries
a typed `Database.Error` with
`INSUFFICIENT_ACCESS_ON_CROSS_REFERENCE_ENTITY`; compact output follows the
Salesforce `id`, `success`, `errors` field order, and pretty output round-trips
the same typed value. No approval request, record lock, or DML operation ran.

The local executable regression also checks exact Salesforce pretty-print
spacing for scalar/list values, rejects inconsistent JSON, proves the fixed
4,096-node typed-conversion bound and composed serialization bound, and keeps
`Approval.ProcessResult` and `Approval.UnlockResult` unsupported.

The temporary `M28CN13ApprovalLockResultOracle` Apex class was deleted through
the Tooling API. A separately guarded query verified no remaining fixture.

`candidate-census.json` is a three-rerun candidate measurement against the
unchanged M22 manifest and Salesforce snapshot. The manifest hash remains
`c352505e5ade7662919f4f32fea230a72342e8dccfbf2bf4725b31ae4c47cbcd`, the
snapshot hash remains
`1d0972ced93edca0053675229378fd805e4feae5596f60d60737a237df80ada0`, and the
denominator remains 1,159. Discovery and parsing are 1,159/1,159 while check,
execution, agreement, and strict compatibility remain 0/1,159. CN13 removes
the `Approval.LockResult` first blocker; the same 1,005 tests now first stop at
missing `Approval.ProcessResult`. That is the census-derived next blocker, not
part of this package. CN13 later passed independent review after bounded
generic-placement corrections and was integrated at `95ea6db`; the sealed
census at `evidence/milestone28/census-14/report.json` reproduces the same
funnel and blocker ranking across three deterministic runs.

Credential-free focused replay:

```bash
cargo +1.88.0 run --locked -- oracle \
  examples/milestone28-cn13-approval-lock-result-oracle/oracle-manifest.json \
  --salesforce-snapshot evidence/milestone28/cn13/salesforce.json \
  --report /tmp/apex-exec-m28-cn13-replay-report.json
cmp evidence/milestone28/cn13/report.json \
  /tmp/apex-exec-m28-cn13-replay-report.json
```
