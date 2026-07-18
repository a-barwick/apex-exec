# ADR 0021: Bind live validation to sealed candidate evidence

**Status:** Accepted
**Date:** 2026-07-18

## Context

M15 records provider-neutral Salesforce inventory, deployment, and test
observations, but its version-1 snapshot does not prove that those observations
belong to the release candidate being replayed. A changed manifest can retain
the same selected test names and accidentally reuse stale org results. The old
snapshot also omits capture age, API version, tool provenance, and the exact
affected request.

M17 must close that gap without moving Salesforce concerns into compiler or
runtime phases and without requiring credentials during offline replay. A live
retrieve can also vary because of CLI conversion or server output, so one
successful retrieval is insufficient evidence that drift normalization is
stable.

## Decision

- Replace hybrid snapshot schema version 1 with a strict version-2 evidence
  envelope. The envelope records SHA-256 identities for the serialized M14
  manifest, exact cached CI result, canonical validation request, retrieved org
  inventory, and complete snapshot.
- Canonical request identity includes changed paths, component-selection mode,
  affected Metadata API selectors and content digests, selected tests, and the
  resulting Salesforce test level.
- Bind evidence to the target alias, 15- or 18-character org ID, project
  `sourceApiVersion`, Apex Exec version, Salesforce CLI version, UTC capture
  time, and an exact maximum-age policy.
- Require live capture to use a cacheable M14 artifact. Offline replay must use
  M14 replay-only mode and reproduce the same cache key and result digest.
- Require replay callers to assert the expected target alias and org ID. Reject
  target, org, API/tool version, maximum-age, expiration, candidate, request,
  inventory, or snapshot-integrity mismatches before release readiness is
  evaluated.
- Retrieve the complete drift/affected scope twice into isolated temporary
  directories. Normalize both inventories through the existing provider-neutral
  inventory boundary and stop before deployment if their digests differ.
- Persist only the allowlisted evidence model. Never serialize the Salesforce
  authentication response, access token, auth URL, instance URL, or verbose org
  display output.

## Consequences

- A reviewed snapshot can no longer approve a different manifest, CI outcome,
  affected request, target, toolchain, API version, or evidence-age policy.
- Credential-free replay still requires the recorded Salesforce CLI version to
  be installed locally; it does not contact the org.
- Updating Salesforce CLI, Apex Exec, project API version, or the maximum-age
  policy intentionally invalidates existing evidence and requires a new live
  capture.
- Live validation performs two metadata retrieves, increasing Salesforce time
  and API consumption in exchange for explicit normalization evidence.
- The SHA-256 seal provides deterministic integrity and mismatch detection, not
  third-party authenticity or non-repudiation. Repository review and trusted CI
  artifact handling remain required.
- Hybrid evidence continues to prove release readiness only. It does not make a
  language behavior **Exact** without matching M13 conformance evidence.
