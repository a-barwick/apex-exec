# Milestone 28 seventh enterprise census

This sanitized three-run report records the frozen Milestone 22 replay after
CN6 generated custom-share SObject support. The manifest hash is
`c352505e5ade7662919f4f32fea230a72342e8dccfbf2bf4725b31ae4c47cbcd`; the
Salesforce snapshot hash is
`1d0972ced93edca0053675229378fd805e4feae5596f60d60737a237df80ada0`.

All 1,159 tests discover and parse, while 0/1,159 check, execute, agree, or
strictly match. Cold/warm/warm durations were 223,670 ms, 73 ms, and 73 ms.
CN6 removed generated `Log__Share` resolution as the first blocker. The new
first blocker is the `System.OrgLimit` type in `LogHandler.cls`, affecting
1,121 tests; `Flow.Interview` affects 18, and the remaining two first-error
families affect 15 and 5 tests.

Credential-free replay:

```bash
cargo +1.88.0 run --release --locked -- enterprise run \
  benchmarks/milestone22/manifest.json \
  --salesforce evidence/milestone22/salesforce.json \
  --output evidence/milestone28/census-7/report.json
```
