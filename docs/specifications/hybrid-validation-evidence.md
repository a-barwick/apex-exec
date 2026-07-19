# Hybrid Validation Evidence

**Status:** Implemented and live-reviewed in M17, extended by M25. The clean,
replayed, and controlled-blocker artifacts are in `evidence/milestone17/`;
M25 profile evidence is in `evidence/milestone25/`.

This specification defines the observable candidate-bound evidence contract
for `apex-exec hybrid`. It sits above M14 hermetic CI and the M15 hybrid
transport. It does not change Apex lexing, parsing, semantic analysis, or
execution.

## Schema

Validation snapshots use strict schema version 3. Unknown fields and earlier
schema versions are rejected. Schema-2 M17 artifacts retain historical review
value but are not current replay inputs. A snapshot contains:

- the exact serialized M14 manifest SHA-256;
- the M14 cache key and exact CI-result SHA-256;
- canonical changed paths;
- component-selection mode;
- every affected Metadata API selector and component-content SHA-256;
- canonical selected Apex tests and `NoTestRun` or `RunSpecifiedTests`;
- target alias and Salesforce org ID;
- project `sourceApiVersion`;
- the canonical source path, exact API version, typed behavior family, and
  project-default/sidecar origin for every Apex class and trigger;
- Apex Exec and Salesforce CLI versions;
- UTC capture time and maximum permitted evidence age;
- normalized retrieved-inventory SHA-256 and retrieval count;
- check-only deployment and normalized test observations; and
- a SHA-256 over the complete snapshot contents, excluding the seal field.

Changed paths, components, and tests use deterministic sorted order and reject
duplicates. Digests are lowercase 64-character SHA-256 text. Org IDs must be
15 or 18 ASCII-alphanumeric characters beginning with `00D`.

## Authenticated capture

An authenticated run:

1. verifies the hermetic M14 inputs;
2. resolves `sourceApiVersion` and every class/trigger sidecar into exact
   effective profiles;
3. records the installed `sf` version;
4. compiles the project, selects affected components, and produces or reuses a
   cacheable M14 CI artifact;
5. checks the supplied alias through non-verbose `sf org display --json` and
   retains only the org ID;
6. retrieves the affected code plus project-owned schema/configuration twice,
   with the explicit API version, into separate project-local `.apex-exec`
   directories whose `main/default` output trees are prepared before invoking
   the Salesforce CLI and removed after capture;
7. rejects the run before deployment unless both normalized inventory digests
   match;
8. runs an API-version-pinned check-only deployment with the bound component
   selectors and test level, converting method-qualified selected tests to
   unique class names only at the Metadata API command boundary; and
9. seals the sanitized snapshot and readiness report.

`--no-cache` is rejected for authenticated evidence because it cannot guarantee
reuse of the exact M14 result during replay.

## Offline replay

Replay is credential-free and makes no org request. It requires:

- `--validation-snapshot <path>`;
- `--expected-target-org <alias>`;
- `--expected-org-id <00D...>`; and
- `--replay`, so M14 loads the exact cached artifact.

Replay validates errors in this order:

1. manifest inputs, project API version, and effective source profiles;
2. snapshot schema, canonical fields, inventory/request digests, and complete
   snapshot seal;
3. installed Salesforce CLI version, expected target and org ID, Apex Exec
   version, API version, exact maximum-age policy, future-clock bound, and
   expiration;
4. M14 cache key and exact CI-result digest; and
5. changed paths, selection mode, affected selectors/digests, selected tests,
   and test level.

Only after those checks pass does replay evaluate local CI policy, Salesforce
deployment success, unaffected schema/configuration drift, and test-outcome
agreement.

The default maximum evidence age is 24 hours. `--max-evidence-age-hours`
changes the policy for both capture and replay; replay rejects a value that does
not exactly match the recorded policy.

## Sanitization and limitations

The snapshot and report serialize only allowlisted evidence fields. The raw org
display response is never stored, so access tokens, auth URLs, instance URLs,
and verbose credential material do not enter the evidence bundle.

The snapshot seal detects accidental or unreviewed alteration but is not a
digital signature. Offline replay requires the recorded Salesforce CLI version
to be installed. Salesforce `RunSpecifiedTests` accepts class names at this
boundary, so it may execute and report non-selected methods from the same
class; the snapshot retains those observations, but readiness compares only
the exact method-qualified tests sealed in the request. Candidate-bound hybrid
evidence is release evidence, not a Salesforce compatibility percentage and
not an **Exact** language claim.
