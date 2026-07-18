# ADR 0017: Compare providers through normalized oracle snapshots

**Status:** Accepted
**Date:** 2026-07-17

## Context

M13 must execute identical conformance fixtures in Apex Exec and a Salesforce
scratch org without moving authentication, CLI transport, log formatting, or
Salesforce-specific response shapes into compiler and runtime phases. Live org
runs are also unsuitable as the only compatibility evidence: they require
credentials, take substantially longer than local tests, and can change as an
org or Salesforce CLI version changes.

Raw debug logs are not a stable comparison format. Apex Exec already owns
structured query, DML, trigger, exception, and test observations, while
Salesforce exposes related behavior through deployment results, execute
anonymous logs, and Apex test JSON.

## Decision

- Conformance suites use a versioned JSON manifest. Each fixture names one SFDX
  project, a compile/invoke/test entry point, and the observable dimensions it
  intends to compare.
- Local execution and Salesforce CLI responses normalize into the same
  versioned snapshot model: compile outcome/category, named JSON values, debug
  output, exceptions/stacks, query effects, DML effects, trigger order, and test
  outcomes.
- The Salesforce adapter owns `sf` command invocation and response/log parsing.
  It does not participate in lexing, parsing, semantic analysis, or runtime
  dispatch.
- A live Salesforce snapshot can be recorded as a durable JSON artifact and
  replayed offline. Differential reports compare only manifest-selected
  dimensions and publish matched/total compatibility coverage overall and per
  dimension.
- Compile diagnostics compare broad categories rather than provider-specific
  wording. Test comparisons use qualified name and outcome; raw messages remain
  in snapshots as evidence. Exception comparisons retain normalized type,
  message, and stack behavior.
- Named runtime values use explicit
  `APEX_EXEC_ORACLE_VALUE|name|json-value` debug markers. Markers are removed
  from ordinary output before comparison.

## Consequences

- Normal compiler and test loops remain org-independent, while an authenticated
  scratch-org run is available through the same CLI.
- Recorded Salesforce evidence can be reviewed, versioned, and reproduced in CI
  without credentials. Newly observed differences are durable when the live
  snapshot/report is committed with its fixture.
- Compatibility claims have a measured denominator chosen explicitly by each
  fixture instead of implying coverage of unobserved behavior.
- Salesforce CLI JSON and log-shape changes are isolated to one adapter and its
  parser tests.
- A recorded match applies only to the fixture dimensions and Salesforce
  environment represented by that snapshot. It is not blanket Apex
  compatibility.
