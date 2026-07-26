# Milestone 28 thirteenth enterprise census

This sanitized three-run report records the frozen Milestone 22 replay after
CN12 `Messaging.SingleEmailMessage` support. The manifest hash is
`c352505e5ade7662919f4f32fea230a72342e8dccfbf2bf4725b31ae4c47cbcd`; the
Salesforce snapshot hash is
`1d0972ced93edca0053675229378fd805e4feae5596f60d60737a237df80ada0`.

All 1,159 tests discover and parse, while 0/1,159 check, execute, agree, or
strictly match. Cold/warm/warm durations were 398,046 ms, 87 ms, and 88 ms.
CN12 removed the missing `Messaging.SingleEmailMessage` diagnostic. The new
first blocker is missing `Approval.LockResult` in
`LogEntryEventBuilder.cls`, affecting 1,005 tests. The remaining first-error
families affect 32 tests or fewer.

Credential-free replay:

```bash
cargo +1.88.0 run --release --locked -- enterprise run \
  benchmarks/milestone22/manifest.json \
  --salesforce evidence/milestone22/salesforce.json \
  --output evidence/milestone28/census-13/report.json
```
