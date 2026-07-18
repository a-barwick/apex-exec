use apex_exec::{
    ci::{CiManifest, CiReports},
    hybrid::{
        ComponentSelectionMode, DeploymentValidation, DriftKind, HYBRID_SCHEMA_VERSION,
        HybridRunOptions, OrgInventory, SalesforceValidationCli, ValidationSnapshot,
        ValidationSource, ValidationTest, run, select_affected_components,
    },
    project,
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

const PROJECT: &str = "examples/milestone15-project";
const MANIFEST: &str = "examples/milestone15-project/apex-exec-ci.json";

#[test]
fn recorded_org_snapshot_produces_a_targeted_release_ready_decision() {
    let (mut manifest, snapshot_path) = ready_fixture("targeted");
    manifest.reports = CiReports::default();
    let outcome = run(
        &manifest,
        &ValidationSource::Snapshot(snapshot_path),
        &no_cache(),
    )
    .unwrap();

    assert!(
        outcome.report.is_ready(),
        "{}",
        outcome.report.render_console()
    );
    assert_eq!(
        outcome.report.affected_components.mode,
        ComponentSelectionMode::Impacted
    );
    assert_eq!(
        outcome.report.affected_tests,
        [
            "ReleaseServiceTest.preparesPriorityInvoice",
            "ReleaseServiceTest.preparesStandardInvoice"
        ]
    );
    assert!(
        outcome
            .report
            .affected_components
            .components
            .iter()
            .any(|component| component.selector() == "ApexClass:ReleaseService")
    );
    assert!(
        outcome
            .report
            .affected_components
            .components
            .iter()
            .all(|component| component.full_name != "AuditService")
    );
    assert_eq!(outcome.report.local.passed_tests, 2);
    assert_eq!(outcome.report.local.line_coverage, 87.5);
    assert_eq!(outcome.report.local.branch_coverage, 100.0);
    assert_eq!(outcome.report.differential_percentage, 100.0);
    assert!(outcome.report.drift.is_empty());
    assert!(outcome.report.blockers.is_empty());
    assert!(outcome.report.render_console().contains("RELEASE READY"));
}

#[test]
fn schema_or_configuration_drift_blocks_release_without_treating_code_as_drift() {
    let (mut manifest, snapshot_path) = ready_fixture("drift");
    manifest.reports = CiReports::default();
    let mut snapshot = ValidationSnapshot::load(&snapshot_path).unwrap();
    let permission = snapshot
        .inventory
        .components
        .iter_mut()
        .find(|component| component.metadata_type == "PermissionSet")
        .unwrap();
    permission.sha256 = "f".repeat(64);
    let class = snapshot
        .inventory
        .components
        .iter_mut()
        .find(|component| component.selector() == "ApexClass:AuditService")
        .unwrap();
    class.sha256 = "e".repeat(64);
    snapshot.write(&snapshot_path).unwrap();

    let outcome = run(
        &manifest,
        &ValidationSource::Snapshot(snapshot_path),
        &no_cache(),
    )
    .unwrap();
    assert!(!outcome.report.is_ready());
    assert_eq!(outcome.report.drift.len(), 1);
    assert_eq!(
        outcome.report.drift[0].component,
        "PermissionSet:Release_Manager"
    );
    assert_eq!(outcome.report.drift[0].kind, DriftKind::ContentMismatch);
    assert!(
        outcome
            .report
            .blockers
            .iter()
            .any(|blocker| blocker.contains("schema/configuration"))
    );
}

#[test]
fn local_versus_org_test_mismatch_and_dry_run_failure_are_both_reported() {
    let (mut manifest, snapshot_path) = ready_fixture("mismatch");
    manifest.reports = CiReports::default();
    let mut snapshot = ValidationSnapshot::load(&snapshot_path).unwrap();
    snapshot.validation.success = false;
    snapshot.validation.component_failures =
        vec!["ApexClass:ReleaseService: compile failure".to_owned()];
    snapshot.validation.tests[0].outcome = "fail".to_owned();
    snapshot.validation.tests[0].message = Some("org-only assertion".to_owned());
    snapshot.write(&snapshot_path).unwrap();

    let outcome = run(
        &manifest,
        &ValidationSource::Snapshot(snapshot_path),
        &no_cache(),
    )
    .unwrap();
    assert!(!outcome.report.is_ready());
    assert_eq!(outcome.report.differential_percentage, 50.0);
    assert_eq!(
        outcome
            .report
            .test_differential
            .iter()
            .filter(|result| !result.matched)
            .count(),
        1
    );
    assert!(
        outcome
            .report
            .blockers
            .iter()
            .any(|blocker| blocker.contains("check-only deployment"))
    );
    assert!(
        outcome
            .report
            .blockers
            .iter()
            .any(|blocker| blocker.contains("test outcomes differ"))
    );
}

#[test]
fn metadata_changes_conservatively_select_every_deployable_component() {
    let mut manifest = CiManifest::load(MANIFEST).unwrap();
    manifest.changed_files = vec![PathBuf::from(
        "force-app/main/default/objects/Invoice__c/fields/Amount__c.field-meta.xml",
    )];
    let compilation = project::compile(PROJECT).unwrap();
    let inventory = OrgInventory::capture(PROJECT, "local").unwrap();
    let selection = select_affected_components(&manifest, &compilation, &inventory);
    assert_eq!(selection.mode, ComponentSelectionMode::ConservativeAll);
    assert_eq!(selection.components, inventory.components);
    assert_eq!(
        selection.directly_changed,
        ["CustomField:Invoice__c.Amount__c"]
    );
}

#[test]
fn validation_snapshots_are_portable_and_strictly_versioned() {
    let (_, snapshot_path) = ready_fixture("portable");
    let snapshot = ValidationSnapshot::load(&snapshot_path).unwrap();
    assert!(!snapshot.authenticated);
    assert_eq!(snapshot.target, "recorded-staging");
    let mut json = serde_json::to_value(snapshot).unwrap();
    json["schemaVersion"] = serde_json::json!(HYBRID_SCHEMA_VERSION + 1);
    fs::write(&snapshot_path, serde_json::to_vec_pretty(&json).unwrap()).unwrap();
    assert!(
        ValidationSnapshot::load(&snapshot_path)
            .unwrap_err()
            .contains("unsupported validation snapshot schema version")
    );
}

#[cfg(unix)]
#[test]
fn authenticated_adapter_uses_non_secret_auth_retrieve_and_check_only_deploy() {
    use std::os::unix::fs::PermissionsExt;

    let root = fixture_root("sf-cli");
    let executable = root.join("sf");
    let log = root.join("commands.log");
    fs::write(
        &executable,
        r#"#!/bin/sh
set -eu
dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
printf '%s\n' "$*" >> "$dir/commands.log"
if [ "$1 $2" = "org display" ]; then
  printf '%s\n' '{"status":0,"result":{"id":"00D000000000001","connectedStatus":"Connected"}}'
elif [ "$1 $2 $3" = "project retrieve start" ]; then
  out=""
  while [ "$#" -gt 0 ]; do
    if [ "$1" = "--output-dir" ]; then
      shift
      out="$1"
    fi
    shift
  done
  mkdir -p "$out"
  cp -R force-app "$out/"
  printf '%s\n' '{"status":0,"result":{"success":true}}'
elif [ "$1 $2 $3" = "project deploy start" ]; then
  printf '%s\n' '{"status":0,"result":{"id":"0Af000000000001","success":true,"details":{"runTestResult":{"successes":[{"name":"ReleaseServiceTest","methodName":"preparesPriorityInvoice"},{"name":"ReleaseServiceTest","methodName":"preparesStandardInvoice"}]}}}}'
else
  printf '%s\n' '{"status":1,"message":"unexpected command"}'
fi
"#,
    )
    .unwrap();
    let mut permissions = fs::metadata(&executable).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&executable, permissions).unwrap();

    let manifest = CiManifest::load(MANIFEST).unwrap();
    let compilation = project::compile(PROJECT).unwrap();
    let inventory = OrgInventory::capture(PROJECT, "local").unwrap();
    let affected = select_affected_components(&manifest, &compilation, &inventory);
    let tests = vec![
        "ReleaseServiceTest.preparesPriorityInvoice".to_owned(),
        "ReleaseServiceTest.preparesStandardInvoice".to_owned(),
    ];
    let snapshot = SalesforceValidationCli::new(&executable)
        .validate(Path::new(PROJECT), "staging", &inventory, &affected, &tests)
        .unwrap();
    assert!(snapshot.authenticated);
    assert_eq!(snapshot.org_id.as_deref(), Some("00D000000000001"));
    assert!(snapshot.validation.success);
    assert_eq!(snapshot.validation.tests.len(), 2);
    let commands = fs::read_to_string(log).unwrap();
    assert!(commands.contains("org display --target-org staging --json"));
    assert!(!commands.contains("--verbose"));
    assert!(commands.contains("project retrieve start"));
    assert!(commands.contains("--metadata CustomField:Invoice__c.Amount__c"));
    assert!(commands.contains("project deploy start --dry-run"));
    assert!(commands.contains("--test-level RunSpecifiedTests"));
    assert!(commands.contains("--tests ReleaseServiceTest.preparesPriorityInvoice"));
}

#[test]
fn hybrid_cli_replays_snapshot_writes_json_and_sets_readiness_exit_status() {
    let root = fixture_root("cli");
    copy_tree(Path::new(PROJECT), &root);
    let manifest_path = root.join("apex-exec-ci.json");
    let mut manifest = CiManifest::generate(&root).unwrap();
    manifest.changed_files = vec![PathBuf::from(
        "force-app/main/default/classes/ReleaseService.cls",
    )];
    manifest.reports = CiReports::default();
    manifest.write(&manifest_path).unwrap();
    let inventory = OrgInventory::capture(&root, "recorded-staging").unwrap();
    let snapshot_path = root.join("validation.json");
    ready_snapshot(inventory).write(&snapshot_path).unwrap();
    let report_path = root.join("readiness.json");

    let output = Command::new(env!("CARGO_BIN_EXE_apex-exec"))
        .args([
            "hybrid",
            manifest_path.to_str().unwrap(),
            "--validation-snapshot",
            snapshot_path.to_str().unwrap(),
            "--report",
            report_path.to_str().unwrap(),
            "--no-cache",
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("RELEASE READY"));
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(&report_path).unwrap()).unwrap();
    assert_eq!(report["ready"], true);

    let mut snapshot = ValidationSnapshot::load(&snapshot_path).unwrap();
    snapshot.validation.tests.pop();
    snapshot.write(&snapshot_path).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_apex-exec"))
        .args([
            "hybrid",
            manifest_path.to_str().unwrap(),
            "--validation-snapshot",
            snapshot_path.to_str().unwrap(),
            "--no-cache",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("RELEASE BLOCKED"));
}

fn ready_fixture(label: &str) -> (CiManifest, PathBuf) {
    let root = fixture_root(label);
    let inventory = OrgInventory::capture(PROJECT, "recorded-staging").unwrap();
    let path = root.join("validation.json");
    ready_snapshot(inventory).write(&path).unwrap();
    (CiManifest::load(MANIFEST).unwrap(), path)
}

fn ready_snapshot(inventory: OrgInventory) -> ValidationSnapshot {
    ValidationSnapshot {
        schema_version: HYBRID_SCHEMA_VERSION,
        target: inventory.target.clone(),
        authenticated: false,
        org_id: Some("00D-recorded".to_owned()),
        inventory,
        validation: DeploymentValidation {
            success: true,
            deployment_id: Some("0Af-recorded".to_owned()),
            component_failures: Vec::new(),
            tests: vec![
                ValidationTest {
                    name: "ReleaseServiceTest.preparesPriorityInvoice".to_owned(),
                    outcome: "pass".to_owned(),
                    message: None,
                },
                ValidationTest {
                    name: "ReleaseServiceTest.preparesStandardInvoice".to_owned(),
                    outcome: "pass".to_owned(),
                    message: None,
                },
            ],
        },
    }
}

fn no_cache() -> HybridRunOptions {
    HybridRunOptions {
        ci: apex_exec::ci::CiRunOptions {
            no_cache: true,
            ..apex_exec::ci::CiRunOptions::default()
        },
    }
}

fn copy_tree(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn fixture_root(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "apex-exec-m15-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    root
}
