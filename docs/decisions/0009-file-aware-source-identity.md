# ADR 0009: Give every source unit an explicit identity

**Status:** Accepted
**Date:** 2026-07-16

## Context

HIR side tables use source spans as stable syntax keys. Anonymous compilation
has one byte-offset space, but a project has many files whose local spans all
begin at zero. ADR 0007 resolved collisions by cloning every cached AST and
rebasing every span into one project-wide byte-offset space.

Rebasing made an unchanged syntax tree acquire different coordinates when file
ordering changed, required a recursive mutation walker that duplicated the
entire AST shape, and coupled diagnostics and runtime frames to offset-range
searches. It also caused stack frames from different files to be rendered
against the exception's primary file.

## Decision

Every `Span` carries a `SourceId` plus file-local start and end offsets.
Anonymous source uses a reserved identity so the public single-source API keeps
its existing behavior.

`ProjectCompiler` assigns a stable identity to each cached path and retains it
when that file is reparsed. Project merging combines immutable ASTs without
rewriting spans. `SourceMap` resolves diagnostics, coverage locations, and each
runtime stack frame directly by source identity.

## Consequences

- Equal local offsets in different files are distinct HIR keys.
- Cached ASTs retain their original coordinates and no longer need an
  AST-shaped span-shifting pass.
- Diagnostics and stack frames select their own source file independently.
- Source identities are compiler-session handles, not persistent artifact
  identifiers; serialized incremental artifacts will need a stable remapping
  layer.
- Any synthetic span that must map to a file needs an explicit source identity.
