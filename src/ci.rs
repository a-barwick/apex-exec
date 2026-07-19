//! Hermetic, cacheable enterprise CI orchestration.
//!
//! This module deliberately sits above project compilation and isolated test
//! execution. It records exact inputs, selects tests through the compiler's
//! dependency graph, shards stable qualified names, emits standard reports,
//! and applies policy without moving CI concerns into language phases.

use crate::{
    ast::ClassMember,
    compatibility::EffectiveProfile,
    project::{self, Compilation, ProjectError},
    test_runner::{self, TestOptions, TestReport},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    time::Instant,
};

pub const CI_SCHEMA_VERSION: u32 = 1;
const CACHE_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CiInput {
    pub path: PathBuf,
    pub sha256: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CiShard {
    pub index: usize,
    pub total: usize,
}

impl Default for CiShard {
    fn default() -> Self {
        Self { index: 0, total: 1 }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CiReports {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub junit: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sarif: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage: Option<PathBuf>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CiPolicy {
    #[serde(default)]
    pub max_test_failures: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_line_coverage: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_branch_coverage: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compatibility_report: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_compatibility: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CiManifest {
    pub schema_version: u32,
    pub tool_version: String,
    pub project: PathBuf,
    pub inputs: Vec<CiInput>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changed_files: Vec<PathBuf>,
    #[serde(default)]
    pub shard: CiShard,
    pub jobs: usize,
    #[serde(default)]
    pub reports: CiReports,
    #[serde(default)]
    pub policy: CiPolicy,
    #[serde(skip)]
    source_path: PathBuf,
    #[serde(skip)]
    project_root: PathBuf,
}

impl CiManifest {
    pub fn generate(project_path: impl AsRef<Path>) -> Result<Self, String> {
        let discovered = project::discover(project_path).map_err(|error| error.render())?;
        let project_root = canonical(&discovered.root)?;
        let inputs = collect_inputs(&project_root, &discovered.source_roots, None)?;
        Ok(Self {
            schema_version: CI_SCHEMA_VERSION,
            tool_version: env!("CARGO_PKG_VERSION").to_owned(),
            project: PathBuf::from("."),
            inputs,
            changed_files: Vec::new(),
            shard: CiShard::default(),
            jobs: 1,
            reports: CiReports {
                junit: Some(PathBuf::from("artifacts/{shard}/junit.xml")),
                sarif: Some(PathBuf::from("artifacts/{shard}/results.sarif")),
                coverage: Some(PathBuf::from("artifacts/{shard}/coverage.xml")),
            },
            policy: CiPolicy::default(),
            source_path: project_root.join("apex-exec-ci.json"),
            project_root,
        })
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let requested = path.as_ref();
        let source = fs::read_to_string(requested).map_err(|error| {
            format!(
                "failed to read CI manifest `{}`: {error}",
                requested.display()
            )
        })?;
        let mut manifest = serde_json::from_str::<Self>(&source)
            .map_err(|error| format!("invalid CI manifest `{}`: {error}", requested.display()))?;
        manifest.source_path = canonical(requested)?;
        let base = manifest
            .source_path
            .parent()
            .expect("a manifest path has a parent");
        let requested_root = if manifest.project.is_absolute() {
            manifest.project.clone()
        } else {
            base.join(&manifest.project)
        };
        manifest.project_root = canonical(&requested_root)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn write(&self, path: impl AsRef<Path>) -> Result<(), String> {
        let path = path.as_ref();
        let absolute_path = absolute(path)?;
        let parent = absolute_path
            .parent()
            .ok_or_else(|| "CI manifest output must have a parent directory".to_owned())?;
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create CI manifest directory `{}`: {error}",
                parent.display()
            )
        })?;
        let resolved_parent = canonical(parent)?;
        let mut portable = self.clone();
        portable.project = relative_between(&resolved_parent, &self.project_root);
        portable.source_path = PathBuf::new();
        portable.project_root = PathBuf::new();
        portable.validate_serialized()?;
        let mut json = serde_json::to_string_pretty(&portable)
            .map_err(|error| format!("failed to serialize CI manifest: {error}"))?;
        json.push('\n');
        write_file(path, json.as_bytes(), "CI manifest")
    }

    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    pub fn sha256(&self) -> Result<String, String> {
        let bytes = serde_json::to_vec(self)
            .map_err(|error| format!("failed to serialize CI manifest identity: {error}"))?;
        Ok(sha256(&bytes))
    }

    pub fn refresh_inputs(&mut self) -> Result<(), String> {
        let discovered = project::discover(&self.project_root).map_err(|error| error.render())?;
        self.inputs = collect_inputs(
            &self.project_root,
            &discovered.source_roots,
            self.policy.compatibility_report.as_deref(),
        )?;
        Ok(())
    }

    pub fn verify_inputs(&self) -> Result<(), String> {
        self.validate()?;
        let discovered = project::discover(&self.project_root).map_err(|error| error.render())?;
        let actual = collect_inputs(
            &self.project_root,
            &discovered.source_roots,
            self.policy.compatibility_report.as_deref(),
        )?;
        if actual == self.inputs {
            return Ok(());
        }
        let expected = self
            .inputs
            .iter()
            .map(|input| (&input.path, &input.sha256))
            .collect::<BTreeMap<_, _>>();
        let current = actual
            .iter()
            .map(|input| (&input.path, &input.sha256))
            .collect::<BTreeMap<_, _>>();
        let mut differences = Vec::new();
        for path in expected
            .keys()
            .chain(current.keys())
            .collect::<BTreeSet<_>>()
        {
            match (expected.get(*path), current.get(*path)) {
                (Some(expected), Some(actual)) if expected != actual => {
                    differences.push(format!("modified `{}`", path.display()));
                }
                (Some(_), None) => differences.push(format!("missing `{}`", path.display())),
                (None, Some(_)) => differences.push(format!("unrecorded `{}`", path.display())),
                _ => {}
            }
        }
        Err(format!(
            "hermetic CI input verification failed: {}",
            differences.join(", ")
        ))
    }

    fn validate(&self) -> Result<(), String> {
        self.validate_serialized()?;
        if self.project_root.as_os_str().is_empty() {
            return Err("CI manifest has no resolved project root".to_owned());
        }
        Ok(())
    }

    fn validate_serialized(&self) -> Result<(), String> {
        if self.schema_version != CI_SCHEMA_VERSION {
            return Err(format!(
                "unsupported CI manifest schema version {}; expected {}",
                self.schema_version, CI_SCHEMA_VERSION
            ));
        }
        if self.tool_version != env!("CARGO_PKG_VERSION") {
            return Err(format!(
                "CI manifest requires apex-exec {}, but this binary is {}",
                self.tool_version,
                env!("CARGO_PKG_VERSION")
            ));
        }
        if self.jobs == 0 {
            return Err("CI jobs must be at least 1".to_owned());
        }
        validate_shard(self.shard)?;
        if self.inputs.is_empty() {
            return Err("CI manifest must record at least one input".to_owned());
        }
        let mut paths = BTreeSet::new();
        for input in &self.inputs {
            validate_relative(&input.path, "CI input")?;
            if !paths.insert(input.path.clone()) {
                return Err(format!(
                    "CI manifest records `{}` more than once",
                    input.path.display()
                ));
            }
            if input.sha256.len() != 64
                || !input
                    .sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            {
                return Err(format!(
                    "CI input `{}` has an invalid SHA-256 digest",
                    input.path.display()
                ));
            }
        }
        for changed in &self.changed_files {
            validate_relative(changed, "changed file")?;
        }
        for (path, label) in [
            (self.reports.junit.as_deref(), "JUnit report"),
            (self.reports.sarif.as_deref(), "SARIF report"),
            (self.reports.coverage.as_deref(), "coverage report"),
            (
                self.policy.compatibility_report.as_deref(),
                "compatibility report",
            ),
        ] {
            if let Some(path) = path {
                validate_relative(path, label)?;
            }
        }
        for value in [
            self.policy.min_line_coverage,
            self.policy.min_branch_coverage,
            self.policy.min_compatibility,
        ]
        .into_iter()
        .flatten()
        {
            if !(0.0..=100.0).contains(&value) || !value.is_finite() {
                return Err("CI percentage policies must be between 0 and 100".to_owned());
            }
        }
        if self.policy.min_compatibility.is_some() != self.policy.compatibility_report.is_some() {
            return Err(
                "compatibility policy requires both `compatibilityReport` and `minCompatibility`"
                    .to_owned(),
            );
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct CiRunOptions {
    pub cache_dir: Option<PathBuf>,
    pub shard: Option<CiShard>,
    pub no_cache: bool,
    pub replay_only: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CiRunResult {
    pub schema_version: u32,
    pub cache_key: String,
    pub shard: CiShard,
    pub selected_tests: Vec<String>,
    pub selection: SelectionMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<EffectiveProfile>,
    pub compile_success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compile_diagnostic: Option<CiDiagnostic>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tests: Option<TestReport>,
    pub compile_duration_ms: u64,
    pub test_duration_ms: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub policy_violations: Vec<String>,
    #[serde(skip)]
    pub cache_hit: bool,
}

impl CiRunResult {
    pub fn is_success(&self) -> bool {
        self.compile_success && self.policy_violations.is_empty()
    }

    pub fn render_console(&self) -> String {
        let mut lines = vec![format!(
            "CI artifact {} (shard {}/{}, {})",
            self.cache_key,
            self.shard.index + 1,
            self.shard.total,
            if self.cache_hit {
                "content-addressed cache hit"
            } else {
                "executed"
            }
        )];
        match &self.compile_diagnostic {
            Some(diagnostic) => lines.push(format!("COMPILE FAIL: {}", diagnostic.message)),
            None => lines.push("COMPILE PASS".to_owned()),
        }
        lines.push(format!(
            "Selection: {} tests ({})",
            self.selected_tests.len(),
            self.selection.label()
        ));
        if !self.profiles.is_empty() {
            lines.push(format!(
                "Profiles: {} effective source bindings",
                self.profiles.len()
            ));
        }
        if let Some(report) = &self.tests {
            lines.push(report.render_console());
        }
        lines.push(format!(
            "Performance: {} ms compile, {} ms tests",
            self.compile_duration_ms, self.test_duration_ms
        ));
        for violation in &self.policy_violations {
            lines.push(format!("POLICY FAIL: {violation}"));
        }
        lines.push(if self.is_success() {
            "CI PASS".to_owned()
        } else {
            "CI FAIL".to_owned()
        });
        lines.join("\n")
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SelectionMode {
    All,
    Impacted,
    ConservativeAll,
}

impl SelectionMode {
    fn label(self) -> &'static str {
        match self {
            Self::All => "all tests",
            Self::Impacted => "dependency-impacted tests",
            Self::ConservativeAll => "all tests (conservative fallback)",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CiDiagnostic {
    pub message: String,
    pub rendered: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CacheArtifact {
    schema_version: u32,
    key: String,
    result_sha256: String,
    result: CiRunResult,
}

pub fn run(manifest: &CiManifest, options: &CiRunOptions) -> Result<CiRunResult, String> {
    manifest.verify_inputs()?;
    let shard = options.shard.unwrap_or(manifest.shard);
    validate_shard(shard)?;
    let key = cache_key(manifest, shard)?;
    let cache_dir = options
        .cache_dir
        .clone()
        .unwrap_or_else(|| manifest.project_root.join(".apex-exec/cache"));
    let artifact_path = cache_dir.join(&key).join("result.json");

    if !options.no_cache && artifact_path.is_file() {
        let mut result = load_artifact(&artifact_path, &key)?;
        result.cache_hit = true;
        emit_reports(manifest, &result)?;
        return Ok(result);
    }
    if options.replay_only {
        return Err(format!(
            "no cached CI artifact `{key}` exists for deterministic replay"
        ));
    }

    let compile_started = Instant::now();
    let compilation = project::compile(&manifest.project_root);
    let compile_duration_ms = elapsed_ms(compile_started);
    let mut result = match compilation {
        Ok(compilation) => execute_tests(
            manifest,
            shard,
            key.clone(),
            compilation,
            compile_duration_ms,
        )?,
        Err(error) => {
            compile_failure_result(manifest, key.clone(), shard, compile_duration_ms, &error)
        }
    };
    apply_policy(manifest, &mut result)?;
    if !options.no_cache {
        store_artifact(&artifact_path, &result)?;
    }
    emit_reports(manifest, &result)?;
    Ok(result)
}

fn compile_failure_result(
    manifest: &CiManifest,
    cache_key: String,
    shard: CiShard,
    compile_duration_ms: u64,
    error: &ProjectError,
) -> CiRunResult {
    CiRunResult {
        schema_version: CACHE_SCHEMA_VERSION,
        cache_key,
        shard,
        selected_tests: Vec::new(),
        selection: if manifest.changed_files.is_empty() {
            SelectionMode::All
        } else {
            SelectionMode::ConservativeAll
        },
        profiles: Vec::new(),
        compile_success: false,
        compile_diagnostic: Some(ci_diagnostic(manifest, error)),
        tests: None,
        compile_duration_ms,
        test_duration_ms: 0,
        policy_violations: Vec::new(),
        cache_hit: false,
    }
}

fn execute_tests(
    manifest: &CiManifest,
    shard: CiShard,
    key: String,
    compilation: Compilation,
    compile_duration_ms: u64,
) -> Result<CiRunResult, String> {
    let (mut selected, selection) = select_impacted_tests(manifest, &compilation);
    selected = selected
        .into_iter()
        .enumerate()
        .filter_map(|(index, name)| (index % shard.total == shard.index).then_some(name))
        .collect();
    let selected_set = selected.iter().cloned().collect::<BTreeSet<_>>();
    let test_started = Instant::now();
    let tests = test_runner::run_selected(
        &compilation,
        &TestOptions {
            filter: None,
            jobs: manifest.jobs,
        },
        &selected_set,
    )?;
    Ok(CiRunResult {
        schema_version: CACHE_SCHEMA_VERSION,
        cache_key: key,
        shard,
        selected_tests: selected,
        selection,
        profiles: compilation.profiles.clone(),
        compile_success: true,
        compile_diagnostic: None,
        tests: Some(tests),
        compile_duration_ms,
        test_duration_ms: elapsed_ms(test_started),
        policy_violations: Vec::new(),
        cache_hit: false,
    })
}

fn select_impacted_tests(
    manifest: &CiManifest,
    compilation: &Compilation,
) -> (Vec<String>, SelectionMode) {
    let descriptors = test_descriptors(compilation);
    let all = descriptors.keys().cloned().collect::<Vec<_>>();
    if manifest.changed_files.is_empty() {
        return (all, SelectionMode::All);
    }

    let mut changed = BTreeSet::new();
    for relative in &manifest.changed_files {
        let absolute = manifest.project_root.join(relative);
        let extension = relative
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if !extension.eq_ignore_ascii_case("cls")
            || !compilation.dependencies.contains_file(&absolute)
        {
            return (all, SelectionMode::ConservativeAll);
        }
        changed.insert(absolute);
    }
    let impacted = compilation.dependencies.dependent_closure(changed);
    let selected = descriptors
        .into_iter()
        .filter_map(|(name, path)| impacted.contains(&path).then_some(name))
        .collect();
    (selected, SelectionMode::Impacted)
}

fn test_descriptors(compilation: &Compilation) -> BTreeMap<String, PathBuf> {
    let mut tests = BTreeMap::new();
    for class in &compilation.program.classes {
        if !class
            .annotations
            .iter()
            .any(|annotation| annotation.kind.is_test())
        {
            continue;
        }
        let Some((path, _, _)) = compilation.source_position(class.span) else {
            continue;
        };
        for member in &class.members {
            let ClassMember::Method(method) = member else {
                continue;
            };
            if method
                .annotations
                .iter()
                .any(|annotation| annotation.kind.is_test())
            {
                tests.insert(
                    format!("{}.{}", class.name.spelling, method.name.spelling),
                    path.clone(),
                );
            }
        }
    }
    tests
}

fn apply_policy(manifest: &CiManifest, result: &mut CiRunResult) -> Result<(), String> {
    let policy = &manifest.policy;
    if let Some(report) = &result.tests {
        if report.failed() > policy.max_test_failures {
            result.policy_violations.push(format!(
                "{} test failures exceed maximum {}",
                report.failed(),
                policy.max_test_failures
            ));
        }
        enforce_percentage(
            "line coverage",
            percentage(report.coverage.covered_lines, report.coverage.total_lines),
            policy.min_line_coverage,
            &mut result.policy_violations,
        );
        enforce_percentage(
            "branch coverage",
            percentage(
                report.coverage.covered_branches,
                report.coverage.total_branches,
            ),
            policy.min_branch_coverage,
            &mut result.policy_violations,
        );
    }
    if let Some(maximum) = policy.max_duration_ms {
        let actual = result.compile_duration_ms + result.test_duration_ms;
        if actual > maximum {
            result.policy_violations.push(format!(
                "CI duration {actual} ms exceeds maximum {maximum} ms"
            ));
        }
    }
    if let (Some(path), Some(minimum)) = (&policy.compatibility_report, policy.min_compatibility) {
        let source = fs::read_to_string(manifest.project_root.join(path)).map_err(|error| {
            format!(
                "failed to read compatibility report `{}`: {error}",
                path.display()
            )
        })?;
        let value = serde_json::from_str::<Value>(&source).map_err(|error| {
            format!("invalid compatibility report `{}`: {error}", path.display())
        })?;
        let actual = value
            .pointer("/coverage/percentage")
            .and_then(Value::as_f64)
            .ok_or_else(|| {
                format!(
                    "compatibility report `{}` has no numeric coverage percentage",
                    path.display()
                )
            })?;
        enforce_percentage(
            "compatibility coverage",
            actual,
            Some(minimum),
            &mut result.policy_violations,
        );
    }
    Ok(())
}

fn enforce_percentage(
    label: &str,
    actual: f64,
    minimum: Option<f64>,
    violations: &mut Vec<String>,
) {
    if let Some(minimum) = minimum
        && actual + f64::EPSILON < minimum
    {
        violations.push(format!(
            "{label} {actual:.2}% is below minimum {minimum:.2}%"
        ));
    }
}

fn emit_reports(manifest: &CiManifest, result: &CiRunResult) -> Result<(), String> {
    if let Some(path) = &manifest.reports.junit {
        let xml = result
            .tests
            .as_ref()
            .map_or_else(empty_junit, TestReport::to_junit_xml);
        write_report(manifest, result, path, xml.as_bytes(), "JUnit report")?;
    }
    if let Some(path) = &manifest.reports.coverage {
        let xml = result
            .tests
            .as_ref()
            .map_or_else(empty_cobertura, TestReport::to_cobertura_xml);
        write_report(manifest, result, path, xml.as_bytes(), "coverage report")?;
    }
    if let Some(path) = &manifest.reports.sarif {
        let json = sarif(result)?;
        write_report(manifest, result, path, json.as_bytes(), "SARIF report")?;
    }
    Ok(())
}

fn write_report(
    manifest: &CiManifest,
    result: &CiRunResult,
    template: &Path,
    contents: &[u8],
    label: &str,
) -> Result<(), String> {
    let rendered = template
        .to_string_lossy()
        .replace("{shard}", &result.shard.index.to_string());
    let path = PathBuf::from(rendered);
    validate_relative(&path, label)?;
    write_file(&manifest.project_root.join(path), contents, label)
}

fn sarif(result: &CiRunResult) -> Result<String, String> {
    let mut results = Vec::new();
    if let Some(diagnostic) = &result.compile_diagnostic {
        results.push(sarif_result(
            "apex-exec.compile",
            &diagnostic.message,
            diagnostic.path.as_deref(),
            diagnostic.line,
            diagnostic.column,
        ));
    }
    if let Some(report) = &result.tests {
        for test in &report.tests {
            if let Some(failure) = &test.failure {
                let location = rendered_location(&failure.rendered);
                results.push(sarif_result(
                    "apex-exec.test",
                    &format!("{}: {}", test.name, failure.message),
                    location.as_ref().map(|location| location.0.as_path()),
                    location.as_ref().map(|location| location.1),
                    location.as_ref().map(|location| location.2),
                ));
            }
        }
    }
    for violation in &result.policy_violations {
        results.push(sarif_result(
            "apex-exec.policy",
            violation,
            None,
            None,
            None,
        ));
    }
    let document = json!({
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "version": "2.1.0",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "apex-exec",
                    "version": env!("CARGO_PKG_VERSION"),
                    "rules": [
                        {"id": "apex-exec.compile", "shortDescription": {"text": "Apex compilation failure"}},
                        {"id": "apex-exec.test", "shortDescription": {"text": "Apex test failure"}},
                        {"id": "apex-exec.policy", "shortDescription": {"text": "Enterprise CI policy failure"}}
                    ]
                }
            },
            "automationDetails": {"id": format!("shard-{}/{}", result.shard.index, result.shard.total)},
            "results": results
        }]
    });
    let mut text = serde_json::to_string_pretty(&document)
        .map_err(|error| format!("failed to serialize SARIF: {error}"))?;
    text.push('\n');
    Ok(text)
}

fn sarif_result(
    rule: &str,
    message: &str,
    path: Option<&Path>,
    line: Option<usize>,
    column: Option<usize>,
) -> Value {
    let mut result = json!({
        "ruleId": rule,
        "level": "error",
        "message": {"text": message}
    });
    if let Some(path) = path {
        result["locations"] = json!([{
            "physicalLocation": {
                "artifactLocation": {"uri": path.to_string_lossy()},
                "region": {
                    "startLine": line.unwrap_or(1),
                    "startColumn": column.unwrap_or(1)
                }
            }
        }]);
    }
    result
}

fn ci_diagnostic(manifest: &CiManifest, error: &ProjectError) -> CiDiagnostic {
    let rendered = error.render();
    let (line, column) = rendered_location(&rendered)
        .map(|(_, line, column)| (line, column))
        .map_or((None, None), |(line, column)| (Some(line), Some(column)));
    let path = error.path().map(|path| {
        path.strip_prefix(&manifest.project_root)
            .map_or_else(|_| path.to_path_buf(), Path::to_path_buf)
    });
    CiDiagnostic {
        message: error.to_string(),
        rendered,
        path,
        line,
        column,
    }
}

fn rendered_location(rendered: &str) -> Option<(PathBuf, usize, usize)> {
    let location = rendered
        .lines()
        .find_map(|line| line.trim_start().strip_prefix("--> "))?;
    let mut parts = location.rsplitn(3, ':');
    let column = parts.next()?.parse().ok()?;
    let line = parts.next()?.parse().ok()?;
    let path = PathBuf::from(parts.next()?);
    Some((path, line, column))
}

fn cache_key(manifest: &CiManifest, shard: CiShard) -> Result<String, String> {
    let mut identity = manifest.clone();
    identity.shard = shard;
    identity.source_path = PathBuf::new();
    identity.project_root = PathBuf::new();
    let profiles = project::discover(&manifest.project_root)
        .map_err(|error| error.render())?
        .effective_profiles();
    let bytes = serde_json::to_vec(&(identity, profiles))
        .map_err(|error| format!("failed to serialize CI cache identity: {error}"))?;
    Ok(sha256(&bytes))
}

fn load_artifact(path: &Path, expected_key: &str) -> Result<CiRunResult, String> {
    let source = fs::read_to_string(path).map_err(|error| {
        format!(
            "failed to read CI cache artifact `{}`: {error}",
            path.display()
        )
    })?;
    let artifact = serde_json::from_str::<CacheArtifact>(&source)
        .map_err(|error| format!("invalid CI cache artifact `{}`: {error}", path.display()))?;
    if artifact.schema_version != CACHE_SCHEMA_VERSION {
        return Err(format!(
            "unsupported CI cache schema version {}; expected {}",
            artifact.schema_version, CACHE_SCHEMA_VERSION
        ));
    }
    if result_sha256(&artifact.result)? != artifact.result_sha256 {
        return Err("CI cache artifact result digest does not match its contents".to_owned());
    }
    if artifact.key != expected_key || artifact.result.cache_key != expected_key {
        return Err("content-addressed CI cache key does not match artifact contents".to_owned());
    }
    Ok(artifact.result)
}

fn store_artifact(path: &Path, result: &CiRunResult) -> Result<(), String> {
    let artifact = CacheArtifact {
        schema_version: CACHE_SCHEMA_VERSION,
        key: result.cache_key.clone(),
        result_sha256: result_sha256(result)?,
        result: result.clone(),
    };
    let mut json = serde_json::to_string_pretty(&artifact)
        .map_err(|error| format!("failed to serialize CI cache artifact: {error}"))?;
    json.push('\n');
    let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
    write_file(&temporary, json.as_bytes(), "temporary CI cache artifact")?;
    fs::rename(&temporary, path).map_err(|error| {
        format!(
            "failed to publish CI cache artifact `{}`: {error}",
            path.display()
        )
    })
}

pub fn write_integrations(
    output_directory: impl AsRef<Path>,
    manifest_path: impl AsRef<Path>,
) -> Result<Vec<PathBuf>, String> {
    let output = output_directory.as_ref();
    fs::create_dir_all(output).map_err(|error| {
        format!(
            "failed to create integration directory `{}`: {error}",
            output.display()
        )
    })?;
    let manifest = manifest_path.as_ref().to_string_lossy();
    let files = [
        (
            "github-actions.yml",
            format!(
                "name: apex-exec\non: [pull_request]\npermissions:\n  contents: read\n  security-events: write\njobs:\n  apex:\n    runs-on: ubuntu-latest\n    strategy:\n      fail-fast: false\n      matrix:\n        shard: [0, 1]\n    steps:\n      - uses: actions/checkout@v4\n        with:\n          fetch-depth: 0\n      - run: git diff --name-only \"${{{{ github.event.pull_request.base.sha }}}}\" > apex-exec-changed.txt\n      - run: apex-exec ci run {manifest} --changed-list apex-exec-changed.txt --shard ${{{{ matrix.shard }}}}/2\n      - uses: github/codeql-action/upload-sarif@v3\n        if: always()\n        with:\n          sarif_file: artifacts/${{{{ matrix.shard }}}}/results.sarif\n"
            ),
        ),
        (
            ".gitlab-ci.yml",
            format!(
                "apex-exec:\n  parallel: 2\n  script:\n    - git diff --name-only \"$CI_MERGE_REQUEST_DIFF_BASE_SHA\" > apex-exec-changed.txt\n    - apex-exec ci run {manifest} --changed-list apex-exec-changed.txt --shard \"$((CI_NODE_INDEX-1))/$CI_NODE_TOTAL\"\n  artifacts:\n    when: always\n    paths: [artifacts/]\n    reports:\n      junit: artifacts/*/junit.xml\n      coverage_report:\n        coverage_format: cobertura\n        path: artifacts/*/coverage.xml\n"
            ),
        ),
        (
            "Jenkinsfile",
            format!(
                "pipeline {{\n  agent any\n  stages {{\n    stage('Changed files') {{ steps {{ sh 'git diff --name-only origin/main > apex-exec-changed.txt' }} }}\n    stage('Apex Exec') {{\n      parallel {{\n        stage('Shard 0') {{ steps {{ sh 'apex-exec ci run {manifest} --changed-list apex-exec-changed.txt --shard 0/2' }} }}\n        stage('Shard 1') {{ steps {{ sh 'apex-exec ci run {manifest} --changed-list apex-exec-changed.txt --shard 1/2' }} }}\n      }}\n    }}\n  }}\n  post {{ always {{ junit 'artifacts/*/junit.xml'; recordCoverage tools: [[parser: 'COBERTURA', pattern: 'artifacts/*/coverage.xml']] }} }}\n}}\n"
            ),
        ),
    ];
    let mut written = Vec::new();
    for (name, contents) in files {
        let path = output.join(name);
        write_file(&path, contents.as_bytes(), "CI integration")?;
        written.push(path);
    }
    Ok(written)
}

fn collect_inputs(
    root: &Path,
    source_roots: &[PathBuf],
    compatibility_report: Option<&Path>,
) -> Result<Vec<CiInput>, String> {
    let mut paths = vec![root.join("sfdx-project.json")];
    for source_root in source_roots {
        collect_regular_files(source_root, &mut paths)?;
    }
    if let Some(report) = compatibility_report {
        validate_relative(report, "compatibility report")?;
        paths.push(root.join(report));
    }
    let mut paths = paths
        .into_iter()
        .map(|path| canonical(&path))
        .collect::<Result<Vec<_>, _>>()?;
    paths.sort();
    paths.dedup();
    paths
        .into_iter()
        .map(|resolved| {
            let relative = resolved
                .strip_prefix(root)
                .map_err(|_| {
                    format!(
                        "CI input `{}` is outside project root `{}`",
                        resolved.display(),
                        root.display()
                    )
                })?
                .to_path_buf();
            let bytes = fs::read(&resolved).map_err(|error| {
                format!("failed to read CI input `{}`: {error}", resolved.display())
            })?;
            Ok(CiInput {
                path: relative,
                sha256: sha256(&bytes),
            })
        })
        .collect()
}

fn collect_regular_files(directory: &Path, paths: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("failed to scan CI input `{}`: {error}", directory.display()))?;
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("failed to scan `{}`: {error}", directory.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| format!("failed to inspect `{}`: {error}", path.display()))?;
        if file_type.is_symlink() {
            return Err(format!(
                "hermetic CI inputs do not permit symlink `{}`",
                path.display()
            ));
        }
        if file_type.is_dir() {
            collect_regular_files(&path, paths)?;
        } else if file_type.is_file() {
            paths.push(path);
        }
    }
    Ok(())
}

fn validate_shard(shard: CiShard) -> Result<(), String> {
    if shard.total == 0 || shard.index >= shard.total {
        return Err(format!(
            "invalid CI shard {}/{}; index is zero-based and must be below a positive total",
            shard.index, shard.total
        ));
    }
    Ok(())
}

fn validate_relative(path: &Path, label: &str) -> Result<(), String> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(format!("{label} path must be a non-empty relative path"));
    }
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!(
            "{label} `{}` must not contain `.` or `..` components",
            path.display()
        ));
    }
    Ok(())
}

fn write_file(path: &Path, contents: &[u8], label: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create {label} directory `{}`: {error}",
                parent.display()
            )
        })?;
    }
    fs::write(path, contents)
        .map_err(|error| format!("failed to write {label} `{}`: {error}", path.display()))
}

fn canonical(path: &Path) -> Result<PathBuf, String> {
    path.canonicalize()
        .map_err(|error| format!("failed to resolve `{}`: {error}", path.display()))
}

fn absolute(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .map_err(|error| format!("failed to resolve current directory: {error}"))
    }
}

fn relative_between(from: &Path, to: &Path) -> PathBuf {
    let from = from.components().collect::<Vec<_>>();
    let to = to.components().collect::<Vec<_>>();
    let common = from
        .iter()
        .zip(&to)
        .take_while(|(left, right)| left == right)
        .count();
    let mut result = PathBuf::new();
    for _ in common..from.len() {
        result.push("..");
    }
    for component in &to[common..] {
        result.push(component.as_os_str());
    }
    if result.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        result
    }
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn result_sha256(result: &CiRunResult) -> Result<String, String> {
    let bytes = serde_json::to_vec(result)
        .map_err(|error| format!("failed to serialize CI cache result: {error}"))?;
    Ok(sha256(&bytes))
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn percentage(covered: usize, total: usize) -> f64 {
    if total == 0 {
        100.0
    } else {
        covered as f64 / total as f64 * 100.0
    }
}

fn empty_junit() -> String {
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuite name=\"apex-exec\" tests=\"0\" failures=\"0\" time=\"0\"/>\n".to_owned()
}

fn empty_cobertura() -> String {
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<coverage line-rate=\"1\" branch-rate=\"1\" lines-covered=\"0\" lines-valid=\"0\" branches-covered=\"0\" branches-valid=\"0\" version=\"apex-exec\" timestamp=\"0\"><sources/><packages/></coverage>\n".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_runner::{CoverageReport, TestFailure, TestResult};
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQUENCE: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn manifest_validation_rejects_every_unsafe_or_incoherent_field() {
        let root = project("validation");
        let mut manifest = CiManifest::generate(&root).unwrap();

        let mut invalid = manifest.clone();
        invalid.schema_version += 1;
        assert!(
            invalid
                .validate_serialized()
                .unwrap_err()
                .contains("schema")
        );
        invalid = manifest.clone();
        invalid.tool_version = "different".to_owned();
        assert!(
            invalid
                .validate_serialized()
                .unwrap_err()
                .contains("requires apex-exec")
        );
        invalid = manifest.clone();
        invalid.jobs = 0;
        assert!(invalid.validate_serialized().unwrap_err().contains("jobs"));
        invalid = manifest.clone();
        invalid.inputs.clear();
        assert!(
            invalid
                .validate_serialized()
                .unwrap_err()
                .contains("at least one input")
        );
        invalid = manifest.clone();
        invalid.inputs.push(invalid.inputs[0].clone());
        assert!(
            invalid
                .validate_serialized()
                .unwrap_err()
                .contains("more than once")
        );
        invalid = manifest.clone();
        invalid.inputs[0].sha256 = "NOT-A-DIGEST".to_owned();
        assert!(
            invalid
                .validate_serialized()
                .unwrap_err()
                .contains("invalid SHA-256")
        );
        invalid = manifest.clone();
        invalid.changed_files = vec![PathBuf::from("../escape.cls")];
        assert!(
            invalid
                .validate_serialized()
                .unwrap_err()
                .contains("must not contain")
        );
        invalid = manifest.clone();
        invalid.reports.sarif = Some(PathBuf::from("/absolute/results.sarif"));
        assert!(
            invalid
                .validate_serialized()
                .unwrap_err()
                .contains("relative path")
        );
        invalid = manifest.clone();
        invalid.policy.min_line_coverage = Some(f64::NAN);
        assert!(
            invalid
                .validate_serialized()
                .unwrap_err()
                .contains("between 0 and 100")
        );
        invalid = manifest.clone();
        invalid.policy.min_compatibility = Some(90.0);
        assert!(
            invalid
                .validate_serialized()
                .unwrap_err()
                .contains("requires both")
        );

        let nested = root.join("configuration/generated/ci.json");
        manifest.write(&nested).unwrap();
        assert_eq!(
            CiManifest::load(&nested).unwrap().project_root(),
            root.canonicalize().unwrap()
        );
        let absolute = root.join("absolute-ci.json");
        let mut absolute_json =
            serde_json::from_str::<Value>(&fs::read_to_string(&nested).unwrap()).unwrap();
        absolute_json["project"] = json!(root.canonicalize().unwrap());
        fs::write(&absolute, serde_json::to_vec(&absolute_json).unwrap()).unwrap();
        assert_eq!(
            CiManifest::load(&absolute).unwrap().project_root(),
            root.canonicalize().unwrap()
        );
        manifest.project_root.clear();
        assert!(
            manifest
                .validate()
                .unwrap_err()
                .contains("resolved project root")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn manifest_loading_and_input_collection_report_structural_failures() {
        let root = project("load-errors");
        let missing = root.join("missing.json");
        assert!(
            CiManifest::load(&missing)
                .unwrap_err()
                .contains("failed to read")
        );

        let invalid = root.join("invalid.json");
        fs::write(&invalid, "{nope").unwrap();
        assert!(
            CiManifest::load(&invalid)
                .unwrap_err()
                .contains("invalid CI manifest")
        );

        let outside = root.parent().unwrap().join(format!(
            "{}-outside",
            root.file_name().unwrap().to_string_lossy()
        ));
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("Outside.cls"), "public class Outside {}").unwrap();
        fs::write(
            root.join("sfdx-project.json"),
            format!(
                r#"{{"packageDirectories":[{{"path":"../{}"}}],"sourceApiVersion":"66.0"}}"#,
                outside.file_name().unwrap().to_string_lossy()
            ),
        )
        .unwrap();
        assert!(
            CiManifest::generate(&root)
                .unwrap_err()
                .contains("outside project root")
        );

        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(outside).unwrap();
    }

    #[test]
    fn policy_and_sarif_cover_test_failure_duration_and_source_location() {
        let root = project("policy");
        let mut manifest = CiManifest::generate(&root).unwrap();
        manifest.policy.max_duration_ms = Some(1);
        let mut result = CiRunResult {
            schema_version: CACHE_SCHEMA_VERSION,
            cache_key: "a".repeat(64),
            shard: CiShard::default(),
            selected_tests: vec!["DemoTest.fails".to_owned()],
            selection: SelectionMode::All,
            profiles: Vec::new(),
            compile_success: true,
            compile_diagnostic: None,
            tests: Some(TestReport {
                tests: vec![TestResult {
                    name: "DemoTest.fails".to_owned(),
                    class_name: "DemoTest".to_owned(),
                    method_name: "fails".to_owned(),
                    output: Vec::new(),
                    failure: Some(TestFailure {
                        exception_type: Some("AssertException".to_owned()),
                        message: "expected failure".to_owned(),
                        rendered: "error: failure\n --> DemoTest.cls:7:9".to_owned(),
                    }),
                }],
                coverage: CoverageReport::default(),
            }),
            compile_duration_ms: 2,
            test_duration_ms: 3,
            policy_violations: Vec::new(),
            cache_hit: false,
        };
        apply_policy(&manifest, &mut result).unwrap();
        assert_eq!(result.policy_violations.len(), 2);
        assert!(result.render_console().contains("POLICY FAIL"));
        assert!(result.render_console().contains("CI FAIL"));

        let sarif = sarif(&result).unwrap();
        assert!(sarif.contains("apex-exec.test"));
        assert!(sarif.contains("\"startLine\": 7"));
        assert!(sarif.contains("\"startColumn\": 9"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn corrupt_cache_artifacts_and_helper_boundaries_are_explicit() {
        let root = project("cache-errors");
        let artifact = root.join("artifact.json");
        fs::write(
            &artifact,
            r#"{"schemaVersion":99,"key":"bad","resultSha256":"","result":{"schemaVersion":1,"cacheKey":"bad","shard":{"index":0,"total":1},"selectedTests":[],"selection":"all","compileSuccess":true,"compileDurationMs":0,"testDurationMs":0}}"#,
        )
        .unwrap();
        assert!(
            load_artifact(&artifact, "bad")
                .unwrap_err()
                .contains("cache schema")
        );

        let result = CiRunResult {
            schema_version: CACHE_SCHEMA_VERSION,
            cache_key: "right".to_owned(),
            shard: CiShard::default(),
            selected_tests: Vec::new(),
            selection: SelectionMode::All,
            profiles: Vec::new(),
            compile_success: true,
            compile_diagnostic: None,
            tests: None,
            compile_duration_ms: 0,
            test_duration_ms: 0,
            policy_violations: Vec::new(),
            cache_hit: false,
        };
        store_artifact(&artifact, &result).unwrap();
        let mut tampered =
            serde_json::from_str::<Value>(&fs::read_to_string(&artifact).unwrap()).unwrap();
        tampered["result"]["selectedTests"] = json!(["Tampered.test"]);
        fs::write(&artifact, serde_json::to_vec(&tampered).unwrap()).unwrap();
        assert!(
            load_artifact(&artifact, "right")
                .unwrap_err()
                .contains("digest does not match")
        );
        store_artifact(&artifact, &result).unwrap();
        assert!(
            load_artifact(&artifact, "wrong")
                .unwrap_err()
                .contains("key does not match")
        );
        assert!(validate_relative(Path::new(""), "test").is_err());
        assert!(validate_relative(Path::new("/absolute"), "test").is_err());
        assert_eq!(percentage(0, 0), 100.0);
        assert_eq!(
            relative_between(Path::new("/one/two"), Path::new("/one/three")),
            PathBuf::from("../three")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn hermetic_inventory_rejects_symlinks() {
        use std::os::unix::fs::symlink;

        let root = project("symlink");
        symlink(
            root.join("force-app/main/default/classes/Demo.cls"),
            root.join("force-app/main/default/classes/Alias.cls"),
        )
        .unwrap();
        assert!(
            CiManifest::generate(&root)
                .unwrap_err()
                .contains("do not permit symlink")
        );
        fs::remove_dir_all(root).unwrap();
    }

    fn project(label: &str) -> PathBuf {
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "apex-exec-ci-unit-{label}-{}-{sequence}",
            std::process::id()
        ));
        let classes = root.join("force-app/main/default/classes");
        fs::create_dir_all(&classes).unwrap();
        fs::write(
            root.join("sfdx-project.json"),
            r#"{"packageDirectories":[{"path":"force-app"}],"sourceApiVersion":"66.0"}"#,
        )
        .unwrap();
        fs::write(classes.join("Demo.cls"), "public class Demo {}").unwrap();
        root
    }
}
