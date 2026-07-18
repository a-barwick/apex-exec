use apex_exec::{
    ci::{self, CiManifest, CiReports, CiRunOptions},
    hybrid::{
        DeploymentTestLevel, HybridRunOptions, SalesforceValidationCli, ValidationSource,
        run_with_cli_at,
    },
};
use chrono::{DateTime, Utc};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const PROJECT: &str = "examples/milestone15-project";
const TARGET: &str = "m17-staging";
const ORG_ID: &str = "00D000000000017";
const READY_DEPLOY: &str = r#"{
  "status": 0,
  "result": {
    "id": "0Af000000000017",
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
const BLOCKED_DEPLOY: &str = r#"{
  "status": 1,
  "result": {
    "id": "0Af000000000018",
    "success": false,
    "details": {
      "componentFailures": [{
        "fullName": "ReleaseService",
        "problem": "controlled M17 validation blocker"
      }],
      "runTestResult": {
        "successes": [
          {"name": "ReleaseServiceTest", "methodName": "preparesPriorityInvoice"},
          {"name": "ReleaseServiceTest", "methodName": "preparesStandardInvoice"}
        ]
      }
    }
  }
}"#;

#[cfg(unix)]
#[test]
fn live_evidence_binds_candidate_request_provenance_and_offline_replay() {
    let fixture = Fixture::new("binding", READY_DEPLOY);
    let live = fixture.live(captured_at()).unwrap();
    live.validation_snapshot.write(&fixture.snapshot).unwrap();
    let evidence = &live.validation_snapshot.evidence;

    assert_eq!(
        evidence.candidate.manifest_sha256,
        fixture.manifest.sha256().unwrap()
    );
    assert_eq!(evidence.candidate.ci_cache_key.len(), 64);
    assert_eq!(evidence.candidate.ci_result_sha256.len(), 64);
    assert_eq!(
        evidence.request.changed_paths,
        [PathBuf::from(
            "force-app/main/default/classes/ReleaseService.cls"
        )]
    );
    assert_eq!(
        evidence.request.test_level,
        DeploymentTestLevel::RunSpecifiedTests
    );
    assert_eq!(evidence.request.selected_tests.len(), 2);
    assert!(!evidence.request.affected_components.is_empty());
    assert!(
        evidence
            .request
            .affected_components
            .iter()
            .all(|component| component.sha256.len() == 64)
    );
    assert_eq!(evidence.target, TARGET);
    assert_eq!(evidence.org_id, ORG_ID);
    assert_eq!(evidence.api_version, "65.0");
    assert_eq!(evidence.apex_exec_version, env!("CARGO_PKG_VERSION"));
    assert_eq!(evidence.salesforce_cli_version, "2.30.8");
    assert_eq!(evidence.retrieval_count, 2);
    assert_eq!(evidence.retrieved_inventory_sha256.len(), 64);
    assert_eq!(live.validation_snapshot.snapshot_sha256.len(), 64);

    let replay = fixture
        .replay(replay_at(), replay_options(&fixture.cache))
        .unwrap();
    assert!(
        replay.report.is_ready(),
        "{}",
        replay.report.render_console()
    );
    assert!(replay.report.replayed);
    assert_eq!(
        replay.report.validation_snapshot_sha256,
        live.validation_snapshot.snapshot_sha256
    );
}

#[cfg(unix)]
#[test]
fn replay_rejects_target_org_cli_and_maximum_age_mismatches() {
    let fixture = Fixture::new("identity-mismatch", READY_DEPLOY);
    fixture.record(captured_at());

    let mut options = replay_options(&fixture.cache);
    options.expected_target_org = Some("other-staging".to_owned());
    assert!(
        fixture
            .replay(replay_at(), options)
            .unwrap_err()
            .contains("target mismatch")
    );

    let mut options = replay_options(&fixture.cache);
    options.expected_org_id = Some("00D000000000019".to_owned());
    assert!(
        fixture
            .replay(replay_at(), options)
            .unwrap_err()
            .contains("org ID mismatch")
    );

    fs::write(fixture.root.join("version.txt"), "2.31.0\n").unwrap();
    assert!(
        fixture
            .replay(replay_at(), replay_options(&fixture.cache))
            .unwrap_err()
            .contains("CLI version mismatch")
    );
    fs::write(fixture.root.join("version.txt"), "2.30.8\n").unwrap();

    let mut options = replay_options(&fixture.cache);
    options.maximum_evidence_age = Duration::from_secs(12 * 60 * 60);
    assert!(
        fixture
            .replay(replay_at(), options)
            .unwrap_err()
            .contains("maximum-age mismatch")
    );
}

#[cfg(unix)]
#[test]
fn replay_rejects_expired_and_api_version_mismatched_evidence() {
    let fixture = Fixture::new("expiration", READY_DEPLOY);
    fixture.record(captured_at());
    assert!(
        fixture
            .replay(
                system_time("2026-07-19T12:00:01Z"),
                replay_options(&fixture.cache),
            )
            .unwrap_err()
            .contains("expired")
    );

    let project_config = fixture.project.join("sfdx-project.json");
    let mut json: serde_json::Value =
        serde_json::from_slice(&fs::read(&project_config).unwrap()).unwrap();
    json["sourceApiVersion"] = serde_json::json!("66.0");
    fs::write(&project_config, serde_json::to_vec_pretty(&json).unwrap()).unwrap();
    let mut manifest = CiManifest::generate(&fixture.project).unwrap();
    manifest.changed_files = fixture.manifest.changed_files.clone();
    manifest.reports = CiReports::default();
    assert!(
        run_with_cli_at(
            &manifest,
            &ValidationSource::Snapshot(fixture.snapshot.clone()),
            &replay_options(&fixture.cache),
            &fixture.cli,
            replay_at(),
        )
        .unwrap_err()
        .contains("API version mismatch")
    );
}

#[cfg(unix)]
#[test]
fn replay_rejects_a_different_sealed_manifest_even_with_a_valid_ci_artifact() {
    let fixture = Fixture::new("candidate-mismatch", READY_DEPLOY);
    fixture.record(captured_at());

    let class = fixture
        .project
        .join("force-app/main/default/classes/ReleaseService.cls");
    let mut source = fs::read_to_string(&class).unwrap();
    source.push('\n');
    fs::write(&class, source).unwrap();
    let mut changed = CiManifest::generate(&fixture.project).unwrap();
    changed.changed_files = fixture.manifest.changed_files.clone();
    changed.reports = CiReports::default();
    ci::run(
        &changed,
        &CiRunOptions {
            cache_dir: Some(fixture.cache.clone()),
            ..CiRunOptions::default()
        },
    )
    .unwrap();

    let error = run_with_cli_at(
        &changed,
        &ValidationSource::Snapshot(fixture.snapshot.clone()),
        &replay_options(&fixture.cache),
        &fixture.cli,
        replay_at(),
    )
    .unwrap_err();
    assert!(error.contains("current M14 manifest"), "{error}");
}

#[cfg(unix)]
#[test]
fn controlled_live_deployment_failure_is_sealed_and_replays_as_a_blocker() {
    let fixture = Fixture::new("controlled-blocker", BLOCKED_DEPLOY);
    let live = fixture.live(captured_at()).unwrap();
    assert!(!live.report.is_ready());
    assert!(
        live.report
            .blockers
            .iter()
            .any(|blocker| blocker.contains("check-only deployment"))
    );
    live.validation_snapshot.write(&fixture.snapshot).unwrap();

    let replay = fixture
        .replay(replay_at(), replay_options(&fixture.cache))
        .unwrap();
    assert!(!replay.report.is_ready());
    assert!(replay.report.replayed);
    assert_eq!(replay.report.blockers, live.report.blockers);
}

#[cfg(unix)]
struct Fixture {
    root: PathBuf,
    project: PathBuf,
    manifest: CiManifest,
    cli: SalesforceValidationCli,
    cache: PathBuf,
    snapshot: PathBuf,
}

#[cfg(unix)]
impl Fixture {
    fn new(label: &str, deploy: &str) -> Self {
        use std::os::unix::fs::PermissionsExt;

        let root = fixture_root(label);
        let project = root.join("project");
        copy_tree(Path::new(PROJECT), &project);
        let manifest_path = project.join("apex-exec-ci.json");
        let mut manifest = CiManifest::load(&manifest_path).unwrap();
        manifest.reports = CiReports::default();
        manifest.write(&manifest_path).unwrap();

        let executable = root.join("sf");
        fs::write(
            &executable,
            r#"#!/bin/sh
set -eu
dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
if [ "${1:-}" = "--version" ]; then
  version=$(cat "$dir/version.txt")
  printf '@salesforce/cli/%s darwin-arm64 node-v20.11.1\n' "$version"
elif [ "$1 $2" = "org display" ]; then
  printf '%s\n' '{"status":0,"result":{"orgId":"00D000000000017","connectedStatus":"Connected","accessToken":"must-not-persist"}}'
elif [ "$1 $2 $3" = "project retrieve start" ]; then
  out=""
  while [ "$#" -gt 0 ]; do
    if [ "$1" = "--output-dir" ]; then shift; out="$1"; fi
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
  mkdir -p "$out"
  cp -R force-app "$out/"
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
        fs::write(root.join("version.txt"), "2.30.8\n").unwrap();
        fs::write(root.join("deploy.json"), deploy).unwrap();

        Self {
            cache: root.join("cache"),
            snapshot: root.join("validation.json"),
            cli: SalesforceValidationCli::new(&executable),
            manifest,
            project,
            root,
        }
    }

    fn live(&self, now: SystemTime) -> Result<apex_exec::hybrid::HybridRunOutcome, String> {
        run_with_cli_at(
            &self.manifest,
            &ValidationSource::TargetOrg(TARGET.to_owned()),
            &HybridRunOptions {
                ci: CiRunOptions {
                    cache_dir: Some(self.cache.clone()),
                    ..CiRunOptions::default()
                },
                ..HybridRunOptions::default()
            },
            &self.cli,
            now,
        )
    }

    fn record(&self, now: SystemTime) {
        self.live(now)
            .unwrap()
            .validation_snapshot
            .write(&self.snapshot)
            .unwrap();
    }

    fn replay(
        &self,
        now: SystemTime,
        options: HybridRunOptions,
    ) -> Result<apex_exec::hybrid::HybridRunOutcome, String> {
        run_with_cli_at(
            &self.manifest,
            &ValidationSource::Snapshot(self.snapshot.clone()),
            &options,
            &self.cli,
            now,
        )
    }
}

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

fn captured_at() -> SystemTime {
    system_time("2026-07-18T12:00:00Z")
}

fn replay_at() -> SystemTime {
    system_time("2026-07-18T13:00:00Z")
}

fn system_time(value: &str) -> SystemTime {
    let parsed = DateTime::parse_from_rfc3339(value)
        .unwrap()
        .with_timezone(&Utc);
    parsed.into()
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
        "apex-exec-m17-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&root).unwrap();
    root
}
