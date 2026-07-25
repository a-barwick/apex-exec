# Milestone 28 third enterprise census

This sanitized report records the frozen Milestone 22 enterprise replay after
the nested-enum equality slice. It uses the checked-in manifest and Salesforce
snapshot and is credential-free to replay. The report was generated with Apex
Exec 0.1.0 and contains three deterministic runs.

The manifest hash is
`c352505e5ade7662919f4f32fea230a72342e8dccfbf2bf4725b31ae4c47cbcd` and the
Salesforce snapshot hash is
`1d0972ced93edca0053675229378fd805e4feae5596f60d60737a237df80ada0`.

| Stage | Count | Denominator |
|---|---:|---:|
| Discovery | 1,159 | 1,159 |
| Parse | 1,159 | 1,159 |
| Check | 0 | 1,159 |
| Execution | 0 | 1,159 |
| Salesforce agreement | 0 | 1,159 |
| Strict compatible | 0 | 1,159 |

The cold, warm, and warm runs took 210,227 ms, 69 ms, and 70 ms. The first
blockers are:

- 1,121 tests: cast from `Map<Id,SObject>` to `Map<Id,Log__c>` is unsupported;
- 18 tests: unknown `Flow.Interview`;
- 15 tests: unknown `System` for
  `System.FeatureManagement.checkPermission`; and
- 5 tests: an additional unknown `System` site.

The next-ranked family is the typed-SObject `Map` cast. No implementation of
that family has started.

Credential-free replay:

```bash
cargo +1.88.0 run --release --locked -- enterprise run \
  benchmarks/milestone22/manifest.json \
  --salesforce evidence/milestone22/salesforce.json \
  --output evidence/milestone28/census-3/report.json
```
