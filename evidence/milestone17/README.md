# Milestone 17 live Salesforce evidence

This bundle records the reviewed M17 run against the user-supplied disposable
Developer Edition. It contains no credentials, access tokens, auth URLs,
instance URLs, refresh tokens, or passwords.

- Candidate manifest SHA-256:
  `083fa8e495a2a5e79ab11e0ffb68753548167eb9971cd5e37eee27b9d9f6a002`
- Clean snapshot seal:
  `95359bcc2cdd7ef4a83410eb2201b05d6ccac462586f66d4f3a856829078c052`
- Controlled-drift snapshot seal:
  `c6b29687ca18ff9c8f26bc43ae8baf896c3ac1910ef5a23e3e1825f641201fea`
- Target alias: `apex-exec-m17`
- Org ID: `00DdL000010oTXlUAM`
- Project API version: `65.0`
- Salesforce CLI: `2.143.6`
- Clean capture: `2026-07-18T19:26:29Z`
- Evidence age policy: 24 hours

The authenticated clean run performed two matching scoped retrievals, passed
the check-only deployment, matched both selected Apex test methods, and found
zero schema/configuration drift. Offline replay reproduced the same ready
decision and clean snapshot seal without an org request.

For the controlled blocker, only the disposable org's
`PermissionSet:Release_Manager` label was changed. Both Salesforce tests and
the check-only deployment still passed, but the unchanged configuration digest
mismatch produced one drift finding and blocked release. The exact baseline was
then restored and the final clean capture was repeated.

`clean-validation.json` and `clean-readiness.json` are the final authenticated
passing snapshot and report. `clean-replay-readiness.json` is the exact
credential-free replay report with the same seal. `blocked-validation.json`
and `blocked-readiness.json` record the controlled live blocker. The seal is
tamper-evident rather than a digital signature, and replay intentionally
rejects the bundle after its recorded age policy expires.
