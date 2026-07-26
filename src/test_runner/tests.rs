use super::{
    CoverageReport, TestReport, TestResult,
    filtering::{TestCase, matches_filter},
    partition_cases,
};
use crate::hir::ClassMemberId;

#[test]
fn wildcard_filters_are_case_insensitive_and_anchored() {
    assert!(matches_filter(
        Some("account*test.*merge*"),
        "AccountServiceTest",
        "shouldMerge",
        "AccountServiceTest.shouldMerge"
    ));
    assert!(!matches_filter(
        Some("service*"),
        "AccountService",
        "works",
        "AccountService.works"
    ));
    assert!(matches_filter(
        Some("ACCOUNTSERVICETEST"),
        "AccountServiceTest",
        "works",
        "AccountServiceTest.works"
    ));
}

#[test]
fn escapes_junit_xml_text() {
    let report = TestReport {
        tests: vec![TestResult {
            name: "Example.works".to_owned(),
            class_name: "a<&\"'".to_owned(),
            method_name: "works".to_owned(),
            output: Vec::new(),
            failure: None,
        }],
        coverage: CoverageReport::default(),
    };

    assert!(
        report
            .to_junit_xml()
            .contains("classname=\"a&lt;&amp;&quot;&apos;\"")
    );
}

#[test]
fn parallel_partition_is_single_pass_complete_and_stable() {
    fn test_case(name: &str, allows_parallel: bool, member_id: usize) -> TestCase {
        TestCase {
            name: name.to_owned(),
            class_name: "SchedulingTest".to_owned(),
            method_name: name.to_owned(),
            target: ClassMemberId {
                class_id: 0,
                member_id,
            },
            setup_methods: Vec::new(),
            allows_parallel,
        }
    }

    let cases = [
        test_case("firstSerial", false, 0),
        test_case("firstParallel", true, 1),
        test_case("secondSerial", false, 2),
        test_case("secondParallel", true, 3),
    ];
    let (serial, parallel) = partition_cases(&cases);
    assert_eq!(
        serial
            .iter()
            .map(|case| case.name.as_str())
            .collect::<Vec<_>>(),
        ["firstSerial", "secondSerial"]
    );
    assert_eq!(
        parallel
            .iter()
            .map(|case| case.name.as_str())
            .collect::<Vec<_>>(),
        ["firstParallel", "secondParallel"]
    );
    let mut targets = serial
        .iter()
        .chain(&parallel)
        .map(|case| case.target.member_id)
        .collect::<Vec<_>>();
    targets.sort_unstable();
    assert_eq!(targets, [0, 1, 2, 3]);
}
