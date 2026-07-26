# North Star grammar census

**Milestone:** M21

**Corpus:** seven pinned files, 14,740 lines, 614,536 bytes

**Executable guard:** `north_star_grammar_census_is_comment_aware_and_stable`
in `tests/milestone21.rs`

This census records the grammar forms that remained after M20. It is computed
from the lexer/parser AST, so comments and string contents cannot create false
matches. The test reparses all seven byte-pinned fixtures and asserts every
count below. The corpus files, fingerprints, and licenses remain unchanged.

## Census and disposition

| Form | Count | Current disposition |
|---|---:|---|
| Non-test annotations | 259 | Lossless AST; typed supported forms are documented below, and unsupported names/effects remain explicit semantic diagnostics |
| Annotation arguments | 115 | Positional and named values retained with source spans |
| `switch on` statements | 8 | Executable scalar labels and typed SObject patterns; unsupported pattern families retain explicit semantic diagnostics |
| `when` arms | 20 | Scalar label lists and typed SObject patterns execute |
| `when else` arms | 2 | Retained; parser requires it to be final |
| Uninitialized local declarators | 48 | Executable as typed null |
| Multi-declarator local statements | 3 | Executable left to right in one lexical scope |
| Multi-expression `for` clauses | 0 in corpus | Executable and covered by focused tests |
| External-ID `upsert` statements | 2 | Schema-backed known SObject types resolve configured external-ID fields; dynamic values and unknown or misconfigured fields remain explicit semantic diagnostics |
| Multi-declarator field statements | 3 | Lossless field-group AST; explicit semantic unsupported diagnostic |
| `final` modifiers | 75 | Existing declaration rules retained; local `final` is an executable exactly-once lexical binding |
| `transient` modifiers | 3 | Supported on fields and omitted from JSON serialization; non-field uses remain explicit semantic diagnostics |
| Static SOQL expressions | 5 | Existing dedicated SOQL AST retained |
| Aggregate select items | 1 | Existing dedicated aggregate AST retained |
| Queries with `LIMIT` | 2 | Existing checked query value nodes retained |
| SOSL / `GROUP BY` / `ORDER BY` / `OFFSET` | 0 | No remaining corpus instance |

The annotation total is:

| Annotation | Count |
|---|---:|
| `@AuraEnabled` | 11 |
| `@InvocableMethod` | 2 |
| `@InvocableVariable` | 36 |
| `@NamespaceAccessible` | 162 |
| `@SuppressWarnings` | 20 |
| `@TestVisible` | 28 |

Known `@IsTest`, `@TestSetup`, and `@future` behavior remains checked and
executable within its existing supported profile. `@AuraEnabled` is typed for
public/global fields and properties, plus public/global static methods; its
`cacheable` and `continuation` options are Boolean method options. It is
runtime-neutral and does not imply Lightning behavior. Accepting other
annotation syntax does not silently provide its Salesforce platform effect.

## Indicator result

M21 passes lexer 7/7 and parser 7/7, for 14/14 ordinary North Star tests.
This is a syntax indicator only. It is not a runtime, platform, enterprise
project, or Salesforce compatibility percentage.
