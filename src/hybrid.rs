//! Hybrid local/Salesforce release-readiness orchestration.
//!
//! M15 composes the hermetic M14 CI boundary with a narrowly scoped,
//! check-only Salesforce validation. Metadata inventory, drift analysis, and
//! local-versus-org test comparison remain provider-neutral so a reviewed org
//! snapshot can be replayed without credentials.

use crate::{
    ci::{self, CiManifest, CiRunOptions, SelectionMode},
    project::{self, Compilation},
};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime},
};

pub const HYBRID_SCHEMA_VERSION: u32 = 2;
pub const DEFAULT_MAX_EVIDENCE_AGE: Duration = Duration::from_secs(24 * 60 * 60);
const REQUIRED_RETRIEVALS: usize = 2;
const MAX_CLOCK_SKEW: Duration = Duration::from_secs(5 * 60);
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum ComponentCategory {
    Code,
    Schema,
    Configuration,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MetadataComponent {
    pub metadata_type: String,
    pub full_name: String,
    pub category: ComponentCategory,
    pub sha256: String,
    pub files: Vec<PathBuf>,
}

impl MetadataComponent {
    fn key(&self) -> ComponentKey {
        ComponentKey {
            metadata_type: self.metadata_type.clone(),
            full_name: self.full_name.clone(),
        }
    }

    pub fn selector(&self) -> String {
        format!("{}:{}", self.metadata_type, self.full_name)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OrgInventory {
    pub schema_version: u32,
    pub target: String,
    pub components: Vec<MetadataComponent>,
}

impl OrgInventory {
    pub fn capture(root: impl AsRef<Path>, target: impl Into<String>) -> Result<Self, String> {
        let root = canonical(root.as_ref())?;
        let mut files = Vec::new();
        collect_files(&root, &root, &mut files)?;
        let mut grouped = BTreeMap::<ComponentKey, ComponentBuilder>::new();
        for relative in files {
            let Some(classified) = classify_component_path(&relative) else {
                continue;
            };
            let absolute = root.join(&relative);
            let bytes = fs::read(&absolute).map_err(|error| {
                format!(
                    "failed to read metadata component `{}`: {error}",
                    absolute.display()
                )
            })?;
            let builder = grouped
                .entry(classified.key)
                .or_insert_with(|| ComponentBuilder {
                    category: classified.category,
                    parts: Vec::new(),
                });
            if builder.category != classified.category {
                return Err(format!(
                    "metadata component `{}` has conflicting categories",
                    classified.role
                ));
            }
            builder.parts.push(ComponentPart {
                role: classified.role,
                path: relative,
                contents: normalize_contents(&absolute, &bytes),
            });
        }
        let mut components = Vec::with_capacity(grouped.len());
        for (key, mut builder) in grouped {
            builder
                .parts
                .sort_by(|left, right| left.role.cmp(&right.role));
            let mut digest = Sha256::new();
            let mut paths = Vec::with_capacity(builder.parts.len());
            for part in builder.parts {
                digest.update(part.role.as_bytes());
                digest.update([0]);
                digest.update(&part.contents);
                digest.update([0xff]);
                paths.push(part.path);
            }
            paths.sort();
            components.push(MetadataComponent {
                metadata_type: key.metadata_type,
                full_name: key.full_name,
                category: builder.category,
                sha256: hex_digest(digest.finalize()),
                files: paths,
            });
        }
        components.sort_by(component_order);
        Ok(Self {
            schema_version: HYBRID_SCHEMA_VERSION,
            target: target.into(),
            components,
        })
    }

    fn validate(&self) -> Result<(), String> {
        if self.schema_version != HYBRID_SCHEMA_VERSION {
            return Err(format!(
                "unsupported org inventory schema version {}; expected {}",
                self.schema_version, HYBRID_SCHEMA_VERSION
            ));
        }
        if self.target.trim().is_empty() {
            return Err("org inventory target cannot be empty".to_owned());
        }
        let mut keys = BTreeSet::new();
        for component in &self.components {
            validate_component(component)?;
            if !keys.insert(component.key()) {
                return Err(format!(
                    "org inventory records `{}` more than once",
                    component.selector()
                ));
            }
        }
        Ok(())
    }

    fn sha256(&self) -> Result<String, String> {
        canonical_sha256(self, "retrieved metadata inventory")
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidationTest {
    pub name: String,
    pub outcome: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeploymentValidation {
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deployment_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub component_failures: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tests: Vec<ValidationTest>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ComponentSelectionMode {
    All,
    Impacted,
    ConservativeAll,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum DeploymentTestLevel {
    NoTestRun,
    RunSpecifiedTests,
}

impl DeploymentTestLevel {
    fn for_tests(tests: &[String]) -> Self {
        if tests.is_empty() {
            Self::NoTestRun
        } else {
            Self::RunSpecifiedTests
        }
    }

    fn as_salesforce_value(self) -> &'static str {
        match self {
            Self::NoTestRun => "NoTestRun",
            Self::RunSpecifiedTests => "RunSpecifiedTests",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateEvidence {
    pub manifest_sha256: String,
    pub ci_cache_key: String,
    pub ci_result_sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AffectedComponentEvidence {
    pub selector: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidationRequest {
    pub changed_paths: Vec<PathBuf>,
    pub component_selection: ComponentSelectionMode,
    pub affected_components: Vec<AffectedComponentEvidence>,
    pub selected_tests: Vec<String>,
    pub test_level: DeploymentTestLevel,
}

impl ValidationRequest {
    fn sha256(&self) -> Result<String, String> {
        canonical_sha256(self, "validation request")
    }

    fn validate(&self) -> Result<(), String> {
        let mut paths = BTreeSet::new();
        for path in &self.changed_paths {
            validate_evidence_relative(path, "changed path")?;
            if !paths.insert(path.clone()) {
                return Err(format!(
                    "validation request records changed path `{}` more than once",
                    path.display()
                ));
            }
        }
        if !self.changed_paths.windows(2).all(|pair| pair[0] < pair[1]) {
            return Err(
                "validation request changed paths must use canonical sorted order".to_owned(),
            );
        }
        let mut components = BTreeSet::new();
        for component in &self.affected_components {
            if component.selector.trim().is_empty() {
                return Err("affected component selector cannot be empty".to_owned());
            }
            validate_sha256(&component.sha256, "affected component")?;
            if !components.insert(component.selector.to_ascii_lowercase()) {
                return Err(format!(
                    "validation request records component `{}` more than once",
                    component.selector
                ));
            }
        }
        if !self.affected_components.windows(2).all(|pair| {
            pair[0].selector.to_ascii_lowercase() < pair[1].selector.to_ascii_lowercase()
        }) {
            return Err(
                "validation request affected components must use canonical sorted order".to_owned(),
            );
        }
        let mut tests = BTreeSet::new();
        for test in &self.selected_tests {
            if test.trim().is_empty() {
                return Err("selected validation test cannot be empty".to_owned());
            }
            if !tests.insert(test.to_ascii_lowercase()) {
                return Err(format!(
                    "validation request records test `{test}` more than once"
                ));
            }
        }
        if !self
            .selected_tests
            .windows(2)
            .all(|pair| pair[0].to_ascii_lowercase() < pair[1].to_ascii_lowercase())
        {
            return Err("validation request tests must use canonical sorted order".to_owned());
        }
        if self.test_level != DeploymentTestLevel::for_tests(&self.selected_tests) {
            return Err("validation request test level does not match selected tests".to_owned());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidationEvidence {
    pub candidate: CandidateEvidence,
    pub request: ValidationRequest,
    pub request_sha256: String,
    pub target: String,
    pub org_id: String,
    pub api_version: String,
    pub apex_exec_version: String,
    pub salesforce_cli_version: String,
    pub captured_at: String,
    pub maximum_age_seconds: u64,
    pub retrieved_inventory_sha256: String,
    pub retrieval_count: usize,
}

impl ValidationEvidence {
    fn validate(&self) -> Result<(), String> {
        validate_sha256(&self.candidate.manifest_sha256, "CI manifest")?;
        validate_sha256(&self.candidate.ci_cache_key, "CI cache key")?;
        validate_sha256(&self.candidate.ci_result_sha256, "CI result")?;
        self.request.validate()?;
        validate_sha256(&self.request_sha256, "validation request")?;
        if self.request.sha256()? != self.request_sha256 {
            return Err("validation request digest does not match its contents".to_owned());
        }
        if self.target.trim().is_empty() {
            return Err("validation evidence target cannot be empty".to_owned());
        }
        validate_org_id(&self.org_id)?;
        validate_api_version(&self.api_version)?;
        if self.apex_exec_version.trim().is_empty() {
            return Err("validation evidence Apex Exec version cannot be empty".to_owned());
        }
        if self.salesforce_cli_version.trim().is_empty() {
            return Err("validation evidence Salesforce CLI version cannot be empty".to_owned());
        }
        parse_capture_time(&self.captured_at)?;
        if self.maximum_age_seconds == 0 {
            return Err("validation evidence maximum age must be greater than zero".to_owned());
        }
        validate_sha256(
            &self.retrieved_inventory_sha256,
            "retrieved metadata inventory",
        )?;
        if self.retrieval_count < REQUIRED_RETRIEVALS {
            return Err(format!(
                "validation evidence requires at least {REQUIRED_RETRIEVALS} normalized retrievals"
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ValidationSnapshot {
    pub schema_version: u32,
    pub authenticated: bool,
    pub evidence: ValidationEvidence,
    pub inventory: OrgInventory,
    pub validation: DeploymentValidation,
    pub snapshot_sha256: String,
}

impl ValidationSnapshot {
    fn new(
        evidence: ValidationEvidence,
        inventory: OrgInventory,
        validation: DeploymentValidation,
    ) -> Result<Self, String> {
        let mut snapshot = Self {
            schema_version: HYBRID_SCHEMA_VERSION,
            authenticated: true,
            evidence,
            inventory,
            validation,
            snapshot_sha256: String::new(),
        };
        snapshot.snapshot_sha256 = snapshot.compute_sha256()?;
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let source = fs::read_to_string(path).map_err(|error| {
            format!(
                "failed to read validation snapshot `{}`: {error}",
                path.display()
            )
        })?;
        let snapshot = serde_json::from_str::<Self>(&source).map_err(|error| {
            format!("invalid validation snapshot `{}`: {error}", path.display())
        })?;
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub fn write(&self, path: impl AsRef<Path>) -> Result<(), String> {
        self.validate()?;
        write_json(path.as_ref(), self, "validation snapshot")
    }

    fn validate(&self) -> Result<(), String> {
        if self.schema_version != HYBRID_SCHEMA_VERSION {
            return Err(format!(
                "unsupported validation snapshot schema version {}; expected {}",
                self.schema_version, HYBRID_SCHEMA_VERSION
            ));
        }
        if !self.authenticated {
            return Err(
                "validation snapshot is not authenticated live Salesforce evidence".to_owned(),
            );
        }
        self.evidence.validate()?;
        self.inventory.validate()?;
        if !self
            .evidence
            .target
            .eq_ignore_ascii_case(&self.inventory.target)
        {
            return Err("validation snapshot and inventory targets do not match".to_owned());
        }
        if self.inventory.sha256()? != self.evidence.retrieved_inventory_sha256 {
            return Err(
                "retrieved metadata inventory digest does not match its contents".to_owned(),
            );
        }
        let mut tests = BTreeSet::new();
        for test in &self.validation.tests {
            if test.name.trim().is_empty() {
                return Err("validation test name cannot be empty".to_owned());
            }
            if !matches!(test.outcome.as_str(), "pass" | "fail") {
                return Err(format!(
                    "validation test `{}` has invalid outcome `{}`",
                    test.name, test.outcome
                ));
            }
            if !tests.insert(test.name.to_ascii_lowercase()) {
                return Err(format!(
                    "validation snapshot records test `{}` more than once",
                    test.name
                ));
            }
        }
        validate_sha256(&self.snapshot_sha256, "validation snapshot")?;
        if self.compute_sha256()? != self.snapshot_sha256 {
            return Err("validation snapshot digest does not match its contents".to_owned());
        }
        Ok(())
    }

    fn compute_sha256(&self) -> Result<String, String> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct SnapshotIdentity<'a> {
            schema_version: u32,
            authenticated: bool,
            evidence: &'a ValidationEvidence,
            inventory: &'a OrgInventory,
            validation: &'a DeploymentValidation,
        }
        canonical_sha256(
            &SnapshotIdentity {
                schema_version: self.schema_version,
                authenticated: self.authenticated,
                evidence: &self.evidence,
                inventory: &self.inventory,
                validation: &self.validation,
            },
            "validation snapshot",
        )
    }
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ComponentSelection {
    pub mode: ComponentSelectionMode,
    pub components: Vec<MetadataComponent>,
    pub directly_changed: Vec<String>,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DriftKind {
    MissingInOrg,
    UnexpectedInOrg,
    ContentMismatch,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DriftFinding {
    pub component: String,
    pub category: ComponentCategory,
    pub kind: DriftKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub org_sha256: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TestDifferential {
    pub name: String,
    pub local_outcome: String,
    pub org_outcome: String,
    pub matched: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalReadiness {
    pub compile_success: bool,
    pub selected_tests: usize,
    pub passed_tests: usize,
    pub failed_tests: usize,
    pub line_coverage: f64,
    pub branch_coverage: f64,
    pub policy_violations: Vec<String>,
    pub selection: SelectionMode,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseReadinessReport {
    pub schema_version: u32,
    pub evidence: ValidationEvidence,
    pub validation_snapshot_sha256: String,
    pub replayed: bool,
    pub ready: bool,
    pub affected_components: ComponentSelection,
    pub affected_tests: Vec<String>,
    pub local: LocalReadiness,
    pub org_validation: DeploymentValidation,
    pub drift: Vec<DriftFinding>,
    pub test_differential: Vec<TestDifferential>,
    pub differential_percentage: f64,
    pub blockers: Vec<String>,
}

impl ReleaseReadinessReport {
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    pub fn write(&self, path: impl AsRef<Path>) -> Result<(), String> {
        write_json(path.as_ref(), self, "release-readiness report")
    }

    pub fn render_console(&self) -> String {
        let mut lines = vec![
            format!(
                "Release readiness for {} ({})",
                self.evidence.target,
                if self.replayed {
                    "candidate-bound evidence replay"
                } else {
                    "authenticated validation org"
                },
            ),
            format!(
                "Evidence: {} captured {} with sf {} (API {})",
                self.validation_snapshot_sha256,
                self.evidence.captured_at,
                self.evidence.salesforce_cli_version,
                self.evidence.api_version
            ),
            format!(
                "Affected: {} components ({:?}), {} tests",
                self.affected_components.components.len(),
                self.affected_components.mode,
                self.affected_tests.len()
            ),
            format!(
                "Local: {} passed, {} failed; {:.2}% line / {:.2}% branch coverage",
                self.local.passed_tests,
                self.local.failed_tests,
                self.local.line_coverage,
                self.local.branch_coverage
            ),
            format!(
                "Org dry-run: {}",
                if self.org_validation.success {
                    "PASS"
                } else {
                    "FAIL"
                }
            ),
            format!(
                "Differential: {}/{} tests matched ({:.2}%)",
                self.test_differential
                    .iter()
                    .filter(|result| result.matched)
                    .count(),
                self.test_differential.len(),
                self.differential_percentage
            ),
            format!("Schema/configuration drift: {} findings", self.drift.len()),
        ];
        for blocker in &self.blockers {
            lines.push(format!("BLOCKER: {blocker}"));
        }
        lines.push(if self.ready {
            "RELEASE READY".to_owned()
        } else {
            "RELEASE BLOCKED".to_owned()
        });
        lines.join("\n")
    }
}

#[derive(Clone, Debug)]
pub enum ValidationSource {
    TargetOrg(String),
    Snapshot(PathBuf),
}

#[derive(Clone, Debug)]
pub struct HybridRunOptions {
    pub ci: CiRunOptions,
    pub maximum_evidence_age: Duration,
    pub expected_target_org: Option<String>,
    pub expected_org_id: Option<String>,
}

impl Default for HybridRunOptions {
    fn default() -> Self {
        Self {
            ci: CiRunOptions::default(),
            maximum_evidence_age: DEFAULT_MAX_EVIDENCE_AGE,
            expected_target_org: None,
            expected_org_id: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct HybridRunOutcome {
    pub report: ReleaseReadinessReport,
    pub validation_snapshot: ValidationSnapshot,
}

pub fn run(
    manifest: &CiManifest,
    source: &ValidationSource,
    options: &HybridRunOptions,
) -> Result<HybridRunOutcome, String> {
    run_with_cli_at(
        manifest,
        source,
        options,
        &SalesforceValidationCli::default(),
        SystemTime::now(),
    )
}

pub fn run_with_cli(
    manifest: &CiManifest,
    source: &ValidationSource,
    options: &HybridRunOptions,
    cli: &SalesforceValidationCli,
) -> Result<HybridRunOutcome, String> {
    run_with_cli_at(manifest, source, options, cli, SystemTime::now())
}

#[doc(hidden)]
pub fn run_with_cli_at(
    manifest: &CiManifest,
    source: &ValidationSource,
    options: &HybridRunOptions,
    cli: &SalesforceValidationCli,
    now: SystemTime,
) -> Result<HybridRunOutcome, String> {
    validate_run_options(source, options)?;
    manifest.verify_inputs()?;
    let api_version = project_api_version(manifest.project_root())?;
    let replayed_snapshot = match source {
        ValidationSource::TargetOrg(_) => None,
        ValidationSource::Snapshot(path) => Some(ValidationSnapshot::load(path)?),
    };
    let salesforce_cli_version = cli.version(manifest.project_root())?;
    if let Some(snapshot) = &replayed_snapshot {
        validate_replay_environment(
            snapshot,
            options,
            &api_version,
            &salesforce_cli_version,
            now,
        )?;
    }
    let local_inventory = OrgInventory::capture(manifest.project_root(), "local")?;
    let compilation = project::compile(manifest.project_root()).map_err(|error| error.render())?;
    let affected = select_affected_components(manifest, &compilation, &local_inventory);
    let ci_result = ci::run(manifest, &options.ci)?;
    let affected_tests = ci_result.selected_tests.clone();
    let candidate = CandidateEvidence {
        manifest_sha256: manifest.sha256()?,
        ci_cache_key: ci_result.cache_key.clone(),
        ci_result_sha256: ci::result_sha256(&ci_result)?,
    };
    let request = validation_request(manifest, &affected, &affected_tests)?;
    let validation_snapshot = match source {
        ValidationSource::TargetOrg(target) => {
            let capture = cli.validate(
                manifest.project_root(),
                target,
                &api_version,
                &local_inventory,
                &affected,
                &affected_tests,
            )?;
            let request_sha256 = request.sha256()?;
            let retrieved_inventory_sha256 = capture.inventory.sha256()?;
            ValidationSnapshot::new(
                ValidationEvidence {
                    candidate,
                    request,
                    request_sha256,
                    target: capture.target,
                    org_id: capture.org_id,
                    api_version,
                    apex_exec_version: env!("CARGO_PKG_VERSION").to_owned(),
                    salesforce_cli_version,
                    captured_at: format_capture_time(now),
                    maximum_age_seconds: options.maximum_evidence_age.as_secs(),
                    retrieved_inventory_sha256,
                    retrieval_count: capture.retrieval_count,
                },
                capture.inventory,
                capture.validation,
            )?
        }
        ValidationSource::Snapshot(_) => {
            let snapshot = replayed_snapshot
                .expect("snapshot sources load evidence before candidate evaluation");
            validate_candidate_binding(&snapshot, &candidate, &request)?;
            snapshot
        }
    };
    let drift = detect_drift(
        &local_inventory,
        &validation_snapshot.inventory,
        &affected.directly_changed,
    );
    let test_differential = compare_tests(&ci_result, &validation_snapshot.validation);
    let differential_percentage = percentage(
        test_differential
            .iter()
            .filter(|result| result.matched)
            .count(),
        test_differential.len(),
    );
    let tests = ci_result.tests.as_ref();
    let local = LocalReadiness {
        compile_success: ci_result.compile_success,
        selected_tests: ci_result.selected_tests.len(),
        passed_tests: tests.map_or(0, |report| report.passed()),
        failed_tests: tests.map_or(0, |report| report.failed()),
        line_coverage: tests.map_or(100.0, |report| {
            percentage(report.coverage.covered_lines, report.coverage.total_lines)
        }),
        branch_coverage: tests.map_or(100.0, |report| {
            percentage(
                report.coverage.covered_branches,
                report.coverage.total_branches,
            )
        }),
        policy_violations: ci_result.policy_violations.clone(),
        selection: ci_result.selection,
    };
    let mut blockers = Vec::new();
    if !ci_result.is_success() {
        blockers.push("local hermetic CI did not pass".to_owned());
    }
    if !validation_snapshot.validation.success {
        blockers.push("Salesforce check-only deployment did not pass".to_owned());
    }
    if !drift.is_empty() {
        blockers.push(format!(
            "{} unaffected schema/configuration components drifted",
            drift.len()
        ));
    }
    let mismatches = test_differential
        .iter()
        .filter(|result| !result.matched)
        .count();
    if mismatches > 0 {
        blockers.push(format!(
            "{mismatches} affected test outcomes differ between local and Salesforce"
        ));
    }
    let ready = blockers.is_empty();
    Ok(HybridRunOutcome {
        report: ReleaseReadinessReport {
            schema_version: HYBRID_SCHEMA_VERSION,
            evidence: validation_snapshot.evidence.clone(),
            validation_snapshot_sha256: validation_snapshot.snapshot_sha256.clone(),
            replayed: matches!(source, ValidationSource::Snapshot(_)),
            ready,
            affected_components: affected,
            affected_tests,
            local,
            org_validation: validation_snapshot.validation.clone(),
            drift,
            test_differential,
            differential_percentage,
            blockers,
        },
        validation_snapshot,
    })
}

fn validate_run_options(
    source: &ValidationSource,
    options: &HybridRunOptions,
) -> Result<(), String> {
    if options.maximum_evidence_age.as_secs() == 0 {
        return Err("maximum validation-evidence age must be at least one second".to_owned());
    }
    match source {
        ValidationSource::TargetOrg(_) => {
            if options.ci.no_cache {
                return Err(
                    "authenticated validation requires a cacheable M14 CI artifact; `--no-cache` is not supported"
                        .to_owned(),
                );
            }
            if options.ci.replay_only {
                return Err("authenticated validation cannot use CI replay-only mode".to_owned());
            }
            if options.expected_target_org.is_some() || options.expected_org_id.is_some() {
                return Err(
                    "expected target and org ID assertions apply only to evidence replay"
                        .to_owned(),
                );
            }
        }
        ValidationSource::Snapshot(_) => {
            if !options.ci.replay_only {
                return Err(
                    "validation snapshot replay requires `--replay` so the exact M14 CI result is reused"
                        .to_owned(),
                );
            }
            if options
                .expected_target_org
                .as_deref()
                .is_none_or(str::is_empty)
            {
                return Err("validation snapshot replay requires an expected target org".to_owned());
            }
            if options.expected_org_id.as_deref().is_none_or(str::is_empty) {
                return Err("validation snapshot replay requires an expected org ID".to_owned());
            }
        }
    }
    Ok(())
}

fn validation_request(
    manifest: &CiManifest,
    affected: &ComponentSelection,
    selected_tests: &[String],
) -> Result<ValidationRequest, String> {
    let mut changed_paths = manifest
        .changed_files
        .iter()
        .map(|path| normalize_relative(path))
        .collect::<Vec<_>>();
    changed_paths.sort();
    let mut affected_components = affected
        .components
        .iter()
        .map(|component| AffectedComponentEvidence {
            selector: component.selector(),
            sha256: component.sha256.clone(),
        })
        .collect::<Vec<_>>();
    affected_components.sort_by(|left, right| {
        left.selector
            .to_ascii_lowercase()
            .cmp(&right.selector.to_ascii_lowercase())
    });
    let mut selected_tests = selected_tests.to_vec();
    selected_tests.sort_by_key(|test| test.to_ascii_lowercase());
    let request = ValidationRequest {
        changed_paths,
        component_selection: affected.mode,
        affected_components,
        test_level: DeploymentTestLevel::for_tests(&selected_tests),
        selected_tests,
    };
    request.validate()?;
    Ok(request)
}

fn validate_replay_environment(
    snapshot: &ValidationSnapshot,
    options: &HybridRunOptions,
    api_version: &str,
    salesforce_cli_version: &str,
    now: SystemTime,
) -> Result<(), String> {
    let evidence = &snapshot.evidence;
    let expected_target = options
        .expected_target_org
        .as_deref()
        .expect("replay options require an expected target");
    if !evidence.target.eq_ignore_ascii_case(expected_target) {
        return Err(format!(
            "validation evidence target mismatch: snapshot records `{}`, replay expected `{expected_target}`",
            evidence.target
        ));
    }
    let expected_org_id = options
        .expected_org_id
        .as_deref()
        .expect("replay options require an expected org ID");
    if evidence.org_id != expected_org_id {
        return Err(format!(
            "validation evidence org ID mismatch: snapshot records `{}`, replay expected `{expected_org_id}`",
            evidence.org_id
        ));
    }
    if evidence.api_version != api_version {
        return Err(format!(
            "validation evidence API version mismatch: snapshot records {}, candidate requires {api_version}",
            evidence.api_version
        ));
    }
    if evidence.apex_exec_version != env!("CARGO_PKG_VERSION") {
        return Err(format!(
            "validation evidence Apex Exec version mismatch: snapshot records {}, this binary is {}",
            evidence.apex_exec_version,
            env!("CARGO_PKG_VERSION")
        ));
    }
    if evidence.salesforce_cli_version != salesforce_cli_version {
        return Err(format!(
            "validation evidence Salesforce CLI version mismatch: snapshot records {}, installed CLI is {salesforce_cli_version}",
            evidence.salesforce_cli_version
        ));
    }
    if evidence.maximum_age_seconds != options.maximum_evidence_age.as_secs() {
        return Err(format!(
            "validation evidence maximum-age mismatch: snapshot records {} seconds, replay requires {} seconds",
            evidence.maximum_age_seconds,
            options.maximum_evidence_age.as_secs()
        ));
    }
    validate_capture_age(evidence, now, options.maximum_evidence_age)
}

fn validate_capture_age(
    evidence: &ValidationEvidence,
    now: SystemTime,
    maximum_age: Duration,
) -> Result<(), String> {
    let captured = parse_capture_time(&evidence.captured_at)?;
    let now = DateTime::<Utc>::from(now);
    let skew = chrono::Duration::from_std(MAX_CLOCK_SKEW)
        .map_err(|_| "failed to represent validation clock skew".to_owned())?;
    if captured > now + skew {
        return Err(format!(
            "validation evidence capture time {} is in the future",
            evidence.captured_at
        ));
    }
    if captured > now {
        return Ok(());
    }
    let age = (now - captured)
        .to_std()
        .map_err(|_| "failed to compute validation evidence age".to_owned())?;
    if age > maximum_age {
        return Err(format!(
            "validation evidence expired after {} seconds; maximum age is {} seconds",
            age.as_secs(),
            maximum_age.as_secs()
        ));
    }
    Ok(())
}

fn validate_candidate_binding(
    snapshot: &ValidationSnapshot,
    candidate: &CandidateEvidence,
    request: &ValidationRequest,
) -> Result<(), String> {
    let recorded = &snapshot.evidence.candidate;
    if recorded.manifest_sha256 != candidate.manifest_sha256 {
        return Err("validation evidence does not match the current M14 manifest".to_owned());
    }
    if recorded.ci_cache_key != candidate.ci_cache_key {
        return Err("validation evidence does not match the current M14 CI cache key".to_owned());
    }
    if recorded.ci_result_sha256 != candidate.ci_result_sha256 {
        return Err("validation evidence does not match the exact M14 CI result".to_owned());
    }
    let recorded = &snapshot.evidence.request;
    if recorded.changed_paths != request.changed_paths {
        return Err(
            "validation evidence changed paths do not match the current request".to_owned(),
        );
    }
    if recorded.component_selection != request.component_selection {
        return Err(
            "validation evidence component-selection mode does not match the current request"
                .to_owned(),
        );
    }
    if recorded.affected_components != request.affected_components {
        return Err(
            "validation evidence affected component selectors or digests do not match the current request"
                .to_owned(),
        );
    }
    if recorded.selected_tests != request.selected_tests {
        return Err(
            "validation evidence selected tests do not match the current request".to_owned(),
        );
    }
    if recorded.test_level != request.test_level {
        return Err("validation evidence test level does not match the current request".to_owned());
    }
    Ok(())
}

pub fn select_affected_components(
    manifest: &CiManifest,
    compilation: &Compilation,
    inventory: &OrgInventory,
) -> ComponentSelection {
    let all = inventory.components.clone();
    if manifest.changed_files.is_empty() {
        return ComponentSelection {
            mode: ComponentSelectionMode::All,
            components: all,
            directly_changed: Vec::new(),
        };
    }
    let by_file = inventory
        .components
        .iter()
        .flat_map(|component| {
            component
                .files
                .iter()
                .map(move |path| (normalize_relative(path), component))
        })
        .collect::<BTreeMap<_, _>>();
    let mut direct = BTreeSet::new();
    let mut changed_sources = BTreeSet::new();
    let mut conservative = false;
    for changed in &manifest.changed_files {
        let normalized = normalize_relative(changed);
        let Some(component) = by_file.get(&normalized) else {
            conservative = true;
            continue;
        };
        direct.insert(component.key());
        if component.metadata_type != "ApexClass" {
            conservative = true;
            continue;
        }
        let source = component
            .files
            .iter()
            .find(|path| path.extension().is_some_and(|extension| extension == "cls"))
            .map(|path| manifest.project_root().join(path));
        match source {
            Some(path) if compilation.dependencies.contains_file(&path) => {
                changed_sources.insert(path);
            }
            _ => conservative = true,
        }
    }
    let directly_changed = direct
        .iter()
        .map(ComponentKey::selector)
        .collect::<Vec<_>>();
    if conservative {
        return ComponentSelection {
            mode: ComponentSelectionMode::ConservativeAll,
            components: all,
            directly_changed,
        };
    }
    let impacted_paths = compilation.dependencies.dependent_closure(changed_sources);
    let mut selected_keys = direct;
    for path in impacted_paths {
        let Ok(relative) = path.strip_prefix(manifest.project_root()) else {
            continue;
        };
        if let Some(component) = by_file.get(&normalize_relative(relative)) {
            selected_keys.insert(component.key());
        }
    }
    let components = inventory
        .components
        .iter()
        .filter(|component| selected_keys.contains(&component.key()))
        .cloned()
        .collect();
    ComponentSelection {
        mode: ComponentSelectionMode::Impacted,
        components,
        directly_changed,
    }
}

pub fn detect_drift(
    local: &OrgInventory,
    org: &OrgInventory,
    directly_changed: &[String],
) -> Vec<DriftFinding> {
    let ignored = directly_changed
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    let local = drift_components(local);
    let org = drift_components(org);
    let keys = local
        .keys()
        .chain(org.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut findings = Vec::new();
    for key in keys {
        if ignored.contains(&key.selector().to_ascii_lowercase()) {
            continue;
        }
        match (local.get(&key), org.get(&key)) {
            (Some(local), Some(org)) if local.sha256 != org.sha256 => {
                findings.push(DriftFinding {
                    component: key.selector(),
                    category: local.category,
                    kind: DriftKind::ContentMismatch,
                    local_sha256: Some(local.sha256.clone()),
                    org_sha256: Some(org.sha256.clone()),
                });
            }
            (Some(local), None) => findings.push(DriftFinding {
                component: key.selector(),
                category: local.category,
                kind: DriftKind::MissingInOrg,
                local_sha256: Some(local.sha256.clone()),
                org_sha256: None,
            }),
            (None, Some(org)) => findings.push(DriftFinding {
                component: key.selector(),
                category: org.category,
                kind: DriftKind::UnexpectedInOrg,
                local_sha256: None,
                org_sha256: Some(org.sha256.clone()),
            }),
            _ => {}
        }
    }
    findings
}

fn drift_components(inventory: &OrgInventory) -> BTreeMap<ComponentKey, &MetadataComponent> {
    inventory
        .components
        .iter()
        .filter(|component| component.category != ComponentCategory::Code)
        .map(|component| (component.key(), component))
        .collect()
}

fn compare_tests(local: &ci::CiRunResult, org: &DeploymentValidation) -> Vec<TestDifferential> {
    let local_results = local
        .tests
        .iter()
        .flat_map(|report| &report.tests)
        .map(|test| {
            (
                test.name.to_ascii_lowercase(),
                (
                    test.name.clone(),
                    if test.failure.is_none() {
                        "pass"
                    } else {
                        "fail"
                    },
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let org_results = org
        .tests
        .iter()
        .map(|test| (test.name.to_ascii_lowercase(), test))
        .collect::<BTreeMap<_, _>>();
    local
        .selected_tests
        .iter()
        .map(|name| {
            let key = name.to_ascii_lowercase();
            let (display, local_outcome) = local_results
                .get(&key)
                .map_or((name.clone(), "missing"), |(name, outcome)| {
                    (name.clone(), *outcome)
                });
            let org_outcome = org_results
                .get(&key)
                .map_or("missing", |test| test.outcome.as_str());
            TestDifferential {
                name: display,
                local_outcome: local_outcome.to_owned(),
                org_outcome: org_outcome.to_owned(),
                matched: local_outcome == org_outcome,
            }
        })
        .collect()
}

#[derive(Clone, Debug)]
pub struct SalesforceValidationCli {
    executable: OsString,
    wait_minutes: u32,
}

struct LiveValidationCapture {
    target: String,
    org_id: String,
    inventory: OrgInventory,
    validation: DeploymentValidation,
    retrieval_count: usize,
}

impl Default for SalesforceValidationCli {
    fn default() -> Self {
        Self {
            executable: OsString::from("sf"),
            wait_minutes: 30,
        }
    }
}

impl SalesforceValidationCli {
    pub fn new(executable: impl Into<OsString>) -> Self {
        Self {
            executable: executable.into(),
            ..Self::default()
        }
    }

    fn validate(
        &self,
        project_root: &Path,
        target_org: &str,
        api_version: &str,
        local_inventory: &OrgInventory,
        affected: &ComponentSelection,
        tests: &[String],
    ) -> Result<LiveValidationCapture, String> {
        if target_org.trim().is_empty() {
            return Err("Salesforce validation org cannot be empty".to_owned());
        }
        validate_api_version(api_version)?;
        let auth = self.command_json(
            project_root,
            &[
                OsString::from("org"),
                OsString::from("display"),
                OsString::from("--target-org"),
                OsString::from(target_org),
                OsString::from("--json"),
            ],
        )?;
        if !response_success(&auth) {
            return Err(format!(
                "Salesforce validation-org authentication failed: {}",
                response_message(&auth)
            ));
        }
        let org_id = find_string(&auth, &["orgId", "id"])
            .ok_or_else(|| {
                "Salesforce validation-org response did not include an org ID".to_owned()
            })?
            .to_owned();
        validate_org_id(&org_id)?;
        let retrieve_components = retrieval_scope(local_inventory, affected);
        let mut inventories = Vec::with_capacity(REQUIRED_RETRIEVALS);
        for _ in 0..REQUIRED_RETRIEVALS {
            let temp = temporary_directory(project_root)?;
            let inventory = (|| {
                let source_root = temp.join("main/default");
                fs::create_dir_all(&source_root).map_err(|error| {
                    format!(
                        "failed to prepare Salesforce retrieve output `{}`: {error}",
                        source_root.display()
                    )
                })?;
                let mut retrieve_args = vec![
                    OsString::from("project"),
                    OsString::from("retrieve"),
                    OsString::from("start"),
                    OsString::from("--target-org"),
                    OsString::from(target_org),
                    OsString::from("--api-version"),
                    OsString::from(api_version),
                    OsString::from("--output-dir"),
                    temp.as_os_str().to_owned(),
                    OsString::from("--wait"),
                    OsString::from(self.wait_minutes.to_string()),
                    OsString::from("--json"),
                ];
                for component in &retrieve_components {
                    retrieve_args.push(OsString::from("--metadata"));
                    retrieve_args.push(OsString::from(component.selector()));
                }
                let retrieve = self.command_json(project_root, &retrieve_args)?;
                if !response_success(&retrieve) {
                    return Err(format!(
                        "failed to retrieve validation-org metadata: {}",
                        response_message(&retrieve)
                    ));
                }
                OrgInventory::capture(&temp, target_org)
            })();
            let _ = fs::remove_dir_all(&temp);
            inventories.push(inventory?);
        }
        let inventory = inventories
            .first()
            .expect("at least two retrievals are required")
            .clone();
        let inventory_sha256 = inventory.sha256()?;
        for repeated in inventories.iter().skip(1) {
            if repeated.sha256()? != inventory_sha256 {
                return Err(
                    "repeated Salesforce metadata retrievals did not normalize identically"
                        .to_owned(),
                );
            }
        }

        let mut deploy_args = vec![
            OsString::from("project"),
            OsString::from("deploy"),
            OsString::from("start"),
            OsString::from("--dry-run"),
            OsString::from("--target-org"),
            OsString::from(target_org),
            OsString::from("--api-version"),
            OsString::from(api_version),
            OsString::from("--wait"),
            OsString::from(self.wait_minutes.to_string()),
            OsString::from("--json"),
        ];
        for component in &affected.components {
            deploy_args.push(OsString::from("--metadata"));
            deploy_args.push(OsString::from(component.selector()));
        }
        let test_level = DeploymentTestLevel::for_tests(tests);
        deploy_args.push(OsString::from("--test-level"));
        deploy_args.push(OsString::from(test_level.as_salesforce_value()));
        if test_level == DeploymentTestLevel::RunSpecifiedTests {
            for test in salesforce_test_classes(tests) {
                deploy_args.push(OsString::from("--tests"));
                deploy_args.push(OsString::from(test));
            }
        }
        let deploy = self.command_json(project_root, &deploy_args)?;
        Ok(LiveValidationCapture {
            target: target_org.to_owned(),
            org_id,
            inventory,
            validation: parse_deployment_validation(&deploy),
            retrieval_count: inventories.len(),
        })
    }

    fn version(&self, current_dir: &Path) -> Result<String, String> {
        let output = self.command_output(current_dir, &[OsString::from("--version")])?;
        if !output.status.success() {
            return Err(format!(
                "Salesforce CLI version check failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        parse_salesforce_cli_version(&String::from_utf8_lossy(&output.stdout))
    }

    fn command_json(&self, current_dir: &Path, arguments: &[OsString]) -> Result<Value, String> {
        let output = self.command_output(current_dir, arguments)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        serde_json::from_str(&stdout).map_err(|error| {
            let stderr = String::from_utf8_lossy(&output.stderr);
            format!(
                "Salesforce CLI returned invalid JSON (status {}): {error}; stderr: {}",
                output.status,
                stderr.trim()
            )
        })
    }

    fn command_output(
        &self,
        current_dir: &Path,
        arguments: &[OsString],
    ) -> Result<std::process::Output, String> {
        Command::new(&self.executable)
            .args(arguments)
            .current_dir(current_dir)
            .env("SF_DISABLE_LOG_FILE", "true")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|error| {
                format!(
                    "failed to start Salesforce CLI `{}`: {error}",
                    Path::new(&self.executable).display()
                )
            })
    }
}

fn retrieval_scope<'a>(
    local: &'a OrgInventory,
    affected: &'a ComponentSelection,
) -> Vec<&'a MetadataComponent> {
    let mut selected = BTreeMap::<ComponentKey, &MetadataComponent>::new();
    for component in &local.components {
        if component.category != ComponentCategory::Code {
            selected.insert(component.key(), component);
        }
    }
    for component in &affected.components {
        selected.insert(component.key(), component);
    }
    selected.into_values().collect()
}

fn parse_deployment_validation(response: &Value) -> DeploymentValidation {
    let mut tests = Vec::new();
    collect_validation_tests(response, &mut tests);
    tests.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
    });
    tests.dedup_by(|left, right| left.name.eq_ignore_ascii_case(&right.name));
    let mut component_failures = Vec::new();
    collect_component_failures(response, &mut component_failures);
    component_failures.sort();
    component_failures.dedup();
    DeploymentValidation {
        success: response_success(response),
        deployment_id: find_string(response, &["id", "deployId"]).map(str::to_owned),
        component_failures,
        tests,
    }
}

fn salesforce_test_classes(tests: &[String]) -> Vec<String> {
    let mut classes = BTreeMap::<String, String>::new();
    for test in tests {
        let class = test
            .rsplit_once('.')
            .map_or(test.as_str(), |(class, _)| class);
        classes
            .entry(class.to_ascii_lowercase())
            .or_insert_with(|| class.to_owned());
    }
    classes.into_values().collect()
}

fn collect_validation_tests(value: &Value, tests: &mut Vec<ValidationTest>) {
    match value {
        Value::Object(object) => {
            let class = object
                .get("name")
                .or_else(|| object.get("className"))
                .and_then(Value::as_str);
            let method = object
                .get("methodName")
                .or_else(|| object.get("MethodName"))
                .and_then(Value::as_str);
            let explicit_outcome = object
                .get("outcome")
                .or_else(|| object.get("Outcome"))
                .and_then(Value::as_str);
            if let (Some(class), Some(method)) = (class, method) {
                let outcome = explicit_outcome.map_or_else(
                    || {
                        if object.contains_key("stackTrace")
                            || object.contains_key("message")
                            || object.contains_key("problem")
                        {
                            "fail"
                        } else {
                            "pass"
                        }
                    },
                    |outcome| {
                        if outcome.eq_ignore_ascii_case("pass")
                            || outcome.eq_ignore_ascii_case("success")
                        {
                            "pass"
                        } else {
                            "fail"
                        }
                    },
                );
                tests.push(ValidationTest {
                    name: format!("{class}.{method}"),
                    outcome: outcome.to_owned(),
                    message: object
                        .get("message")
                        .or_else(|| object.get("problem"))
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                });
            }
            for child in object.values() {
                collect_validation_tests(child, tests);
            }
        }
        Value::Array(values) => {
            for child in values {
                collect_validation_tests(child, tests);
            }
        }
        _ => {}
    }
}

fn collect_component_failures(value: &Value, failures: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            if let Some(entries) = object.get("componentFailures") {
                collect_failure_entries(entries, failures);
            }
            for child in object.values() {
                collect_component_failures(child, failures);
            }
        }
        Value::Array(values) => {
            for child in values {
                collect_component_failures(child, failures);
            }
        }
        _ => {}
    }
}

fn collect_failure_entries(value: &Value, failures: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            let name = object
                .get("fullName")
                .and_then(Value::as_str)
                .unwrap_or("component");
            let problem = object
                .get("problem")
                .or_else(|| object.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("validation failed");
            failures.push(format!("{name}: {problem}"));
        }
        Value::Array(values) => {
            for value in values {
                collect_failure_entries(value, failures);
            }
        }
        Value::String(message) => failures.push(message.clone()),
        _ => {}
    }
}

fn response_success(value: &Value) -> bool {
    value
        .get("result")
        .and_then(|result| result.get("success"))
        .and_then(Value::as_bool)
        .or_else(|| value.get("success").and_then(Value::as_bool))
        .unwrap_or_else(|| value.get("status").and_then(Value::as_i64) == Some(0))
}

fn response_message(value: &Value) -> &str {
    find_string(value, &["message", "name", "status"]).unwrap_or("unknown Salesforce CLI failure")
}

fn find_string<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    match value {
        Value::Object(object) => {
            for key in keys {
                if let Some(found) = object.get(*key).and_then(Value::as_str) {
                    return Some(found);
                }
            }
            object.values().find_map(|child| find_string(child, keys))
        }
        Value::Array(values) => values.iter().find_map(|child| find_string(child, keys)),
        _ => None,
    }
}

fn project_api_version(project_root: &Path) -> Result<String, String> {
    let path = project_root.join("sfdx-project.json");
    let source = fs::read_to_string(&path).map_err(|error| {
        format!(
            "failed to read Salesforce project configuration `{}`: {error}",
            path.display()
        )
    })?;
    let json = serde_json::from_str::<Value>(&source).map_err(|error| {
        format!(
            "invalid Salesforce project configuration `{}`: {error}",
            path.display()
        )
    })?;
    let version = json
        .get("sourceApiVersion")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            format!(
                "Salesforce project configuration `{}` must declare `sourceApiVersion` for candidate-bound validation",
                path.display()
            )
        })?
        .to_owned();
    validate_api_version(&version)?;
    Ok(version)
}

fn validate_api_version(version: &str) -> Result<(), String> {
    let Some((major, minor)) = version.split_once('.') else {
        return Err(format!(
            "Salesforce API version `{version}` must use `<major>.<minor>` format"
        ));
    };
    if major.is_empty()
        || minor.is_empty()
        || !major.bytes().all(|byte| byte.is_ascii_digit())
        || !minor.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(format!(
            "Salesforce API version `{version}` must use `<major>.<minor>` format"
        ));
    }
    Ok(())
}

fn validate_org_id(org_id: &str) -> Result<(), String> {
    if !matches!(org_id.len(), 15 | 18)
        || !org_id.starts_with("00D")
        || !org_id.bytes().all(|byte| byte.is_ascii_alphanumeric())
    {
        return Err(format!(
            "Salesforce validation org ID `{org_id}` is not a 15- or 18-character 00D ID"
        ));
    }
    Ok(())
}

fn parse_salesforce_cli_version(output: &str) -> Result<String, String> {
    let version = output
        .split_whitespace()
        .find_map(|token| token.strip_prefix("@salesforce/cli/"))
        .filter(|version| !version.is_empty())
        .ok_or_else(|| {
            format!(
                "Salesforce CLI version output was not recognized: {}",
                output.trim()
            )
        })?;
    Ok(version.to_owned())
}

fn format_capture_time(time: SystemTime) -> String {
    DateTime::<Utc>::from(time).to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn parse_capture_time(value: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(value)
        .map(|time| time.with_timezone(&Utc))
        .map_err(|error| format!("invalid validation evidence capture time `{value}`: {error}"))
}

fn validate_sha256(value: &str, label: &str) -> Result<(), String> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(format!("{label} has an invalid SHA-256 digest"));
    }
    Ok(())
}

fn canonical_sha256(value: &impl Serialize, label: &str) -> Result<String, String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| format!("failed to serialize {label}: {error}"))?;
    Ok(hex_digest(Sha256::digest(bytes)))
}

fn validate_evidence_relative(path: &Path, label: &str) -> Result<(), String> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!(
            "{label} `{}` must be a safe relative path",
            path.display()
        ));
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ComponentKey {
    metadata_type: String,
    full_name: String,
}

impl ComponentKey {
    fn selector(&self) -> String {
        format!("{}:{}", self.metadata_type, self.full_name)
    }
}

struct ComponentBuilder {
    category: ComponentCategory,
    parts: Vec<ComponentPart>,
}

struct ComponentPart {
    role: String,
    path: PathBuf,
    contents: Vec<u8>,
}

struct ClassifiedPath {
    key: ComponentKey,
    category: ComponentCategory,
    role: String,
}

fn classify_component_path(path: &Path) -> Option<ClassifiedPath> {
    let parts = path
        .components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>();
    let file = *parts.last()?;
    if let Some(index) = parts.iter().position(|part| *part == "classes") {
        if index + 1 == parts.len() - 1
            && (file.ends_with(".cls") || file.ends_with(".cls-meta.xml"))
        {
            let name = file
                .strip_suffix(".cls-meta.xml")
                .or_else(|| file.strip_suffix(".cls"))?;
            return Some(classified("ApexClass", name, ComponentCategory::Code, file));
        }
    }
    if let Some(index) = parts.iter().position(|part| *part == "triggers") {
        if index + 1 == parts.len() - 1
            && (file.ends_with(".trigger") || file.ends_with(".trigger-meta.xml"))
        {
            let name = file
                .strip_suffix(".trigger-meta.xml")
                .or_else(|| file.strip_suffix(".trigger"))?;
            return Some(classified(
                "ApexTrigger",
                name,
                ComponentCategory::Code,
                file,
            ));
        }
    }
    if let Some(index) = parts.iter().position(|part| *part == "objects") {
        let object = *parts.get(index + 1)?;
        if index + 2 == parts.len() - 1
            && (file.ends_with(".object-meta.xml") || file.ends_with(".object"))
        {
            return Some(classified(
                "CustomObject",
                object,
                ComponentCategory::Schema,
                file,
            ));
        }
        let subfolder = *parts.get(index + 2)?;
        if index + 3 == parts.len() - 1 {
            let member = metadata_member_name(file)?;
            let (metadata_type, category) = match subfolder {
                "fields" => ("CustomField", ComponentCategory::Schema),
                "indexes" => ("Index", ComponentCategory::Schema),
                "recordTypes" => ("RecordType", ComponentCategory::Configuration),
                "validationRules" => ("ValidationRule", ComponentCategory::Configuration),
                "businessProcesses" => ("BusinessProcess", ComponentCategory::Configuration),
                "compactLayouts" => ("CompactLayout", ComponentCategory::Configuration),
                "fieldSets" => ("FieldSet", ComponentCategory::Configuration),
                "listViews" => ("ListView", ComponentCategory::Configuration),
                "sharingReasons" => ("SharingReason", ComponentCategory::Configuration),
                "webLinks" => ("WebLink", ComponentCategory::Configuration),
                _ => return None,
            };
            return Some(classified(
                metadata_type,
                &format!("{object}.{member}"),
                category,
                file,
            ));
        }
    }
    let folder_index = parts.iter().enumerate().find_map(|(index, part)| {
        generic_metadata_type(part).map(|metadata_type| (index, metadata_type))
    });
    let (index, metadata_type) = folder_index?;
    if index + 1 != parts.len() - 1 {
        return None;
    }
    let name = metadata_member_name(file)?;
    let category = if metadata_type == "CustomObject" || metadata_type == "CustomField" {
        ComponentCategory::Schema
    } else {
        ComponentCategory::Configuration
    };
    Some(classified(metadata_type, name, category, file))
}

fn classified(
    metadata_type: &str,
    full_name: &str,
    category: ComponentCategory,
    role: &str,
) -> ClassifiedPath {
    ClassifiedPath {
        key: ComponentKey {
            metadata_type: metadata_type.to_owned(),
            full_name: full_name.to_owned(),
        },
        category,
        role: role.to_owned(),
    }
}

fn generic_metadata_type(folder: &str) -> Option<&'static str> {
    Some(match folder {
        "applications" => "CustomApplication",
        "customMetadata" => "CustomMetadata",
        "flows" => "Flow",
        "groups" => "Group",
        "labels" => "CustomLabels",
        "layouts" => "Layout",
        "namedCredentials" => "NamedCredential",
        "permissionsets" => "PermissionSet",
        "profiles" => "Profile",
        "queues" => "Queue",
        "remoteSiteSettings" => "RemoteSiteSetting",
        "roles" => "Role",
        "settings" => "Settings",
        "tabs" => "CustomTab",
        "workflows" => "Workflow",
        _ => return None,
    })
}

fn metadata_member_name(file: &str) -> Option<&str> {
    let source = file.strip_suffix("-meta.xml").unwrap_or(file);
    source.split('.').next()
}

fn normalize_contents(path: &Path, bytes: &[u8]) -> Vec<u8> {
    if path.extension().is_some_and(|extension| extension == "xml") {
        let text = String::from_utf8_lossy(bytes).replace("\r\n", "\n");
        let characters = text.trim().chars().collect::<Vec<_>>();
        let mut normalized = String::with_capacity(text.len());
        let mut index = 0;
        while index < characters.len() {
            let character = characters[index];
            normalized.push(character);
            index += 1;
            if character != '>' {
                continue;
            }
            let whitespace_start = index;
            while index < characters.len() && characters[index].is_whitespace() {
                index += 1;
            }
            if index >= characters.len() || characters[index] != '<' {
                normalized.extend(characters[whitespace_start..index].iter());
            }
        }
        normalized.into_bytes()
    } else {
        String::from_utf8_lossy(bytes)
            .replace("\r\n", "\n")
            .into_bytes()
    }
}

fn collect_files(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| format!("failed to read `{}`: {error}", directory.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("failed to read `{}`: {error}", directory.display()))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let file_type = entry
            .file_type()
            .map_err(|error| format!("failed to inspect `{}`: {error}", entry.path().display()))?;
        if file_type.is_symlink() {
            return Err(format!(
                "metadata inventory refuses symlink `{}`",
                entry.path().display()
            ));
        }
        if file_type.is_dir() {
            let name = entry.file_name();
            if name == ".git" || name == ".apex-exec" {
                continue;
            }
            collect_files(root, &entry.path(), files)?;
        } else if file_type.is_file() {
            files.push(
                entry
                    .path()
                    .strip_prefix(root)
                    .expect("walked paths remain below inventory root")
                    .to_owned(),
            );
        }
    }
    Ok(())
}

fn validate_component(component: &MetadataComponent) -> Result<(), String> {
    if component.metadata_type.trim().is_empty() || component.full_name.trim().is_empty() {
        return Err("metadata component type and full name cannot be empty".to_owned());
    }
    if component.sha256.len() != 64
        || !component
            .sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(format!(
            "metadata component `{}` has an invalid SHA-256 digest",
            component.selector()
        ));
    }
    for path in &component.files {
        validate_relative(path)?;
    }
    Ok(())
}

fn validate_relative(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!(
            "metadata component path `{}` must be a safe relative path",
            path.display()
        ));
    }
    Ok(())
}

fn normalize_relative(path: &Path) -> PathBuf {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => Some(value),
            Component::CurDir => None,
            _ => None,
        })
        .collect()
}

fn component_order(left: &MetadataComponent, right: &MetadataComponent) -> std::cmp::Ordering {
    left.metadata_type
        .to_ascii_lowercase()
        .cmp(&right.metadata_type.to_ascii_lowercase())
        .then_with(|| {
            left.full_name
                .to_ascii_lowercase()
                .cmp(&right.full_name.to_ascii_lowercase())
        })
}

fn temporary_directory(project_root: &Path) -> Result<PathBuf, String> {
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let parent = project_root.join(".apex-exec");
    fs::create_dir_all(&parent)
        .map_err(|error| format!("failed to create temporary metadata parent: {error}"))?;
    let path = parent.join(format!(
        "apex-exec-hybrid-{}-{sequence}",
        std::process::id()
    ));
    fs::create_dir(&path)
        .map_err(|error| format!("failed to create temporary metadata directory: {error}"))?;
    Ok(path)
}

fn canonical(path: &Path) -> Result<PathBuf, String> {
    path.canonicalize()
        .map_err(|error| format!("failed to resolve `{}`: {error}", path.display()))
}

fn hex_digest(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn percentage(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        100.0
    } else {
        numerator as f64 * 100.0 / denominator as f64
    }
}

fn write_json(path: &Path, value: &impl Serialize, label: &str) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create {label} directory `{}`: {error}",
                parent.display()
            )
        })?;
    }
    let mut json = serde_json::to_string_pretty(value)
        .map_err(|error| format!("failed to serialize {label}: {error}"))?;
    json.push('\n');
    fs::write(path, json)
        .map_err(|error| format!("failed to write {label} `{}`: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn salesforce_transport_collapses_selected_methods_to_unique_test_classes() {
        assert_eq!(
            salesforce_test_classes(&[
                "ReleaseServiceTest.preparesStandardInvoice".to_owned(),
                "releaseServiceTest.preparesPriorityInvoice".to_owned(),
                "SmokeTest".to_owned(),
            ]),
            ["ReleaseServiceTest", "SmokeTest"]
        );
    }

    #[test]
    fn inventory_groups_source_and_sidecars_and_classifies_metadata() {
        let root = fixture_root("inventory");
        write(
            &root.join("force-app/main/default/classes/Demo.cls"),
            "public class Demo {}",
        );
        write(
            &root.join("force-app/main/default/classes/Demo.cls-meta.xml"),
            "<ApexClass>\n  <apiVersion>65.0</apiVersion>\n</ApexClass>",
        );
        write(
            &root.join("force-app/main/default/objects/Invoice__c/fields/Amount__c.field-meta.xml"),
            "<CustomField><type>Number</type></CustomField>",
        );
        write(
            &root.join("force-app/main/default/permissionsets/Billing.permissionset-meta.xml"),
            "<PermissionSet/>",
        );
        let inventory = OrgInventory::capture(&root, "local").unwrap();
        assert_eq!(inventory.components.len(), 3);
        let class = inventory
            .components
            .iter()
            .find(|component| component.selector() == "ApexClass:Demo")
            .unwrap();
        assert_eq!(class.files.len(), 2);
        assert_eq!(class.category, ComponentCategory::Code);
        assert_eq!(
            inventory
                .components
                .iter()
                .find(|component| component.metadata_type == "CustomField")
                .unwrap()
                .category,
            ComponentCategory::Schema
        );
        assert_eq!(
            inventory
                .components
                .iter()
                .find(|component| component.metadata_type == "PermissionSet")
                .unwrap()
                .category,
            ComponentCategory::Configuration
        );

        let reformatted = fixture_root("inventory-reformatted");
        write(
            &reformatted.join("force-app/main/default/classes/Demo.cls"),
            "public class Demo {}",
        );
        write(
            &reformatted.join("force-app/main/default/classes/Demo.cls-meta.xml"),
            "<ApexClass>\r\n\t<apiVersion>65.0</apiVersion>   \r\n</ApexClass>",
        );
        let reformatted = OrgInventory::capture(&reformatted, "org").unwrap();
        assert_eq!(
            class.sha256,
            reformatted
                .components
                .iter()
                .find(|component| component.selector() == "ApexClass:Demo")
                .unwrap()
                .sha256
        );
    }

    #[test]
    fn drift_is_scoped_to_schema_and_configuration_and_ignores_intended_changes() {
        let component = |metadata_type: &str,
                         full_name: &str,
                         category: ComponentCategory,
                         digest: &str| MetadataComponent {
            metadata_type: metadata_type.to_owned(),
            full_name: full_name.to_owned(),
            category,
            sha256: digest.repeat(64),
            files: vec![PathBuf::from("force-app/component")],
        };
        let local = OrgInventory {
            schema_version: HYBRID_SCHEMA_VERSION,
            target: "local".to_owned(),
            components: vec![
                component("ApexClass", "Demo", ComponentCategory::Code, "a"),
                component(
                    "CustomField",
                    "Invoice__c.Amount__c",
                    ComponentCategory::Schema,
                    "b",
                ),
                component(
                    "PermissionSet",
                    "Billing",
                    ComponentCategory::Configuration,
                    "c",
                ),
            ],
        };
        let mut org = local.clone();
        org.target = "staging".to_owned();
        org.components[0].sha256 = "d".repeat(64);
        org.components[1].sha256 = "e".repeat(64);
        org.components.pop();
        let findings = detect_drift(
            &local,
            &org,
            &["CustomField:Invoice__c.Amount__c".to_owned()],
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].component, "PermissionSet:Billing");
        assert_eq!(findings[0].kind, DriftKind::MissingInOrg);
    }

    #[test]
    fn deployment_parser_normalizes_successes_failures_and_component_errors() {
        let response = serde_json::json!({
            "status": 1,
            "result": {
                "id": "0Af000000000001",
                "success": false,
                "details": {
                    "componentFailures": [{
                        "fullName": "BillingService",
                        "problem": "Invalid type"
                    }],
                    "runTestResult": {
                        "successes": [{"name": "BillingTest", "methodName": "passes"}],
                        "failures": [{
                            "name": "BillingTest",
                            "methodName": "fails",
                            "message": "assertion failed"
                        }]
                    }
                }
            }
        });
        let parsed = parse_deployment_validation(&response);
        assert!(!parsed.success);
        assert_eq!(parsed.deployment_id.as_deref(), Some("0Af000000000001"));
        assert_eq!(parsed.component_failures, ["BillingService: Invalid type"]);
        assert_eq!(
            parsed.tests,
            [
                ValidationTest {
                    name: "BillingTest.fails".to_owned(),
                    outcome: "fail".to_owned(),
                    message: Some("assertion failed".to_owned()),
                },
                ValidationTest {
                    name: "BillingTest.passes".to_owned(),
                    outcome: "pass".to_owned(),
                    message: None,
                }
            ]
        );
    }

    #[test]
    fn snapshots_reject_duplicate_tests_and_unsafe_component_paths() {
        let root = fixture_root("snapshot-validation");
        let path = root.join("snapshot.json");
        let mut snapshot = valid_snapshot();
        snapshot.inventory.components.push(MetadataComponent {
            metadata_type: "CustomObject".to_owned(),
            full_name: "Invoice__c".to_owned(),
            category: ComponentCategory::Schema,
            sha256: "a".repeat(64),
            files: vec![PathBuf::from("../escape")],
        });
        assert!(snapshot.write(&path).unwrap_err().contains("safe relative"));

        snapshot.inventory.components.clear();
        snapshot.validation.tests = vec![
            ValidationTest {
                name: "DemoTest.same".to_owned(),
                outcome: "pass".to_owned(),
                message: None,
            },
            ValidationTest {
                name: "demotest.SAME".to_owned(),
                outcome: "pass".to_owned(),
                message: None,
            },
        ];
        assert!(
            snapshot
                .write(&path)
                .unwrap_err()
                .contains("more than once")
        );
    }

    #[test]
    fn snapshot_digest_rejects_tampered_validation_results() {
        let root = fixture_root("snapshot-tamper");
        let mut snapshot = valid_snapshot();
        snapshot.validation.success = true;
        assert!(
            snapshot
                .write(root.join("snapshot.json"))
                .unwrap_err()
                .contains("snapshot digest does not match")
        );
    }

    #[test]
    fn replay_environment_rejects_org_api_tool_age_and_clock_mismatches() {
        let snapshot = valid_snapshot();
        let now = system_time("2026-07-18T13:00:00Z");
        let options = replay_options();
        validate_replay_environment(&snapshot, &options, "65.0", "2.30.8", now).unwrap();

        let mut mismatch = options.clone();
        mismatch.expected_target_org = Some("production".to_owned());
        assert!(
            validate_replay_environment(&snapshot, &mismatch, "65.0", "2.30.8", now)
                .unwrap_err()
                .contains("target mismatch")
        );

        let mut mismatch = options.clone();
        mismatch.expected_org_id = Some("00D000000000002".to_owned());
        assert!(
            validate_replay_environment(&snapshot, &mismatch, "65.0", "2.30.8", now)
                .unwrap_err()
                .contains("org ID mismatch")
        );
        assert!(
            validate_replay_environment(&snapshot, &options, "66.0", "2.30.8", now)
                .unwrap_err()
                .contains("API version mismatch")
        );
        assert!(
            validate_replay_environment(&snapshot, &options, "65.0", "2.31.0", now)
                .unwrap_err()
                .contains("CLI version mismatch")
        );

        let mut mismatch = snapshot.clone();
        mismatch.evidence.apex_exec_version = "different".to_owned();
        assert!(
            validate_replay_environment(&mismatch, &options, "65.0", "2.30.8", now)
                .unwrap_err()
                .contains("Apex Exec version mismatch")
        );

        let mut mismatch = options.clone();
        mismatch.maximum_evidence_age = Duration::from_secs(60 * 60);
        assert!(
            validate_replay_environment(&snapshot, &mismatch, "65.0", "2.30.8", now)
                .unwrap_err()
                .contains("maximum-age mismatch")
        );

        assert!(
            validate_replay_environment(
                &snapshot,
                &options,
                "65.0",
                "2.30.8",
                system_time("2026-07-19T13:00:01Z"),
            )
            .unwrap_err()
            .contains("expired")
        );

        let mut future = snapshot;
        future.evidence.captured_at = "2026-07-18T13:06:00Z".to_owned();
        assert!(
            validate_replay_environment(&future, &options, "65.0", "2.30.8", now)
                .unwrap_err()
                .contains("in the future")
        );
    }

    #[test]
    fn candidate_binding_reports_manifest_result_and_request_mismatches() {
        let snapshot = valid_snapshot();
        let candidate = snapshot.evidence.candidate.clone();
        let request = snapshot.evidence.request.clone();
        validate_candidate_binding(&snapshot, &candidate, &request).unwrap();

        let mut mismatch = candidate.clone();
        mismatch.manifest_sha256 = "d".repeat(64);
        assert!(
            validate_candidate_binding(&snapshot, &mismatch, &request)
                .unwrap_err()
                .contains("M14 manifest")
        );
        let mut mismatch = candidate.clone();
        mismatch.ci_result_sha256 = "d".repeat(64);
        assert!(
            validate_candidate_binding(&snapshot, &mismatch, &request)
                .unwrap_err()
                .contains("exact M14 CI result")
        );
        let mut mismatch = candidate.clone();
        mismatch.ci_cache_key = "d".repeat(64);
        assert!(
            validate_candidate_binding(&snapshot, &mismatch, &request)
                .unwrap_err()
                .contains("M14 CI cache key")
        );

        let mut mismatch = request.clone();
        mismatch.changed_paths = vec![PathBuf::from("force-app/changed.cls")];
        assert!(
            validate_candidate_binding(&snapshot, &candidate, &mismatch)
                .unwrap_err()
                .contains("changed paths")
        );
        let mut mismatch = request;
        mismatch.affected_components = vec![AffectedComponentEvidence {
            selector: "ApexClass:Changed".to_owned(),
            sha256: "e".repeat(64),
        }];
        assert!(
            validate_candidate_binding(&snapshot, &candidate, &mismatch)
                .unwrap_err()
                .contains("selectors or digests")
        );

        let mut mismatch = snapshot.evidence.request.clone();
        mismatch.component_selection = ComponentSelectionMode::Impacted;
        assert!(
            validate_candidate_binding(&snapshot, &candidate, &mismatch)
                .unwrap_err()
                .contains("component-selection mode")
        );
        let mut mismatch = snapshot.evidence.request.clone();
        mismatch.selected_tests = vec!["DemoTest.passes".to_owned()];
        assert!(
            validate_candidate_binding(&snapshot, &candidate, &mismatch)
                .unwrap_err()
                .contains("selected tests")
        );
        let mut mismatch = snapshot.evidence.request.clone();
        mismatch.test_level = DeploymentTestLevel::RunSpecifiedTests;
        assert!(
            validate_candidate_binding(&snapshot, &candidate, &mismatch)
                .unwrap_err()
                .contains("test level")
        );
    }

    #[test]
    fn validation_requests_reject_duplicates_and_inconsistent_test_levels() {
        let mut request = ValidationRequest {
            changed_paths: vec![PathBuf::from("force-app/Demo.cls")],
            component_selection: ComponentSelectionMode::Impacted,
            affected_components: Vec::new(),
            selected_tests: vec!["DemoTest.passes".to_owned()],
            test_level: DeploymentTestLevel::NoTestRun,
        };
        assert!(request.validate().unwrap_err().contains("test level"));
        request.test_level = DeploymentTestLevel::RunSpecifiedTests;
        request
            .changed_paths
            .push(PathBuf::from("force-app/Demo.cls"));
        assert!(request.validate().unwrap_err().contains("more than once"));
    }

    #[test]
    fn salesforce_cli_version_parser_requires_the_official_version_token() {
        assert_eq!(
            parse_salesforce_cli_version("@salesforce/cli/2.30.8 darwin-arm64 node-v20.11.1\n")
                .unwrap(),
            "2.30.8"
        );
        assert!(
            parse_salesforce_cli_version("sf version unknown")
                .unwrap_err()
                .contains("not recognized")
        );
    }

    #[test]
    fn malformed_snapshot_fails_before_salesforce_cli_execution() {
        let root = fixture_root("snapshot-error-phase");
        let snapshot = root.join("invalid.json");
        fs::write(&snapshot, "{}\n").unwrap();
        let manifest = CiManifest::load("examples/milestone15-project/apex-exec-ci.json").unwrap();
        let error = run_with_cli_at(
            &manifest,
            &ValidationSource::Snapshot(snapshot),
            &replay_options(),
            &SalesforceValidationCli::new(root.join("missing-sf")),
            system_time("2026-07-18T13:00:00Z"),
        )
        .unwrap_err();
        assert!(error.contains("invalid validation snapshot"), "{error}");
        assert!(!error.contains("failed to start Salesforce CLI"));
    }

    #[test]
    fn run_options_require_cacheable_capture_and_exact_replay_assertions() {
        let target = ValidationSource::TargetOrg("staging".to_owned());
        let mut options = HybridRunOptions::default();
        options.ci.no_cache = true;
        assert!(
            validate_run_options(&target, &options)
                .unwrap_err()
                .contains("cacheable M14 CI artifact")
        );

        let snapshot = ValidationSource::Snapshot(PathBuf::from("validation.json"));
        let options = HybridRunOptions::default();
        assert!(
            validate_run_options(&snapshot, &options)
                .unwrap_err()
                .contains("requires `--replay`")
        );
        let mut options = HybridRunOptions::default();
        options.ci.replay_only = true;
        assert!(
            validate_run_options(&snapshot, &options)
                .unwrap_err()
                .contains("expected target org")
        );
    }

    fn valid_snapshot() -> ValidationSnapshot {
        let inventory = OrgInventory {
            schema_version: HYBRID_SCHEMA_VERSION,
            target: "staging".to_owned(),
            components: Vec::new(),
        };
        let request = ValidationRequest {
            changed_paths: Vec::new(),
            component_selection: ComponentSelectionMode::All,
            affected_components: Vec::new(),
            selected_tests: Vec::new(),
            test_level: DeploymentTestLevel::NoTestRun,
        };
        ValidationSnapshot::new(
            ValidationEvidence {
                candidate: CandidateEvidence {
                    manifest_sha256: "a".repeat(64),
                    ci_cache_key: "b".repeat(64),
                    ci_result_sha256: "c".repeat(64),
                },
                request_sha256: request.sha256().unwrap(),
                request,
                target: "staging".to_owned(),
                org_id: "00D000000000001".to_owned(),
                api_version: "65.0".to_owned(),
                apex_exec_version: env!("CARGO_PKG_VERSION").to_owned(),
                salesforce_cli_version: "2.30.8".to_owned(),
                captured_at: "2026-07-18T12:00:00Z".to_owned(),
                maximum_age_seconds: DEFAULT_MAX_EVIDENCE_AGE.as_secs(),
                retrieved_inventory_sha256: inventory.sha256().unwrap(),
                retrieval_count: REQUIRED_RETRIEVALS,
            },
            inventory,
            DeploymentValidation::default(),
        )
        .unwrap()
    }

    fn replay_options() -> HybridRunOptions {
        HybridRunOptions {
            ci: CiRunOptions {
                replay_only: true,
                ..CiRunOptions::default()
            },
            expected_target_org: Some("staging".to_owned()),
            expected_org_id: Some("00D000000000001".to_owned()),
            ..HybridRunOptions::default()
        }
    }

    fn system_time(value: &str) -> SystemTime {
        let parsed = DateTime::parse_from_rfc3339(value)
            .unwrap()
            .with_timezone(&Utc);
        parsed.into()
    }

    fn write(path: &Path, contents: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn fixture_root(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "apex-exec-hybrid-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }
}
