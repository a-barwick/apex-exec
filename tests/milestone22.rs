use serde_json::Value;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[test]
fn enterprise_cli_freezes_and_measures_the_complete_denominator() {
    let root = fixture_root();
    let classes = root.join("force-app/main/default/classes");
    fs::create_dir_all(&classes).unwrap();
    fs::write(
        root.join("sfdx-project.json"),
        r#"{"packageDirectories":[{"path":"force-app","default":true}]}"#,
    )
    .unwrap();
    fs::write(
        classes.join("EnterpriseSupport.cls"),
        "public class EnterpriseSupport { public static Integer value() { return 1; } }",
    )
    .unwrap();
    let mut source = String::from("@IsTest public class EnterpriseTest {\n");
    for index in 0..100 {
        source.push_str(&format!(
            "@IsTest static void case{index:03}() {{ System.assertEquals(1, EnterpriseSupport.value()); }}\n"
        ));
    }
    source.push_str("}\n");
    fs::write(classes.join("EnterpriseTest.cls"), source).unwrap();

    let manifest = root.join("manifest.json");
    let capture = root.join("salesforce.json");
    let report = root.join("report.json");
    let binary = env!("CARGO_BIN_EXE_apex-exec");
    let generated = Command::new(binary)
        .args([
            "enterprise",
            "manifest",
            root.to_str().unwrap(),
            "--name",
            "Enterprise CLI fixture",
            "--repository",
            "https://example.com/enterprise.git",
            "--commit",
            &"a".repeat(40),
            "--tag",
            "v1.0.0",
            "--api-version",
            "65.0",
            "--package-root",
            "force-app",
            "--test-root",
            "force-app/main/default/classes",
            "--output",
            manifest.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        generated.status.success(),
        "{}",
        String::from_utf8_lossy(&generated.stderr)
    );

    let response = root.join("test-response.json");
    let tests = (0..100)
        .map(|index| {
            serde_json::json!({
                "ApexClass": {"Name": "EnterpriseTest"},
                "MethodName": format!("case{index:03}"),
                "Outcome": "Pass"
            })
        })
        .collect::<Vec<_>>();
    fs::write(
        &response,
        serde_json::json!({"status": 0, "result": {"tests": tests}}).to_string(),
    )
    .unwrap();
    let sf = fake_sf(&root, &response);
    let captured = Command::new(binary)
        .args([
            "enterprise",
            "capture",
            manifest.to_str().unwrap(),
            "--target-org",
            "fixture",
            "--sf",
            sf.to_str().unwrap(),
            "--output",
            capture.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        captured.status.success(),
        "{}",
        String::from_utf8_lossy(&captured.stderr)
    );

    let measured = Command::new(binary)
        .args([
            "enterprise",
            "run",
            manifest.to_str().unwrap(),
            "--salesforce",
            capture.to_str().unwrap(),
            "--output",
            report.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(
        measured.status.success(),
        "{}",
        String::from_utf8_lossy(&measured.stderr)
    );
    let report: Value = serde_json::from_str(&fs::read_to_string(report).unwrap()).unwrap();
    assert_eq!(report["rawDenominator"], 100);
    assert_eq!(report["counts"]["strictCompatible"]["count"], 100);
    assert_eq!(report["deterministicReruns"], 3);
    fs::remove_dir_all(root).unwrap();
}

fn fake_sf(root: &Path, response: &Path) -> PathBuf {
    let path = root.join("sf");
    fs::write(
        &path,
        format!(
            "#!/bin/sh\n\
             if [ \"$1\" = \"--version\" ]; then\n\
               printf '%s' '@salesforce/cli/2.134.1 test-platform node-v22.0.0'\n\
             elif [ \"$1\" = \"org\" ]; then\n\
               printf '%s' '{{\"status\":0,\"result\":{{\"id\":\"00D000000000001AAA\",\"connectedStatus\":\"Connected\",\"accessToken\":\"do-not-record\"}}}}'\n\
             else\n\
               exec cat '{}'\n\
             fi\n",
            response.display()
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&path).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&path, permissions).unwrap();
    path
}

fn fixture_root() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "apex-exec-milestone22-{}-{unique}",
        std::process::id()
    ))
}
