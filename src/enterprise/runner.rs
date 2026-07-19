use super::{EnterpriseManifest, SalesforceCapture};
use crate::{
    compatibility::{ApiVersion, CompatibilityProfile, ProfileOrigin},
    platform,
    project::{Compilation, SourceFile, compile_source_subset},
    test_runner::{self, TestOptions},
    token::TokenKind,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

const ENTERPRISE_REPORT_SCHEMA_VERSION: u32 = 1;
const REQUIRED_RERUNS: usize = 3;
type SourceInventory = BTreeMap<PathBuf, SourceFile>;
type OwnerIndex = BTreeMap<String, PathBuf>;
type DependencyIndex = BTreeMap<PathBuf, BTreeSet<PathBuf>>;

#[derive(Clone, Debug)]
pub struct EnterpriseRunOptions {
    pub reruns: usize,
}

impl Default for EnterpriseRunOptions {
    fn default() -> Self {
        Self {
            reruns: REQUIRED_RERUNS,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StageMetric {
    pub count: usize,
    pub denominator: usize,
    pub basis_points: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnterpriseCounts {
    pub discovery: StageMetric,
    pub parse: StageMetric,
    pub check: StageMetric,
    pub execution: StageMetric,
    pub agreement: StageMetric,
    pub strict_compatible: StageMetric,
    pub matching_passes: usize,
    pub matching_failures: usize,
    pub outcome_mismatches: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnterpriseBlocker {
    pub phase: String,
    pub family: String,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnterpriseTestMeasurement {
    pub name: String,
    pub salesforce_outcome: String,
    pub required_source_closure: Vec<PathBuf>,
    pub discovered: bool,
    pub parsed: bool,
    pub checked: bool,
    pub executed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_outcome: Option<String>,
    pub outcome_agrees: bool,
    pub strict_compatible: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blockers: Vec<EnterpriseBlocker>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnterpriseBlockerSummary {
    pub phase: String,
    pub family: String,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<usize>,
    pub impacted_tests: usize,
    pub tests: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnterpriseTiming {
    pub run: usize,
    pub cache_state: String,
    pub duration_ms: u128,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnterpriseReport {
    pub schema_version: u32,
    pub tool_version: String,
    pub manifest_sha256: String,
    pub salesforce_snapshot_sha256: String,
    pub raw_denominator: usize,
    pub deterministic_reruns: usize,
    pub counts: EnterpriseCounts,
    pub tests: Vec<EnterpriseTestMeasurement>,
    pub blockers: Vec<EnterpriseBlockerSummary>,
    pub timings: Vec<EnterpriseTiming>,
}

impl EnterpriseReport {
    pub fn write(&self, path: impl AsRef<Path>) -> Result<(), String> {
        let path = path.as_ref();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create enterprise report directory `{}`: {error}",
                    parent.display()
                )
            })?;
        }
        let mut json = serde_json::to_string_pretty(self)
            .map_err(|error| format!("failed to serialize enterprise report: {error}"))?;
        json.push('\n');
        fs::write(path, json).map_err(|error| {
            format!(
                "failed to write enterprise report `{}`: {error}",
                path.display()
            )
        })
    }
}

pub fn run(
    manifest: &EnterpriseManifest,
    capture: &SalesforceCapture,
    options: &EnterpriseRunOptions,
) -> Result<EnterpriseReport, String> {
    if options.reruns != REQUIRED_RERUNS {
        return Err(format!(
            "enterprise compatibility evidence requires exactly {REQUIRED_RERUNS} local reruns"
        ));
    }
    manifest.verify_inputs()?;
    let manifest_sha256 = manifest.sha256()?;
    if capture.manifest_sha256 != manifest_sha256 {
        return Err("Salesforce capture does not match the enterprise manifest".to_owned());
    }

    let inventory = Inventory::load(manifest)?;
    let schema = platform::import_metadata(
        manifest
            .package_roots
            .iter()
            .map(|root| manifest.project_root().join(root)),
    )
    .map_err(|error| format!("enterprise metadata import failed: {error}"));
    let mut compilation_cache =
        BTreeMap::<Vec<PathBuf>, Result<Compilation, EnterpriseBlocker>>::new();
    let mut reference = None;
    let mut timings = Vec::new();
    for index in 0..options.reruns {
        let started = Instant::now();
        let measurements = measure_once(
            manifest,
            capture,
            &inventory,
            schema.as_ref(),
            &mut compilation_cache,
        );
        timings.push(EnterpriseTiming {
            run: index + 1,
            cache_state: if index == 0 { "cold" } else { "warm" }.to_owned(),
            duration_ms: started.elapsed().as_millis(),
        });
        if let Some(expected) = &reference {
            if expected != &measurements {
                return Err(format!(
                    "enterprise local rerun {} produced non-deterministic compatibility results",
                    index + 1
                ));
            }
        } else {
            reference = Some(measurements);
        }
    }
    let tests = reference.expect("three required reruns always produce a reference result");
    let counts = build_counts(&tests);
    let blockers = summarize_blockers(&tests);
    Ok(EnterpriseReport {
        schema_version: ENTERPRISE_REPORT_SCHEMA_VERSION,
        tool_version: env!("CARGO_PKG_VERSION").to_owned(),
        manifest_sha256,
        salesforce_snapshot_sha256: capture.snapshot_sha256.clone(),
        raw_denominator: capture.tests.len(),
        deterministic_reruns: options.reruns,
        counts,
        tests,
        blockers,
        timings,
    })
}

struct Inventory {
    sources: SourceInventory,
    test_classes: BTreeMap<String, PathBuf>,
    dependencies: DependencyIndex,
    parse_failures: BTreeMap<PathBuf, EnterpriseBlocker>,
    implicit_sources: Vec<PathBuf>,
}

impl Inventory {
    fn load(manifest: &EnterpriseManifest) -> Result<Self, String> {
        let (sources, owners) = load_sources(manifest)?;
        let (parse_failures, dependencies) = analyze_sources(&sources, &owners);
        let (test_classes, implicit_sources) = index_test_and_implicit_sources(manifest, &sources);
        Ok(Self {
            sources,
            test_classes,
            dependencies,
            parse_failures,
            implicit_sources,
        })
    }

    fn closure(&self, root: &Path) -> Vec<PathBuf> {
        let mut visited = BTreeSet::new();
        let mut queue = VecDeque::from([root.to_path_buf()]);
        queue.extend(self.implicit_sources.iter().cloned());
        while let Some(path) = queue.pop_front() {
            if !visited.insert(path.clone()) {
                continue;
            }
            if visited.len() > self.sources.len() {
                break;
            }
            if let Some(dependencies) = self.dependencies.get(&path) {
                queue.extend(dependencies.iter().cloned());
            }
        }
        visited.into_iter().collect()
    }
}

fn load_sources(manifest: &EnterpriseManifest) -> Result<(SourceInventory, OwnerIndex), String> {
    let api_version = manifest.candidate.api_version.parse::<ApiVersion>()?;
    let profile = CompatibilityProfile::for_api_version(api_version)?;
    let mut sources = BTreeMap::new();
    let mut owners = BTreeMap::new();
    for input in &manifest.inputs {
        if !is_apex(&input.path) {
            continue;
        }
        let absolute = manifest.project_root().join(&input.path);
        let source = fs::read_to_string(&absolute).map_err(|error| {
            format!(
                "failed to read pinned Apex source `{}`: {error}",
                input.path.display()
            )
        })?;
        let owner = input
            .path
            .file_stem()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                format!(
                    "pinned Apex source `{}` has a non-Unicode name",
                    input.path.display()
                )
            })?
            .to_ascii_lowercase();
        if owners.insert(owner.clone(), input.path.clone()).is_some() {
            return Err(format!(
                "enterprise candidate declares duplicate Apex owner `{owner}`"
            ));
        }
        sources.insert(
            input.path.clone(),
            SourceFile {
                path: absolute,
                source,
                profile,
                profile_origin: ProfileOrigin::ProjectDefault,
            },
        );
    }
    if sources.is_empty() {
        return Err("enterprise candidate contains no Apex source".to_owned());
    }
    Ok((sources, owners))
}

fn analyze_sources(
    sources: &SourceInventory,
    owners: &OwnerIndex,
) -> (BTreeMap<PathBuf, EnterpriseBlocker>, DependencyIndex) {
    let mut parse_failures = BTreeMap::new();
    let mut dependencies = BTreeMap::new();
    for (path, file) in sources {
        let tokens = match crate::tokenize(&file.source) {
            Ok(tokens) => tokens,
            Err(diagnostic) => {
                record_source_failure(&mut parse_failures, path, file, "syntax.lexer", diagnostic);
                dependencies.insert(path.clone(), BTreeSet::new());
                continue;
            }
        };
        if let Err(diagnostic) = crate::parse(&file.source) {
            record_source_failure(&mut parse_failures, path, file, "syntax.parser", diagnostic);
        }
        let references = tokens
            .into_iter()
            .filter_map(|token| match token.kind {
                TokenKind::Identifier(name) => Some(name.to_ascii_lowercase()),
                _ => None,
            })
            .filter_map(|name| owners.get(&name).cloned())
            .filter(|dependency| dependency != path)
            .collect();
        dependencies.insert(path.clone(), references);
    }
    (parse_failures, dependencies)
}

fn record_source_failure(
    failures: &mut BTreeMap<PathBuf, EnterpriseBlocker>,
    path: &Path,
    file: &SourceFile,
    family: &str,
    diagnostic: crate::diagnostic::Diagnostic,
) {
    let (line, column) = source_line_column(&file.source, diagnostic.span.start);
    failures.insert(
        path.to_path_buf(),
        source_blocker(
            "parse",
            family,
            diagnostic.message,
            path.to_path_buf(),
            line,
            column,
        ),
    );
}

fn index_test_and_implicit_sources(
    manifest: &EnterpriseManifest,
    sources: &SourceInventory,
) -> (BTreeMap<String, PathBuf>, Vec<PathBuf>) {
    let mut test_classes = BTreeMap::new();
    let mut implicit_sources = Vec::new();
    for path in sources.keys() {
        if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("trigger"))
        {
            implicit_sources.push(path.clone());
        }
        if manifest
            .test_roots
            .iter()
            .any(|root| path.starts_with(root))
        {
            let class = path
                .file_stem()
                .and_then(|name| name.to_str())
                .expect("source inventory already validated Unicode file names")
                .to_ascii_lowercase();
            test_classes.insert(class, path.clone());
        }
    }
    (test_classes, implicit_sources)
}

fn measure_once(
    manifest: &EnterpriseManifest,
    capture: &SalesforceCapture,
    inventory: &Inventory,
    schema: Result<&platform::SchemaCatalog, &String>,
    cache: &mut BTreeMap<Vec<PathBuf>, Result<Compilation, EnterpriseBlocker>>,
) -> Vec<EnterpriseTestMeasurement> {
    capture
        .tests
        .iter()
        .map(|salesforce| measure_test(manifest, salesforce, inventory, schema, cache))
        .collect()
}

fn measure_test(
    manifest: &EnterpriseManifest,
    salesforce: &super::SalesforceTestOutcome,
    inventory: &Inventory,
    schema: Result<&platform::SchemaCatalog, &String>,
    cache: &mut BTreeMap<Vec<PathBuf>, Result<Compilation, EnterpriseBlocker>>,
) -> EnterpriseTestMeasurement {
    let class_name = salesforce
        .name
        .split_once('.')
        .map_or("", |(class, _)| class)
        .to_ascii_lowercase();
    let Some(test_path) = inventory.test_classes.get(&class_name) else {
        return failed_measurement(
            &salesforce.name,
            &salesforce.outcome,
            Vec::new(),
            false,
            false,
            vec![blocker(
                "discovery",
                "source.test-class-missing",
                "Salesforce test class is absent from the pinned test roots",
                None,
            )],
        );
    };
    let closure = inventory.closure(test_path);
    let parse_failures = closure
        .iter()
        .filter_map(|path| inventory.parse_failures.get(path).cloned())
        .collect::<Vec<_>>();
    if !parse_failures.is_empty() {
        return failed_measurement(
            &salesforce.name,
            &salesforce.outcome,
            closure,
            true,
            false,
            parse_failures,
        );
    }
    let compilation = match compile_test_closure(manifest, inventory, schema, cache, &closure) {
        Ok(compilation) => compilation,
        Err(blockers) => {
            return failed_measurement(
                &salesforce.name,
                &salesforce.outcome,
                closure,
                true,
                true,
                blockers,
            );
        }
    };
    execute_measurement(salesforce, test_path, closure, &compilation)
}

fn compile_test_closure(
    manifest: &EnterpriseManifest,
    inventory: &Inventory,
    schema: Result<&platform::SchemaCatalog, &String>,
    cache: &mut BTreeMap<Vec<PathBuf>, Result<Compilation, EnterpriseBlocker>>,
    closure: &[PathBuf],
) -> Result<Compilation, Vec<EnterpriseBlocker>> {
    let schema = schema
        .map_err(|message| vec![blocker("check", "platform.metadata", message.clone(), None)])?;
    let result = cache.entry(closure.to_vec()).or_insert_with(|| {
        let files = closure
            .iter()
            .map(|path| inventory.sources[path].clone())
            .collect::<Vec<_>>();
        compile_source_subset(manifest.project_root().to_path_buf(), &files, schema).map_err(
            |error| {
                let path = error.path().map(|path| {
                    path.strip_prefix(manifest.project_root())
                        .unwrap_or(path)
                        .to_path_buf()
                });
                blocker("check", "semantic.unsupported", error.to_string(), path)
            },
        )
    });
    result.clone().map_err(|failure| vec![failure])
}

fn execute_measurement(
    salesforce: &super::SalesforceTestOutcome,
    test_path: &Path,
    closure: Vec<PathBuf>,
    compilation: &Compilation,
) -> EnterpriseTestMeasurement {
    let selected = BTreeSet::from([salesforce.name.clone()]);
    let report = test_runner::run_selected(
        compilation,
        &TestOptions {
            filter: None,
            jobs: 1,
        },
        &selected,
    );
    let local_outcome = match report {
        Ok(report) if report.tests.len() == 1 => {
            if report.tests[0].failure.is_none() {
                "pass"
            } else {
                "fail"
            }
        }
        Ok(report) => {
            let detail = format!(
                "Apex Exec selected {} terminal results; expected exactly one",
                report.tests.len()
            );
            return execution_failure(
                salesforce,
                test_path,
                closure,
                "runner.test-not-discovered",
                detail,
            );
        }
        Err(message) => {
            return execution_failure(salesforce, test_path, closure, "runner.error", message);
        }
    };
    completed_measurement(salesforce, test_path, closure, local_outcome)
}

fn execution_failure(
    salesforce: &super::SalesforceTestOutcome,
    test_path: &Path,
    closure: Vec<PathBuf>,
    family: &str,
    detail: impl Into<String>,
) -> EnterpriseTestMeasurement {
    failed_measurement(
        &salesforce.name,
        &salesforce.outcome,
        closure,
        true,
        true,
        vec![blocker(
            "execution",
            family,
            detail,
            Some(test_path.to_path_buf()),
        )],
    )
}

fn completed_measurement(
    salesforce: &super::SalesforceTestOutcome,
    test_path: &Path,
    closure: Vec<PathBuf>,
    local_outcome: &str,
) -> EnterpriseTestMeasurement {
    let outcome_agrees = local_outcome == salesforce.outcome;
    let blockers = (!outcome_agrees)
        .then(|| {
            blocker(
                "agreement",
                "outcome.mismatch",
                format!(
                    "Salesforce outcome `{}` differs from local outcome `{local_outcome}`",
                    salesforce.outcome
                ),
                Some(test_path.to_path_buf()),
            )
        })
        .into_iter()
        .collect();
    EnterpriseTestMeasurement {
        name: salesforce.name.clone(),
        salesforce_outcome: salesforce.outcome.clone(),
        required_source_closure: closure,
        discovered: true,
        parsed: true,
        checked: true,
        executed: true,
        local_outcome: Some(local_outcome.to_owned()),
        outcome_agrees,
        strict_compatible: outcome_agrees,
        blockers,
    }
}

fn failed_measurement(
    name: &str,
    salesforce_outcome: &str,
    closure: Vec<PathBuf>,
    discovered: bool,
    parsed: bool,
    blockers: Vec<EnterpriseBlocker>,
) -> EnterpriseTestMeasurement {
    EnterpriseTestMeasurement {
        name: name.to_owned(),
        salesforce_outcome: salesforce_outcome.to_owned(),
        required_source_closure: closure,
        discovered,
        parsed,
        checked: false,
        executed: false,
        local_outcome: None,
        outcome_agrees: false,
        strict_compatible: false,
        blockers,
    }
}

fn build_counts(tests: &[EnterpriseTestMeasurement]) -> EnterpriseCounts {
    let denominator = tests.len();
    let metric = |count| StageMetric {
        count,
        denominator,
        basis_points: if denominator == 0 {
            0
        } else {
            ((count * 10_000) / denominator) as u32
        },
    };
    let matching_passes = tests
        .iter()
        .filter(|test| test.outcome_agrees && test.local_outcome.as_deref() == Some("pass"))
        .count();
    let matching_failures = tests
        .iter()
        .filter(|test| test.outcome_agrees && test.local_outcome.as_deref() == Some("fail"))
        .count();
    EnterpriseCounts {
        discovery: metric(tests.iter().filter(|test| test.discovered).count()),
        parse: metric(tests.iter().filter(|test| test.parsed).count()),
        check: metric(tests.iter().filter(|test| test.checked).count()),
        execution: metric(tests.iter().filter(|test| test.executed).count()),
        agreement: metric(tests.iter().filter(|test| test.outcome_agrees).count()),
        strict_compatible: metric(tests.iter().filter(|test| test.strict_compatible).count()),
        matching_passes,
        matching_failures,
        outcome_mismatches: tests
            .iter()
            .filter(|test| test.executed && !test.outcome_agrees)
            .count(),
    }
}

fn summarize_blockers(tests: &[EnterpriseTestMeasurement]) -> Vec<EnterpriseBlockerSummary> {
    let mut grouped = BTreeMap::<EnterpriseBlocker, Vec<String>>::new();
    for test in tests {
        for blocker in &test.blockers {
            grouped
                .entry(blocker.clone())
                .or_default()
                .push(test.name.clone());
        }
    }
    let mut summaries = grouped
        .into_iter()
        .map(|(blocker, tests)| EnterpriseBlockerSummary {
            phase: blocker.phase,
            family: blocker.family,
            detail: blocker.detail,
            path: blocker.path,
            line: blocker.line,
            column: blocker.column,
            impacted_tests: tests.len(),
            tests,
        })
        .collect::<Vec<_>>();
    summaries.sort_by(|left, right| {
        right
            .impacted_tests
            .cmp(&left.impacted_tests)
            .then_with(|| left.phase.cmp(&right.phase))
            .then_with(|| left.family.cmp(&right.family))
    });
    summaries
}

fn blocker(
    phase: &str,
    family: &str,
    detail: impl Into<String>,
    path: Option<PathBuf>,
) -> EnterpriseBlocker {
    EnterpriseBlocker {
        phase: phase.to_owned(),
        family: family.to_owned(),
        detail: detail.into(),
        path,
        line: None,
        column: None,
    }
}

fn source_blocker(
    phase: &str,
    family: &str,
    detail: impl Into<String>,
    path: PathBuf,
    line: usize,
    column: usize,
) -> EnterpriseBlocker {
    EnterpriseBlocker {
        phase: phase.to_owned(),
        family: family.to_owned(),
        detail: detail.into(),
        path: Some(path),
        line: Some(line),
        column: Some(column),
    }
}

fn source_line_column(source: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(source.len());
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    let line = source[..offset]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1;
    let column = source[line_start..offset].chars().count() + 1;
    (line, column)
}

fn is_apex(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("cls") || extension.eq_ignore_ascii_case("trigger")
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enterprise::{CandidateIdentity, SalesforceTestOutcome};
    use std::{
        sync::atomic::{AtomicUsize, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    static NEXT_FIXTURE: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn reports_strict_agreement_matching_failures_closures_and_warm_reruns() {
        let root = fixture_project();
        let manifest = fixture_manifest(&root);
        let manifest_sha = manifest.sha256().unwrap();
        let tests = (0..100)
            .map(|index| SalesforceTestOutcome {
                name: format!("EnterpriseTest.case{index:03}"),
                outcome: if index == 98 { "fail" } else { "pass" }.to_owned(),
                message: None,
                stack: None,
            })
            .collect();
        let capture =
            SalesforceCapture::fixture(manifest_sha, vec!["EnterpriseTest".to_owned()], tests);

        let report = run(&manifest, &capture, &EnterpriseRunOptions::default()).unwrap();

        assert_eq!(report.raw_denominator, 100);
        assert_eq!(report.counts.strict_compatible.count, 99);
        assert_eq!(report.counts.matching_passes, 98);
        assert_eq!(report.counts.matching_failures, 1);
        assert_eq!(report.counts.outcome_mismatches, 1);
        assert_eq!(report.tests[0].required_source_closure.len(), 2);
        assert_eq!(report.timings.len(), 3);
        assert_eq!(report.timings[0].cache_state, "cold");
        assert_eq!(report.timings[1].cache_state, "warm");
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_snapshot_drift_and_nonstandard_rerun_counts() {
        let root = fixture_project();
        let manifest = fixture_manifest(&root);
        let mut capture = SalesforceCapture::fixture(
            "0".repeat(64),
            vec!["EnterpriseTest".to_owned()],
            (0..100)
                .map(|index| SalesforceTestOutcome {
                    name: format!("EnterpriseTest.case{index:03}"),
                    outcome: "pass".to_owned(),
                    message: None,
                    stack: None,
                })
                .collect(),
        );
        let error = run(&manifest, &capture, &EnterpriseRunOptions::default()).unwrap_err();
        assert!(error.contains("does not match"));

        capture.manifest_sha256 = manifest.sha256().unwrap();
        let error = run(&manifest, &capture, &EnterpriseRunOptions { reruns: 2 }).unwrap_err();
        assert!(error.contains("exactly 3"));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn keeps_parse_failures_in_the_raw_denominator() {
        let root = fixture_project();
        let classes = root.join("force-app/main/default/classes");
        fs::write(
            classes.join("EnterpriseSupport.cls"),
            "public class EnterpriseSupport { public static Integer value( { return 1; } }",
        )
        .unwrap();
        let manifest = fixture_manifest(&root);
        let capture = SalesforceCapture::fixture(
            manifest.sha256().unwrap(),
            vec!["EnterpriseTest".to_owned()],
            (0..100)
                .map(|index| SalesforceTestOutcome {
                    name: format!("EnterpriseTest.case{index:03}"),
                    outcome: "pass".to_owned(),
                    message: None,
                    stack: None,
                })
                .collect(),
        );

        let report = run(&manifest, &capture, &EnterpriseRunOptions::default()).unwrap();

        assert_eq!(report.raw_denominator, 100);
        assert_eq!(report.counts.parse.count, 0);
        assert_eq!(report.counts.strict_compatible.count, 0);
        assert_eq!(report.blockers[0].phase, "parse");
        assert_eq!(report.blockers[0].impacted_tests, 100);
        assert_eq!(report.tests[0].blockers[0].line, Some(1));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn every_test_closure_includes_package_triggers() {
        let root = fixture_project();
        let classes = root.join("force-app/main/default/classes");
        fs::write(
            classes.join("EnterpriseTrigger.trigger"),
            "trigger EnterpriseTrigger on Account (before insert) {}",
        )
        .unwrap();
        let manifest = fixture_manifest(&root);

        let inventory = Inventory::load(&manifest).unwrap();
        let test_path = inventory.test_classes["enterprisetest"].clone();
        let closure = inventory.closure(&test_path);

        assert!(closure.iter().any(|path| {
            path.file_name()
                .is_some_and(|name| name == "EnterpriseTrigger.trigger")
        }));
        fs::remove_dir_all(root).unwrap();
    }

    fn fixture_manifest(root: &Path) -> EnterpriseManifest {
        EnterpriseManifest::generate(
            root,
            CandidateIdentity {
                name: "Enterprise fixture".to_owned(),
                repository: "https://example.com/enterprise.git".to_owned(),
                git_commit: "a".repeat(40),
                git_tag: "v1.0.0".to_owned(),
                api_version: "65.0".to_owned(),
            },
            vec![PathBuf::from("force-app")],
            vec![PathBuf::from("force-app/main/default/classes")],
        )
        .unwrap()
    }

    fn fixture_project() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "apex-exec-enterprise-runner-{}-{unique}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        let classes = root.join("force-app/main/default/classes");
        fs::create_dir_all(&classes).unwrap();
        fs::write(
            root.join("sfdx-project.json"),
            r#"{"packageDirectories":[{"path":"force-app","default":true}],"sourceApiVersion":"66.0"}"#,
        )
        .unwrap();
        fs::write(
            classes.join("EnterpriseSupport.cls"),
            "public class EnterpriseSupport { public static Integer count = 0; public static Integer value() { count++; return count; } }",
        )
        .unwrap();
        let mut test_class = String::from("@IsTest public class EnterpriseTest {\n");
        for index in 0..100 {
            let expected = if index >= 98 { 2 } else { 1 };
            test_class.push_str(&format!(
                "@IsTest static void case{index:03}() {{ System.assertEquals({expected}, EnterpriseSupport.value()); }}\n"
            ));
        }
        test_class.push_str("}\n");
        fs::write(classes.join("EnterpriseTest.cls"), test_class).unwrap();
        root
    }
}
