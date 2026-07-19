# North Star Apex corpus

This directory preserves difficult, real-world Apex source files as milestone
indicators for Apex Exec. The corpus is deliberately not a conformance suite:
passing a lexer or parser goal says that a compiler phase accepts the source,
not that Salesforce behavior is implemented or matched.

## Running the indicators

The normal test suite verifies that every vendored source remains byte-for-byte
pinned. To see the current highest compiler stage for each source:

```bash
cargo test --test north_star reports_current_north_star_progress -- --nocapture
```

All lexer and parser goals are ordinary tests after M21. Run the complete
indicator suite with:

```bash
cargo test --test north_star
```

Run one source or one compiler phase by filtering the test name, for example:

```bash
cargo test --test north_star lexes_puff
cargo test --test north_star parses_rollup_service
```

When a goal becomes part of the supported compatibility surface, promote it
out of `#[ignore]` rather than weakening or replacing the source.

M19 promoted all seven lexer goals, and M21 promoted all seven parser goals.
Those 14 gates measure acceptance of this pinned syntax corpus only;
semantic/runtime compatibility remains covered by focused conformance and
enterprise-project tests. The executable comment-aware M21 census and
construct dispositions are documented in
[`docs/NORTH_STAR_GRAMMAR_CENSUS.md`](../../docs/NORTH_STAR_GRAMMAR_CENSUS.md).

## Selection method

On 2026-07-13, 12 established public Apex repositories were shallow-cloned and
roughly 113,000 lines of `.cls` and `.trigger` production source were surveyed.
Generated `MetadataService.cls` copies, tests, and duplicate packaging trees
were excluded. Candidate production files were ranked using Lizard 1.21.2 in
Java-compatible mode, considering aggregate and maximum cyclomatic complexity,
function count, token count, and non-comment lines. The final set also preserves
syntax that a Java-oriented metric cannot reliably score, such as Apex
generics, properties, annotations, SOQL/DML, casts, and unsigned shifts.

The metrics are selection evidence, not baselines that Apex Exec must reproduce.
Lizard's Java grammar can undercount Apex-specific constructs.

| Fixture | Why it is here | Lines | Aggregate CCN | Max function CCN |
|---|---|---:|---:|---:|
| `SOQL.cls` | Large fluent query DSL; overloads, nested types, generics, and expression density | 3,498 | 623 | 7 |
| `Logger.cls` | Production logging engine with a broad method surface, annotations, and platform types | 4,104 | 475 | 18 |
| `Rollup.cls` | Large rollup engine combining deep control flow, inner types, collections, dynamic queries, and DML | 3,120 | 560 | 31 |
| `RollupService.cls` | DLRS orchestration with batch/schedule behavior, exceptions, savepoints, SOQL, and DML | 1,789 | 261 | 61 |
| `fflib_SObjectDomain.cls` | Enterprise domain/trigger dispatch with inheritance, virtual methods, and inner classes | 1,125 | 176 | 19 |
| `Puff.cls` | DEFLATE/Huffman algorithm port with dense loops, arrays, bitwise operations, and unsigned shifts | 763 | partial | 9 partial |
| `JSONParse.cls` | Compact generic/casting stressor with `instanceof`, exceptions, maps, lists, and regex | 341 | 57 | 7 |

`Puff.cls` is marked partial because the Java-compatible analyzer stopped after
its early methods; that parser mismatch is itself why the file is a useful Apex
syntax North Star.

## Provenance

Every source is an unmodified copy from the named immutable commit. SHA-256 is
recorded here for independent verification, while `tests/north_star.rs` checks a
stable FNV-1a content fingerprint during every normal test run. The upstream
license texts are preserved in [`licenses/`](licenses/).

| Fixture | Upstream source | Commit | License | SHA-256 |
|---|---|---|---|---|
| `SOQL.cls` | [beyond-the-cloud-dev/soql-lib](https://github.com/beyond-the-cloud-dev/soql-lib/blob/bcb5c898620e338fd944ad9b5dffcfd2e63c0424/force-app/main/default/classes/standard-soql/SOQL.cls) | `bcb5c898620e338fd944ad9b5dffcfd2e63c0424` | MIT | `b92cd6771aae533c7aa68a821c4769092b7c5a9ded811d83a4fef626e647b812` |
| `Logger.cls` | [jongpie/NebulaLogger](https://github.com/jongpie/NebulaLogger/blob/32ec1a74670c7d6437b30c51116480f4882959fa/nebula-logger/core/main/logger-engine/classes/Logger.cls) | `32ec1a74670c7d6437b30c51116480f4882959fa` | MIT | `04d496aa9972701452322dd1dcfae510f13a9f2464228e5abc9db4a148912faf` |
| `Rollup.cls` | [jamessimone/apex-rollup](https://github.com/jamessimone/apex-rollup/blob/93991b0b0879d96fea1916a7da85c8cdcf2ecdfe/rollup/core/classes/Rollup.cls) | `93991b0b0879d96fea1916a7da85c8cdcf2ecdfe` | MIT | `88ec936b19ae7412718a041fcc51766566603d62c1585c5a8b2dfe6e461a9296` |
| `RollupService.cls` | [SFDO-Community/declarative-lookup-rollup-summaries](https://github.com/SFDO-Community/declarative-lookup-rollup-summaries/blob/5dbd186bb057bc8aea13d24dcf94d561830376c1/dlrs/main/classes/RollupService.cls) | `5dbd186bb057bc8aea13d24dcf94d561830376c1` | BSD-3-Clause | `e35ccdf43f8b9e3f4e7b26a8d72a32afb34f7583d99bd2d9f4433fdbb690a65b` |
| `fflib_SObjectDomain.cls` | [apex-enterprise-patterns/fflib-apex-common](https://github.com/apex-enterprise-patterns/fflib-apex-common/blob/dc89e51f5419507031d086e87cf9188770b64faa/sfdx-source/apex-common/main/classes/fflib_SObjectDomain.cls) | `dc89e51f5419507031d086e87cf9188770b64faa` | BSD-3-Clause | `d5254859c263c311a3e7ec6e0c2f63e1c25c23e2eb22257dbc93ad6451bdc070` |
| `Puff.cls` | [pdalcol/Zippex](https://github.com/pdalcol/Zippex/blob/9b04e9036d114418f35e05ca909e9cefdf0d44d2/classes/Puff.cls) | `9b04e9036d114418f35e05ca909e9cefdf0d44d2` | MIT | `b488d795c80d78a0f06d1349dc55e57e23cd7444556f5c08fa861863e8759a56` |
| `JSONParse.cls` | [open-force/jsonparse](https://github.com/open-force/jsonparse/blob/69da320c172da34d4112835509184a31ce500f80/JSONParse.cls) | `69da320c172da34d4112835509184a31ce500f80` | MIT | `80737d8d2e082a53f4aadf280e82b9a6d193cf8c007cd5564887c6e16242248c` |

## Maintenance rules

- Do not reformat or minimize corpus files.
- Pin upgrades to an immutable upstream commit and update the source link,
  license if needed, line/byte counts, SHA-256, and FNV-1a fingerprint together.
- Add a file for a distinct grammar or execution challenge, not merely because
  it is longer than an existing fixture.
- Keep generated sources out of the primary corpus. They may become a separate
  scale benchmark later.
