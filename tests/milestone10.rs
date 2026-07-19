use apex_exec::{
    check, execute, project,
    runtime::{HttpResponseData, Interpreter, RecordingHost},
};
use std::collections::BTreeMap;

const EXAMPLE: &str = "examples/milestone10-project";

#[test]
fn core_platform_value_types_execute_end_to_end() {
    let output = execute(
        r#"
        Date dateValue = Date.newInstance(2024, 2, 29);
        Datetime timestamp = Datetime.valueOfGmt('2025-01-01 00:00:00');
        Time timeValue = Time.newInstance(23, 59, 59, 500);
        Decimal amount = 1.25 + 2;
        Id recordId = Id.valueOf('001000000000001');
        Blob bytes = Blob.valueOf('abc');

        System.debug(dateValue.addYears(1).format());
        System.debug(timestamp.addMinutes(90).format());
        System.debug(timeValue.addMilliseconds(500).format());
        System.debug(amount.setScale(2));
        System.debug(recordId.to18());
        System.debug(EncodingUtil.base64Encode(bytes));
        "#,
    )
    .unwrap();

    assert_eq!(
        output,
        [
            "2025-02-28",
            "2025-01-01 01:30:00",
            "00:00:00.000",
            "3.25",
            "001000000000001AAA",
            "YWJj",
        ]
    );
}

#[test]
fn json_regex_and_schema_describe_cover_nested_runtime_values() {
    let compilation = project::compile(EXAMPLE).unwrap();
    let output = compilation.invoke("PlatformDemo.run").unwrap();
    assert_eq!(
        output,
        ["2026-07-18 | 2026-07-17 10:00:00 | 10 | 12.25 | bWlsZXN0b25lLTEw | 10 | true | BYg"]
    );
}

#[test]
fn callouts_are_host_mocked_and_limit_counted() {
    let checked = check(
        r#"
        HttpRequest request = new HttpRequest();
        request.setEndpoint('https://example.test/v1');
        request.setMethod('POST');
        request.setHeader('Content-Type', 'application/json');
        request.setBody('{"ok":true}');
        HttpResponse response = new Http().send(request);
        System.debug(response.getStatusCode());
        System.debug(response.getHeader('x-trace'));
        System.debug(response.getBody());
        System.debug(Limits.getCallouts());
        "#,
    )
    .unwrap();

    let mut headers = BTreeMap::new();
    headers.insert("x-trace".to_owned(), "deterministic".to_owned());
    let mut host = RecordingHost::default();
    host.enqueue_http_response(HttpResponseData {
        status_code: 202,
        status: "Accepted".to_owned(),
        body: "{\"queued\":true}".to_owned(),
        headers,
    });
    let output = Interpreter::with_host(&mut host).execute(&checked).unwrap();

    assert_eq!(output, ["202", "deterministic", "{\"queued\":true}", "1"]);
    assert_eq!(host.callout_requests().len(), 1);
    assert_eq!(
        host.callout_requests()[0].endpoint,
        "https://example.test/v1"
    );
    assert_eq!(host.callout_requests()[0].method, "POST");
    assert_eq!(
        host.callout_requests()[0].headers["content-type"],
        "application/json"
    );
}

#[test]
fn deterministic_services_are_configurable_at_the_host_boundary() {
    let checked = check(
        r#"
        System.debug(System.currentTimeMillis());
        System.debug(System.now());
        System.debug(Date.today());
        System.debug(UserInfo.getUserName());
        System.debug(Math.random());
        "#,
    )
    .unwrap();
    let mut host = RecordingHost::default();
    host.set_now_millis(0);
    host.set_user_context(apex_exec::runtime::UserContext {
        user_id: "005000000000001AAA".to_owned(),
        username: "configured@example.test".to_owned(),
    });

    let output = Interpreter::with_host(&mut host).execute(&checked).unwrap();
    assert_eq!(output[0], "0");
    assert_eq!(output[1], "1970-01-01 00:00:00");
    assert_eq!(output[2], "1970-01-01");
    assert_eq!(output[3], "configured@example.test");
    assert!(output[4].starts_with("0."));
}

#[test]
fn unsupported_platform_apis_name_the_profile() {
    let error = check("Date value = Date.parse('tomorrow');").unwrap_err();
    assert_eq!(
        error.message,
        "unsupported API `Date.parse` in compatibility profile `salesforce-api-66.0`"
    );
}

#[test]
fn invalid_platform_inputs_are_typed_runtime_failures() {
    let error = execute("Date value = Date.valueOf('2025-02-29');").unwrap_err();
    assert_eq!(
        error.exception_type.as_deref(),
        Some("IllegalArgumentException")
    );
    assert!(error.message.contains("invalid Date"));

    let error = execute("Id value = Id.valueOf('not-an-id');").unwrap_err();
    assert_eq!(
        error.exception_type.as_deref(),
        Some("IllegalArgumentException")
    );
    assert!(error.message.contains("Salesforce ID"));

    let error = execute("Pattern value = Pattern.compile('[');").unwrap_err();
    assert_eq!(
        error.exception_type.as_deref(),
        Some("IllegalArgumentException")
    );
    assert!(error.message.contains("invalid regex"));
}

#[test]
fn platform_object_accessors_and_unconfigured_callouts_are_explicit() {
    let output = execute(
        r#"
        HttpRequest request = new HttpRequest();
        request.setEndpoint('callout:Local/test');
        request.setMethod('PATCH');
        request.setTimeout(2500);
        request.setHeader('X-Mode', 'local');
        System.debug(request.getEndpoint());
        System.debug(request.getMethod());
        System.debug(request.getTimeout());
        System.debug(request.getHeader('x-mode'));

        HttpResponse response = new HttpResponse();
        response.setStatusCode(204);
        response.setStatus('No Content');
        response.setBody('');
        response.setHeader('X-Result', 'empty');
        System.debug(response.getStatusCode());
        System.debug(response.getStatus());
        System.debug(response.getBody());
        System.debug(response.getHeader('x-result'));

        Object boxed = 42;
        System.debug(boxed.toString());
        "#,
    )
    .unwrap();
    assert_eq!(
        output,
        [
            "callout:Local/test",
            "PATCH",
            "2500",
            "local",
            "204",
            "No Content",
            "",
            "empty",
            "42",
        ]
    );

    let error = execute(
        r#"
        HttpRequest request = new HttpRequest();
        request.setEndpoint('https://example.test');
        HttpResponse response = new Http().send(request);
        "#,
    )
    .unwrap_err();
    assert_eq!(error.exception_type.as_deref(), Some("CalloutException"));
    assert!(error.message.contains("salesforce-api-66.0"));
    assert!(error.message.contains("no configured mock"));
}

#[test]
fn json_and_regex_errors_remain_catchable_platform_failures() {
    let output = execute(
        r#"
        try {
            Object value = JSON.deserializeUntyped('{bad json}');
        } catch (IllegalArgumentException error) {
            System.debug(error.getTypeName());
        }
        try {
            Pattern value = Pattern.compile('(');
        } catch (IllegalArgumentException error) {
            System.debug(error.getMessage().contains('invalid regex'));
        }
        "#,
    )
    .unwrap();
    assert_eq!(output, ["IllegalArgumentException", "true"]);
}

#[test]
fn unsupported_instance_apis_name_the_owner_and_profile() {
    let error =
        check("Datetime value = Datetime.now(); String zone = value.formatGmt('z');").unwrap_err();
    assert_eq!(
        error.message,
        "unsupported API `Datetime.formatGmt` in compatibility profile `salesforce-api-66.0`"
    );
}

#[test]
fn milestone10_example_tests_pass_with_full_production_coverage() {
    let compilation = project::compile(EXAMPLE).unwrap();
    let report = apex_exec::test_runner::run(
        &compilation,
        &apex_exec::test_runner::TestOptions::default(),
    )
    .unwrap();

    assert!(report.is_success(), "{}", report.render_console());
    assert_eq!(report.tests.len(), 4);
    assert_eq!(report.coverage.covered_lines, report.coverage.total_lines);
    assert_eq!(
        report.coverage.covered_branches,
        report.coverage.total_branches
    );
}
