# Metadata accounting

**Status:** Implemented in M26.

The bundled schema-1 catalog contains 548 Metadata API parent types: 527 from
the pinned Salesforce source registry and 21 additional types returned by at
least one guarded `describeMetadata` run. It records exact profile discovery
for API 31.0 and API 60.0 through 66.0.

## File and component identity

Every regular file below an SFDX package root receives exactly one disposition:
`recognizedMetadata`, `intentionalNonMetadata`, or `unsupportedMetadata` with
a reason. Parent/child metadata uses `Parent.Child`; folder members use
`Folder/Member`; namespaces and every dot in multipart full names are
preserved. Bundle and mixed-content members contribute to one component digest.

## Capability accounting

Reports publish independent numerator, denominator, and percentage values for
catalog types, package files, components, retrieve, deploy, drift, and local
semantics. `orgUnavailable` means the guarded org did not return that catalog
type for the exact API profile; it is not treated as source absence.

Project-owned missing and content-mismatch findings are drift blockers.
Type-wide retrieved components absent from package roots are separate org-only
findings, so clean inventories can have zero unexplained project drift while
still accounting for org-managed configuration.
