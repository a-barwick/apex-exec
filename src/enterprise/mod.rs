//! Reproducible representative-project compatibility baselines.
//!
//! Enterprise baselines sit above the compiler/runtime pipeline. They bind an
//! immutable third-party candidate and its package roots before any local
//! compatibility result is inspected.

mod manifest;
mod runner;
mod salesforce;

pub use manifest::{
    CandidateIdentity, ENTERPRISE_SCHEMA_VERSION, EnterpriseInput, EnterpriseManifest,
};
pub use runner::{
    EnterpriseBlocker, EnterpriseBlockerSummary, EnterpriseCounts, EnterpriseReport,
    EnterpriseRunOptions, EnterpriseTestMeasurement, EnterpriseTiming, StageMetric, run,
};
pub use salesforce::{
    EnterpriseSalesforceCli, SalesforceCapture, SalesforceCaptureOptions, SalesforceTestOutcome,
};
