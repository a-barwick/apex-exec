# Milestone 28 enterprise census evidence

This sanitized report records the first post-C1 replay of the frozen
Milestone 22 enterprise denominator. It uses the checked-in manifest and
Salesforce snapshot, so it is credential-free to replay. The report was
generated with Apex Exec 0.1.0 and contains three deterministic runs.

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

The cold, warm, and warm runs took 220,015 ms, 78 ms, and 75 ms. C1 removed
the prior `Id.getSObjectType` blocker. The new first blockers are:

- 1,126 tests: `transient` on a property is parsed but unsupported;
- 18 tests: unknown `Flow.Interview`;
- 15 tests: unknown `System` for `System.FeatureManagement.checkPermission`.

The next-ranked family is `transient` property semantics. No implementation of
that family has started.

Credential-free replay:

```bash
cargo +1.88.0 run --release --locked -- enterprise run \
  benchmarks/milestone22/manifest.json \
  --salesforce evidence/milestone22/salesforce.json \
  --output evidence/milestone28/census-1/report.json
```
