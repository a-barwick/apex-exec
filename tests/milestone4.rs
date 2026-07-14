use apex_exec::{check, execute, span::Span};

const CORE_SAMPLE: &str = include_str!("../examples/methods-exceptions.apex");

fn assert_check_error_contains(source: &str, expected_fragments: &[&str]) {
    let error = check(source).unwrap_err();
    let message = error.message.to_ascii_lowercase();
    for fragment in expected_fragments {
        assert!(
            message.contains(&fragment.to_ascii_lowercase()),
            "expected checker error `{}` to contain `{fragment}`",
            error.message
        );
    }
}

fn assert_runtime_exception(source: &str, expected_type: &str) {
    let error = execute(source).unwrap_err();
    assert_eq!(error.exception_type.as_deref(), Some(expected_type));
    assert!(
        !error.message.is_empty(),
        "runtime exception needs a message"
    );
}

#[test]
fn executes_the_milestone_four_core_sample() {
    check(CORE_SAMPLE).unwrap();
    assert_eq!(
        execute(CORE_SAMPLE).unwrap(),
        [
            "120",
            "String",
            "Object",
            "division finished",
            "quotient=4",
            "division finished",
            "division failed",
        ]
    );
}

#[test]
fn method_names_are_case_insensitive_and_forward_calls_are_hoisted() {
    let source = r#"
        String first(String value) {
            return SeCoNd(value);
        }

        String second(String value) {
            return value.toUpperCase();
        }

        System.debug(FiRsT('ready'));
    "#;

    assert_eq!(execute(source).unwrap(), ["READY"]);
}

#[test]
fn overload_resolution_prefers_an_exact_type_over_object() {
    let source = r#"
        String identify(String value) {
            return 'String overload';
        }

        String identify(Object value) {
            return 'Object overload';
        }

        String identify(Exception value) {
            return 'Exception overload';
        }

        Object boxedString = 'text';
        System.debug(identify('text'));
        System.debug(identify(boxedString));
        System.debug(identify(42));
        System.debug(identify(null));
        System.debug(identify(new IllegalArgumentException()));
    "#;

    assert_eq!(
        execute(source).unwrap(),
        [
            "String overload",
            "Object overload",
            "Object overload",
            "String overload",
            "Exception overload"
        ]
    );
}

#[test]
fn overload_resolution_rejects_an_ambiguous_null_argument() {
    let source = r#"
        String identify(String value) {
            return 'String';
        }

        String identify(Integer value) {
            return 'Integer';
        }

        System.debug(identify(null));
    "#;

    assert_check_error_contains(source, &["ambiguous", "identify"]);
}

#[test]
fn user_defined_methods_support_recursion() {
    let source = r#"
        Integer factorial(Integer value) {
            if (value <= 1) {
                return 1;
            }
            return value * factorial(value - 1);
        }

        System.debug(factorial(6));
    "#;

    assert_eq!(execute(source).unwrap(), ["720"]);
}

#[test]
fn typed_and_void_methods_return_to_their_callers() {
    let source = r#"
        Integer twice(Integer value) {
            return value * 2;
        }

        void emit(String value) {
            System.debug(value);
            return;
        }

        emit(String.valueOf(twice(4)));
    "#;

    assert_eq!(execute(source).unwrap(), ["8"]);
}

#[test]
fn checker_rejects_invalid_and_missing_method_returns() {
    assert_check_error_contains(
        "Integer wrong() { return 'text'; }",
        &["return", "string", "integer"],
    );
    assert_check_error_contains("void wrong() { return 1; }", &["void", "return"]);
    assert_check_error_contains(
        "Integer incomplete(Boolean branch) { if (branch) { return 1; } }",
        &["return"],
    );
}

#[test]
fn try_catch_and_finally_preserve_exception_messages_and_order() {
    let source = r#"
        String recover() {
            try {
                throw new IllegalArgumentException('bad input');
            } catch (IllegalArgumentException error) {
                System.debug(error.getMessage());
                return 'caught';
            } finally {
                System.debug('finally');
            }
        }

        System.debug(recover());
    "#;

    assert_eq!(execute(source).unwrap(), ["bad input", "finally", "caught"]);
}

#[test]
fn finally_runs_while_a_return_value_unwinds() {
    let source = r#"
        Integer answer() {
            try {
                return 7;
            } finally {
                System.debug('finally');
            }
        }

        System.debug(answer());
    "#;

    assert_eq!(execute(source).unwrap(), ["finally", "7"]);
}

#[test]
fn abrupt_completion_in_finally_overrides_a_pending_return() {
    let source = r#"
        Integer answer() {
            try {
                return 1;
            } finally {
                return 2;
            }
        }

        System.debug(answer());
    "#;

    assert_eq!(execute(source).unwrap(), ["2"]);
}

#[test]
fn finally_runs_during_break_and_continue_unwinding() {
    let source = r#"
        Integer loopWithCleanup() {
            Integer total = 0;
            for (Integer index = 0; index < 4; index++) {
                try {
                    if (index == 0) continue;
                    if (index == 2) break;
                    total = total + index;
                } finally {
                    total = total + 10;
                }
            }
            return total;
        }

        System.debug(loopWithCleanup());
    "#;

    assert_eq!(execute(source).unwrap(), ["31"]);
}

#[test]
fn explicit_core_exception_construction_preserves_type_and_message() {
    let source = r#"
        void fail() {
            throw new IllegalArgumentException('bad input');
        }

        fail();
    "#;

    let error = execute(source).unwrap_err();
    assert_eq!(
        error.exception_type.as_deref(),
        Some("IllegalArgumentException")
    );
    assert_eq!(error.message, "bad input");
    assert_eq!(error.stack_trace[0].method, "fail");
}

#[test]
fn null_dereferences_are_promoted_to_null_pointer_exception() {
    assert_runtime_exception(
        "String text = null; System.debug(text.length());",
        "NullPointerException",
    );
}

#[test]
fn list_bounds_failures_are_promoted_to_list_exception() {
    assert_runtime_exception(
        "List<Integer> values = new List<Integer>{1}; System.debug(values[1]);",
        "ListException",
    );
}

#[test]
fn arithmetic_failures_are_promoted_to_math_exception() {
    assert_runtime_exception("Integer value = 10 / 0;", "MathException");
}

#[test]
fn object_downcasts_succeed_or_raise_type_exception() {
    let valid = r#"
        Object boxed = 'Apex';
        String text = (String) boxed;
        System.debug(text.toUpperCase());
    "#;
    assert_eq!(execute(valid).unwrap(), ["APEX"]);

    assert_runtime_exception(
        "Object boxed = 42; String text = (String) boxed;",
        "TypeException",
    );
}

#[test]
fn unhandled_runtime_exceptions_include_source_mapped_method_frames() {
    let source = r#"
        Integer explode(Integer denominator) {
            return 10 / denominator;
        }

        Integer middle(Integer denominator) {
            return explode(denominator);
        }

        Integer outer() {
            return middle(0);
        }

        System.debug(outer());
    "#;

    let error = execute(source).unwrap_err();
    assert_eq!(error.exception_type.as_deref(), Some("MathException"));

    let methods = error
        .stack_trace
        .iter()
        .map(|frame| frame.method.as_str())
        .collect::<Vec<_>>();
    assert!(
        methods.starts_with(&["explode", "middle", "outer"]),
        "expected innermost-to-outermost frames, found {methods:?}"
    );

    for frame in error.stack_trace.iter().take(3) {
        assert_ne!(frame.span, Span::new(0, 0));
        assert!(frame.span.start < frame.span.end);
        assert!(frame.span.end <= source.len());
    }
}
