# ADR 0028: Bind compatibility profiles per source

**Status:** Accepted
**Date:** 2026-07-19

## Context

Salesforce selects language and runtime behavior from API metadata. An SFDX
project supplies `sourceApiVersion`, while Apex class and trigger sidecars can
select a different effective version. Apex Exec previously used the string
`m10-common` only in selected platform diagnostics. The compiler, runtime,
cache, oracle, and validation layers otherwise had no shared version identity.

Scattered version comparisons would let semantic checking and execution
disagree, allow a sidecar-only change to reuse stale checked results, and make
recorded evidence ambiguous. Treating every historical Salesforce version as
equivalent would also turn unsupported behavior into a silent approximation.

## Decision

Project discovery resolves one exact typed `CompatibilityProfile` for every
`.cls` and `.trigger` source. A valid matching sidecar takes precedence over
the required project default. Profile selection is complete before lexing and
parsing; source text and spans remain unchanged.

The initial closed catalog models:

- API 31.0 as `LegacyApi31`; and
- API 60.0 through 66.0 as `CurrentApi60To66`.

Exact API identity is retained even when multiple versions share a reviewed
behavior family. Versions outside that catalog, nonzero minor versions, absent
project defaults, and malformed sidecars fail explicitly.

Semantic HIR stores the per-source map. Runtime expression entry updates the
typed execution context from the expression span; asynchronous work inherits
that context. Host calls accept typed profiles where behavior crosses the
platform boundary. Current-only null-aware syntax and curated platform APIs
are rejected while checking legacy sources. Null `instanceof` execution uses
the reviewed API 31.0 versus current behavior.

Parsed-source cache identity remains source-text based so a sidecar-only change
can reuse immutable AST. Checked-unit fingerprints include the exact profile,
so that same change invalidates semantic/runtime results. CI cache keys and
results, oracle observations/comparison dimensions, and hybrid validation
schema 3 all carry canonical exact per-source profiles. Replay rejects any
effective-profile mismatch.

## Consequences

- Mixed-version projects have one authoritative precedence rule and one typed
  profile identity across compiler, runtime, host, CI, oracle, and validation.
- Adding a reviewed behavior family requires a catalog entry and explicit
  semantic/runtime disposition rather than a default fallthrough.
- Parsed AST reuse survives metadata-only profile changes, while checked and
  executable state cannot become stale.
- Hybrid schema 2 evidence remains a historical artifact but is intentionally
  incompatible with current schema-3 replay because it lacks per-source
  profile binding.
- API 32.0–59.0, API 67.0 and later, historical conversion quirks, and the
  complete versioned platform surface remain explicitly unmodeled.
- The broader single intrinsic descriptor/handler catalog remains S1-05 work;
  this decision supplies its typed profile boundary without claiming that
  consolidation is complete.
