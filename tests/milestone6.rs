use apex_exec::ast::{AnnotationKind, ClassMember};
use apex_exec::project;
use apex_exec::test_runner::{self, TestOptions};
use apex_exec::{check, execute, parse};
use std::{fs, path::PathBuf, process::Command};

const TEST_CLASS: &str = "
@IsTest(SeeAllData=false)
private class CalculatorTest {
    @TestSetup
    static void setupData() {
        System.debug('setup');
    }

    @IsTest
    static void addsValues() {
        System.assertEquals(4, 2 + 2, 'addition should work');
    }
}
";

#[test]
fn parses_and_checks_supported_test_annotations_and_signatures() {
    let parsed = parse(TEST_CLASS).unwrap();
    assert_eq!(parsed.classes.len(), 1);
    assert!(matches!(
        parsed.classes[0].annotations[0].kind,
        AnnotationKind::IsTest {
            see_all_data: Some(false)
        }
    ));

    let methods = parsed.classes[0]
        .members
        .iter()
        .filter_map(|member| match member {
            ClassMember::Method(method) => Some(method),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(methods[0].annotations[0].kind.is_test_setup());
    assert!(methods[1].annotations[0].kind.is_test());
    check(TEST_CLASS).unwrap();
}

#[test]
fn rejects_unsupported_annotations_data_access_and_invalid_test_methods() {
    let unsupported = parse("@Future public class Worker {}").unwrap_err();
    assert_eq!(unsupported.message, "`@future` is only valid on methods");

    let see_all_data = check("@IsTest(SeeAllData=true) private class DataTest {}").unwrap_err();
    assert!(
        see_all_data
            .message
            .contains("unsupported without an org data host")
    );

    let wrong_signature = check(
        "@IsTest private class WrongTest {
            @IsTest void broken(Integer value) {}
        }",
    )
    .unwrap_err();
    assert_eq!(
        wrong_signature.message,
        "`@IsTest` method must be static void with no parameters and a body"
    );

    let wrong_owner = check(
        "public class WrongOwner {
            @TestSetup static void setupData() {}
        }",
    )
    .unwrap_err();
    assert_eq!(
        wrong_owner.message,
        "`@TestSetup` methods require an `@IsTest` class"
    );
}

#[test]
fn system_assertions_pass_or_raise_catchable_assert_exceptions() {
    let source = "
        System.assert(true);
        System.assertEquals('value', 'value');
        System.assertNotEquals(1, 2);
        try {
            System.assertEquals(4, 5, 'numbers differ');
        } catch (AssertException error) {
            System.debug(error.getTypeName());
            System.debug(error.getMessage());
        }
    ";
    let output = execute(source).unwrap();
    assert_eq!(output[0], "AssertException");
    assert!(output[1].contains("numbers differ"));
    assert!(output[1].contains("expected 4, actual 5"));

    let failure = execute("System.assert(false, 'expected failure');").unwrap_err();
    assert_eq!(failure.exception_type.as_deref(), Some("AssertException"));
    assert!(failure.message.contains("expected failure"));
}

#[test]
fn discovers_filters_isolates_and_runs_tests_with_coverage_and_junit() {
    let root = test_project();
    let compilation = project::compile(&root).unwrap();
    let report = test_runner::run(
        &compilation,
        &TestOptions {
            filter: None,
            jobs: 4,
        },
    )
    .unwrap();

    assert_eq!(report.tests.len(), 3);
    assert_eq!(report.passed(), 2);
    assert_eq!(report.failed(), 1);
    assert_eq!(
        report
            .tests
            .iter()
            .map(|test| test.name.as_str())
            .collect::<Vec<_>>(),
        [
            "CalculatorTest.addsNegativeValues",
            "CalculatorTest.addsPositiveValues",
            "CalculatorTest.expectedFailure",
        ]
    );
    assert!(report.tests.iter().all(|test| test.output == ["setup"]));
    let failure = report.tests[2].failure.as_ref().unwrap();
    assert_eq!(failure.exception_type.as_deref(), Some("AssertException"));
    assert!(failure.message.contains("expected runner failure"));

    assert_eq!(report.coverage.files.len(), 1);
    assert_eq!(report.coverage.covered_lines, report.coverage.total_lines);
    assert_eq!(report.coverage.covered_branches, 2);
    assert_eq!(report.coverage.total_branches, 2);
    let junit = report.to_junit_xml();
    assert!(junit.contains("tests=\"3\" failures=\"1\""));
    assert!(junit.contains("classname=\"CalculatorTest\""));
    assert!(junit.contains("type=\"AssertException\""));

    let filtered = test_runner::run(
        &compilation,
        &TestOptions {
            filter: Some("CalculatorTest.addsPositiveValues".to_owned()),
            jobs: 2,
        },
    )
    .unwrap();
    assert_eq!(filtered.tests.len(), 1);
    assert!(filtered.is_success());
    assert_eq!(filtered.coverage.covered_branches, 1);
    assert_eq!(filtered.coverage.total_branches, 2);

    let no_match = test_runner::run(
        &compilation,
        &TestOptions {
            filter: Some("MissingTest".to_owned()),
            jobs: 1,
        },
    )
    .unwrap_err();
    assert_eq!(no_match, "no Apex tests matched filter `MissingTest`");

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn test_cli_runs_a_filtered_test_and_writes_junit() {
    let root = test_project();
    let junit = root.join("test-results.xml");
    let output = Command::new(env!("CARGO_BIN_EXE_apex-exec"))
        .args([
            "test",
            root.to_str().unwrap(),
            "addsPositiveValues",
            "--jobs",
            "2",
            "--junit",
            junit.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("PASS CalculatorTest.addsPositiveValues"));
    assert!(stdout.contains("Summary: 1 passed, 0 failed, 1 total"));
    assert!(stdout.contains("1/2 branches (50.00%)"));
    assert!(output.stderr.is_empty());
    let xml = fs::read_to_string(&junit).unwrap();
    assert!(xml.contains("tests=\"1\" failures=\"0\""));

    let failing = Command::new(env!("CARGO_BIN_EXE_apex-exec"))
        .args(["test", root.to_str().unwrap(), "expectedFailure"])
        .output()
        .unwrap();
    assert!(!failing.status.success());
    assert!(
        String::from_utf8(failing.stdout)
            .unwrap()
            .contains("FAIL CalculatorTest.expectedFailure: AssertException")
    );
    assert!(failing.stderr.is_empty());

    fs::remove_dir_all(root).unwrap();
}

fn test_project() -> PathBuf {
    let unique = format!(
        "apex-exec-m6-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let root = std::env::temp_dir().join(unique);
    let classes = root.join("force-app/main/default/classes");
    fs::create_dir_all(&classes).unwrap();
    fs::write(
        root.join("sfdx-project.json"),
        r#"{"packageDirectories":[{"path":"force-app","default":true}]}"#,
    )
    .unwrap();
    fs::write(
        classes.join("Calculator.cls"),
        "public class Calculator {
    public static Integer calls = 0;

    public static Integer add(Integer left, Integer right) {
        calls++;
        if (left < 0) {
            return right;
        } else {
            return left + right;
        }
    }
}",
    )
    .unwrap();
    fs::write(
        classes.join("CalculatorTest.cls"),
        "@IsTest
private class CalculatorTest {
    private static Integer setupRuns = 0;

    @TestSetup
    static void setupData() {
        setupRuns++;
        System.debug('setup');
    }

    @IsTest
    static void addsPositiveValues() {
        System.assertEquals(1, setupRuns, 'setup must run once in this test');
        System.assertEquals(0, Calculator.calls, 'production static state must be isolated');
        System.assertEquals(5, Calculator.add(2, 3));
    }

    @IsTest
    static void addsNegativeValues() {
        System.assertEquals(1, setupRuns, 'setup must run once in this test');
        System.assertEquals(0, Calculator.calls, 'production static state must be isolated');
        System.assertEquals(3, Calculator.add(-2, 3));
    }

    @IsTest
    static void expectedFailure() {
        System.assert(false, 'expected runner failure');
    }
}",
    )
    .unwrap();
    root
}
