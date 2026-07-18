# ADR 0016: Debug through deterministic runtime snapshots

**Status:** Accepted
**Date:** 2026-07-17

## Context

M12 requires interactive editor debugging without allowing protocol threads,
editor timing, or background pauses to change Apex execution. The tree-walking
interpreter already owns scopes, values, calls, coverage observations, and the
platform transaction timeline. Letting a DAP adapter reach into those private
structures during execution would couple transport state to language semantics
and make deterministic replay harder.

## Decision

- The interpreter captures immutable debugger snapshots immediately before
  executable statements. Each snapshot contains its source span, source-mapped
  call frames, visible variables rendered through the runtime's value
  formatter, and the visible transaction-timeline boundary.
- One launch executes synchronously to completion or failure. A debugger
  session navigates the resulting trace for entry stops, verified breakpoints,
  step-in, step-over, and step-out.
- DAP and LSP remain thin `Content-Length` framed JSON adapters. They use public
  debugger and editor services rather than owning compiler or runtime
  semantics.
- The project compiler source map remains the authority for debugger, editor,
  diagnostic, coverage, definition, reference, and rename locations.
- REPL snippets commit only after the accumulated source checks and executes.
  Accepted input is deterministically replayed to reconstruct persistent state.

## Consequences

- Debugging cannot introduce races or change program behavior based on editor
  response time.
- Protocol tests can exercise complete sessions without threads, sockets, or
  wall-clock waits.
- Breakpoints inspect statement-boundary snapshots rather than a suspended live
  interpreter. Runtime mutation/evaluation from a debug console is deferred.
- REPL replay is simple and deterministic but is not suitable for
  nondeterministic external side effects; the current platform profile already
  excludes those effects.
