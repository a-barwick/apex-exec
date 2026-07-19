# Dependency policy

Dependency findings are release inputs, not informational CI noise. The
required checks cover the locked Rust graph and the complete website graph,
including build and development tooling.

## Rust

CI runs both:

```bash
cargo audit --deny warnings
cargo deny check advisories bans sources
```

`.cargo/audit.toml` and `deny.toml` contain no advisory exceptions. Published
RustSec vulnerabilities, yanked crates, disallowed wildcard requirements, and
unknown registries or Git sources fail. Duplicate crate versions are visible
warnings under Cargo Deny rather than an immediate release blocker because
the current graph contains legitimate transitive duplication.

Adding a Rust advisory exception requires a separate reviewed change that
records the advisory ID, affected path and feature, exposure analysis,
unavailable remediation, owner, expiry date, and removal trigger. S0-05 adds
no such exception.

## Website

CI runs the complete npm audit through:

```bash
python3 tools/dependencies/check_npm_audit.py
```

The checker rejects operational audit failures, new advisory IDs, severity
changes, new affected dependency paths, and expired allowances. The only
current allowance is
[GHSA-qx2v-qp2m-jg93](https://github.com/advisories/GHSA-qx2v-qp2m-jg93),
the moderate PostCSS stringification issue embedded by Next. The four npm
vulnerability records (`postcss`, `next`, `@unpic/react`, and `vinext`) all
resolve to that one advisory.

As of 2026-07-18, `npm audit fix` removes every other finding. For this
advisory npm proposes only `npm audit fix --force`, which would replace Next
16.2.10 with the incompatible Next 9.3.3. The site does not accept
user-authored CSS, reducing exposure, but that is not treated as a fix. The
checked-in allowance records the rationale, exact paths, review triggers, and
a 2026-08-18 expiry. Any lockfile change or new relevant release requires
review before that date.

The production-only reproduction remains:

```bash
npm audit --omit=dev --audit-level=moderate
```

It exits nonzero with two moderate package records for the same advisory.
That expected nonzero reproduction is not labeled a pass; the policy checker
is the enforceable gate.
