# Security policy

## Supported versions

Apex Exec has no supported public release yet. The current development branch
receives security fixes, but no version carries a support-duration or response
time commitment.

| Version | Supported |
|---|---|
| Unreleased development branch | Best effort |
| Published releases | None yet |

This status will be revised as part of the open-source release gate after the
owner selects a license and release policy.

## Report a vulnerability

Do not open a public issue for a suspected vulnerability. Use the repository's
[private vulnerability reporting form](https://github.com/a-barwick/apex-exec/security/advisories/new).
If GitHub does not offer that form, contact the repository owner through the
private contact method on the
[owner's GitHub profile](https://github.com/a-barwick) and include the
repository name.

Include:

- the affected commit or version;
- the vulnerable component and configuration;
- reproduction steps or a minimal proof of concept;
- expected impact and prerequisites;
- any known mitigation;
- whether the report or exploit details have been shared elsewhere.

Do not include Salesforce credentials, auth URLs, access tokens, customer
source, or production data. Use synthetic fixtures and redact secrets.

## Handling

Maintainers will keep a valid report private while they reproduce, assess, and
prepare a fix. No response or remediation deadline is promised before a public
support policy is selected. Coordinated disclosure timing will be agreed with
the reporter when practical.

Dependency advisory handling is separately enforced by
[`docs/DEPENDENCY_POLICY.md`](docs/DEPENDENCY_POLICY.md).
