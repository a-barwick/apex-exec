# ADR 0019: Compose hybrid validation above hermetic CI

**Status:** Accepted
**Date:** 2026-07-17

## Context

M15 must reserve Salesforce compute for affected final validation while
remaining deterministic and useful without org credentials. Reimplementing
dependency selection, tests, or policy inside an org adapter would create a
second CI authority. Comparing current source directly with an org would also
mislabel intended release changes as drift, while persisting Salesforce auth
material would make reports unsafe and non-portable.

## Decision

Treat the sealed M14 CI manifest and result as the local release-candidate
authority. Build a provider-neutral metadata inventory above project
compilation, group source and sidecars by Metadata API identity, and reuse the
compiler dependency graph for affected Apex components. Fall back to the
complete project for metadata, triggers, deletions, and unknown paths.

Compare project-owned schema and configuration only when a component is not
directly changed. Validate intended payload through an authenticated,
check-only Salesforce deployment and compare the selected local test outcomes
with its normalized test results.

Keep Salesforce transport optional. The live adapter checks an existing alias
without verbose auth output, retrieves scoped metadata into an isolated
temporary directory, and never stores credentials. A versioned validation
snapshot records only inventory, org identity, deployment observations, and
test outcomes so the release decision can be replayed offline.

## Consequences

- Compiler, runtime, oracle, and CI phase ownership remains unchanged.
- Local policy, org validation, drift, and test differential have one explicit
  release-readiness result.
- Intended metadata changes are not reported as environmental drift.
- Unknown dependency effects cost more Salesforce validation but cannot
  silently omit components or tests.
- Offline replay is deterministic and credential-free, but its confidence is
  scoped to the age and target of the recorded snapshot.
- Drift covers project-owned supported metadata; discovering arbitrary org-only
  configuration requires a broader future Metadata API inventory.
