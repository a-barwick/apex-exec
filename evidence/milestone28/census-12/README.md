# Milestone 28 twelfth enterprise census

This sanitized three-run report records the frozen Milestone 22 replay after
CN11 single-record dynamic-query support. The manifest hash is
`c352505e5ade7662919f4f32fea230a72342e8dccfbf2bf4725b31ae4c47cbcd`; the
Salesforce snapshot hash is
`1d0972ced93edca0053675229378fd805e4feae5596f60d60737a237df80ada0`.

All 1,159 tests discover and parse, while 0/1,159 check, execute, agree, or
strictly match. Cold/warm/warm durations were 227,540 ms, 74 ms, and 73 ms.
CN11 removed the `List<SObject>`-to-`Log__c` cast diagnostic. The new first
blocker is missing `Messaging.SingleEmailMessage` in `LoggerEmailSender.cls`,
affecting 1,117 tests. The remaining first-error families affect 18, 15, 5, 3,
and 1 tests.

Credential-free replay:

```bash
cargo run --release --locked -- enterprise run \
  benchmarks/milestone22/manifest.json \
  --salesforce evidence/milestone22/salesforce.json \
  --output evidence/milestone28/census-12/report.json
```
