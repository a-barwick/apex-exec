use apex_exec::{
    ci::{CiManifest, CiReports, CiRunOptions},
    hybrid::{
        ComponentSelectionMode, DriftKind, HybridRunOptions, OrgInventory, SalesforceValidationCli,
        ValidationSnapshot, ValidationSource, run_with_cli, select_affected_components,
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
const TARGET: &str = "recorded-staging";
const ORG_ID: &str = "00D000000000001";
const READY_DEPLOY: &str = r#"{
  "status": 0,
  "result": {
    "id": "0Af000000000001",
    "success": true,
    "details": {
      "runTestResult": {
        "successes": [
          {"name": "ReleaseServiceTest", "methodName": "preparesPriorityInvoice"},
          {"name": "ReleaseServiceTest", "methodName": "preparesStandardInvoice"}
        ]
      }
    }
  }
}"#;
const FAILED_DEPLOY: &str = r#"{
  "status": 1,
  "result": {
    "id": "0Af000000000002",
    "success": false,
    "details": {
      "componentFailures": [{
        "fullName": "ReleaseService",
        "problem": "controlled compile failure"
      }],
      "runTestResult": {
        "successes": [{
          "name": "ReleaseServiceTest",
          "methodName": "preparesStandardInvoice"
        }],
        "failures": [{
          "name": "ReleaseServiceTest",
          "methodName": "preparesPriorityInvoice",
          "message": "controlled org-only assertion"
        }]
      }
    }
  }
}"#;
const MISSING_TEST_DEPLOY: &str = r#"{
  "status": 0,
  "result": {
    "id": "0Af000000000003",
    "success": true,
    "details": {
      "runTestResult": {
        "successes": [{
          "name": "ReleaseServiceTest",
          "methodName": "preparesPriorityInvoice"
        }]
      }
    }
  }
}"#;

#[cfg(unix)]
#[test]
fn recorded_org_snapshot_produces_a_targeted_release_ready_decision() {
    let fixture = ready_fixture("targeted");
    let outcome = replay(&fixture).unwrap();

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

#[cfg(unix)]
#[test]
fn schema_or_configuration_drift_blocks_release_without_treating_code_as_drift() {
    let fixture = live_fixture("drift", READY_DEPLOY, "drift");
    let outcome = run_with_cli(
        &fixture.manifest,
        &ValidationSource::TargetOrg(TARGET.to_owned()),
        &live_options(&fixture.cache),
        &fixture.cli,
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

#[cfg(unix)]
#[test]
fn local_versus_org_test_mismatch_and_dry_run_failure_are_both_reported() {
    let fixture = live_fixture("mismatch", FAILED_DEPLOY, "clean");
    let outcome = run_with_cli(
        &fixture.manifest,
        &ValidationSource::TargetOrg(TARGET.to_owned()),
        &live_options(&fixture.cache),
        &fixture.cli,
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

#[cfg(unix)]
#[test]
fn validation_snapshots_are_portable_and_strictly_versioned() {
    let fixture = ready_fixture("portable");
    let snapshot = ValidationSnapshot::load(&fixture.snapshot).unwrap();
    assert!(snapshot.authenticated);
    assert_eq!(snapshot.evidence.target, TARGET);
    let mut json = serde_json::to_value(snapshot).unwrap();
    json["schemaVersion"] = serde_json::json!(4);
    fs::write(&fixture.snapshot, serde_json::to_vec_pretty(&json).unwrap()).unwrap();
    assert!(
        ValidationSnapshot::load(&fixture.snapshot)
            .unwrap_err()
            .contains("unsupported validation snapshot schema version")
    );
}

#[cfg(unix)]
#[test]
fn authenticated_adapter_uses_non_secret_auth_repeated_retrieve_and_check_only_deploy() {
    let fixture = live_fixture("sf-cli", READY_DEPLOY, "reformat-second");
    let outcome = run_with_cli(
        &fixture.manifest,
        &ValidationSource::TargetOrg(TARGET.to_owned()),
        &live_options(&fixture.cache),
        &fixture.cli,
    )
    .unwrap();
    assert!(outcome.validation_snapshot.authenticated);
    assert_eq!(outcome.validation_snapshot.evidence.org_id, ORG_ID);
    assert_eq!(outcome.validation_snapshot.evidence.retrieval_count, 2);
    assert_eq!(outcome.validation_snapshot.evidence.api_version, "65.0");
    assert_eq!(
        outcome.validation_snapshot.evidence.salesforce_cli_version,
        "2.30.8"
    );
    assert_eq!(
        outcome.validation_snapshot.evidence.request.changed_paths,
        [PathBuf::from(
            "force-app/main/default/classes/ReleaseService.cls"
        )]
    );
    assert_eq!(
        outcome.validation_snapshot.evidence.request.test_level,
        apex_exec::hybrid::DeploymentTestLevel::RunSpecifiedTests
    );
    assert!(
        outcome
            .validation_snapshot
            .evidence
            .request
            .affected_components
            .iter()
            .all(|component| component.sha256.len() == 64)
    );
    assert!(outcome.validation_snapshot.validation.success);
    assert_eq!(outcome.validation_snapshot.validation.tests.len(), 2);
    let commands = fs::read_to_string(fixture.root.join("commands.log")).unwrap();
    assert!(commands.contains("org display --target-org recorded-staging --json"));
    assert!(!commands.contains("--verbose"));
    assert_eq!(commands.matches("project retrieve start").count(), 2);
    assert!(commands.contains("--api-version 65.0"));
    assert!(commands.contains("--metadata CustomField:Invoice__c.Amount__c"));
    assert!(commands.contains("project deploy start --dry-run"));
    assert!(commands.contains("--test-level RunSpecifiedTests"));
    assert!(commands.contains("--tests ReleaseServiceTest"));
    assert_eq!(commands.matches("--tests ReleaseServiceTest").count(), 1);
    assert!(!commands.contains("--tests ReleaseServiceTest."));

    let snapshot = serde_json::to_string(&outcome.validation_snapshot).unwrap();
    let report = serde_json::to_string(&outcome.report).unwrap();
    for secret in [
        "secret-access-token",
        "https://staging.example.invalid",
        "force://client:secret:refresh-token@example.invalid",
    ] {
        assert!(!snapshot.contains(secret));
        assert!(!report.contains(secret));
    }
}

#[cfg(unix)]
#[test]
fn repeated_retrieval_content_changes_fail_before_deployment() {
    let fixture = live_fixture("unstable-retrieval", READY_DEPLOY, "diverge-second");
    let error = run_with_cli(
        &fixture.manifest,
        &ValidationSource::TargetOrg(TARGET.to_owned()),
        &live_options(&fixture.cache),
        &fixture.cli,
    )
    .unwrap_err();
    assert!(error.contains("did not normalize identically"), "{error}");
    let commands = fs::read_to_string(fixture.root.join("commands.log")).unwrap();
    assert_eq!(commands.matches("project retrieve start").count(), 2);
    assert!(!commands.contains("project deploy start"));
}

#[cfg(unix)]
#[test]
fn hybrid_cli_replays_snapshot_writes_json_and_sets_readiness_exit_status() {
    let fixture = ready_fixture("cli-ready");
    let report_path = fixture.root.join("readiness.json");
    let output = hybrid_cli(&fixture, &report_path);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("RELEASE READY"));
    let report: serde_json::Value =
        serde_json::from_slice(&fs::read(&report_path).unwrap()).unwrap();
    assert_eq!(report["ready"], true);

    let blocked = live_fixture("cli-blocked", MISSING_TEST_DEPLOY, "clean");
    let live = run_with_cli(
        &blocked.manifest,
        &ValidationSource::TargetOrg(TARGET.to_owned()),
        &live_options(&blocked.cache),
        &blocked.cli,
    )
    .unwrap();
    live.validation_snapshot.write(&blocked.snapshot).unwrap();
    let output = hybrid_cli(&blocked, &blocked.root.join("blocked.json"));
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("RELEASE BLOCKED"));
}

#[cfg(unix)]
#[test]
fn tampered_replay_fails_before_writing_a_readiness_report() {
    let fixture = ready_fixture("cli-tamper");
    let mut json: serde_json::Value =
        serde_json::from_slice(&fs::read(&fixture.snapshot).unwrap()).unwrap();
    json["evidence"]["capturedAt"] = serde_json::json!("2026-07-18T00:00:00Z");
    fs::write(&fixture.snapshot, serde_json::to_vec_pretty(&json).unwrap()).unwrap();
    let report = fixture.root.join("must-not-exist.json");
    let output = hybrid_cli(&fixture, &report);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("snapshot digest does not match"));
    assert!(!report.exists());
}

#[cfg(unix)]
struct Fixture {
    root: PathBuf,
    manifest_path: PathBuf,
    manifest: CiManifest,
    cli: SalesforceValidationCli,
    cache: PathBuf,
    snapshot: PathBuf,
}

#[cfg(unix)]
fn ready_fixture(label: &str) -> Fixture {
    let fixture = live_fixture(label, READY_DEPLOY, "clean");
    let outcome = run_with_cli(
        &fixture.manifest,
        &ValidationSource::TargetOrg(TARGET.to_owned()),
        &live_options(&fixture.cache),
        &fixture.cli,
    )
    .unwrap();
    outcome
        .validation_snapshot
        .write(&fixture.snapshot)
        .unwrap();
    fixture
}

#[cfg(unix)]
fn replay(fixture: &Fixture) -> Result<apex_exec::hybrid::HybridRunOutcome, String> {
    run_with_cli(
        &fixture.manifest,
        &ValidationSource::Snapshot(fixture.snapshot.clone()),
        &replay_options(&fixture.cache),
        &fixture.cli,
    )
}

#[cfg(unix)]
fn live_fixture(label: &str, deploy: &str, retrieve_mode: &str) -> Fixture {
    use std::os::unix::fs::PermissionsExt;

    let root = fixture_root(label);
    let executable = root.join("sf");
    fs::write(
        &executable,
        r#"#!/bin/sh
set -eu
dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
printf '%s\n' "$*" >> "$dir/commands.log"
if [ "${1:-}" = "--version" ]; then
  printf '%s\n' '@salesforce/cli/2.30.8 darwin-arm64 node-v20.11.1'
elif [ "$1 $2" = "org display" ]; then
  cat "$dir/auth.json"
elif [ "$1 $2 $3" = "project retrieve start" ]; then
  out=""
  while [ "$#" -gt 0 ]; do
    if [ "$1" = "--output-dir" ]; then
      shift
      out="$1"
    fi
    shift
  done
  case "$out" in
    "$PWD"/.apex-exec/*) ;;
    *)
      printf '%s\n' '{"status":1,"message":"retrieve output was outside the project"}'
      exit 0
      ;;
  esac
  if [ ! -d "$out/main/default" ]; then
    printf '%s\n' '{"status":1,"message":"retrieve output tree was not prepared"}'
    exit 0
  fi
  count=0
  if [ -f "$dir/retrieve-count" ]; then count=$(cat "$dir/retrieve-count"); fi
  count=$((count + 1))
  printf '%s\n' "$count" > "$dir/retrieve-count"
  mkdir -p "$out"
  cp -R force-app "$out/"
  mode=$(cat "$dir/retrieve-mode")
  permission="$out/force-app/main/default/permissionsets/Release_Manager.permissionset-meta.xml"
  if [ "$mode" = "drift" ]; then
    printf '%s\n' '<PermissionSet><label>Controlled Drift</label></PermissionSet>' > "$permission"
  elif [ "$mode" = "reformat-second" ] && [ "$count" -eq 2 ]; then
    tr -d '\n' < "$permission" > "$permission.compact"
    mv "$permission.compact" "$permission"
  elif [ "$mode" = "diverge-second" ] && [ "$count" -eq 2 ]; then
    printf '%s\n' '<PermissionSet><label>Unstable Retrieval</label></PermissionSet>' > "$permission"
  fi
  printf '%s\n' '{"status":0,"result":{"success":true}}'
elif [ "$1 $2 $3" = "project deploy start" ]; then
  cat "$dir/deploy.json"
else
  printf '%s\n' '{"status":1,"message":"unexpected command"}'
fi
"#,
    )
    .unwrap();
    let mut permissions = fs::metadata(&executable).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&executable, permissions).unwrap();
    fs::write(root.join("retrieve-mode"), retrieve_mode).unwrap();
    fs::write(root.join("deploy.json"), deploy).unwrap();
    fs::write(
        root.join("auth.json"),
        format!(
            r#"{{"status":0,"result":{{"orgId":"{ORG_ID}","connectedStatus":"Connected","accessToken":"secret-access-token","instanceUrl":"https://staging.example.invalid","sfdxAuthUrl":"force://client:secret:refresh-token@example.invalid"}}}}"#
        ),
    )
    .unwrap();

    let project = root.join("project");
    copy_tree(Path::new(PROJECT), &project);
    let manifest_path = project.join("apex-exec-ci.json");
    let mut manifest = CiManifest::load(&manifest_path).unwrap();
    manifest.reports = CiReports::default();
    manifest.write(&manifest_path).unwrap();
    Fixture {
        cache: root.join("cache"),
        snapshot: root.join("validation.json"),
        cli: SalesforceValidationCli::new(&executable),
        manifest_path,
        manifest,
        root,
    }
}

#[cfg(unix)]
fn live_options(cache: &Path) -> HybridRunOptions {
    HybridRunOptions {
        ci: CiRunOptions {
            cache_dir: Some(cache.to_owned()),
            ..CiRunOptions::default()
        },
        ..HybridRunOptions::default()
    }
}

#[cfg(unix)]
fn replay_options(cache: &Path) -> HybridRunOptions {
    HybridRunOptions {
        ci: CiRunOptions {
            cache_dir: Some(cache.to_owned()),
            replay_only: true,
            ..CiRunOptions::default()
        },
        expected_target_org: Some(TARGET.to_owned()),
        expected_org_id: Some(ORG_ID.to_owned()),
        ..HybridRunOptions::default()
    }
}

#[cfg(unix)]
fn hybrid_cli(fixture: &Fixture, report: &Path) -> std::process::Output {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut paths = vec![fixture.root.clone()];
    paths.extend(std::env::split_paths(&path));
    Command::new(env!("CARGO_BIN_EXE_apex-exec"))
        .args([
            "hybrid",
            fixture.manifest_path.to_str().unwrap(),
            "--validation-snapshot",
            fixture.snapshot.to_str().unwrap(),
            "--expected-target-org",
            TARGET,
            "--expected-org-id",
            ORG_ID,
            "--replay",
            "--cache-dir",
            fixture.cache.to_str().unwrap(),
            "--report",
            report.to_str().unwrap(),
        ])
        .env("PATH", std::env::join_paths(paths).unwrap())
        .output()
        .unwrap()
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
