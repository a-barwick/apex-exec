use crate::{
    ast::{ClassMember, Statement},
    hir::ClassMemberId,
    project::Compilation,
    runtime::{BranchHits, ExecutionTrace, Interpreter},
    span::Span,
};
use std::{
    collections::{BTreeMap, BTreeSet},
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

#[derive(Clone, Debug)]
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

    pub fn render_console(&self) -> String {
        let mut lines = Vec::new();
        for test in &self.tests {
            if let Some(failure) = &test.failure {
                lines.push(format!("FAIL {}: {}", test.name, failure.summary()));
                for line in failure.rendered.lines() {
                    lines.push(format!("  {line}"));
                }
            } else {
                lines.push(format!("PASS {}", test.name));
            }
            for output in &test.output {
                lines.push(format!("  debug: {output}"));
            }
        }
        lines.push(String::new());
        lines.push("Coverage:".to_owned());
        if self.coverage.files.is_empty() {
            lines.push("  no executable production lines".to_owned());
        } else {
            for file in &self.coverage.files {
                lines.push(format!(
                    "  {}: {}/{} lines ({:.2}%), {}/{} branches ({:.2}%)",
                    file.path.display(),
                    file.covered_lines,
                    file.total_lines,
                    percentage(file.covered_lines, file.total_lines),
                    file.covered_branches,
                    file.total_branches,
                    percentage(file.covered_branches, file.total_branches),
                ));
            }
        }
        lines.push(format!(
            "Summary: {} passed, {} failed, {} total; {}/{} lines ({:.2}%), {}/{} branches ({:.2}%)",
            self.passed(),
            self.failed(),
            self.tests.len(),
            self.coverage.covered_lines,
            self.coverage.total_lines,
            percentage(self.coverage.covered_lines, self.coverage.total_lines),
            self.coverage.covered_branches,
            self.coverage.total_branches,
            percentage(
                self.coverage.covered_branches,
                self.coverage.total_branches
            ),
        ));
        lines.join("\n")
    }

    pub fn to_junit_xml(&self) -> String {
        let mut xml = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<testsuite name=\"apex-exec\" tests=\"{}\" failures=\"{}\" time=\"0\">\n",
            self.tests.len(),
            self.failed()
        );
        xml.push_str("  <properties>\n");
        xml.push_str(&format!(
            "    <property name=\"line-rate\" value=\"{:.6}\"/>\n",
            rate(self.coverage.covered_lines, self.coverage.total_lines)
        ));
        xml.push_str(&format!(
            "    <property name=\"branch-rate\" value=\"{:.6}\"/>\n",
            rate(self.coverage.covered_branches, self.coverage.total_branches)
        ));
        xml.push_str("  </properties>\n");
        for test in &self.tests {
            xml.push_str(&format!(
                "  <testcase classname=\"{}\" name=\"{}\" time=\"0\">\n",
                xml_escape(&test.class_name),
                xml_escape(&test.method_name)
            ));
            if let Some(failure) = &test.failure {
                xml.push_str(&format!(
                    "    <failure type=\"{}\" message=\"{}\">{}</failure>\n",
                    xml_escape(failure.exception_type.as_deref().unwrap_or("RuntimeError")),
                    xml_escape(&failure.message),
                    xml_escape(&failure.rendered)
                ));
            }
            if !test.output.is_empty() {
                xml.push_str(&format!(
                    "    <system-out>{}</system-out>\n",
                    xml_escape(&test.output.join("\n"))
                ));
            }
            xml.push_str("  </testcase>\n");
        }
        xml.push_str("</testsuite>\n");
        xml
    }
}

#[derive(Clone, Debug)]
pub struct TestResult {
    pub name: String,
    pub class_name: String,
    pub method_name: String,
    pub output: Vec<String>,
    pub failure: Option<TestFailure>,
}

#[derive(Clone, Debug)]
pub struct TestFailure {
    pub exception_type: Option<String>,
    pub message: String,
    pub rendered: String,
}

impl TestFailure {
    fn summary(&self) -> String {
        self.exception_type.as_ref().map_or_else(
            || self.message.clone(),
            |ty| format!("{ty}: {}", self.message),
        )
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CoverageReport {
    pub files: Vec<FileCoverage>,
    pub total_lines: usize,
    pub covered_lines: usize,
    pub total_branches: usize,
    pub covered_branches: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileCoverage {
    pub path: PathBuf,
    pub total_lines: usize,
    pub covered_lines: usize,
    pub total_branches: usize,
    pub covered_branches: usize,
}

#[derive(Clone, Debug)]
struct TestCase {
    name: String,
    class_name: String,
    method_name: String,
    target: ClassMemberId,
    setup_methods: Vec<ClassMemberId>,
}

#[derive(Clone, Debug)]
struct ExecutedCase {
    result: TestResult,
    trace: ExecutionTrace,
}

pub fn run(compilation: &Compilation, options: &TestOptions) -> Result<TestReport, String> {
    if options.jobs == 0 {
        return Err("test jobs must be at least 1".to_owned());
    }
    let cases = discover_tests(compilation, options.filter.as_deref());
    if cases.is_empty() {
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
                    let execution = Interpreter::new().run_test(
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

fn discover_tests(compilation: &Compilation, filter: Option<&str>) -> Vec<TestCase> {
    let mut cases = Vec::new();
    for (class_id, class) in compilation.program.classes.iter().enumerate() {
        if !class
            .annotations
            .iter()
            .any(|annotation| annotation.kind.is_test())
        {
            continue;
        }
        let setup_methods = class
            .members
            .iter()
            .enumerate()
            .filter_map(|(member_id, member)| match member {
                ClassMember::Method(method)
                    if method
                        .annotations
                        .iter()
                        .any(|annotation| annotation.kind.is_test_setup()) =>
                {
                    Some(ClassMemberId {
                        class_id,
                        member_id,
                    })
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        for (member_id, member) in class.members.iter().enumerate() {
            let ClassMember::Method(method) = member else {
                continue;
            };
            if !method
                .annotations
                .iter()
                .any(|annotation| annotation.kind.is_test())
            {
                continue;
            }
            let name = format!("{}.{}", class.name.spelling, method.name.spelling);
            if !matches_filter(filter, &class.name.spelling, &method.name.spelling, &name) {
                continue;
            }
            cases.push(TestCase {
                name,
                class_name: class.name.spelling.clone(),
                method_name: method.name.spelling.clone(),
                target: ClassMemberId {
                    class_id,
                    member_id,
                },
                setup_methods: setup_methods.clone(),
            });
        }
    }
    cases.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
    });
    cases
}

fn matches_filter(filter: Option<&str>, class: &str, method: &str, full_name: &str) -> bool {
    let Some(filter) = filter else {
        return true;
    };
    let filter = filter.to_ascii_lowercase();
    let class = class.to_ascii_lowercase();
    let method = method.to_ascii_lowercase();
    let full_name = full_name.to_ascii_lowercase();
    if filter.contains('*') {
        wildcard_matches(&filter, &full_name)
    } else if filter.contains('.') {
        filter == full_name
    } else {
        filter == class || filter == method
    }
}

fn wildcard_matches(pattern: &str, value: &str) -> bool {
    let parts = pattern.split('*').collect::<Vec<_>>();
    let mut cursor = 0usize;
    for (index, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        let Some(found) = value[cursor..].find(part) else {
            return false;
        };
        if index == 0 && !pattern.starts_with('*') && found != 0 {
            return false;
        }
        cursor += found + part.len();
    }
    pattern.ends_with('*')
        || parts
            .last()
            .is_some_and(|part| value[cursor..].ends_with(part))
}

fn build_coverage<'a>(
    compilation: &Compilation,
    traces: impl Iterator<Item = &'a ExecutionTrace>,
) -> CoverageReport {
    let mut statement_spans = BTreeSet::new();
    let mut branch_spans = BTreeSet::new();
    collect_coverage_candidates(compilation, &mut statement_spans, &mut branch_spans);

    let mut executed_statements = BTreeSet::<Span>::new();
    let mut branches = BTreeMap::<Span, BranchHits>::new();
    for trace in traces {
        executed_statements.extend(trace.executed_statements.iter().copied());
        for (span, hits) in &trace.branches {
            let aggregate = branches.entry(*span).or_default();
            aggregate.true_hits += hits.true_hits;
            aggregate.false_hits += hits.false_hits;
        }
    }

    #[derive(Default)]
    struct FileBuilder {
        total_lines: BTreeSet<usize>,
        covered_lines: BTreeSet<usize>,
        total_branches: usize,
        covered_branches: usize,
    }

    let mut files = BTreeMap::<PathBuf, FileBuilder>::new();
    for span in statement_spans {
        let Some((path, line)) = compilation.source_location(span) else {
            continue;
        };
        let path = relative_path(compilation, path);
        let file = files.entry(path).or_default();
        file.total_lines.insert(line);
        if executed_statements.contains(&span) {
            file.covered_lines.insert(line);
        }
    }
    for span in branch_spans {
        let Some((path, _)) = compilation.source_location(span) else {
            continue;
        };
        let path = relative_path(compilation, path);
        let file = files.entry(path).or_default();
        file.total_branches += 2;
        let hits = branches.get(&span).copied().unwrap_or_default();
        file.covered_branches += usize::from(hits.true_hits > 0) + usize::from(hits.false_hits > 0);
    }

    let files = files
        .into_iter()
        .map(|(path, file)| FileCoverage {
            path,
            total_lines: file.total_lines.len(),
            covered_lines: file.covered_lines.len(),
            total_branches: file.total_branches,
            covered_branches: file.covered_branches,
        })
        .collect::<Vec<_>>();
    CoverageReport {
        total_lines: files.iter().map(|file| file.total_lines).sum(),
        covered_lines: files.iter().map(|file| file.covered_lines).sum(),
        total_branches: files.iter().map(|file| file.total_branches).sum(),
        covered_branches: files.iter().map(|file| file.covered_branches).sum(),
        files,
    }
}

fn relative_path(compilation: &Compilation, path: PathBuf) -> PathBuf {
    path.strip_prefix(&compilation.root)
        .map_or_else(|_| path.clone(), PathBuf::from)
}

fn collect_coverage_candidates(
    compilation: &Compilation,
    statements: &mut BTreeSet<Span>,
    branches: &mut BTreeSet<Span>,
) {
    for class in &compilation.program.classes {
        if class
            .annotations
            .iter()
            .any(|annotation| annotation.kind.is_test())
        {
            continue;
        }
        for member in &class.members {
            match member {
                ClassMember::Constructor(constructor) => {
                    visit_statement(&constructor.body, statements, branches)
                }
                ClassMember::Method(method) => {
                    if let Some(body) = &method.body {
                        visit_statement(body, statements, branches);
                    }
                }
                ClassMember::Property(property) => {
                    for accessor in &property.accessors {
                        if let Some(body) = &accessor.body {
                            visit_statement(body, statements, branches);
                        }
                    }
                }
                ClassMember::Field(_) => {}
            }
        }
    }
}

fn visit_statement(
    statement: &Statement,
    statements: &mut BTreeSet<Span>,
    branches: &mut BTreeSet<Span>,
) {
    if !matches!(statement, Statement::Block { .. }) {
        statements.insert(statement.span());
    }
    match statement {
        Statement::Block {
            statements: body, ..
        } => {
            for statement in body {
                visit_statement(statement, statements, branches);
            }
        }
        Statement::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            branches.insert(condition.span());
            visit_statement(then_branch, statements, branches);
            if let Some(else_branch) = else_branch {
                visit_statement(else_branch, statements, branches);
            }
        }
        Statement::While {
            condition, body, ..
        }
        | Statement::DoWhile {
            condition, body, ..
        } => {
            branches.insert(condition.span());
            visit_statement(body, statements, branches);
        }
        Statement::For {
            initializer,
            condition,
            update,
            body,
            ..
        } => {
            if let Some(initializer) = initializer {
                visit_statement(initializer, statements, branches);
            }
            if let Some(condition) = condition {
                branches.insert(condition.span());
            }
            if let Some(update) = update {
                visit_statement(update, statements, branches);
            }
            visit_statement(body, statements, branches);
        }
        Statement::ForEach { body, .. } => visit_statement(body, statements, branches),
        Statement::Try {
            try_block,
            catches,
            finally_block,
            ..
        } => {
            visit_statement(try_block, statements, branches);
            for catch in catches {
                visit_statement(&catch.body, statements, branches);
            }
            if let Some(finally_block) = finally_block {
                visit_statement(finally_block, statements, branches);
            }
        }
        Statement::VariableDeclaration { .. }
        | Statement::Expression { .. }
        | Statement::Break { .. }
        | Statement::Continue { .. }
        | Statement::Throw { .. }
        | Statement::Return { .. } => {}
    }
}

fn percentage(covered: usize, total: usize) -> f64 {
    rate(covered, total) * 100.0
}

fn rate(covered: usize, total: usize) -> f64 {
    if total == 0 {
        1.0
    } else {
        covered as f64 / total as f64
    }
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcard_filters_are_case_insensitive_and_anchored() {
        assert!(wildcard_matches(
            "account*test.*merge*",
            "accountservicetest.shouldmerge"
        ));
        assert!(!wildcard_matches("service*", "accountservice"));
        assert!(matches_filter(
            Some("ACCOUNTSERVICETEST"),
            "AccountServiceTest",
            "works",
            "AccountServiceTest.works"
        ));
    }

    #[test]
    fn escapes_junit_xml_text() {
        assert_eq!(xml_escape("a<&\"'"), "a&lt;&amp;&quot;&apos;");
    }
}
