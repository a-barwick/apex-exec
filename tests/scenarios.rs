use apex_exec::{check, execute, parse, tokenize};
use std::process::Command;

fn assert_scenario(path: &str, source: &str, expected_output: &[&str]) {
    let tokens = tokenize(source).unwrap_or_else(|error| panic!("{path} failed lexing: {error}"));
    assert!(tokens.len() > 1, "{path} should contain executable tokens");

    let program = parse(source).unwrap_or_else(|error| panic!("{path} failed parsing: {error}"));
    assert!(
        !program.statements.is_empty(),
        "{path} should contain statements"
    );

    check(source).unwrap_or_else(|error| panic!("{path} failed semantic checking: {error}"));
    let output = execute(source).unwrap_or_else(|error| panic!("{path} failed execution: {error}"));
    assert_eq!(
        output, expected_output,
        "library output differed for {path}"
    );

    let cli = Command::new(env!("CARGO_BIN_EXE_apex-exec"))
        .args(["run", path])
        .output()
        .unwrap_or_else(|error| panic!("failed to launch CLI for {path}: {error}"));
    assert!(
        cli.status.success(),
        "CLI failed for {path}: {}",
        String::from_utf8_lossy(&cli.stderr)
    );
    let expected_stdout = expected_output.join("\n") + "\n";
    assert_eq!(
        String::from_utf8(cli.stdout).unwrap(),
        expected_stdout,
        "CLI output differed for {path}"
    );
    assert!(cli.stderr.is_empty(), "CLI wrote stderr for {path}");
}

#[test]
fn runs_billing_summary_through_every_stage_and_the_cli() {
    assert_scenario(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/scenarios/billing_summary.apex"
        ),
        include_str!("scenarios/billing_summary.apex"),
        &["billableLines=5", "subtotal=2250", "status=priority"],
    );
}

#[test]
fn runs_retry_policy_through_every_stage_and_the_cli() {
    assert_scenario(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/scenarios/retry_policy.apex"
        ),
        include_str!("scenarios/retry_policy.apex"),
        &["attempts=3", "result=connected"],
    );
}

#[test]
fn runs_batch_processing_through_every_stage_and_the_cli() {
    assert_scenario(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/scenarios/batch_processing.apex"
        ),
        include_str!("scenarios/batch_processing.apex"),
        &["batches=3", "processed=9", "result=complete"],
    );
}
