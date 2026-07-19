use std::process::Command;

#[test]
fn non_debug_cli_run_does_not_render_values_for_discarded_snapshots() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/scenarios/instrumentation_cycle.apex"
    );

    let output = Command::new(env!("CARGO_BIN_EXE_apex-exec"))
        .args(["run", path])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "CLI failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "1\n");
    assert!(output.stderr.is_empty());
}
