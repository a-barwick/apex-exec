# Diagnostics

## Status

Single-span lexer, parser, and semantic diagnostics are implemented. Structured
categories, notes, multiple labels, recovery, and runtime stacks are planned.

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
