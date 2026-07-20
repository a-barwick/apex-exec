# ADR 0029: Separate metadata catalog, file accounting, and org discovery

**Status:** Accepted
**Date:** 2026-07-19

## Context

M15 inferred 28 metadata types from paths. Unknown unchanged files disappeared,
multipart full names were truncated, and one percentage could not honestly
describe source recognition, Metadata API transport, drift comparison, and
local runtime semantics. Org `describeMetadata` is also capability- and
org-dependent: it is evidence about one profile and org, not a complete source
format registry.

## Decision

- Pin a generated catalog from
  `@salesforce/source-deploy-retrieve` 12.34.5 and the guarded
  `describeMetadata` results for every modeled API profile.
- Use the union of 527 source-registry parents and 21 additional live-described
  types as the 548-type catalog denominator. Preserve parent/child,
  decomposed, bundle, mixed-content, folder, sidecar, namespace, and multipart
  full-name conventions.
- Give every type explicit inventory, retrieve, deploy, drift, and local
  semantics states. A type absent from one profile's live description is
  `orgUnavailable`, not silently removed.
- Give every package-root file exactly one disposition: recognized metadata,
  intentional non-metadata, or unsupported metadata with a reason.
- Keep project drift and org-only discovery separate. Missing or mutated
  project-owned components block readiness; components found only by
  type-wide org retrieval are reported explicitly.
- Extend strict hybrid evidence to schema 4 so catalog capabilities, file
  dispositions, component accounting, and separate denominators are sealed.

## Consequences

- `apex-exec metadata inventory` reports metadata without compiling Apex.
- Unknown files and unavailable types remain measurable.
- Type-wide configuration retrieval costs more than member-only retrieval but
  can observe org-only additions.
- Schema-3 M25 snapshots remain historical and cannot replay as schema 4.
- Only four catalog types currently have local semantics. Catalog accounting
  does not imply local execution compatibility.
