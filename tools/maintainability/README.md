# Maintainability ratchet

The Lizard gate records existing production complexity debt without treating
that debt as a passing quality target. It uses Lizard 1.21.2 in Rust mode over
`src`, with warnings defined as strictly greater than 80 NLOC or 15 cyclomatic
complexity.

Run the local gate with:

```bash
tools/maintainability/check_lizard.sh
```

Each recorded function has independent NLOC and cyclomatic-complexity caps. A
recorded function may improve, but either metric growing above its committed
cap fails. Any unrecorded function crossing either threshold also fails.
Function identity uses the repository path, function name, Lizard-normalized
signature, and an occurrence index for otherwise identical definitions; line
numbers are evidence only.

The S0-05 baseline is the exact result at integration claim checkpoint
`8ef94d8`, where the pinned command reports 73 threshold violations. The
earlier audit described approximately 74 warnings across a pre-claim audit
snapshot. This baseline preserves that historical finding and records the
reproducible checkpoint result rather than rewriting the earlier estimate.

Regenerating the JSON is not an ordinary implementation step. A reviewed
refactor may render a candidate with:

```bash
uvx --from lizard==1.21.2 lizard \
  src \
  --languages rust \
  --CCN 15 \
  --Threshold nloc=80 \
  --csv \
  --ignore_warnings -1 |
  python3 tools/maintainability/check_lizard.py render-baseline \
    --source-revision <reviewed-commit> \
    --captured-on <YYYY-MM-DD>
```

Reviewers must verify that a baseline change removes resolved debt or
deliberately records a separately approved exception; it must not normalize a
regression.
