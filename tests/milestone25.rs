use apex_exec::{
    ci::{CiManifest, CiRunOptions},
    compatibility::ProfileOrigin,
    oracle::{self, ConformanceManifest},
    project::{self, ProjectCompiler},
    test_runner::{self, TestOptions},
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const PROJECT: &str = "examples/milestone25-project";
const ORACLE_MANIFEST: &str = "examples/milestone25-oracle/oracle-manifest.json";

#[test]
fn mixed_project_defaults_and_class_trigger_sidecars_execute_their_effective_profiles() {
    let compilation = project::compile(PROJECT).unwrap();
    assert_eq!(
        compilation.invoke("M25ProfileDemo.run").unwrap(),
        ["true|false|true|1|1|current"]
    );

    let profiles = compilation.effective_profiles();
    assert_eq!(profiles.len(), 5);
    let legacy_class = profile_for(profiles, "M25Legacy.cls");
    assert_eq!(legacy_class.api_version.to_string(), "31.0");
    assert_eq!(legacy_class.origin, ProfileOrigin::Sidecar);
    let legacy_trigger = profile_for(profiles, "M25LegacyTrigger.trigger");
    assert_eq!(legacy_trigger.api_version.to_string(), "31.0");
    assert_eq!(legacy_trigger.origin, ProfileOrigin::Sidecar);
    let current = profile_for(profiles, "M25Current.cls");
    assert_eq!(current.api_version.to_string(), "65.0");
    assert_eq!(current.origin, ProfileOrigin::ProjectDefault);

    let report = test_runner::run(
        &compilation,
        &TestOptions {
            filter: Some("M25ProfileDemoTest.mixedProfiles".to_owned()),
            jobs: 1,
        },
    )
    .unwrap();
    assert_eq!(report.tests.len(), 1);
    assert!(report.tests[0].failure.is_none());
}

#[test]
fn legacy_profiles_reject_unmodeled_syntax_and_curated_platform_apis() {
    let syntax = temporary_project("legacy-syntax");
    write_project(
        &syntax,
        "31.0",
        "LegacySyntax",
        "public class LegacySyntax { public static String run() { String value = null; return value ?? 'fallback'; } }",
        None,
    );
    let error = project::compile(&syntax).unwrap_err().render();
    assert!(error.contains("null coalescing"));
    assert!(error.contains("salesforce-api-31.0"));

    let platform = temporary_project("legacy-platform");
    write_project(
        &platform,
        "31.0",
        "LegacyPlatform",
        "public class LegacyPlatform { public static Date run() { return Date.today(); } }",
        None,
    );
    let error = project::compile(&platform).unwrap_err().render();
    assert!(error.contains("API `date.today`"));
    assert!(error.contains("not modeled"));
    assert!(error.contains("salesforce-api-31.0"));

    fs::remove_dir_all(syntax).unwrap();
    fs::remove_dir_all(platform).unwrap();
}

#[test]
fn unmodeled_versions_and_invalid_sidecars_fail_before_semantic_execution() {
    let unmodeled = temporary_project("unmodeled");
    write_project(
        &unmodeled,
        "59.0",
        "Unmodeled",
        "public class Unmodeled {}",
        None,
    );
    let error = project::discover(&unmodeled).unwrap_err().render();
    assert!(error.contains("no modeled compatibility profile"));

    let invalid = temporary_project("invalid-sidecar");
    write_project(
        &invalid,
        "65.0",
        "InvalidSidecar",
        "public class InvalidSidecar {}",
        Some("<ApexClass><apiVersion>31.0</ApexClass>"),
    );
    let error = project::discover(&invalid).unwrap_err().render();
    assert!(error.contains("invalid Apex metadata sidecar"));
    assert!(error.contains("missing `</apiVersion>`"));

    fs::remove_dir_all(unmodeled).unwrap();
    fs::remove_dir_all(invalid).unwrap();
}

#[test]
fn sidecar_profile_changes_invalidate_checked_units_and_ci_cache_identity() {
    let root = temporary_project("profile-cache");
    write_project(
        &root,
        "65.0",
        "ProfileCache",
        "public class ProfileCache { public static Boolean run() { Object value = null; return value instanceof String; } }",
        Some(&sidecar("31.0", "ApexClass")),
    );
    let sidecar_path = class_directory(&root).join("ProfileCache.cls-meta.xml");

    let mut compiler = ProjectCompiler::new();
    let legacy = compiler.compile(&root).unwrap();
    assert_eq!(legacy.invoke("ProfileCache.run").unwrap(), ["true"]);
    assert_eq!(legacy.incremental.parsed_files.len(), 1);

    let mut manifest = CiManifest::generate(&root).unwrap();
    let first = apex_exec::ci::run(
        &manifest,
        &CiRunOptions {
            no_cache: true,
            ..CiRunOptions::default()
        },
    )
    .unwrap();
    assert_eq!(first.profiles[0].api_version.to_string(), "31.0");

    fs::write(&sidecar_path, sidecar("65.0", "ApexClass")).unwrap();
    let current = compiler.compile(&root).unwrap();
    assert_eq!(current.invoke("ProfileCache.run").unwrap(), ["false"]);
    assert!(current.incremental.parsed_files.is_empty());
    assert_eq!(
        current.incremental.reused_files,
        [class_directory(&root).join("ProfileCache.cls")]
    );
    assert_eq!(
        current.incremental.invalidated_files,
        [class_directory(&root).join("ProfileCache.cls")]
    );
    manifest.refresh_inputs().unwrap();
    let second = apex_exec::ci::run(
        &manifest,
        &CiRunOptions {
            no_cache: true,
            ..CiRunOptions::default()
        },
    )
    .unwrap();
    assert_eq!(second.profiles[0].api_version.to_string(), "65.0");
    assert_ne!(first.cache_key, second.cache_key);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn oracle_snapshot_and_report_bind_every_effective_profile() {
    let manifest = ConformanceManifest::load(ORACLE_MANIFEST).unwrap();
    let local = oracle::run_local(&manifest);
    assert_eq!(local.fixtures.len(), 1);
    assert_eq!(local.fixtures[0].profiles.len(), 4);
    assert_eq!(local.fixtures[0].tests[0].outcome, "pass");

    let salesforce = oracle::OracleSnapshot::load("evidence/milestone25/salesforce.json").unwrap();
    assert_eq!(salesforce.provider, oracle::OracleProvider::Salesforce);
    let report = oracle::compare(&manifest, &local, &salesforce).unwrap();
    assert!(report.is_match());
    assert_eq!(report.coverage.matched, 3);
    assert_eq!(report.coverage.total, 3);
}

#[test]
fn milestone25_cli_checks_invokes_and_tests_the_complete_slice() {
    let binary = env!("CARGO_BIN_EXE_apex-exec");
    let check = Command::new(binary)
        .args(["check", PROJECT])
        .output()
        .unwrap();
    assert!(
        check.status.success(),
        "{}",
        String::from_utf8_lossy(&check.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&check.stdout).trim(),
        "OK (4 classes, 5 source files)"
    );

    let invoke = Command::new(binary)
        .args(["invoke", PROJECT, "M25ProfileDemo.run"])
        .output()
        .unwrap();
    assert!(
        invoke.status.success(),
        "{}",
        String::from_utf8_lossy(&invoke.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&invoke.stdout).trim(),
        "true|false|true|1|1|current"
    );

    let tests = Command::new(binary)
        .args([
            "test",
            PROJECT,
            "--filter",
            "M25ProfileDemoTest.mixedProfiles",
            "--jobs",
            "1",
        ])
        .output()
        .unwrap();
    assert!(
        tests.status.success(),
        "{}",
        String::from_utf8_lossy(&tests.stderr)
    );
    assert!(String::from_utf8_lossy(&tests.stdout).contains("1 passed, 0 failed"));
}

fn profile_for<'a>(
    profiles: &'a [apex_exec::compatibility::EffectiveProfile],
    suffix: &str,
) -> &'a apex_exec::compatibility::EffectiveProfile {
    profiles
        .iter()
        .find(|profile| profile.source.ends_with(suffix))
        .unwrap_or_else(|| panic!("missing effective profile for {suffix}"))
}

fn write_project(
    root: &Path,
    project_version: &str,
    class_name: &str,
    source: &str,
    metadata: Option<&str>,
) {
    let classes = class_directory(root);
    fs::create_dir_all(&classes).unwrap();
    fs::write(
        root.join("sfdx-project.json"),
        format!(
            r#"{{"packageDirectories":[{{"path":"force-app","default":true}}],"sourceApiVersion":"{project_version}"}}"#
        ),
    )
    .unwrap();
    fs::write(classes.join(format!("{class_name}.cls")), source).unwrap();
    if let Some(metadata) = metadata {
        fs::write(classes.join(format!("{class_name}.cls-meta.xml")), metadata).unwrap();
    }
}

fn class_directory(root: &Path) -> PathBuf {
    root.join("force-app/main/default/classes")
}

fn sidecar(version: &str, kind: &str) -> String {
    format!("<{kind}><apiVersion>{version}</apiVersion><status>Active</status></{kind}>")
}

fn temporary_project(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "apex-exec-m25-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
