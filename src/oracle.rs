//! Salesforce differential conformance fixtures and normalized observations.
//!
//! The oracle keeps transport concerns outside compiler/runtime phases. Local
//! execution and Salesforce CLI responses are reduced to the same stable model,
//! which can be recorded in source control and compared without org access.

use crate::{
    diagnostic::Diagnostic,
    platform::DmlOperation,
    project::{self, Compilation, ProjectErrorKind},
    runtime::{
        Interpreter, PlatformHost, QueryKind, RecordingHost, RuntimeTriggerEvent, TriggerPhase,
        TriggerStage,
    },
    test_runner::{self, TestOptions},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
};

pub const ORACLE_SCHEMA_VERSION: u32 = 1;
const VALUE_MARKER: &str = "APEX_EXEC_ORACLE_VALUE|";

#[derive(Clone, Debug)]
pub struct ConformanceManifest {
    pub fixtures: Vec<ConformanceFixture>,
    source_path: PathBuf,
}

#[derive(Clone, Debug)]
pub struct ConformanceFixture {
    pub name: String,
    pub project: PathBuf,
    pub entrypoint: FixtureEntrypoint,
    pub compare: Vec<ComparisonScope>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManifestFile {
    schema_version: u32,
    fixtures: Vec<FixtureFile>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct FixtureFile {
    name: String,
    project: PathBuf,
    entrypoint: FixtureEntrypoint,
    compare: Vec<ComparisonScope>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum FixtureEntrypoint {
    Compile,
    Invoke { target: String },
    Test { filter: String },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "camelCase")]
pub enum ComparisonScope {
    Compile,
    Values,
    Output,
    Exceptions,
    Queries,
    Dml,
    Triggers,
    Tests,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OracleSnapshot {
    pub schema_version: u32,
    pub provider: OracleProvider,
    pub target: String,
    pub fixtures: Vec<FixtureObservation>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum OracleProvider {
    ApexExec,
    Salesforce,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FixtureObservation {
    pub name: String,
    pub compile: CompileObservation,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub values: BTreeMap<String, Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exception: Option<ExceptionObservation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub queries: Vec<QueryObservation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dml: Vec<DmlObservation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<TriggerObservation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tests: Vec<TestObservation>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompileObservation {
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic_category: Option<DiagnosticCategory>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum DiagnosticCategory {
    Lexical,
    Syntax,
    Semantic,
    Project,
    Io,
    Transport,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExceptionObservation {
    pub exception_type: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stack: Vec<StackObservation>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StackObservation {
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct QueryObservation {
    pub kind: String,
    pub objects: Vec<String>,
    pub rows: usize,
    pub succeeded: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DmlObservation {
    pub operation: String,
    pub objects: Vec<String>,
    pub records: usize,
    pub succeeded: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TriggerObservation {
    pub trigger: String,
    pub object: String,
    pub operation: String,
    pub phase: String,
    pub stage: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TestObservation {
    pub name: String,
    pub outcome: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OracleReport {
    pub fixtures: Vec<FixtureComparison>,
    pub coverage: CompatibilityCoverage,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixtureComparison {
    pub name: String,
    pub dimensions: Vec<DimensionComparison>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DimensionComparison {
    pub scope: ComparisonScope,
    pub matched: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub difference: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatibilityCoverage {
    pub matched: usize,
    pub total: usize,
    pub percentage: f64,
    pub by_scope: BTreeMap<ComparisonScope, CoverageMeasure>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverageMeasure {
    pub matched: usize,
    pub total: usize,
    pub percentage: f64,
}

impl ConformanceManifest {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let requested = path.as_ref();
        let source = fs::read_to_string(requested).map_err(|error| {
            format!(
                "failed to read oracle manifest `{}`: {error}",
                requested.display()
            )
        })?;
        let parsed: ManifestFile = serde_json::from_str(&source).map_err(|error| {
            format!("invalid oracle manifest `{}`: {error}", requested.display())
        })?;
        if parsed.schema_version != ORACLE_SCHEMA_VERSION {
            return Err(format!(
                "unsupported oracle manifest schema version {}; expected {}",
                parsed.schema_version, ORACLE_SCHEMA_VERSION
            ));
        }
        if parsed.fixtures.is_empty() {
            return Err("oracle manifest must contain at least one fixture".to_owned());
        }
        let source_path = requested.canonicalize().map_err(|error| {
            format!(
                "failed to resolve oracle manifest `{}`: {error}",
                requested.display()
            )
        })?;
        let base = source_path
            .parent()
            .expect("a canonical manifest path has a parent");
        let mut names = BTreeSet::new();
        let mut fixtures = Vec::with_capacity(parsed.fixtures.len());
        for fixture in parsed.fixtures {
            validate_fixture_name(&fixture.name)?;
            if !names.insert(fixture.name.to_ascii_lowercase()) {
                return Err(format!("duplicate oracle fixture `{}`", fixture.name));
            }
            validate_relative_project_path(&fixture.project)?;
            let project = base
                .join(&fixture.project)
                .canonicalize()
                .map_err(|error| {
                    format!(
                        "failed to resolve project `{}` for fixture `{}`: {error}",
                        fixture.project.display(),
                        fixture.name
                    )
                })?;
            if !project.starts_with(base) {
                return Err(format!(
                    "fixture `{}` project must remain below the manifest directory",
                    fixture.name
                ));
            }
            validate_entrypoint(&fixture.name, &fixture.entrypoint)?;
            validate_scopes(&fixture.name, &fixture.entrypoint, &fixture.compare)?;
            fixtures.push(ConformanceFixture {
                name: fixture.name,
                project,
                entrypoint: fixture.entrypoint,
                compare: fixture.compare,
            });
        }
        Ok(Self {
            fixtures,
            source_path,
        })
    }

    pub fn source_path(&self) -> &Path {
        &self.source_path
    }
}

impl OracleSnapshot {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let source = fs::read_to_string(path)
            .map_err(|error| format!("failed to read snapshot `{}`: {error}", path.display()))?;
        let snapshot: Self = serde_json::from_str(&source)
            .map_err(|error| format!("invalid oracle snapshot `{}`: {error}", path.display()))?;
        if snapshot.schema_version != ORACLE_SCHEMA_VERSION {
            return Err(format!(
                "unsupported oracle snapshot schema version {}; expected {}",
                snapshot.schema_version, ORACLE_SCHEMA_VERSION
            ));
        }
        validate_snapshot_names(&snapshot)?;
        Ok(snapshot)
    }

    pub fn write(&self, path: impl AsRef<Path>) -> Result<(), String> {
        let path = path.as_ref();
        let mut json = serde_json::to_string_pretty(self)
            .map_err(|error| format!("failed to serialize oracle snapshot: {error}"))?;
        json.push('\n');
        fs::write(path, json)
            .map_err(|error| format!("failed to write snapshot `{}`: {error}", path.display()))
    }
}

impl OracleReport {
    pub fn is_match(&self) -> bool {
        self.coverage.matched == self.coverage.total
    }

    pub fn render_console(&self) -> String {
        let mut lines = Vec::new();
        for fixture in &self.fixtures {
            for dimension in &fixture.dimensions {
                if dimension.matched {
                    lines.push(format!("MATCH {} {:?}", fixture.name, dimension.scope));
                } else {
                    lines.push(format!(
                        "DIFF  {} {:?}: {}",
                        fixture.name,
                        dimension.scope,
                        dimension.difference.as_deref().unwrap_or("values differ")
                    ));
                }
            }
        }
        lines.push(String::new());
        lines.push(format!(
            "Compatibility coverage: {}/{} dimensions matched ({:.2}%)",
            self.coverage.matched, self.coverage.total, self.coverage.percentage
        ));
        for (scope, measure) in &self.coverage.by_scope {
            lines.push(format!(
                "  {:?}: {}/{} ({:.2}%)",
                scope, measure.matched, measure.total, measure.percentage
            ));
        }
        lines.join("\n")
    }

    pub fn write(&self, path: impl AsRef<Path>) -> Result<(), String> {
        let path = path.as_ref();
        let mut json = serde_json::to_string_pretty(self)
            .map_err(|error| format!("failed to serialize oracle report: {error}"))?;
        json.push('\n');
        fs::write(path, json)
            .map_err(|error| format!("failed to write report `{}`: {error}", path.display()))
    }
}

pub fn run_local(manifest: &ConformanceManifest) -> OracleSnapshot {
    OracleSnapshot {
        schema_version: ORACLE_SCHEMA_VERSION,
        provider: OracleProvider::ApexExec,
        target: "local".to_owned(),
        fixtures: manifest.fixtures.iter().map(observe_local).collect(),
    }
}

pub fn run_salesforce(
    manifest: &ConformanceManifest,
    target_org: &str,
) -> Result<OracleSnapshot, String> {
    SalesforceCli::default().run(manifest, target_org)
}

pub fn compare(
    manifest: &ConformanceManifest,
    local: &OracleSnapshot,
    salesforce: &OracleSnapshot,
) -> Result<OracleReport, String> {
    if local.provider != OracleProvider::ApexExec {
        return Err("local snapshot provider must be `apexExec`".to_owned());
    }
    if salesforce.provider != OracleProvider::Salesforce {
        return Err("Salesforce snapshot provider must be `salesforce`".to_owned());
    }
    validate_snapshot_names(local)?;
    validate_snapshot_names(salesforce)?;
    let local_by_name = observation_map(local);
    let salesforce_by_name = observation_map(salesforce);
    let mut fixtures = Vec::with_capacity(manifest.fixtures.len());
    let mut coverage = CompatibilityCoverage::default();
    for fixture in &manifest.fixtures {
        let local = local_by_name.get(&fixture.name.to_ascii_lowercase());
        let salesforce = salesforce_by_name.get(&fixture.name.to_ascii_lowercase());
        let mut dimensions = Vec::with_capacity(fixture.compare.len());
        for scope in &fixture.compare {
            let (matched, difference) = match (local, salesforce) {
                (Some(local), Some(salesforce)) => compare_scope(*scope, local, salesforce),
                (None, _) => (false, Some("local snapshot is missing fixture".to_owned())),
                (_, None) => (
                    false,
                    Some("Salesforce snapshot is missing fixture".to_owned()),
                ),
            };
            coverage.total += 1;
            let measure = coverage.by_scope.entry(*scope).or_default();
            measure.total += 1;
            if matched {
                coverage.matched += 1;
                measure.matched += 1;
            }
            dimensions.push(DimensionComparison {
                scope: *scope,
                matched,
                difference,
            });
        }
        fixtures.push(FixtureComparison {
            name: fixture.name.clone(),
            dimensions,
        });
    }
    coverage.percentage = percentage(coverage.matched, coverage.total);
    for measure in coverage.by_scope.values_mut() {
        measure.percentage = percentage(measure.matched, measure.total);
    }
    Ok(OracleReport { fixtures, coverage })
}

fn observe_local(fixture: &ConformanceFixture) -> FixtureObservation {
    let mut observation = FixtureObservation {
        name: fixture.name.clone(),
        ..FixtureObservation::default()
    };
    let discovered = match project::discover(&fixture.project) {
        Ok(project) => project,
        Err(error) => {
            observation.compile =
                compile_error(project_error_category(error.kind()), error.to_string());
            return observation;
        }
    };
    for source in &discovered.files {
        if let Err(diagnostic) = crate::tokenize(&source.source) {
            observation.compile = compile_error(DiagnosticCategory::Lexical, diagnostic.message);
            return observation;
        }
        if let Err(diagnostic) = crate::parse(&source.source) {
            observation.compile = compile_error(DiagnosticCategory::Syntax, diagnostic.message);
            return observation;
        }
    }
    let compilation = match project::compile(&fixture.project) {
        Ok(compilation) => compilation,
        Err(error) => {
            let category = match error.kind() {
                ProjectErrorKind::Diagnostic => DiagnosticCategory::Semantic,
                other => project_error_category(other),
            };
            observation.compile = compile_error(category, error.to_string());
            return observation;
        }
    };
    observation.compile.success = true;
    match &fixture.entrypoint {
        FixtureEntrypoint::Compile => {}
        FixtureEntrypoint::Invoke { target } => {
            observe_local_invocation(&compilation, target, &mut observation)
        }
        FixtureEntrypoint::Test { filter } => {
            observe_local_tests(&compilation, filter, &mut observation)
        }
    }
    observation
}

fn observe_local_invocation(
    compilation: &Compilation,
    target: &str,
    observation: &mut FixtureObservation,
) {
    let Some((class, method)) = target.split_once('.') else {
        observation.exception = Some(ExceptionObservation {
            exception_type: "InvocationError".to_owned(),
            message: "invocation target must have the form Class.method".to_owned(),
            stack: Vec::new(),
        });
        return;
    };
    let mut host = RecordingHost::default();
    let result =
        Interpreter::with_host(&mut host).invoke_static(&compilation.program, class, method);
    let output = match result {
        Ok(output) => output,
        Err(diagnostic) => {
            observation.exception = Some(local_exception(compilation, &diagnostic));
            host.take_debug_output()
        }
    };
    let (values, output) = extract_values(output);
    observation.values = values;
    observation.output = output;
    observation.queries = host
        .query_events()
        .iter()
        .map(|event| QueryObservation {
            kind: match event.kind {
                QueryKind::Soql => "soql",
                QueryKind::Sosl => "sosl",
            }
            .to_owned(),
            objects: event.objects.clone(),
            rows: event.rows,
            succeeded: event.succeeded,
        })
        .collect();
    observation.dml = host
        .dml_events()
        .iter()
        .map(|event| DmlObservation {
            operation: dml_name(event.operation).to_owned(),
            objects: event.objects.clone(),
            records: event.records,
            succeeded: event.succeeded,
        })
        .collect();
    observation.triggers = host
        .trigger_events()
        .iter()
        .map(normalize_local_trigger)
        .collect();
}

fn observe_local_tests(
    compilation: &Compilation,
    filter: &str,
    observation: &mut FixtureObservation,
) {
    match test_runner::run(
        compilation,
        &TestOptions {
            filter: Some(filter.to_owned()),
            jobs: 1,
        },
    ) {
        Ok(report) => {
            for test in report.tests {
                let (values, output) = extract_values(test.output);
                observation.values.extend(values);
                observation.output.extend(output);
                observation.tests.push(TestObservation {
                    name: test.name,
                    outcome: if test.failure.is_some() {
                        "fail".to_owned()
                    } else {
                        "pass".to_owned()
                    },
                    message: test.failure.as_ref().map(|failure| failure.message.clone()),
                    stack: test.failure.map(|failure| failure.rendered),
                });
            }
        }
        Err(message) => {
            observation.exception = Some(ExceptionObservation {
                exception_type: "TestRunnerError".to_owned(),
                message,
                stack: Vec::new(),
            });
        }
    }
}

fn local_exception(compilation: &Compilation, diagnostic: &Diagnostic) -> ExceptionObservation {
    let stack = diagnostic
        .stack_trace
        .iter()
        .map(|frame| StackObservation {
            method: frame.method.clone(),
            line: compilation
                .source_position(frame.span)
                .map(|(_, line, _)| line),
        })
        .collect();
    ExceptionObservation {
        exception_type: diagnostic
            .exception_type
            .clone()
            .unwrap_or_else(|| "RuntimeError".to_owned()),
        message: diagnostic.message.clone(),
        stack,
    }
}

fn extract_values(lines: Vec<String>) -> (BTreeMap<String, Value>, Vec<String>) {
    let mut values = BTreeMap::new();
    let mut output = Vec::new();
    for line in lines {
        if let Some(marker) = line.strip_prefix(VALUE_MARKER)
            && let Some((name, encoded)) = marker.split_once('|')
            && !name.is_empty()
        {
            let value =
                serde_json::from_str(encoded).unwrap_or_else(|_| Value::String(encoded.to_owned()));
            values.insert(name.to_owned(), value);
        } else {
            output.push(line);
        }
    }
    (values, output)
}

fn normalize_local_trigger(event: &RuntimeTriggerEvent) -> TriggerObservation {
    TriggerObservation {
        trigger: event.trigger.clone(),
        object: event.object.clone(),
        operation: dml_name(event.operation).to_owned(),
        phase: match event.phase {
            TriggerPhase::Before => "before",
            TriggerPhase::After => "after",
        }
        .to_owned(),
        stage: match event.stage {
            TriggerStage::Enter => "enter",
            TriggerStage::Exit => "exit",
        }
        .to_owned(),
    }
}

#[derive(Clone, Debug)]
pub struct SalesforceCli {
    executable: OsString,
    wait_minutes: u32,
}

impl Default for SalesforceCli {
    fn default() -> Self {
        Self {
            executable: OsString::from("sf"),
            wait_minutes: 30,
        }
    }
}

impl SalesforceCli {
    pub fn new(executable: impl Into<OsString>) -> Self {
        Self {
            executable: executable.into(),
            ..Self::default()
        }
    }

    pub fn run(
        &self,
        manifest: &ConformanceManifest,
        target_org: &str,
    ) -> Result<OracleSnapshot, String> {
        if target_org.trim().is_empty() {
            return Err("Salesforce target org cannot be empty".to_owned());
        }
        let mut fixtures = Vec::with_capacity(manifest.fixtures.len());
        for fixture in &manifest.fixtures {
            fixtures.push(self.observe_fixture(fixture, target_org)?);
        }
        Ok(OracleSnapshot {
            schema_version: ORACLE_SCHEMA_VERSION,
            provider: OracleProvider::Salesforce,
            target: target_org.to_owned(),
            fixtures,
        })
    }

    fn observe_fixture(
        &self,
        fixture: &ConformanceFixture,
        target_org: &str,
    ) -> Result<FixtureObservation, String> {
        let discovered = project::discover(&fixture.project)
            .map_err(|error| format!("fixture `{}`: {}", fixture.name, error.render()))?;
        let mut deploy_args = vec![
            OsString::from("project"),
            OsString::from("deploy"),
            OsString::from("start"),
            OsString::from("--target-org"),
            OsString::from(target_org),
            OsString::from("--wait"),
            OsString::from(self.wait_minutes.to_string()),
            OsString::from("--json"),
        ];
        for source_root in &discovered.source_roots {
            deploy_args.push(OsString::from("--source-dir"));
            deploy_args.push(source_root.as_os_str().to_owned());
        }
        let deploy = self.command_json(&discovered.root, &deploy_args, None)?;
        let mut observation = parse_salesforce_deploy(&fixture.name, &deploy);
        if !observation.compile.success {
            return Ok(observation);
        }
        match &fixture.entrypoint {
            FixtureEntrypoint::Compile => {}
            FixtureEntrypoint::Invoke { target } => {
                let source = format!("{target}();\n");
                let args = [
                    OsString::from("apex"),
                    OsString::from("run"),
                    OsString::from("--target-org"),
                    OsString::from(target_org),
                    OsString::from("--json"),
                ];
                let result = self.command_json(&discovered.root, &args, Some(&source))?;
                apply_salesforce_execution(&mut observation, &result);
            }
            FixtureEntrypoint::Test { filter } => {
                let args = [
                    OsString::from("apex"),
                    OsString::from("run"),
                    OsString::from("test"),
                    OsString::from("--target-org"),
                    OsString::from(target_org),
                    OsString::from("--tests"),
                    OsString::from(filter),
                    OsString::from("--wait"),
                    OsString::from(self.wait_minutes.to_string()),
                    OsString::from("--result-format"),
                    OsString::from("json"),
                    OsString::from("--json"),
                ];
                let result = self.command_json(&discovered.root, &args, None)?;
                apply_salesforce_tests(&mut observation, &result);
            }
        }
        Ok(observation)
    }

    fn command_json(
        &self,
        current_dir: &Path,
        arguments: &[OsString],
        stdin: Option<&str>,
    ) -> Result<Value, String> {
        let mut command = Command::new(&self.executable);
        command
            .args(arguments)
            .current_dir(current_dir)
            .env("SF_DISABLE_LOG_FILE", "true")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if stdin.is_some() {
            command.stdin(Stdio::piped());
        }
        let mut child = command.spawn().map_err(|error| {
            format!(
                "failed to start Salesforce CLI `{}`: {error}",
                Path::new(&self.executable).display()
            )
        })?;
        if let Some(stdin) = stdin {
            child
                .stdin
                .take()
                .expect("stdin was configured")
                .write_all(stdin.as_bytes())
                .map_err(|error| format!("failed to send Apex to Salesforce CLI: {error}"))?;
        }
        let output = child
            .wait_with_output()
            .map_err(|error| format!("failed to wait for Salesforce CLI: {error}"))?;
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
}

fn parse_salesforce_deploy(name: &str, response: &Value) -> FixtureObservation {
    let success = find_bool(response, "success").unwrap_or_else(|| {
        find_u64(response, "status") == Some(0)
            || find_string(response, "status")
                .is_some_and(|status| status.eq_ignore_ascii_case("succeeded"))
    });
    let diagnostics = collect_failure_messages(response);
    FixtureObservation {
        name: name.to_owned(),
        compile: CompileObservation {
            success,
            diagnostic_category: (!success).then(|| {
                diagnostics
                    .first()
                    .map_or(DiagnosticCategory::Unknown, |message| {
                        classify_diagnostic(message)
                    })
            }),
            diagnostics,
        },
        ..FixtureObservation::default()
    }
}

fn apply_salesforce_execution(observation: &mut FixtureObservation, response: &Value) {
    if find_bool(response, "compiled") == Some(false) {
        observation.compile.success = false;
        let problem = find_string(response, "compileProblem").unwrap_or("compile failed");
        observation.compile.diagnostic_category = Some(classify_diagnostic(problem));
        observation.compile.diagnostics = vec![problem.to_owned()];
        return;
    }
    let logs = find_string(response, "logs").unwrap_or_default();
    let debug = salesforce_debug_output(logs);
    let (values, output) = extract_values(debug);
    observation.values = values;
    observation.output = output;
    if find_bool(response, "success") == Some(false) {
        let message = find_string(response, "exceptionMessage")
            .unwrap_or("Salesforce execution failed")
            .to_owned();
        let stack = find_string(response, "exceptionStackTrace").unwrap_or_default();
        observation.exception = Some(ExceptionObservation {
            exception_type: salesforce_exception_type(&message),
            message: strip_exception_type(&message).to_owned(),
            stack: parse_salesforce_stack(stack),
        });
    }
    observation.queries = parse_salesforce_queries(logs);
    observation.dml = parse_salesforce_dml(logs);
    observation.triggers = parse_salesforce_triggers(logs);
}

fn apply_salesforce_tests(observation: &mut FixtureObservation, response: &Value) {
    let mut tests = Vec::new();
    collect_test_results(response, &mut tests);
    tests.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
    });
    observation.tests = tests;
}

fn salesforce_debug_output(logs: &str) -> Vec<String> {
    logs.lines()
        .filter_map(|line| {
            let (_, message) = line.split_once("|USER_DEBUG|")?;
            let message = message
                .split_once("|DEBUG|")
                .map_or(message, |(_, message)| message);
            Some(message.trim().to_owned())
        })
        .collect()
}

fn parse_salesforce_queries(logs: &str) -> Vec<QueryObservation> {
    let mut queries = Vec::new();
    let mut pending: Option<QueryObservation> = None;
    for line in logs.lines() {
        if line.contains("|SOQL_EXECUTE_BEGIN|") {
            let statement = line.rsplit('|').next().unwrap_or_default();
            pending = Some(QueryObservation {
                kind: "soql".to_owned(),
                objects: soql_objects(statement),
                rows: 0,
                succeeded: false,
            });
        } else if line.contains("|SOQL_EXECUTE_END|")
            && let Some(mut query) = pending.take()
        {
            query.rows = value_after(line, "Rows:").unwrap_or(0);
            query.succeeded = true;
            queries.push(query);
        }
    }
    if let Some(query) = pending {
        queries.push(query);
    }
    queries
}

fn parse_salesforce_dml(logs: &str) -> Vec<DmlObservation> {
    let mut events = Vec::new();
    let mut pending: Option<DmlObservation> = None;
    for line in logs.lines() {
        if line.contains("|DML_BEGIN|") {
            pending = Some(DmlObservation {
                operation: value_text_after(line, "Op:")
                    .unwrap_or("unknown")
                    .to_ascii_lowercase(),
                objects: value_text_after(line, "Type:")
                    .map(|value| vec![value.to_owned()])
                    .unwrap_or_default(),
                records: value_after(line, "Rows:").unwrap_or(0),
                succeeded: false,
            });
        } else if line.contains("|DML_END|")
            && let Some(mut event) = pending.take()
        {
            event.succeeded = true;
            events.push(event);
        }
    }
    if let Some(event) = pending {
        events.push(event);
    }
    events
}

fn parse_salesforce_triggers(logs: &str) -> Vec<TriggerObservation> {
    let mut events = Vec::new();
    for line in logs.lines() {
        let stage = if line.contains("|CODE_UNIT_STARTED|") {
            "enter"
        } else if line.contains("|CODE_UNIT_FINISHED|") {
            "exit"
        } else {
            continue;
        };
        let Some((left, event)) = line.split_once(" trigger event ") else {
            continue;
        };
        let unit = left.rsplit('|').next().unwrap_or(left);
        let Some((trigger, object)) = unit.rsplit_once(" on ") else {
            continue;
        };
        let event = event.split('|').next().unwrap_or(event);
        let lower = event.to_ascii_lowercase();
        let phase = if lower.starts_with("before") {
            "before"
        } else if lower.starts_with("after") {
            "after"
        } else {
            continue;
        };
        let operation = lower.strip_prefix(phase).unwrap_or(&lower).to_owned();
        events.push(TriggerObservation {
            trigger: trigger.to_owned(),
            object: object.to_owned(),
            operation,
            phase: phase.to_owned(),
            stage: stage.to_owned(),
        });
    }
    events
}

fn collect_test_results(value: &Value, results: &mut Vec<TestObservation>) {
    match value {
        Value::Object(object) => {
            let method = object
                .get("MethodName")
                .or_else(|| object.get("methodName"))
                .and_then(Value::as_str);
            let class = object
                .get("ApexClass")
                .and_then(|value| value.get("Name"))
                .or_else(|| object.get("className"))
                .and_then(Value::as_str);
            let outcome = object
                .get("Outcome")
                .or_else(|| object.get("outcome"))
                .and_then(Value::as_str);
            if let (Some(class), Some(method), Some(outcome)) = (class, method, outcome) {
                let name = format!("{class}.{method}");
                if !results.iter().any(|result| result.name == name) {
                    results.push(TestObservation {
                        name,
                        outcome: if outcome.eq_ignore_ascii_case("pass") {
                            "pass"
                        } else {
                            "fail"
                        }
                        .to_owned(),
                        message: object
                            .get("Message")
                            .or_else(|| object.get("message"))
                            .and_then(Value::as_str)
                            .filter(|value| !value.is_empty())
                            .map(str::to_owned),
                        stack: object
                            .get("StackTrace")
                            .or_else(|| object.get("stackTrace"))
                            .and_then(Value::as_str)
                            .filter(|value| !value.is_empty())
                            .map(str::to_owned),
                    });
                }
            }
            for child in object.values() {
                collect_test_results(child, results);
            }
        }
        Value::Array(values) => {
            for child in values {
                collect_test_results(child, results);
            }
        }
        _ => {}
    }
}

fn compare_scope(
    scope: ComparisonScope,
    local: &FixtureObservation,
    salesforce: &FixtureObservation,
) -> (bool, Option<String>) {
    let (left, right) = match scope {
        ComparisonScope::Compile => (
            serde_json::json!({
                "success": local.compile.success,
                "diagnosticCategory": local.compile.diagnostic_category,
            }),
            serde_json::json!({
                "success": salesforce.compile.success,
                "diagnosticCategory": salesforce.compile.diagnostic_category,
            }),
        ),
        ComparisonScope::Values => (
            serde_json::to_value(&local.values).expect("values serialize"),
            serde_json::to_value(&salesforce.values).expect("values serialize"),
        ),
        ComparisonScope::Output => (
            serde_json::to_value(&local.output).expect("output serializes"),
            serde_json::to_value(&salesforce.output).expect("output serializes"),
        ),
        ComparisonScope::Exceptions => (
            serde_json::to_value(&local.exception).expect("exception serializes"),
            serde_json::to_value(&salesforce.exception).expect("exception serializes"),
        ),
        ComparisonScope::Queries => (
            serde_json::to_value(&local.queries).expect("queries serialize"),
            serde_json::to_value(&salesforce.queries).expect("queries serialize"),
        ),
        ComparisonScope::Dml => (
            serde_json::to_value(&local.dml).expect("DML serializes"),
            serde_json::to_value(&salesforce.dml).expect("DML serializes"),
        ),
        ComparisonScope::Triggers => (
            serde_json::to_value(&local.triggers).expect("triggers serialize"),
            serde_json::to_value(&salesforce.triggers).expect("triggers serialize"),
        ),
        ComparisonScope::Tests => (
            normalized_test_outcomes(&local.tests),
            normalized_test_outcomes(&salesforce.tests),
        ),
    };
    if left == right {
        (true, None)
    } else {
        (
            false,
            Some(format!(
                "local={} salesforce={}",
                compact_json(&left),
                compact_json(&right)
            )),
        )
    }
}

fn validate_fixture_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        return Err("oracle fixture name cannot be empty".to_owned());
    }
    if name.contains('/') || name.contains('\\') {
        return Err(format!(
            "oracle fixture name `{name}` cannot contain path separators"
        ));
    }
    Ok(())
}

fn validate_relative_project_path(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err("oracle fixture project must be a non-empty relative path".to_owned());
    }
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
    {
        return Err(format!(
            "oracle fixture project `{}` cannot escape the manifest directory",
            path.display()
        ));
    }
    Ok(())
}

fn validate_entrypoint(name: &str, entrypoint: &FixtureEntrypoint) -> Result<(), String> {
    let target = match entrypoint {
        FixtureEntrypoint::Compile => return Ok(()),
        FixtureEntrypoint::Invoke { target } => target,
        FixtureEntrypoint::Test { filter } => filter,
    };
    let Some((class, method)) = target.split_once('.') else {
        return Err(format!(
            "fixture `{name}` target must have the form Class.method"
        ));
    };
    if class.is_empty() || method.is_empty() || method.contains('.') {
        return Err(format!(
            "fixture `{name}` target must have the form Class.method"
        ));
    }
    Ok(())
}

fn validate_scopes(
    name: &str,
    entrypoint: &FixtureEntrypoint,
    scopes: &[ComparisonScope],
) -> Result<(), String> {
    if scopes.is_empty() {
        return Err(format!(
            "fixture `{name}` must select at least one comparison scope"
        ));
    }
    let mut unique = BTreeSet::new();
    for scope in scopes {
        if !unique.insert(*scope) {
            return Err(format!(
                "fixture `{name}` repeats comparison scope `{scope:?}`"
            ));
        }
    }
    if !unique.contains(&ComparisonScope::Compile) {
        return Err(format!(
            "fixture `{name}` must always compare compile behavior"
        ));
    }
    if matches!(entrypoint, FixtureEntrypoint::Compile) && scopes.len() != 1 {
        return Err(format!(
            "compile-only fixture `{name}` can compare only the compile scope"
        ));
    }
    Ok(())
}

fn validate_snapshot_names(snapshot: &OracleSnapshot) -> Result<(), String> {
    let mut names = BTreeSet::new();
    for fixture in &snapshot.fixtures {
        validate_fixture_name(&fixture.name)?;
        if !names.insert(fixture.name.to_ascii_lowercase()) {
            return Err(format!(
                "oracle snapshot contains duplicate fixture `{}`",
                fixture.name
            ));
        }
    }
    Ok(())
}

fn observation_map(snapshot: &OracleSnapshot) -> BTreeMap<String, &FixtureObservation> {
    snapshot
        .fixtures
        .iter()
        .map(|fixture| (fixture.name.to_ascii_lowercase(), fixture))
        .collect()
}

fn compile_error(category: DiagnosticCategory, message: String) -> CompileObservation {
    CompileObservation {
        success: false,
        diagnostic_category: Some(category),
        diagnostics: vec![message],
    }
}

fn project_error_category(kind: ProjectErrorKind) -> DiagnosticCategory {
    match kind {
        ProjectErrorKind::Project => DiagnosticCategory::Project,
        ProjectErrorKind::Io => DiagnosticCategory::Io,
        ProjectErrorKind::Diagnostic => DiagnosticCategory::Semantic,
    }
}

fn classify_diagnostic(message: &str) -> DiagnosticCategory {
    let lower = message.to_ascii_lowercase();
    if lower.contains("unexpected token")
        || lower.contains("expecting")
        || lower.contains("unexpected character")
        || lower.contains("unexpected syntax")
    {
        DiagnosticCategory::Syntax
    } else if lower.contains("invalid identifier") || lower.contains("illegal character") {
        DiagnosticCategory::Lexical
    } else if lower.contains("variable does not exist")
        || lower.contains("invalid type")
        || lower.contains("method does not exist")
        || lower.contains("unknown")
        || lower.contains("cannot assign")
        || lower.contains("incompatible")
    {
        DiagnosticCategory::Semantic
    } else {
        DiagnosticCategory::Unknown
    }
}

fn collect_failure_messages(value: &Value) -> Vec<String> {
    let mut messages = Vec::new();
    collect_key_strings(value, "problem", &mut messages);
    collect_key_strings(value, "message", &mut messages);
    messages.sort();
    messages.dedup();
    messages
}

fn collect_key_strings(value: &Value, key: &str, results: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            if let Some(value) = object.get(key).and_then(Value::as_str)
                && !value.is_empty()
            {
                results.push(value.to_owned());
            }
            for child in object.values() {
                collect_key_strings(child, key, results);
            }
        }
        Value::Array(values) => {
            for child in values {
                collect_key_strings(child, key, results);
            }
        }
        _ => {}
    }
}

fn find_bool(value: &Value, key: &str) -> Option<bool> {
    find_key(value, key).and_then(Value::as_bool)
}

fn find_u64(value: &Value, key: &str) -> Option<u64> {
    find_key(value, key).and_then(Value::as_u64)
}

fn find_string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    find_key(value, key).and_then(Value::as_str)
}

fn find_key<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    match value {
        Value::Object(object) => object
            .get(key)
            .or_else(|| object.values().find_map(|child| find_key(child, key))),
        Value::Array(values) => values.iter().find_map(|child| find_key(child, key)),
        _ => None,
    }
}

fn salesforce_exception_type(message: &str) -> String {
    message
        .split_once(':')
        .map(|(exception_type, _)| {
            exception_type
                .trim()
                .strip_prefix("System.")
                .unwrap_or(exception_type.trim())
                .to_owned()
        })
        .filter(|value| value.ends_with("Exception"))
        .unwrap_or_else(|| "RuntimeError".to_owned())
}

fn strip_exception_type(message: &str) -> &str {
    message
        .split_once(':')
        .filter(|(prefix, _)| prefix.trim().ends_with("Exception"))
        .map_or(message, |(_, message)| message.trim())
}

fn parse_salesforce_stack(stack: &str) -> Vec<StackObservation> {
    stack
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let method = line
                .strip_prefix("Class.")
                .and_then(|line| line.split_once(':').map(|(method, _)| method))
                .unwrap_or_else(|| line.split_once(':').map_or(line, |(method, _)| method));
            if method.is_empty() {
                return None;
            }
            Some(StackObservation {
                method: method.rsplit('.').next().unwrap_or(method).to_owned(),
                line: value_after(line, "line "),
            })
        })
        .collect()
}

fn soql_objects(statement: &str) -> Vec<String> {
    let words = statement.split_whitespace().collect::<Vec<_>>();
    words
        .windows(2)
        .filter(|pair| pair[0].eq_ignore_ascii_case("from"))
        .map(|pair| pair[1].trim_matches(',').to_owned())
        .collect()
}

fn value_after(line: &str, marker: &str) -> Option<usize> {
    let value = value_text_after(line, marker)?;
    let digits = value
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    (!digits.is_empty()).then(|| digits.parse::<usize>().ok())?
}

fn value_text_after<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    let (_, tail) = line.split_once(marker)?;
    Some(tail.split('|').next().unwrap_or(tail).trim())
}

fn dml_name(operation: DmlOperation) -> &'static str {
    match operation {
        DmlOperation::Insert => "insert",
        DmlOperation::Update => "update",
        DmlOperation::Upsert => "upsert",
        DmlOperation::Delete => "delete",
        DmlOperation::Undelete => "undelete",
    }
}

fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "<unserializable>".to_owned())
}

fn normalized_test_outcomes(tests: &[TestObservation]) -> Value {
    Value::Array(
        tests
            .iter()
            .map(|test| {
                serde_json::json!({
                    "name": test.name,
                    "outcome": test.outcome,
                })
            })
            .collect(),
    )
}

fn percentage(matched: usize, total: usize) -> f64 {
    if total == 0 {
        100.0
    } else {
        matched as f64 / total as f64 * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_typed_values_without_polluting_output() {
        let (values, output) = extract_values(vec![
            "ordinary".to_owned(),
            "APEX_EXEC_ORACLE_VALUE|count|3".to_owned(),
            "APEX_EXEC_ORACLE_VALUE|ready|true".to_owned(),
            "APEX_EXEC_ORACLE_VALUE|name|\"Ada\"".to_owned(),
        ]);
        assert_eq!(values["count"], serde_json::json!(3));
        assert_eq!(values["ready"], serde_json::json!(true));
        assert_eq!(values["name"], serde_json::json!("Ada"));
        assert_eq!(output, ["ordinary"]);
    }

    #[test]
    fn normalizes_salesforce_execution_logs() {
        let response = serde_json::json!({
            "status": 0,
            "result": {
                "compiled": true,
                "success": true,
                "logs": concat!(
                    "12:00:00.0|USER_DEBUG|[1]|DEBUG|APEX_EXEC_ORACLE_VALUE|count|2\n",
                    "12:00:00.0|DML_BEGIN|[2]|Op:Insert|Type:Invoice__c|Rows:1\n",
                    "12:00:00.0|CODE_UNIT_STARTED|[EXTERNAL]|InvoiceTrigger on Invoice__c trigger event BeforeInsert|x\n",
                    "12:00:00.0|CODE_UNIT_FINISHED|InvoiceTrigger on Invoice__c trigger event BeforeInsert|x\n",
                    "12:00:00.0|DML_END|[2]\n",
                    "12:00:00.0|SOQL_EXECUTE_BEGIN|[3]|Aggregations:0|SELECT Id FROM Invoice__c\n",
                    "12:00:00.0|SOQL_EXECUTE_END|[3]|Rows:1\n",
                    "12:00:00.0|USER_DEBUG|[4]|DEBUG|done\n"
                )
            }
        });
        let mut observation = FixtureObservation {
            name: "sample".to_owned(),
            compile: CompileObservation {
                success: true,
                ..CompileObservation::default()
            },
            ..FixtureObservation::default()
        };
        apply_salesforce_execution(&mut observation, &response);
        assert_eq!(observation.values["count"], serde_json::json!(2));
        assert_eq!(observation.output, ["done"]);
        assert_eq!(observation.dml[0].operation, "insert");
        assert_eq!(observation.queries[0].objects, ["Invoice__c"]);
        assert_eq!(observation.triggers.len(), 2);
        assert_eq!(observation.triggers[0].stage, "enter");
        assert_eq!(observation.triggers[1].stage, "exit");
    }

    #[test]
    fn normalizes_runtime_exceptions_and_salesforce_stacks() {
        let response = serde_json::json!({
            "result": {
                "compiled": true,
                "success": false,
                "exceptionMessage": "System.MathException: Division by zero",
                "exceptionStackTrace": "Class.Calculator.divide: line 8, column 1\nAnonymousBlock: line 1, column 1",
                "logs": ""
            }
        });
        let mut observation = FixtureObservation::default();
        apply_salesforce_execution(&mut observation, &response);
        let exception = observation.exception.unwrap();
        assert_eq!(exception.exception_type, "MathException");
        assert_eq!(exception.message, "Division by zero");
        assert_eq!(exception.stack[0].method, "divide");
        assert_eq!(exception.stack[0].line, Some(8));
    }

    #[test]
    fn comparison_measures_each_selected_dimension() {
        let manifest = ConformanceManifest {
            fixtures: vec![ConformanceFixture {
                name: "one".to_owned(),
                project: PathBuf::new(),
                entrypoint: FixtureEntrypoint::Invoke {
                    target: "Demo.run".to_owned(),
                },
                compare: vec![
                    ComparisonScope::Compile,
                    ComparisonScope::Values,
                    ComparisonScope::Output,
                ],
            }],
            source_path: PathBuf::new(),
        };
        let local = OracleSnapshot {
            schema_version: 1,
            provider: OracleProvider::ApexExec,
            target: "local".to_owned(),
            fixtures: vec![FixtureObservation {
                name: "one".to_owned(),
                compile: CompileObservation {
                    success: true,
                    ..CompileObservation::default()
                },
                values: BTreeMap::from([("answer".to_owned(), serde_json::json!(42))]),
                output: vec!["local".to_owned()],
                ..FixtureObservation::default()
            }],
        };
        let mut salesforce = local.clone();
        salesforce.provider = OracleProvider::Salesforce;
        salesforce.target = "scratch".to_owned();
        salesforce.fixtures[0].output = vec!["org".to_owned()];
        let report = compare(&manifest, &local, &salesforce).unwrap();
        assert_eq!(report.coverage.matched, 2);
        assert_eq!(report.coverage.total, 3);
        assert!((report.coverage.percentage - 66.666_666).abs() < 0.001);
        assert!(!report.is_match());
        assert!(
            report.fixtures[0].dimensions[2]
                .difference
                .as_deref()
                .unwrap()
                .contains("local")
        );
    }

    #[test]
    fn parses_salesforce_test_result_shapes_without_duplicates() {
        let response = serde_json::json!({
            "result": {
                "tests": [{
                    "ApexClass": {"Name": "DemoTest"},
                    "MethodName": "works",
                    "Outcome": "Pass",
                    "Message": null,
                    "StackTrace": null
                }]
            }
        });
        let mut observation = FixtureObservation::default();
        apply_salesforce_tests(&mut observation, &response);
        assert_eq!(
            observation.tests,
            [TestObservation {
                name: "DemoTest.works".to_owned(),
                outcome: "pass".to_owned(),
                message: None,
                stack: None,
            }]
        );
    }

    #[test]
    fn diagnostic_categories_are_stable_and_broad() {
        assert_eq!(
            classify_diagnostic("Unexpected token '?'"),
            DiagnosticCategory::Syntax
        );
        assert_eq!(
            classify_diagnostic("Variable does not exist: missing"),
            DiagnosticCategory::Semantic
        );
        assert_eq!(
            classify_diagnostic("Organization policy rejected this"),
            DiagnosticCategory::Unknown
        );
    }
}
