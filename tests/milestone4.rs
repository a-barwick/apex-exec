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

        String identifyError(Exception value) {
            return 'Exception overload';
        }

        String identifyError(Object value) {
            return 'Object overload';
        }

        Object boxedString = 'text';
        System.debug(identify('text'));
        System.debug(identify(boxedString));
        System.debug(identify(42));
        System.debug(identify(null));
        System.debug(identifyError(new IllegalArgumentException()));
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

    let unrelated_null = r#"
        String identify(String value) { return 'String'; }
        String identify(Exception value) { return 'Exception'; }
        System.debug(identify(null));
    "#;
    assert_check_error_contains(unrelated_null, &["ambiguous", "identify"]);

    let crossing = r#"
        String identify(Object left, MathException right) { return 'first'; }
        String identify(Exception left, Object right) { return 'second'; }
        MathException error = new MathException();
        System.debug(identify(error, error));
    "#;
    assert_check_error_contains(crossing, &["ambiguous", "identify"]);
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

        Integer mandatoryDoReturn() {
            do {
                return 7;
            } while (false);
        }

        emit(String.valueOf(twice(4)));
        emit(String.valueOf(mandatoryDoReturn()));
    "#;

    assert_eq!(execute(source).unwrap(), ["8", "7"]);
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
    assert_check_error_contains("void emit(Integer void) {}", &["parameter name"]);
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
fn generic_catches_preserve_dynamic_type_and_rethrow_state() {
    let source = r#"
        void reroute() {
            try {
                Integer impossible = 1 / 0;
            } catch (ListException wrongType) {
                System.debug('unreachable');
            } catch (Exception error) {
                System.debug(error.getTypeName());
                throw error;
            } finally {
                System.debug('inner finally');
            }
        }

        try {
            reroute();
        } catch (Exception error) {
            System.debug(error.getMessage());
            System.debug(error.getStackTraceString().contains('reroute'));
        } finally {
            System.debug('outer finally');
        }
    "#;

    assert_eq!(
        execute(source).unwrap(),
        [
            "MathException",
            "inner finally",
            "division by zero",
            "true",
            "outer finally"
        ]
    );
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

    assert_eq!(
        execute("Exception empty = new Exception(null); System.debug(empty.getMessage());")
            .unwrap(),
        [""]
    );
    assert_check_error_contains(
        "throw new Exception('first', 'second');",
        &["zero or one", "argument"],
    );
}

#[test]
fn null_dereferences_are_promoted_to_null_pointer_exception() {
    assert_runtime_exception(
        "String text = null; System.debug(text.length());",
        "NullPointerException",
    );
}

#[test]
fn nullable_primitive_operations_are_catchable_null_pointer_exceptions() {
    let source = r#"
        try {
            Boolean flag = null;
            if (flag) System.debug('unreachable');
        } catch (NullPointerException error) {
            System.debug(error.getTypeName());
        }

        try {
            Integer number = null;
            Integer result = +number;
        } catch (NullPointerException error) {
            System.debug(error.getTypeName());
        }
    "#;

    assert_eq!(
        execute(source).unwrap(),
        ["NullPointerException", "NullPointerException"]
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
fn mutation_during_iteration_is_a_catchable_final_exception() {
    let source = r#"
        List<Integer> values = new List<Integer>{1, 2};
        try {
            for (Integer value : values) {
                values.add(3);
            }
        } catch (FinalException error) {
            System.debug(error.getTypeName());
        }
    "#;

    assert_eq!(execute(source).unwrap(), ["FinalException"]);
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

    assert_check_error_contains(
        "MathException math = new MathException(); \
         IllegalArgumentException unrelated = (IllegalArgumentException) math;",
        &["cannot cast", "MathException", "IllegalArgumentException"],
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

    let division = source.find("/").unwrap();
    let explode_call = source.find("explode(denominator)").unwrap();
    let middle_call = source.find("middle(0)").unwrap();
    assert_eq!(error.stack_trace[0].span, Span::new(division, division + 1));
    assert_eq!(
        error.stack_trace[1].span,
        Span::new(explode_call, explode_call + "explode(denominator)".len())
    );
    assert_eq!(
        error.stack_trace[2].span,
        Span::new(middle_call, middle_call + "middle(0)".len())
    );

    let rendered = error.render("stack.apex", source);
    assert!(rendered.contains("at explode (stack.apex:3:"));
    assert!(rendered.contains("at middle (stack.apex:7:"));
    assert!(rendered.contains("at outer (stack.apex:11:"));
}

#[test]
fn caught_exceptions_retain_their_originating_method_frame() {
    let source = r#"
        String caughtStack() {
            try {
                Integer impossible = 1 / 0;
                return 'unreachable';
            } catch (MathException error) {
                return error.getStackTraceString();
            }
        }

        System.debug(caughtStack().contains('caughtStack'));
    "#;

    assert_eq!(execute(source).unwrap(), ["true"]);
}
