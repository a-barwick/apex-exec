mod coverage;
mod filtering;
mod report;
#[cfg(test)]
mod tests;

use self::{coverage::build_coverage, filtering::discover_tests};
use crate::{
    project::Compilation,
    runtime::{ExecutionTrace, Interpreter},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
};

#[derive(Clone, Debug)]
pub struct TestOptions {
    pub filter: Option<String>,
    pub jobs: usize,
}

impl Default for TestOptions {
    fn default() -> Self {
        Self {
            filter: None,
            jobs: thread::available_parallelism().map_or(1, usize::from),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TestReport {
    pub tests: Vec<TestResult>,
    pub coverage: CoverageReport,
}

impl TestReport {
    pub fn passed(&self) -> usize {
        self.tests
            .iter()
            .filter(|test| test.failure.is_none())
            .count()
    }

    pub fn failed(&self) -> usize {
        self.tests.len() - self.passed()
    }

    pub fn is_success(&self) -> bool {
        self.failed() == 0
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TestResult {
    pub name: String,
    pub class_name: String,
    pub method_name: String,
    pub output: Vec<String>,
    pub failure: Option<TestFailure>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TestFailure {
    pub exception_type: Option<String>,
    pub message: String,
    pub rendered: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CoverageReport {
    pub files: Vec<FileCoverage>,
    pub total_lines: usize,
    pub covered_lines: usize,
    pub total_branches: usize,
    pub covered_branches: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileCoverage {
    pub path: PathBuf,
    pub total_lines: usize,
    pub covered_lines: usize,
    pub executable_line_numbers: Vec<usize>,
    pub covered_line_numbers: Vec<usize>,
    pub total_branches: usize,
    pub covered_branches: usize,
}

#[derive(Clone, Debug)]
struct ExecutedCase {
    result: TestResult,
    trace: ExecutionTrace,
}

pub fn run(compilation: &Compilation, options: &TestOptions) -> Result<TestReport, String> {
    run_internal(compilation, options, None, false)
}

/// Runs the exact qualified test names supplied by a CI selection pass.
///
/// An empty selection is a valid no-op report: a dependency analysis may
/// correctly determine that an isolated production change impacts no Apex
/// tests.
pub fn run_selected(
    compilation: &Compilation,
    options: &TestOptions,
    selected: &BTreeSet<String>,
) -> Result<TestReport, String> {
    run_internal(compilation, options, Some(selected), true)
}

fn run_internal(
    compilation: &Compilation,
    options: &TestOptions,
    selected: Option<&BTreeSet<String>>,
    allow_empty: bool,
) -> Result<TestReport, String> {
    if options.jobs == 0 {
        return Err("test jobs must be at least 1".to_owned());
    }
    let mut cases = discover_tests(compilation, options.filter.as_deref());
    if let Some(selected) = selected {
        cases.retain(|case| selected.contains(&case.name));
    }
    if cases.is_empty() {
        if allow_empty {
            return Ok(TestReport {
                tests: Vec::new(),
                coverage: build_coverage(compilation, std::iter::empty()),
            });
        }
        return Err(match &options.filter {
            Some(filter) => format!("no Apex tests matched filter `{filter}`"),
            None => "no Apex tests were discovered".to_owned(),
        });
    }

    let next = AtomicUsize::new(0);
    let executed = Mutex::new(Vec::with_capacity(cases.len()));
    let jobs = options.jobs.min(cases.len());
    thread::scope(|scope| {
        for _ in 0..jobs {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(case) = cases.get(index) else {
                        break;
                    };
                    let mut host = crate::runtime::RecordingHost::default();
                    host.set_security_policy(compilation.security.clone());
                    host.set_database_fixtures(compilation.database_fixtures.clone());
                    let execution = Interpreter::with_host(host).run_test(
                        &compilation.program,
                        &case.setup_methods,
                        case.target,
                    );
                    let failure = execution.diagnostic.as_ref().map(|diagnostic| TestFailure {
                        exception_type: diagnostic.exception_type.clone(),
                        message: diagnostic.message.clone(),
                        rendered: compilation.render_diagnostic(diagnostic),
                    });
                    executed
                        .lock()
                        .expect("test result lock poisoned")
                        .push(ExecutedCase {
                            result: TestResult {
                                name: case.name.clone(),
                                class_name: case.class_name.clone(),
                                method_name: case.method_name.clone(),
                                output: execution.output,
                                failure,
                            },
                            trace: execution.trace,
                        });
                }
            });
        }
    });

    let mut executed = executed.into_inner().expect("test result lock poisoned");
    executed.sort_by(|left, right| {
        left.result
            .name
            .to_ascii_lowercase()
            .cmp(&right.result.name.to_ascii_lowercase())
    });
    let coverage = build_coverage(compilation, executed.iter().map(|test| &test.trace));
    Ok(TestReport {
        tests: executed.into_iter().map(|test| test.result).collect(),
        coverage,
    })
}
