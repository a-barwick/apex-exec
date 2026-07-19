use apex_exec::{
    project,
    test_runner::{TestOptions, run as run_tests},
    token::TokenKind,
    tokenize,
};
use std::process::Command;

const PROJECT: &str = "examples/milestone18-project";

#[test]
fn milestone18_lexes_null_aware_operators_with_maximal_munch_and_exact_spans() {
    let source = "String value = item?.name ?? fallback ? 'yes' : 'no';";
    let tokens = tokenize(source).unwrap();

    for (kind, spelling) in [
        (TokenKind::SafeNavigation, "?."),
        (TokenKind::NullCoalesce, "??"),
        (TokenKind::Question, "?"),
    ] {
        let token = tokens.iter().find(|token| token.kind == kind).unwrap();
        assert_eq!(&source[token.span.start..token.span.end], spelling);
    }
}

#[test]
fn milestone18_project_checks_invokes_and_tests_with_full_branch_coverage() {
    let compilation = project::compile(PROJECT).unwrap();
    let output = compilation.invoke("NullAwareProfile.run").unwrap();
    assert_eq!(
        output,
        [
            "GRACE|Ada|SKIPPED|selected-1",
            "2",
            "1",
            "GRACE|Ada|SKIPPED|selected-1"
        ]
    );

    let report = run_tests(&compilation, &TestOptions::default()).unwrap();
    assert!(report.is_success(), "{}", report.render_console());
    assert_eq!(report.tests.len(), 4);
    assert_eq!(report.coverage.covered_lines, report.coverage.total_lines);
    assert_eq!(
        report.coverage.covered_branches,
        report.coverage.total_branches
    );
    assert!(
        report.coverage.total_branches >= 22,
        "null-aware expressions should contribute meaningful branch coverage"
    );
}

#[test]
fn milestone18_cli_runs_the_complete_slice_end_to_end() {
    let binary = env!("CARGO_BIN_EXE_apex-exec");
    for arguments in [
        vec!["check", PROJECT],
        vec!["invoke", PROJECT, "NullAwareProfile.run"],
        vec!["test", PROJECT],
    ] {
        let output = Command::new(binary).args(&arguments).output().unwrap();
        assert!(
            output.status.success(),
            "CLI {:?} failed: {}",
            arguments,
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
