use apex_exec::ci::{CiManifest, CiRunOptions, CiShard, SelectionMode, run, write_integrations};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[test]
fn hermetic_manifest_round_trips_and_detects_modified_missing_and_unrecorded_inputs() {
    let root = test_project("hermetic");
    let manifest_path = root.join("ci.json");
    let manifest = CiManifest::generate(&root).unwrap();
    assert!(
        manifest
            .inputs
            .iter()
            .any(|input| input.path == Path::new("sfdx-project.json"))
    );
    assert!(
        manifest
            .inputs
            .iter()
            .any(|input| input.path.ends_with("PricingService.cls"))
    );
    assert!(manifest.inputs.iter().all(|input| input.sha256.len() == 64));
    manifest.write(&manifest_path).unwrap();

    let loaded = CiManifest::load(&manifest_path).unwrap();
    assert_eq!(loaded.inputs, manifest.inputs);
    assert_eq!(loaded.project_root(), root.canonicalize().unwrap());
    loaded.verify_inputs().unwrap();

    let pricing = classes(&root).join("PricingService.cls");
    let original = fs::read_to_string(&pricing).unwrap();
    fs::write(&pricing, original.replace("quantity * 10", "quantity * 11")).unwrap();
    let modified = loaded.verify_inputs().unwrap_err();
    assert!(modified.contains("modified"));
    assert!(modified.contains("PricingService.cls"));

    fs::write(&pricing, original).unwrap();
    let audit = classes(&root).join("AuditService.cls");
    let audit_source = fs::read_to_string(&audit).unwrap();
    fs::remove_file(&audit).unwrap();
    assert!(loaded.verify_inputs().unwrap_err().contains("missing"));

    fs::write(&audit, audit_source).unwrap();
    fs::write(
        classes(&root).join("Unrecorded.cls"),
        "public class Unrecorded {}",
    )
    .unwrap();
    assert!(loaded.verify_inputs().unwrap_err().contains("unrecorded"));

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn impacted_selection_is_transitive_and_shards_are_disjoint_complete_and_cacheable() {
    let root = test_project("selection");
    let manifest_path = root.join("ci.json");
    let cache = root.join("shared-cache");
    let mut manifest = CiManifest::generate(&root).unwrap();
    manifest.changed_files = vec![PathBuf::from(
        "force-app/main/default/classes/PricingService.cls",
    )];
    manifest.shard = CiShard { index: 0, total: 2 };
    manifest.write(&manifest_path).unwrap();
    let manifest = CiManifest::load(&manifest_path).unwrap();

    let shard_zero = run(
        &manifest,
        &CiRunOptions {
            cache_dir: Some(cache.clone()),
            ..CiRunOptions::default()
        },
    )
    .unwrap();
    let shard_one = run(
        &manifest,
        &CiRunOptions {
            cache_dir: Some(cache.clone()),
            shard: Some(CiShard { index: 1, total: 2 }),
            ..CiRunOptions::default()
        },
    )
    .unwrap();

    assert_eq!(shard_zero.selection, SelectionMode::Impacted);
    assert_eq!(shard_one.selection, SelectionMode::Impacted);
    let zero = shard_zero
        .selected_tests
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let one = shard_one
        .selected_tests
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    assert!(zero.is_disjoint(&one));
    let union = zero.union(&one).cloned().collect::<BTreeSet<_>>();
    assert_eq!(
        union,
        BTreeSet::from([
            "CheckoutServiceTest.quotesCart".to_owned(),
            "PricingServiceTest.pricesBulk".to_owned(),
            "PricingServiceTest.pricesSingle".to_owned(),
        ])
    );
    assert!(
        !union
            .iter()
            .any(|name| name.starts_with("AuditServiceTest"))
    );
    assert!(shard_zero.is_success());
    assert!(shard_one.is_success());

    let replay = run(
        &manifest,
        &CiRunOptions {
            cache_dir: Some(cache),
            replay_only: true,
            ..CiRunOptions::default()
        },
    )
    .unwrap();
    assert!(replay.cache_hit);
    assert_eq!(replay.cache_key, shard_zero.cache_key);
    assert_eq!(replay.tests, shard_zero.tests);
    assert_eq!(replay.cache_key.len(), 64);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn metadata_and_unknown_changes_fall_back_to_all_tests() {
    let root = test_project("fallback");
    let manifest_path = root.join("ci.json");
    let mut manifest = CiManifest::generate(&root).unwrap();
    manifest.changed_files = vec![PathBuf::from(
        "force-app/main/default/objects/Order__c/Order__c.object-meta.xml",
    )];
    manifest.write(&manifest_path).unwrap();
    let result = run(
        &CiManifest::load(&manifest_path).unwrap(),
        &CiRunOptions {
            no_cache: true,
            ..CiRunOptions::default()
        },
    )
    .unwrap();

    assert_eq!(result.selection, SelectionMode::ConservativeAll);
    assert_eq!(result.selected_tests.len(), 4);
    assert!(
        result
            .selected_tests
            .contains(&"AuditServiceTest.recordsMessage".to_owned())
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn reports_are_standard_shard_aware_and_policy_gates_are_enforced() {
    let root = test_project("reports-policy");
    let compatibility = root.join("compatibility.json");
    fs::write(
        &compatibility,
        r#"{"coverage":{"matched":3,"total":4,"percentage":75.0}}"#,
    )
    .unwrap();
    let manifest_path = root.join("ci.json");
    let mut manifest = CiManifest::generate(&root).unwrap();
    manifest.changed_files = vec![PathBuf::from(
        "force-app/main/default/classes/AuditService.cls",
    )];
    manifest.policy.min_line_coverage = Some(100.0);
    manifest.policy.compatibility_report = Some(PathBuf::from("compatibility.json"));
    manifest.policy.min_compatibility = Some(90.0);
    manifest.refresh_inputs().unwrap();
    manifest.write(&manifest_path).unwrap();

    let result = run(
        &CiManifest::load(&manifest_path).unwrap(),
        &CiRunOptions {
            no_cache: true,
            ..CiRunOptions::default()
        },
    )
    .unwrap();
    assert!(!result.is_success());
    assert!(
        result
            .policy_violations
            .iter()
            .any(|violation| violation.contains("line coverage"))
    );
    assert!(
        result
            .policy_violations
            .iter()
            .any(|violation| violation.contains("compatibility coverage 75.00%"))
    );

    let junit = fs::read_to_string(root.join("artifacts/0/junit.xml")).unwrap();
    assert!(junit.starts_with("<?xml version=\"1.0\""));
    assert!(junit.contains("<testsuite"));
    let cobertura = fs::read_to_string(root.join("artifacts/0/coverage.xml")).unwrap();
    assert!(cobertura.contains("<coverage line-rate="));
    assert!(cobertura.contains("filename=\"force-app/main/default/classes/AuditService.cls\""));
    let sarif = fs::read_to_string(root.join("artifacts/0/results.sarif")).unwrap();
    let sarif = serde_json::from_str::<Value>(&sarif).unwrap();
    assert_eq!(sarif["version"], "2.1.0");
    assert_eq!(sarif["runs"][0]["automationDetails"]["id"], "shard-0/1");
    assert!(
        sarif["runs"][0]["results"]
            .as_array()
            .unwrap()
            .iter()
            .any(|result| result["ruleId"] == "apex-exec.policy")
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn compile_failures_produce_source_mapped_sarif_without_running_tests() {
    let root = test_project("compile-failure");
    fs::write(
        classes(&root).join("PricingService.cls"),
        "public class PricingService {
            public static Integer price(Integer quantity) {
                return missing;
            }
        }",
    )
    .unwrap();
    let manifest_path = root.join("ci.json");
    CiManifest::generate(&root)
        .unwrap()
        .write(&manifest_path)
        .unwrap();
    let result = run(
        &CiManifest::load(&manifest_path).unwrap(),
        &CiRunOptions {
            no_cache: true,
            ..CiRunOptions::default()
        },
    )
    .unwrap();

    assert!(!result.compile_success);
    assert!(result.tests.is_none());
    let diagnostic = result.compile_diagnostic.unwrap();
    assert!(diagnostic.message.contains("unknown variable `missing`"));
    assert!(diagnostic.path.unwrap().ends_with("PricingService.cls"));
    assert!(diagnostic.line.is_some());

    let sarif = fs::read_to_string(root.join("artifacts/0/results.sarif")).unwrap();
    assert!(sarif.contains("\"ruleId\": \"apex-exec.compile\""));
    assert!(sarif.contains("\"startLine\""));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn integration_templates_and_cli_cover_generation_execution_cache_and_replay() {
    let root = test_project("cli");
    let manifest = root.join("ci.json");
    let generated = Command::new(env!("CARGO_BIN_EXE_apex-exec"))
        .args([
            "ci",
            "manifest",
            root.to_str().unwrap(),
            "--output",
            manifest.to_str().unwrap(),
            "--changed",
            "force-app/main/default/classes/PricingService.cls",
            "--shards",
            "2",
            "--jobs",
            "2",
            "--min-line-coverage",
            "0",
        ])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );
    assert!(String::from_utf8_lossy(&generated.stdout).contains("Wrote hermetic CI manifest"));

    let cache = root.join("cache");
    let changed_list = root.join("changed.txt");
    fs::write(
        &changed_list,
        "force-app/main/default/classes/PricingService.cls\n",
    )
    .unwrap();
    let first = Command::new(env!("CARGO_BIN_EXE_apex-exec"))
        .args([
            "ci",
            "run",
            manifest.to_str().unwrap(),
            "--cache-dir",
            cache.to_str().unwrap(),
            "--changed-list",
            changed_list.to_str().unwrap(),
            "--shard",
            "0/2",
        ])
        .output()
        .unwrap();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    assert!(String::from_utf8_lossy(&first.stdout).contains("CI PASS"));

    let replay = Command::new(env!("CARGO_BIN_EXE_apex-exec"))
        .args([
            "ci",
            "run",
            manifest.to_str().unwrap(),
            "--cache-dir",
            cache.to_str().unwrap(),
            "--changed-list",
            changed_list.to_str().unwrap(),
            "--shard",
            "0/2",
            "--replay",
        ])
        .output()
        .unwrap();
    assert!(replay.status.success());
    assert!(String::from_utf8_lossy(&replay.stdout).contains("content-addressed cache hit"));

    let integration_dir = root.join("ci-integrations");
    let integrations = write_integrations(&integration_dir, "ci.json").unwrap();
    assert_eq!(integrations.len(), 3);
    assert!(integration_dir.join("github-actions.yml").is_file());
    assert!(
        fs::read_to_string(integration_dir.join("github-actions.yml"))
            .unwrap()
            .contains("--changed-list apex-exec-changed.txt")
    );
    assert!(
        fs::read_to_string(integration_dir.join("github-actions.yml"))
            .unwrap()
            .contains("apex-exec ci run")
    );
    assert!(
        !fs::read_to_string(integration_dir.join("github-actions.yml"))
            .unwrap()
            .contains("cargo run")
    );
    assert!(
        fs::read_to_string(integration_dir.join(".gitlab-ci.yml"))
            .unwrap()
            .contains("coverage_format: cobertura")
    );
    assert!(
        fs::read_to_string(integration_dir.join("Jenkinsfile"))
            .unwrap()
            .contains("--shard 1/2")
    );

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn replay_requires_an_exact_cached_artifact_and_invalid_shards_are_rejected() {
    let root = test_project("replay-errors");
    let manifest_path = root.join("ci.json");
    CiManifest::generate(&root)
        .unwrap()
        .write(&manifest_path)
        .unwrap();
    let manifest = CiManifest::load(&manifest_path).unwrap();
    let missing = run(
        &manifest,
        &CiRunOptions {
            cache_dir: Some(root.join("empty-cache")),
            replay_only: true,
            ..CiRunOptions::default()
        },
    )
    .unwrap_err();
    assert!(missing.contains("no cached CI artifact"));

    let invalid = run(
        &manifest,
        &CiRunOptions {
            shard: Some(CiShard { index: 2, total: 2 }),
            ..CiRunOptions::default()
        },
    )
    .unwrap_err();
    assert!(invalid.contains("invalid CI shard"));
    fs::remove_dir_all(root).unwrap();
}

fn test_project(label: &str) -> PathBuf {
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!(
        "apex-exec-m14-{label}-{}-{sequence}",
        std::process::id()
    ));
    let classes = classes(&root);
    fs::create_dir_all(&classes).unwrap();
    fs::write(
        root.join("sfdx-project.json"),
        r#"{"packageDirectories":[{"path":"force-app","default":true}]}"#,
    )
    .unwrap();
    fs::write(
        classes.join("PricingService.cls"),
        "public class PricingService {
            public static Integer price(Integer quantity) {
                if (quantity > 1) {
                    return quantity * 10;
                }
                return 10;
            }
        }",
    )
    .unwrap();
    fs::write(
        classes.join("CheckoutService.cls"),
        "public class CheckoutService {
            public static Integer quote(Integer quantity) {
                return PricingService.price(quantity);
            }
        }",
    )
    .unwrap();
    fs::write(
        classes.join("AuditService.cls"),
        "public class AuditService {
            public static String record(String message) {
                return 'audit:' + message;
            }
        }",
    )
    .unwrap();
    fs::write(
        classes.join("PricingServiceTest.cls"),
        "@IsTest private class PricingServiceTest {
            @IsTest static void pricesBulk() {
                System.assertEquals(30, PricingService.price(3));
            }
            @IsTest static void pricesSingle() {
                System.assertEquals(10, PricingService.price(1));
            }
        }",
    )
    .unwrap();
    fs::write(
        classes.join("CheckoutServiceTest.cls"),
        "@IsTest private class CheckoutServiceTest {
            @IsTest static void quotesCart() {
                System.assertEquals(20, CheckoutService.quote(2));
            }
        }",
    )
    .unwrap();
    fs::write(
        classes.join("AuditServiceTest.cls"),
        "@IsTest private class AuditServiceTest {
            @IsTest static void recordsMessage() {
                System.assertEquals('audit:ready', AuditService.record('ready'));
            }
        }",
    )
    .unwrap();
    let object = root.join("force-app/main/default/objects/Order__c");
    fs::create_dir_all(&object).unwrap();
    fs::write(
        object.join("Order__c.object-meta.xml"),
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>
        <CustomObject xmlns=\"http://soap.sforce.com/2006/04/metadata\">
          <label>Order</label>
          <pluralLabel>Orders</pluralLabel>
          <nameField><label>Order Number</label><type>Text</type></nameField>
          <deploymentStatus>Deployed</deploymentStatus>
          <sharingModel>ReadWrite</sharingModel>
        </CustomObject>",
    )
    .unwrap();
    root
}

fn classes(root: &Path) -> PathBuf {
    root.join("force-app/main/default/classes")
}
