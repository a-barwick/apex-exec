use apex_exec::ast::{AnnotationKind, ClassMember};
use apex_exec::{check, execute, parse};

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
    assert_eq!(unsupported.message, "unsupported annotation `@Future`");

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
