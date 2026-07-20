# Sharing and security profile semantics

## Scope and profile boundary

M27 models sharing and data-access behavior for the closed current-profile
family, Salesforce API 60.0 through 66.0. API 67.0 and later remain rejected by
project discovery. In particular, Apex Exec does not silently adopt the API
67.0 user-mode and `with sharing` defaults while those profiles are outside the
modeled catalog.

The implemented local model covers:

- `with sharing`, `without sharing`, explicit `inherited sharing`, and omitted
  sharing declarations;
- private, public-read-only, and public-read-write organization-wide defaults;
- record ownership, a bounded role hierarchy, groups, and explicit read/edit
  grants;
- per-user or wildcard object CRUD and field read/create/update permissions;
- static SOQL `WITH SECURITY_ENFORCED`, `WITH USER_MODE`, and
  `WITH SYSTEM_MODE`;
- dynamic query access through `AccessLevel`;
- Database-method access levels and statement DML `AS USER` / `AS SYSTEM`;
- `Security.stripInaccessible`, `SObjectAccessDecision`, `AccessType`, and
  catchable `NoAccessException`; and
- deterministic `System.runAs` owner-context fixtures.

## Sharing propagation

Checked HIR records one of four class states. `with sharing` and
`without sharing` replace the caller mode. Explicit `inherited sharing` and an
omitted declaration inherit an established caller mode, but differ at a
top-level class entry in API 60.0–66.0: explicit inherited sharing enters with
sharing, while an omitted declaration enters without sharing. Triggers execute
without sharing. Queued deterministic work inherits the execution context
and effective user captured when it is submitted. Role-hierarchy traversal
visits each unique role at most once and rejects cycles explicitly.

Record visibility is evaluated before query filtering, aggregation, ordering,
offset, and limit. Owners can read and edit their record. A viewer whose role
is an ancestor of the owner's role can read and edit. Groups and explicit
grants use case-insensitive stable principal identities. Public-read-only
permits reads but still checks ownership, hierarchy, or a grant for edits.
Controlled-by-parent visibility fails explicitly because parent-derived access
is not modeled.

## Query and DML access

For the modeled API 60.0–66.0 family, an operation with no explicit access
level retains the legacy system CRUD/FLS default. Class sharing still controls
record visibility. `WITH USER_MODE` and user-mode DML override the class mode
and enforce record sharing, object permissions, and field permissions.
`WITH SYSTEM_MODE` and explicitly system-mode DML bypass CRUD/FLS checks but
retain the effective class sharing mode.

`WITH SECURITY_ENFORCED` checks object permission and selected fields. It does
not check condition, grouping, having, or ordering fields. User-mode queries
check those supporting fields as well. Permission failures are catchable
`QueryException` values.

User-mode Database DML preserves scalar/list result shapes and partial-save
ordering. CRUD/FLS-denied rows carry `CANNOT_INSERT_UPDATE_ACTIVATE_ENTITY`;
record-sharing denials carry `INSUFFICIENT_ACCESS_OR_READONLY`; an atomic
operation raises `DmlException`. An owner assigned automatically by insert is
not treated as a caller-written `OwnerId` for FLS, while an explicitly supplied
owner remains permission checked. User-mode external-ID upsert is rejected
explicitly pending a reviewed Salesforce fixture for its create/update split.

## Sanitization

`Security.stripInaccessible` returns cloned records and never mutates its input.
It strips inaccessible root, parent-relationship, and child-subquery fields and
returns deterministic object-to-field sets. The optional Boolean controls
whether missing root object permission raises `NoAccessException`.

Relationship traversal preserves aliases and repeated references through a
memo table, stops cycles, and enforces a depth limit of 32 and a node limit of
10,000. Limit failures are explicit rather than returning a partially
sanitized graph.

## Deterministic fixtures

Projects can provide `.apex-exec/security.json` schema version 1. The file can
declare users and roles, role-parent edges, groups, object and field
permissions, explicit record grants, and initial records. The principal `*`
is a deliberate fallback for users created during `System.runAs` tests.
Optional local-only standard-object schema can live under
`.apex-exec/schema`; it is compiled locally but is not part of a Salesforce
source deployment.

`System.runAs` is accepted only inside an `@IsTest` class. It switches the
effective user for sharing, permission checks, `UserInfo`, ownership defaults,
and work queued inside the block, then restores the prior user on every exit.

Malformed fixture files, unknown schema versions, invalid record IDs, and
unsupported structured field values fail project compilation. An absent
security fixture is not interpreted as full access: CRUD/FLS-enforcing APIs
fail explicitly. Sharing-only execution can still prove owner/private behavior
without a permission fixture.

## Explicit limitations

The local policy does not model criteria-based sharing rules, restriction or
scoping rules, territories, account/opportunity/case teams, queues, implicit
collaboration access, manual-share lifecycle, owner-transfer side effects,
managed-package security, class access, session permission changes, or
automation that mutates access. Profiles and permission sets are represented
by deterministic fixture permissions rather than inferred from org metadata.
These behaviors must use hybrid validation or fail through an explicit
unsupported boundary; they must not be approximated as system mode.
