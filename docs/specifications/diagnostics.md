# Diagnostics

## Status

Single-span lexer, parser, and semantic diagnostics are implemented. Runtime
failures additionally carry an exception type and ordered source call frames.
Project diagnostics retain file-aware source identities, and project errors
expose broad machine-readable error kinds. Stable per-diagnostic codes, notes,
multiple labels, and recovery remain planned.

## Requirements

Diagnostics must:

- Identify the source file, line, and column.
- Highlight the relevant source span.
- Preserve the spelling used in source.
- Distinguish invalid syntax from unsupported behavior as the error model grows.
- Avoid claiming compatibility with Salesforce's exact diagnostic wording.
- Be deterministic and suitable for future machine-readable output.

## Current rendering

```text
error: unknown variable `mesage`
 --> script.apex:2:14
  |
2 | System.debug(mesage);
  |              ^^^^^^^
```

Unhandled M4 exceptions retain the same primary source highlight and append
the exception type plus an innermost-to-outermost Apex call stack:

```text
error: MathException: division by zero
 --> script.apex:2:15
  |
2 |     return 10 / divisor;
  |               ^
Apex stack trace:
  at divide (script.apex:2:15)
  at outer (script.apex:6:12)
```

The public `Diagnostic` exposes `exception_type` and `stack_trace` separately
from the human renderer. Compile diagnostics leave both empty. Single-source
rendering maps spans against the supplied source text. Project
`ProjectError::render()` resolves the primary span and every runtime frame
independently through its `SourceId`, so one stack may name multiple files.
Salesforce-exact formatting still requires later differential validation.

## M16 expression diagnostics

**Implemented.** A missing ternary arm or colon is a parser diagnostic.
Non-Boolean conditions, Void arms, unknown runtime target types, impossible
`instanceof` relationships, and statically always-true runtime-type tests are
semantic diagnostics. Null Boolean conditions are catchable runtime
`NullPointerException` values. Safe navigation and null coalescing tokenize
through the shared `?` punctuation but remain explicit parser errors until
M18.

## Planned structured diagnostics

The renderer already distinguishes lexical, syntax, name/type, unsupported
platform, and runtime failures in human-readable text. A future stable code
system should represent those categories plus compatibility mismatches without
making callers parse wording.

The internal representation should eventually carry primary and secondary
spans, notes, and suggested fixes alongside that code. Human wording may then
improve without breaking integrations.

## M21 grammar-only diagnostics

**Implemented.** Arbitrary annotations, switch arms, external-ID DML fields,
multi-declarator fields, and `transient` syntax retain their spelling and spans.
Where executable semantics are not implemented, semantic checking reports an
explicit unsupported diagnostic. Parser acceptance never falls through to
runtime approximation.
