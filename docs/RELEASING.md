# Release process

Apex Exec does not yet have an authorized public release. This document
provides a reproducible checklist without choosing the owner-reserved license,
public API, distribution channel, or long-term support policy.

## Hard blockers

The repository owner must explicitly:

1. select the project license and add the corresponding repository and Cargo
   metadata;
2. choose a binary-first product policy or identify the Rust modules covered
   by semantic-version compatibility;
3. approve the final stabilization integration and the first public release.

`Cargo.toml` keeps `publish = false` until those decisions are recorded. Public
Rust modules are implementation visibility, not a supported semver promise.
No release tag, crate publication, binary upload, or public hosting action
should bypass these blockers.

## Required repository rule

Protect `main` and the stabilization integration branch with the stable
required check:

```text
Required CI gate
```

Require the branch to be current before merge and do not allow administrators
or merge automation to bypass a failing or missing required check. The
workflow's final aggregation job fails when any Rust, website,
maintainability, dependency, or editor layer fails or is cancelled.

Repository rules are GitHub-hosted state and are not activated merely by
committing a workflow file. The integration owner must verify the rule in the
repository settings before declaring the S0 release-document gate complete.

## Candidate checklist

After the owner decisions and S0 integration:

1. Start from a clean clone of the exact candidate commit.
2. Confirm `Cargo.toml`, `CHANGELOG.md`, public status copy, and the selected
   license agree with the intended version and distribution.
3. Review every dependency allowance and remove fixed or expired entries.
4. Run the full required CI workflow and all milestone-specific CLI examples.
5. Run `cargo package --locked` only if crate distribution is authorized.
6. Build release binaries on the selected supported platforms.
7. Smoke-test each packaged binary from outside the repository.
8. Generate SHA-256 checksums and retain build provenance.
9. Create a signed or otherwise owner-approved `v<version>` tag.
10. Publish release notes from `CHANGELOG.md` and link the exact verification
    evidence.

The repository intentionally has no publishing workflow yet. Adding one is
part of the later open-source release gate and must encode the chosen license,
API, artifact, signing, and access policies rather than guessing them here.

## Changelog discipline

User-visible changes enter `CHANGELOG.md` under `Unreleased` as Added, Changed,
Deprecated, Removed, Fixed, or Security. At release time, move those entries
under a dated version heading and create a fresh `Unreleased` section. Do not
rewrite historical release notes except to correct an error transparently.
