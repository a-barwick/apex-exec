# ADR 0001: Begin with a tree-walking interpreter

**Status:** Accepted  
**Date:** 2026-07-10

## Context

The early milestones prioritize language correctness, diagnostics, and rapid
grammar iteration. A bytecode compiler or optimized VM would add substantial
design work before the supported language can express useful programs.

## Decision

Execute the semantically checked AST directly with a tree-walking interpreter.
Keep semantic analysis separate so execution never performs name or type
resolution as a substitute for compilation.

Introduce a typed high-level or executable intermediate representation before
classes, inheritance, and overload resolution make direct AST execution
unwieldy.

## Consequences

- Early features have a short implementation path.
- AST nodes and runtime values remain easy to inspect during development.
- Compiler phases must preserve clean boundaries to make later lowering viable.
- Performance is secondary during the early language milestones.
- This decision does not commit the mature runtime to AST execution.
