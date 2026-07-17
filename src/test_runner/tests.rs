use super::{CoverageReport, TestReport, TestResult, filtering::matches_filter};

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
