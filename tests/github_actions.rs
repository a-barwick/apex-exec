use std::{fs, path::PathBuf, process::Command};

const CI_WORKFLOW: &str = include_str!("../.github/workflows/ci.yml");
const RELEASE_WORKFLOW: &str = include_str!("../.github/workflows/release.yml");
const SETUP_ACTION: &str = include_str!("../.github/actions/setup-rust/action.yml");

#[test]
fn ci_workflow_gates_prs_pushes_and_merge_queues_with_every_confidence_layer() {
    for trigger in [
        "pull_request:",
        "push:",
        "merge_group:",
        "workflow_dispatch:",
        "schedule:",
    ] {
        assert!(
            CI_WORKFLOW.contains(trigger),
            "CI workflow is missing trigger {trigger}"
        );
    }
    for required in [
        "cargo fmt --check",
        "cargo test --locked",
        "cargo clippy --locked --all-targets -- -D warnings",
        "actionlint/cmd/actionlint@v1.7.12",
        "zizmor==1.27.0",
        ".github/scripts/run-apex-regression.sh",
        "cargo test --locked --test north_star lexes_json_parse -- --ignored --exact",
        "examples/${{ matrix.project }}/apex-exec-ci.json",
        "milestone14-project",
        "milestone15-project",
        "--replay",
        "github/codeql-action/upload-sarif@",
        "Required CI gate",
    ] {
        assert!(
            CI_WORKFLOW.contains(required),
            "CI workflow is missing required layer {required}"
        );
    }
    for operating_system in ["ubuntu-latest", "macos-latest", "windows-latest"] {
        assert!(
            CI_WORKFLOW.contains(operating_system),
            "CI workflow does not test {operating_system}"
        );
    }
    assert!(
        !CI_WORKFLOW.contains("pull_request_target"),
        "untrusted pull-request code must not run with target-branch privileges"
    );
    assert!(CI_WORKFLOW.contains("contents: read"));
    assert!(CI_WORKFLOW.contains("cancel-in-progress: true"));
    assert!(
        CI_WORKFLOW.contains(
            "key: apex-results-${{ matrix.project }}-${{ matrix.shard }}-${{ github.sha }}"
        )
    );
    let result_cache = CI_WORKFLOW
        .split("Restore content-addressed Apex results")
        .nth(1)
        .unwrap()
        .split("Run impacted Apex tests and policy gates")
        .next()
        .unwrap();
    assert!(
        !result_cache.contains("restore-keys"),
        "Apex result caches must never restore artifacts from an older commit"
    );
}

#[test]
fn release_workflow_verifies_tags_and_only_publishes_verified_native_binaries() {
    for required in [
        "tags:",
        "\"v*\"",
        "needs: verify",
        "needs: binaries",
        "cargo test --locked",
        "cargo test --locked --test north_star lexes_json_parse -- --ignored --exact",
        ".github/scripts/run-apex-regression.sh",
        "Run and replay hermetic Apex CI gates",
        "examples/milestone15-project/apex-exec-ci.json",
        "ubuntu-latest",
        "macos-latest",
        "windows-latest",
        "SHA256SUMS",
        "gh release upload",
        "contents: write",
    ] {
        assert!(
            RELEASE_WORKFLOW.contains(required),
            "release workflow is missing {required}"
        );
    }
    assert!(RELEASE_WORKFLOW.contains("if: startsWith(github.ref, 'refs/tags/v')"));
    assert!(!RELEASE_WORKFLOW.contains("pull_request"));
}

#[test]
fn shared_setup_pins_rust_and_partitions_caches_by_platform_and_lockfile() {
    assert!(SETUP_ACTION.contains("rustup toolchain install \"${RUST_VERSION}\""));
    assert!(SETUP_ACTION.contains("${{ runner.os }}"));
    assert!(SETUP_ACTION.contains("${{ runner.arch }}"));
    assert!(SETUP_ACTION.contains("${{ hashFiles('Cargo.lock') }}"));
}

#[test]
fn all_remote_actions_are_immutable_commit_pins() {
    for source in [CI_WORKFLOW, RELEASE_WORKFLOW, SETUP_ACTION] {
        for line in source.lines() {
            let Some(reference) = line.trim().strip_prefix("uses: ") else {
                continue;
            };
            let reference = reference.split_whitespace().next().unwrap();
            if reference.starts_with("./") {
                continue;
            }
            let (_, revision) = reference
                .rsplit_once('@')
                .unwrap_or_else(|| panic!("action reference has no revision: {reference}"));
            assert_eq!(
                revision.len(),
                40,
                "action is not pinned to a full commit: {reference}"
            );
            assert!(
                revision.bytes().all(|byte| byte.is_ascii_hexdigit()),
                "action commit is not hexadecimal: {reference}"
            );
        }
    }
}

#[cfg(unix)]
#[test]
fn complex_apex_regression_script_runs_against_the_built_cli() {
    let artifact_dir = temporary_artifact_directory();
    if artifact_dir.exists() {
        fs::remove_dir_all(&artifact_dir).unwrap();
    }
    let result = Command::new("bash")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/.github/scripts/run-apex-regression.sh"
        ))
        .env("APEX_EXEC_BIN", env!("CARGO_BIN_EXE_apex-exec"))
        .env("CI_ARTIFACT_DIR", &artifact_dir)
        .output()
        .unwrap();

    assert!(
        result.status.success(),
        "regression script failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    let stdout = String::from_utf8(result.stdout).unwrap();
    assert!(stdout.contains("Apex regression suite passed 16 end-to-end cases."));
    assert_eq!(
        fs::read_dir(&artifact_dir).unwrap().count(),
        16,
        "each end-to-end case should preserve one diagnostic log"
    );

    fs::remove_dir_all(artifact_dir).unwrap();
}

#[cfg(unix)]
fn temporary_artifact_directory() -> PathBuf {
    std::env::temp_dir().join(format!("apex-exec-github-actions-{}", std::process::id()))
}
