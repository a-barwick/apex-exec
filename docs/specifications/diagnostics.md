# Diagnostics

## Status

Single-span lexer, parser, and semantic diagnostics are implemented. M4 runtime
failures additionally carry an exception type and ordered source call frames.
Stable diagnostic codes, notes, multiple labels, and recovery remain planned.

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
from the human renderer. Compile diagnostics leave both empty. Stack frames
currently identify method call sites in one source file; cross-file names and
Salesforce-exact formatting depend on M5 project compilation and later
differential validation.

## Future diagnostic categories

- Lexical errors
- Syntax errors
- Unsupported syntax
- Name-resolution errors
- Type errors
- Unsupported platform APIs
- Runtime exceptions
- Compatibility mismatches

The internal representation should eventually carry a stable category or code,
primary and secondary spans, notes, and suggested fixes. Human wording may
improve without breaking integrations.
