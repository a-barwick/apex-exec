use apex_exec::{
    oracle::{ConformanceManifest, OracleProvider, compare, run_local},
    project,
    test_runner::{TestOptions, run as run_tests},
};
use std::process::Command;

const PROJECT: &str = "examples/milestone16-project";
const MANIFEST: &str = "examples/milestone16-project/oracle-manifest.json";

#[test]
fn milestone16_project_checks_invokes_and_tests_with_full_branch_coverage() {
    let compilation = project::compile(PROJECT).unwrap();
    let output = compilation.invoke("ConditionalTypes.run").unwrap();
    assert_eq!(
        output,
        [
            "primary:String|secondary:Other",
            "primary:String|secondary:Other"
        ]
    );

    let report = run_tests(&compilation, &TestOptions::default()).unwrap();
    assert!(report.is_success(), "{}", report.render_console());
    assert_eq!(report.tests.len(), 1);
    assert_eq!(
        report.coverage.covered_branches,
        report.coverage.total_branches
    );
    assert_eq!(report.coverage.total_branches, 4);
}

#[test]
fn milestone16_oracle_fixture_compares_the_supported_dimensions() {
    let manifest = ConformanceManifest::load(MANIFEST).unwrap();
    let local = run_local(&manifest);
    assert_eq!(local.fixtures.len(), 2);
    assert!(local.fixtures.iter().all(|fixture| fixture.compile.success));
    assert_eq!(
        local.fixtures[0].output,
        [
            "primary:String|secondary:Other",
            "primary:String|secondary:Other"
        ]
    );
    assert_eq!(local.fixtures[1].tests[0].outcome, "pass");

    let mut salesforce_shaped = local.clone();
    salesforce_shaped.provider = OracleProvider::Salesforce;
    salesforce_shaped.target = "recorded-m16-reference".to_owned();
    let report = compare(&manifest, &local, &salesforce_shaped).unwrap();
    assert!(report.is_match());
    assert_eq!(report.coverage.matched, 4);
    assert_eq!(report.coverage.total, 4);
}

#[test]
fn milestone16_cli_runs_the_complete_slice_end_to_end() {
    let binary = env!("CARGO_BIN_EXE_apex-exec");
    for arguments in [
        vec!["check", PROJECT],
        vec!["invoke", PROJECT, "ConditionalTypes.run"],
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
