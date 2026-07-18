use apex_exec::{
    debugger::{DebuggerSession, StopReason},
    editor::{CoverageState, coverage_overlays},
    project,
    test_runner::{TestOptions, run as run_tests},
};
use std::{
    io::Write,
    process::{Command, Stdio},
};

#[test]
fn persistent_repl_cli_commits_valid_state_and_rejects_invalid_snippets_transactionally() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_apex-exec"))
        .arg("repl")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(
            b"Integer total = 40;\ntotal = total + 2;\nSystem.debug(total);\ntotal = 'bad';\nSystem.debug(total);\n:quit\n",
        )
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "42\n42\n");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("cannot assign String to Integer"));
}

#[test]
fn debugger_steps_into_calls_and_exposes_source_mapped_frames_and_variables() {
    let source = r#"public class Helper {
    public static Integer twice(Integer value) {
        Integer result = value * 2;
        return result;
    }
}
Integer answer = Helper.twice(21);
System.debug(answer);"#;
    let mut debugger = DebuggerSession::for_script("debug.apex", source).unwrap();
    let breakpoints = debugger.set_breakpoints("debug.apex", &[3]);
    assert!(breakpoints[0].verified);
    assert_eq!(debugger.start(false).reason, StopReason::Breakpoint);
    let frames = debugger.stack_frames();
    assert_eq!(frames[0].name, "twice");
    assert_eq!(frames[1].name, "<anonymous>");
    assert_eq!(debugger.variables()[0].name, "value");
    assert_eq!(debugger.variables()[0].value, "21");
    assert_eq!(debugger.step_out().position.unwrap().line, 8);
    assert_eq!(debugger.continue_execution().reason, StopReason::Complete);
    assert_eq!(debugger.output(), ["42"]);
}

#[test]
fn project_debugging_exposes_database_changes_and_transaction_timeline() {
    let compilation = project::compile("examples/milestone9-project").unwrap();
    let mut debugger = DebuggerSession::for_project(&compilation, "TriggerDemo.run").unwrap();
    let demo = "examples/milestone9-project/force-app/main/default/classes/TriggerDemo.cls";
    assert!(
        debugger
            .set_breakpoints(demo, &[6])
            .first()
            .unwrap()
            .verified
    );
    assert_eq!(debugger.start(false).reason, StopReason::Breakpoint);
    assert_eq!(debugger.inspect_database().visible_transaction_events, 0);
    assert_eq!(debugger.continue_execution().reason, StopReason::Complete);
    let inspection = debugger.inspect_database();
    assert_eq!(inspection.dml_events.len(), 4);
    assert!(inspection.visible_transaction_events > inspection.dml_events.len());
    assert_eq!(debugger.output(), ["Increased", "Restored", "1"]);
}

#[test]
fn coverage_overlays_preserve_every_executable_line_state() {
    let compilation = project::compile("examples/milestone9-project").unwrap();
    let report = run_tests(
        &compilation,
        &TestOptions {
            filter: None,
            jobs: 2,
        },
    )
    .unwrap();
    let overlays = coverage_overlays(&report);
    assert!(!overlays.is_empty());
    assert_eq!(
        overlays
            .iter()
            .flat_map(|overlay| &overlay.lines)
            .filter(|(_, state)| *state == CoverageState::Uncovered)
            .count(),
        0
    );
    assert_eq!(
        overlays.iter().flat_map(|overlay| &overlay.lines).count(),
        report.coverage.total_lines
    );
}
