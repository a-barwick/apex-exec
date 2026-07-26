# Milestone 28 CN14 approval-process-result evidence

This sanitized bundle records the focused comparison for the
`Approval.ProcessResult` surface selected by the sealed fourteenth census.
Captured on 2026-07-26 with Apex Exec 0.1.0, Salesforce API 65.0, and
Salesforce CLI 2.144.6, it covers scalar/direct-List and class-literal identity,
deterministic `JSON.deserialize` construction, exact scalar/list compact and
pretty serialization, accessor defaults, and typed result/error values.

The guarded disposable Developer Edition org had verified ID
`00DdL000010oTXlUAM`. Local and Salesforce results matched both selected
dimensions:

- compile: 1/1;
- values: 1/1.

The total is 2/2 dimensions (**100.00%**). Success and failure values were
constructed only through JSON with fixed synthetic IDs. The frozen-source
failure carries a typed `Database.Error` with `NO_APPLICABLE_PROCESS`.
Salesforce emits `actorIds`, `entityId`, `errors`, `instanceId`,
`instanceStatus`, `newWorkitemIds`, and `success` in that exact order. Missing
errors remain null, missing work-item IDs become an empty list, and missing
success becomes false.

No approval request, approval operation, record mutation, or DML ran. The
temporary `M28CN14ApprovalProcessResultOracle` Apex class was deleted through
the Tooling API, and a separately guarded query verified it absent. The earlier
development probe class was cleaned up and separately verified absent as well.

The local executable regressions reject ProcessResult values in Set, Map,
Iterable, nested/custom generic, bodyless signature, cast, type-literal, typed
JSON, enhanced-for, `Database.Batchable`, and generic catch positions. Typed
conversion and composed serialization retain the existing fixed node, depth,
and element budgets. `Approval.UnlockResult`, approval requests,
`Approval.process`, and every other approval operation remain unsupported.

Independent review approved the architecture and implementation logic but
initially requested stronger executable evidence for non-null hidden
actor/instance fields, explicit-null normalization, direct typed-List
deserialization, and exact list/pretty output, plus the pending candidate
census artifact. The oracle, local assertions, Salesforce snapshot, and census
were corrected accordingly. Focused re-review approved all corrections with no
new blocker.

The later integration-owner review of immutable candidate `bd267585` found one
remaining blocker: unknown fields on ProcessResult and nested error objects are
discarded without charging their nested JSON values to the typed conversion
node/depth budget. Independent 5,000-element unknown-array reproductions exited
zero, bypassing the 4,096-node limit. This bundle remains valid Salesforce
provenance for the measured value surface, but it is not integration approval.
CN14 requires bounded accounting regressions and a new reviewed SHA.

Credential-free focused replay:

```bash
cargo +1.88.0 run --locked -- oracle \
  examples/milestone28-cn14-approval-process-result-oracle/oracle-manifest.json \
  --salesforce-snapshot evidence/milestone28/cn14/salesforce.json \
  --report /tmp/apex-exec-m28-cn14-replay-report.json
cmp evidence/milestone28/cn14/report.json \
  /tmp/apex-exec-m28-cn14-replay-report.json
```

`candidate-census.json` records the required three-rerun candidate measurement
against the unchanged Milestone 22 inputs. The manifest and Salesforce
snapshot hashes remain
`c352505e5ade7662919f4f32fea230a72342e8dccfbf2bf4725b31ae4c47cbcd` and
`1d0972ced93edca0053675229378fd805e4feae5596f60d60737a237df80ada0`;
the denominator remains 1,159. Discovery and parsing are 1,159/1,159, while
checking, execution, agreement, and strict compatibility remain 0/1,159.
Matching passes, matching failures, and outcome mismatches are all zero. The
cold/warm/warm runs took 470,593 ms, 103 ms, and 103 ms.

CN14 removes the `Approval.ProcessResult` first blocker. The same 1,005 tests
now first stop at missing `Approval.UnlockResult` in
`LogEntryEventBuilder.cls`. That is the census-derived next blocker only; CN14
does not claim or implement it.
