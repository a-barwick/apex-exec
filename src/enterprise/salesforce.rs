use super::EnterpriseManifest;
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    path::Path,
    process::{Command, Stdio},
    time::SystemTime,
};

const SALESFORCE_CAPTURE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug)]
pub struct SalesforceCaptureOptions {
    pub target_org: String,
    pub wait_minutes: u32,
}

impl Default for SalesforceCaptureOptions {
    fn default() -> Self {
        Self {
            target_org: String::new(),
            wait_minutes: 60,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SalesforceTestOutcome {
    pub name: String,
    pub outcome: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SalesforceCapture {
    pub schema_version: u32,
    pub manifest_sha256: String,
    pub target_org: String,
    pub org_id: String,
    pub api_version: String,
    pub salesforce_cli_version: String,
    pub captured_at: String,
    pub test_level: String,
    pub source_test_classes: Vec<String>,
    pub tests: Vec<SalesforceTestOutcome>,
    pub snapshot_sha256: String,
}

impl SalesforceCapture {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref();
        let source = fs::read_to_string(path).map_err(|error| {
            format!(
                "failed to read enterprise Salesforce capture `{}`: {error}",
                path.display()
            )
        })?;
        let capture = serde_json::from_str::<Self>(&source).map_err(|error| {
            format!(
                "invalid enterprise Salesforce capture `{}`: {error}",
                path.display()
            )
        })?;
        capture.validate()?;
        Ok(capture)
    }

    pub fn write(&self, path: impl AsRef<Path>) -> Result<(), String> {
        self.validate()?;
        let path = path.as_ref();
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create enterprise capture directory `{}`: {error}",
                    parent.display()
                )
            })?;
        }
        let mut json = serde_json::to_string_pretty(self)
            .map_err(|error| format!("failed to serialize Salesforce capture: {error}"))?;
        json.push('\n');
        fs::write(path, json).map_err(|error| {
            format!(
                "failed to write enterprise Salesforce capture `{}`: {error}",
                path.display()
            )
        })
    }

    pub fn passed(&self) -> usize {
        self.tests
            .iter()
            .filter(|test| test.outcome == "pass")
            .count()
    }

    pub fn failed(&self) -> usize {
        self.tests.len() - self.passed()
    }

    #[cfg(test)]
    pub(super) fn fixture(
        manifest_sha256: String,
        source_test_classes: Vec<String>,
        tests: Vec<SalesforceTestOutcome>,
    ) -> Self {
        let mut capture = Self {
            schema_version: SALESFORCE_CAPTURE_SCHEMA_VERSION,
            manifest_sha256,
            target_org: "fixture-org".to_owned(),
            org_id: "00D000000000001AAA".to_owned(),
            api_version: "65.0".to_owned(),
            salesforce_cli_version: "@salesforce/cli/2.134.1".to_owned(),
            captured_at: "2026-07-19T00:00:00Z".to_owned(),
            test_level: "RunLocalTests".to_owned(),
            source_test_classes,
            tests,
            snapshot_sha256: String::new(),
        };
        capture.seal().expect("fixture capture should serialize");
        capture
    }

    fn seal(&mut self) -> Result<(), String> {
        self.snapshot_sha256.clear();
        let bytes = serde_json::to_vec(self)
            .map_err(|error| format!("failed to serialize Salesforce capture seal: {error}"))?;
        self.snapshot_sha256 = sha256(&bytes);
        Ok(())
    }

    fn validate(&self) -> Result<(), String> {
        if self.schema_version != SALESFORCE_CAPTURE_SCHEMA_VERSION {
            return Err(format!(
                "unsupported enterprise Salesforce capture schema {}; expected {}",
                self.schema_version, SALESFORCE_CAPTURE_SCHEMA_VERSION
            ));
        }
        validate_sha256(&self.manifest_sha256, "manifest")?;
        validate_sha256(&self.snapshot_sha256, "snapshot")?;
        if self.target_org.trim().is_empty() || self.salesforce_cli_version.trim().is_empty() {
            return Err("enterprise Salesforce identity fields cannot be empty".to_owned());
        }
        if self.org_id.len() != 18
            || !self.org_id.starts_with("00D")
            || !self.org_id.bytes().all(|byte| byte.is_ascii_alphanumeric())
        {
            return Err("enterprise Salesforce org ID must be an 18-character 00D ID".to_owned());
        }
        if self.test_level != "RunLocalTests" {
            return Err("enterprise Salesforce capture must use RunLocalTests".to_owned());
        }
        DateTime::parse_from_rfc3339(&self.captured_at)
            .map_err(|error| format!("invalid enterprise capture timestamp: {error}"))?;
        validate_sorted_unique(&self.source_test_classes, "source test classes")?;
        if self.tests.len() < 100 {
            return Err(format!(
                "enterprise Salesforce denominator has {} tests; at least 100 are required",
                self.tests.len()
            ));
        }
        let names = self
            .tests
            .iter()
            .map(|test| test.name.clone())
            .collect::<Vec<_>>();
        validate_sorted_unique(&names, "Salesforce tests")?;
        for test in &self.tests {
            if !matches!(test.outcome.as_str(), "pass" | "fail") {
                return Err(format!(
                    "enterprise Salesforce test `{}` has unsupported outcome `{}`",
                    test.name, test.outcome
                ));
            }
        }
        let mut unsigned = self.clone();
        unsigned.snapshot_sha256.clear();
        let expected = sha256(
            serde_json::to_vec(&unsigned)
                .map_err(|error| format!("failed to verify Salesforce capture seal: {error}"))?,
        );
        if expected != self.snapshot_sha256 {
            return Err("enterprise Salesforce capture digest mismatch".to_owned());
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct EnterpriseSalesforceCli {
    executable: OsString,
}

impl Default for EnterpriseSalesforceCli {
    fn default() -> Self {
        Self {
            executable: OsString::from("sf"),
        }
    }
}

impl EnterpriseSalesforceCli {
    pub fn new(executable: impl Into<OsString>) -> Self {
        let executable = executable.into();
        let path = Path::new(&executable);
        let executable = if path.is_relative() && path.components().count() > 1 {
            std::env::current_dir()
                .map(|current| current.join(path).into_os_string())
                .unwrap_or(executable)
        } else {
            executable
        };
        Self { executable }
    }

    pub fn capture(
        &self,
        manifest: &EnterpriseManifest,
        options: &SalesforceCaptureOptions,
    ) -> Result<SalesforceCapture, String> {
        manifest.verify_inputs()?;
        if options.target_org.trim().is_empty() {
            return Err("enterprise capture requires a Salesforce target org".to_owned());
        }
        if options.wait_minutes == 0 {
            return Err("enterprise Salesforce wait must be at least one minute".to_owned());
        }
        let source_test_classes = source_test_classes(manifest)?;
        let version = self.command_text(manifest.project_root(), &[OsString::from("--version")])?;
        let salesforce_cli_version = version
            .split_whitespace()
            .next()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "Salesforce CLI version response omitted its version".to_owned())?
            .to_owned();
        let org = self.command_json(
            manifest.project_root(),
            &[
                OsString::from("org"),
                OsString::from("display"),
                OsString::from("--target-org"),
                OsString::from(&options.target_org),
                OsString::from("--json"),
            ],
        )?;
        if find_u64(&org, "status") != Some(0) {
            return Err("Salesforce CLI could not resolve the enterprise target org".to_owned());
        }
        let org_id = find_string(&org, "id")
            .ok_or_else(|| "Salesforce org display omitted the org ID".to_owned())?
            .to_owned();
        if let Some(status) = find_string(&org, "connectedStatus")
            && !status.eq_ignore_ascii_case("connected")
        {
            return Err(format!(
                "Salesforce target org is not connected (status `{status}`)"
            ));
        }
        let test_response = self.command_json(
            manifest.project_root(),
            &[
                OsString::from("apex"),
                OsString::from("run"),
                OsString::from("test"),
                OsString::from("--target-org"),
                OsString::from(&options.target_org),
                OsString::from("--test-level"),
                OsString::from("RunLocalTests"),
                OsString::from("--wait"),
                OsString::from(options.wait_minutes.to_string()),
                OsString::from("--result-format"),
                OsString::from("json"),
                OsString::from("--api-version"),
                OsString::from(&manifest.candidate.api_version),
                OsString::from("--json"),
            ],
        )?;
        let tests = parse_test_outcomes(&test_response)?;
        let allowed = source_test_classes
            .iter()
            .map(|name| name.to_ascii_lowercase())
            .collect::<BTreeSet<_>>();
        let unexpected = tests
            .iter()
            .filter_map(|test| {
                let class = test.name.split_once('.').map_or("", |(class, _)| class);
                (!allowed.contains(&class.to_ascii_lowercase())).then(|| test.name.clone())
            })
            .collect::<Vec<_>>();
        if !unexpected.is_empty() {
            return Err(format!(
                "Salesforce discovered tests outside the pinned test roots: {}",
                unexpected.join(", ")
            ));
        }
        if tests.len() < manifest.minimum_test_methods {
            return Err(format!(
                "Salesforce discovered {} candidate tests; at least {} are required",
                tests.len(),
                manifest.minimum_test_methods
            ));
        }
        let captured_at =
            DateTime::<Utc>::from(SystemTime::now()).to_rfc3339_opts(SecondsFormat::Secs, true);
        let mut capture = SalesforceCapture {
            schema_version: SALESFORCE_CAPTURE_SCHEMA_VERSION,
            manifest_sha256: manifest.sha256()?,
            target_org: options.target_org.clone(),
            org_id,
            api_version: manifest.candidate.api_version.clone(),
            salesforce_cli_version,
            captured_at,
            test_level: "RunLocalTests".to_owned(),
            source_test_classes,
            tests,
            snapshot_sha256: String::new(),
        };
        capture.seal()?;
        capture.validate()?;
        Ok(capture)
    }

    fn command_json(&self, current_dir: &Path, arguments: &[OsString]) -> Result<Value, String> {
        let output = Command::new(&self.executable)
            .args(arguments)
            .current_dir(current_dir)
            .env("SF_DISABLE_LOG_FILE", "true")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|error| {
                format!(
                    "failed to start Salesforce CLI `{}`: {error}",
                    Path::new(&self.executable).display()
                )
            })?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        serde_json::from_str(&stdout).map_err(|error| {
            let stderr = String::from_utf8_lossy(&output.stderr);
            format!(
                "Salesforce CLI returned invalid JSON (status {}): {error}; stderr: {}",
                output.status,
                stderr.trim()
            )
        })
    }

    fn command_text(&self, current_dir: &Path, arguments: &[OsString]) -> Result<String, String> {
        let output = Command::new(&self.executable)
            .args(arguments)
            .current_dir(current_dir)
            .env("SF_DISABLE_LOG_FILE", "true")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|error| {
                format!(
                    "failed to start Salesforce CLI `{}`: {error}",
                    Path::new(&self.executable).display()
                )
            })?;
        if !output.status.success() {
            return Err(format!(
                "Salesforce CLI version command failed with status {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        String::from_utf8(output.stdout)
            .map_err(|error| format!("Salesforce CLI version output was not UTF-8: {error}"))
    }
}

fn source_test_classes(manifest: &EnterpriseManifest) -> Result<Vec<String>, String> {
    let mut classes = Vec::new();
    for input in &manifest.inputs {
        if input
            .path
            .extension()
            .is_some_and(|extension| extension == "cls")
            && manifest
                .test_roots
                .iter()
                .any(|root| input.path.starts_with(root))
        {
            let name = input
                .path
                .file_stem()
                .and_then(|name| name.to_str())
                .ok_or_else(|| {
                    format!(
                        "enterprise test source `{}` has a non-Unicode class name",
                        input.path.display()
                    )
                })?;
            classes.push(name.to_owned());
        }
    }
    classes.sort_by_key(|name| name.to_ascii_lowercase());
    classes.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    if classes.is_empty() {
        return Err("enterprise test roots contain no Apex classes".to_owned());
    }
    Ok(classes)
}

fn parse_test_outcomes(response: &Value) -> Result<Vec<SalesforceTestOutcome>, String> {
    let mut collected = Vec::new();
    collect_test_results(response, &mut collected);
    let mut tests = BTreeMap::<String, SalesforceTestOutcome>::new();
    for test in collected {
        let key = test.name.to_ascii_lowercase();
        if let Some(previous) = tests.get(&key) {
            if previous != &test {
                return Err(format!(
                    "Salesforce returned conflicting outcomes for `{}`",
                    test.name
                ));
            }
        } else {
            tests.insert(key, test);
        }
    }
    let mut tests = tests.into_values().collect::<Vec<_>>();
    tests.sort_by(|left, right| {
        left.name
            .to_ascii_lowercase()
            .cmp(&right.name.to_ascii_lowercase())
    });
    if tests.is_empty() {
        return Err("Salesforce returned no enterprise test outcomes".to_owned());
    }
    Ok(tests)
}

fn collect_test_results(value: &Value, tests: &mut Vec<SalesforceTestOutcome>) {
    match value {
        Value::Array(values) => {
            for value in values {
                collect_test_results(value, tests);
            }
        }
        Value::Object(map) => {
            let class = map
                .get("ApexClass")
                .and_then(|value| value.get("Name"))
                .and_then(Value::as_str)
                .or_else(|| map.get("name").and_then(Value::as_str));
            let method = map
                .get("MethodName")
                .and_then(Value::as_str)
                .or_else(|| map.get("methodName").and_then(Value::as_str));
            let outcome = map
                .get("Outcome")
                .and_then(Value::as_str)
                .or_else(|| map.get("outcome").and_then(Value::as_str));
            if let (Some(class), Some(method), Some(outcome)) = (class, method, outcome) {
                tests.push(SalesforceTestOutcome {
                    name: format!("{class}.{method}"),
                    outcome: if outcome.eq_ignore_ascii_case("pass") {
                        "pass".to_owned()
                    } else {
                        "fail".to_owned()
                    },
                    message: map
                        .get("Message")
                        .or_else(|| map.get("message"))
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    stack: map
                        .get("StackTrace")
                        .or_else(|| map.get("stackTrace"))
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                });
            }
            for child in map.values() {
                collect_test_results(child, tests);
            }
        }
        _ => {}
    }
}

fn find_string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    match value {
        Value::Object(map) => map
            .get(key)
            .and_then(Value::as_str)
            .or_else(|| map.values().find_map(|value| find_string(value, key))),
        Value::Array(values) => values.iter().find_map(|value| find_string(value, key)),
        _ => None,
    }
}

fn find_u64(value: &Value, key: &str) -> Option<u64> {
    match value {
        Value::Object(map) => map
            .get(key)
            .and_then(Value::as_u64)
            .or_else(|| map.values().find_map(|value| find_u64(value, key))),
        Value::Array(values) => values.iter().find_map(|value| find_u64(value, key)),
        _ => None,
    }
}

fn validate_sorted_unique(values: &[String], label: &str) -> Result<(), String> {
    let mut previous = None;
    for value in values {
        if value.trim().is_empty() {
            return Err(format!("enterprise {label} cannot contain an empty value"));
        }
        if previous
            .is_some_and(|prior: &String| prior.to_ascii_lowercase() >= value.to_ascii_lowercase())
        {
            return Err(format!(
                "enterprise {label} must be unique and case-insensitively sorted"
            ));
        }
        previous = Some(value);
    }
    Ok(())
}

fn validate_sha256(digest: &str, label: &str) -> Result<(), String> {
    if digest.len() != 64
        || !digest
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err(format!("enterprise Salesforce {label} digest is invalid"));
    }
    Ok(())
}

fn sha256(bytes: impl AsRef<[u8]>) -> String {
    Sha256::digest(bytes.as_ref())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enterprise::CandidateIdentity;
    use std::{
        os::unix::fs::PermissionsExt,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn capture_binds_org_tool_candidate_and_complete_denominator() {
        let root = fixture_root("capture");
        write(
            &root.join("force-app/tests/DemoTest.cls"),
            "@IsTest private class DemoTest {}",
        );
        let manifest = manifest(&root);
        let tests = (0..100)
            .map(|index| {
                serde_json::json!({
                    "ApexClass": {"Name": "DemoTest"},
                    "MethodName": format!("test{index:03}"),
                    "Outcome": if index == 99 { "Fail" } else { "Pass" }
                })
            })
            .collect::<Vec<_>>();
        let response = root.join("tests.json");
        write(
            &response,
            &serde_json::json!({"status": 0, "result": {"tests": tests}}).to_string(),
        );
        let sf = fake_sf(&root, &response);
        let relative_sf = sf.strip_prefix(std::env::current_dir().unwrap()).unwrap();
        let capture = EnterpriseSalesforceCli::new(relative_sf)
            .capture(
                &manifest,
                &SalesforceCaptureOptions {
                    target_org: "benchmark".to_owned(),
                    wait_minutes: 10,
                },
            )
            .unwrap();
        assert_eq!(capture.tests.len(), 100);
        assert_eq!(capture.passed(), 99);
        assert_eq!(capture.failed(), 1);
        assert_eq!(capture.org_id, "00D000000000001AAA");
        let output = root.join("capture.json");
        capture.write(&output).unwrap();
        let serialized = fs::read_to_string(&output).unwrap();
        assert!(!serialized.contains("accessToken"));
        assert!(!serialized.contains("never-serialize"));
        assert!(!serialized.contains("instanceUrl"));
        assert_eq!(SalesforceCapture::load(&output).unwrap(), capture);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn capture_rejects_org_tests_outside_the_pinned_roots() {
        let root = fixture_root("foreign");
        write(
            &root.join("force-app/tests/DemoTest.cls"),
            "@IsTest private class DemoTest {}",
        );
        let manifest = manifest(&root);
        let tests = (0..100)
            .map(|index| {
                serde_json::json!({
                    "ApexClass": {"Name": if index == 99 { "ForeignTest" } else { "DemoTest" }},
                    "MethodName": format!("test{index:03}"),
                    "Outcome": "Pass"
                })
            })
            .collect::<Vec<_>>();
        let response = root.join("tests.json");
        write(
            &response,
            &serde_json::json!({"status": 0, "result": {"tests": tests}}).to_string(),
        );
        let error = EnterpriseSalesforceCli::new(fake_sf(&root, &response))
            .capture(
                &manifest,
                &SalesforceCaptureOptions {
                    target_org: "benchmark".to_owned(),
                    wait_minutes: 10,
                },
            )
            .unwrap_err();
        assert!(error.contains("outside the pinned test roots"));
        fs::remove_dir_all(root).unwrap();
    }

    fn manifest(root: &Path) -> EnterpriseManifest {
        EnterpriseManifest::generate(
            root,
            CandidateIdentity {
                name: "Candidate".to_owned(),
                repository: "https://example.com/candidate.git".to_owned(),
                git_commit: "a".repeat(40),
                git_tag: "v1.0.0".to_owned(),
                api_version: "65.0".to_owned(),
            },
            vec![PathBuf::from("force-app")],
            vec![PathBuf::from("force-app/tests")],
        )
        .unwrap()
    }

    fn fake_sf(root: &Path, test_response: &Path) -> PathBuf {
        let path = root.join("sf");
        write(
            &path,
            &format!(
                "#!/bin/sh\n\
                 if [ \"$1\" = \"--version\" ]; then\n\
                   printf '%s' '@salesforce/cli/2.134.1 test-platform node-v22.0.0'\n\
                 elif [ \"$1\" = \"org\" ]; then\n\
                   printf '%s' '{{\"status\":0,\"result\":{{\"id\":\"00D000000000001AAA\",\"instanceUrl\":\"https://example.my.salesforce.com\",\"accessToken\":\"never-serialize\"}}}}'\n\
                 else\n\
                   exec cat '{}'\n\
                 fi\n",
                test_response.display()
            ),
        );
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&path, permissions).unwrap();
        path
    }

    fn fixture_root(label: &str) -> PathBuf {
        std::env::current_dir()
            .unwrap()
            .join("target")
            .join(format!(
                "apex-exec-enterprise-salesforce-{label}-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ))
    }

    fn write(path: &Path, contents: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }
}
