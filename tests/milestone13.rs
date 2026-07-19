use apex_exec::oracle::{
    ComparisonScope, ConformanceManifest, DiagnosticCategory, OracleProvider, OracleSnapshot,
    SalesforceCli, compare, run_local,
};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

const MANIFEST: &str = "examples/milestone13-oracle/oracle-manifest.json";
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

fn temporary_directory(label: &str) -> PathBuf {
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "apex-exec-m13-{label}-{}-{sequence}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn local_oracle_observes_every_milestone_dimension() {
    let manifest = ConformanceManifest::load(MANIFEST).unwrap();
    let snapshot = run_local(&manifest);

    assert_eq!(snapshot.provider, OracleProvider::ApexExec);
    assert_eq!(snapshot.fixtures.len(), 3);

    let behavior = &snapshot.fixtures[0];
    assert!(behavior.compile.success);
    assert_eq!(behavior.values["rowCount"], serde_json::json!(1));
    assert_eq!(behavior.values["status"], serde_json::json!("prepared"));
    assert_eq!(behavior.output, ["loaded invoice"]);
    assert_eq!(behavior.queries.len(), 1);
    assert_eq!(behavior.queries[0].kind, "soql");
    assert_eq!(behavior.queries[0].objects, ["Invoice__c"]);
    assert_eq!(behavior.queries[0].rows, 1);
    assert_eq!(behavior.dml.len(), 1);
    assert_eq!(behavior.dml[0].operation, "insert");
    assert_eq!(behavior.dml[0].records, 1);
    assert_eq!(
        behavior
            .triggers
            .iter()
            .map(|event| (event.phase.as_str(), event.stage.as_str()))
            .collect::<Vec<_>>(),
        [
            ("before", "enter"),
            ("before", "exit"),
            ("after", "enter"),
            ("after", "exit"),
        ]
    );

    let exception = snapshot.fixtures[1].exception.as_ref().unwrap();
    assert_eq!(exception.exception_type, "MathException");
    assert_eq!(exception.message, "division by zero");
    assert_eq!(exception.stack[0].method, "fail");
    assert_eq!(exception.stack[0].line, Some(19));

    assert_eq!(snapshot.fixtures[2].tests.len(), 1);
    assert_eq!(
        snapshot.fixtures[2].tests[0].name,
        "OracleDemoTest.confirmsBehavior"
    );
    assert_eq!(snapshot.fixtures[2].tests[0].outcome, "pass");
}

#[test]
fn recorded_snapshot_replay_is_complete_and_detects_regressions() {
    let manifest = ConformanceManifest::load(MANIFEST).unwrap();
    let local = run_local(&manifest);
    let mut recorded = local.clone();
    recorded.provider = OracleProvider::Salesforce;
    recorded.target = "recorded-scratch-org".to_owned();

    let exact = compare(&manifest, &local, &recorded).unwrap();
    assert!(exact.is_match());
    assert_eq!(exact.coverage.matched, 10);
    assert_eq!(exact.coverage.total, 10);
    assert_eq!(exact.coverage.percentage, 100.0);
    assert_eq!(
        exact.coverage.by_scope[&ComparisonScope::Compile].matched,
        3
    );

    recorded.fixtures[0].values.insert(
        "status".to_owned(),
        serde_json::json!("different-on-salesforce"),
    );
    recorded.fixtures[2].tests[0].outcome = "fail".to_owned();
    let regression = compare(&manifest, &local, &recorded).unwrap();
    assert!(!regression.is_match());
    assert_eq!(regression.coverage.matched, 8);
    assert_eq!(regression.coverage.total, 10);
    assert!(
        regression
            .render_console()
            .contains("DIFF  data-and-trigger-behavior Values")
    );
    assert!(
        regression
            .render_console()
            .contains("DIFF  apex-test-outcome Tests")
    );
}

#[test]
fn snapshots_round_trip_and_reject_wrong_schema_or_provider() {
    let manifest = ConformanceManifest::load(MANIFEST).unwrap();
    let local = run_local(&manifest);
    let directory = temporary_directory("snapshot");
    let path = directory.join("snapshot.json");
    local.write(&path).unwrap();
    assert_eq!(OracleSnapshot::load(&path).unwrap(), local);

    let mut wrong_provider = local.clone();
    wrong_provider.provider = OracleProvider::Salesforce;
    assert!(
        compare(&manifest, &wrong_provider, &wrong_provider)
            .unwrap_err()
            .contains("local snapshot provider")
    );

    fs::write(
        &path,
        r#"{"schemaVersion":99,"provider":"salesforce","target":"x","fixtures":[]}"#,
    )
    .unwrap();
    assert!(
        OracleSnapshot::load(&path)
            .unwrap_err()
            .contains("unsupported oracle snapshot schema version")
    );
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn manifest_validation_rejects_unsafe_and_ambiguous_fixtures() {
    let directory = temporary_directory("manifest");
    let project = directory.join("project");
    fs::create_dir_all(&project).unwrap();

    let escaping = directory.join("escaping.json");
    fs::write(
        &escaping,
        r#"{
            "schemaVersion": 1,
            "fixtures": [{
                "name": "escape",
                "project": "../outside",
                "entrypoint": {"kind": "compile"},
                "compare": ["compile"]
            }]
        }"#,
    )
    .unwrap();
    assert!(
        ConformanceManifest::load(&escaping)
            .unwrap_err()
            .contains("cannot escape")
    );

    let duplicate_scope = directory.join("duplicate-scope.json");
    fs::write(
        &duplicate_scope,
        r#"{
            "schemaVersion": 1,
            "fixtures": [{
                "name": "duplicate",
                "project": "project",
                "entrypoint": {"kind": "invoke", "target": "Demo.run"},
                "compare": ["compile", "output", "output"]
            }]
        }"#,
    )
    .unwrap();
    assert!(
        ConformanceManifest::load(&duplicate_scope)
            .unwrap_err()
            .contains("repeats comparison scope")
    );

    let missing_compile = directory.join("missing-compile.json");
    fs::write(
        &missing_compile,
        r#"{
            "schemaVersion": 1,
            "fixtures": [{
                "name": "missing",
                "project": "project",
                "entrypoint": {"kind": "test", "filter": "DemoTest.works"},
                "compare": ["tests"]
            }]
        }"#,
    )
    .unwrap();
    assert!(
        ConformanceManifest::load(&missing_compile)
            .unwrap_err()
            .contains("must always compare compile")
    );
    fs::remove_dir_all(directory).unwrap();
}

#[cfg(unix)]
#[test]
fn salesforce_cli_adapter_normalizes_deploy_invoke_and_test_json() {
    use std::os::unix::fs::PermissionsExt;

    let directory = temporary_directory("sf-cli");
    let executable = directory.join("sf");
    fs::write(
        &executable,
        r#"#!/bin/sh
if [ "$1" = "project" ]; then
  printf '%s\n' '{"status":0,"result":{"success":true,"status":"Succeeded"}}'
elif [ "$3" = "test" ]; then
  printf '%s\n' '{"status":0,"result":{"tests":[{"ApexClass":{"Name":"OracleDemoTest"},"MethodName":"confirmsBehavior","Outcome":"Pass"}]}}'
else
  source=$(cat)
  case "$source" in
    *fail*)
      printf '%s\n' '{"status":1,"result":{"compiled":true,"success":false,"exceptionMessage":"System.MathException: division by zero","exceptionStackTrace":"Class.OracleDemo.fail: line 19, column 1","logs":""}}'
      ;;
    *)
      printf '%s\n' '{"status":0,"result":{"compiled":true,"success":true,"logs":"x|DML_BEGIN|[1]|Op:Insert|Type:Invoice__c|Rows:1\nx|DML_END|[1]\nx|SOQL_EXECUTE_BEGIN|[2]|Aggregations:0|SELECT Id FROM Invoice__c\nx|SOQL_EXECUTE_END|[2]|Rows:1\nx|USER_DEBUG|[3]|DEBUG|APEX_EXEC_ORACLE_VALUE|rowCount|1\nx|USER_DEBUG|[4]|DEBUG|loaded invoice"}}'
      ;;
  esac
fi
"#,
    )
    .unwrap();
    let mut permissions = fs::metadata(&executable).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&executable, permissions).unwrap();

    let manifest = ConformanceManifest::load(MANIFEST).unwrap();
    let snapshot = SalesforceCli::new(&executable)
        .run(&manifest, "scratch")
        .unwrap();
    assert_eq!(snapshot.provider, OracleProvider::Salesforce);
    assert!(
        snapshot
            .fixtures
            .iter()
            .all(|fixture| fixture.compile.success)
    );
    assert_eq!(
        snapshot.fixtures[0].values["rowCount"],
        serde_json::json!(1)
    );
    assert_eq!(snapshot.fixtures[0].queries[0].rows, 1);
    assert_eq!(snapshot.fixtures[0].dml[0].records, 1);
    assert_eq!(
        snapshot.fixtures[1]
            .exception
            .as_ref()
            .unwrap()
            .exception_type,
        "MathException"
    );
    assert_eq!(snapshot.fixtures[2].tests[0].outcome, "pass");
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn oracle_cli_replays_snapshot_writes_report_and_sets_exit_status() {
    let manifest = ConformanceManifest::load(MANIFEST).unwrap();
    let local = run_local(&manifest);
    let mut recorded = local.clone();
    recorded.provider = OracleProvider::Salesforce;
    recorded.target = "recorded".to_owned();
    let directory = temporary_directory("cli");
    let snapshot = directory.join("salesforce.json");
    let report = directory.join("report.json");
    recorded.write(&snapshot).unwrap();

    let success = Command::new(env!("CARGO_BIN_EXE_apex-exec"))
        .args([
            "oracle",
            MANIFEST,
            "--salesforce-snapshot",
            snapshot.to_str().unwrap(),
            "--report",
            report.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(success.status.success());
    assert!(
        String::from_utf8_lossy(&success.stdout)
            .contains("Compatibility coverage: 10/10 dimensions matched (100.00%)")
    );
    assert!(report.is_file());

    recorded.fixtures[0].output.push("org-only".to_owned());
    recorded.write(&snapshot).unwrap();
    let mismatch = Command::new(env!("CARGO_BIN_EXE_apex-exec"))
        .args([
            "oracle",
            MANIFEST,
            "--salesforce-snapshot",
            snapshot.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(!mismatch.status.success());
    assert!(String::from_utf8_lossy(&mismatch.stdout).contains("DIFF"));
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn compile_failures_receive_phase_categories_before_comparison() {
    let directory = temporary_directory("compile-category");
    let project = directory.join("project");
    let classes = project.join("force-app/main/default/classes");
    fs::create_dir_all(&classes).unwrap();
    fs::write(
        project.join("sfdx-project.json"),
        r#"{"packageDirectories":[{"path":"force-app","default":true}],"sourceApiVersion":"66.0"}"#,
    )
    .unwrap();
    fs::write(
        classes.join("Broken.cls"),
        "public class Broken { public static void run( { }",
    )
    .unwrap();
    let manifest_path = directory.join("manifest.json");
    fs::write(
        &manifest_path,
        r#"{
            "schemaVersion": 1,
            "fixtures": [{
                "name": "syntax-error",
                "project": "project",
                "entrypoint": {"kind": "compile"},
                "compare": ["compile"]
            }]
        }"#,
    )
    .unwrap();
    let manifest = ConformanceManifest::load(&manifest_path).unwrap();
    let snapshot = run_local(&manifest);
    assert!(!snapshot.fixtures[0].compile.success);
    assert_eq!(
        snapshot.fixtures[0].compile.diagnostic_category,
        Some(DiagnosticCategory::Syntax)
    );
    fs::remove_dir_all(directory).unwrap();
}
