use apex_exec::{check, execute, parse};

#[test]
fn executes_milestone_two_exit_program() {
    let source = r#"
        Integer total = 0;
        for (Integer i = 0; i < 10; i++) {
            total = total + i;
        }
        System.debug(total);
    "#;
    assert_eq!(execute(source).unwrap(), ["45"]);
}

#[test]
fn applies_arithmetic_precedence_associativity_and_unary_operators() {
    let source = r#"
        Integer precedence = 2 + 3 * 4;
        Integer grouped = (2 + 3) * 4;
        Integer division = 20 / 3;
        Integer remainder = 20 % 3;
        Integer leftAssociative = 10 - 3 - 2;
        Integer unary = -5 + +2;
        System.debug(precedence);
        System.debug(grouped);
        System.debug(division);
        System.debug(remainder);
        System.debug(leftAssociative);
        System.debug(unary);
    "#;
    assert_eq!(execute(source).unwrap(), ["14", "20", "6", "2", "5", "-3"]);
}

#[test]
fn evaluates_comparison_equality_and_short_circuit_boolean_operators() {
    let source = r#"
        Integer mutations = 0;
        Boolean comparisons = 1 < 2 && 2 <= 2 && 3 > 2 && 3 >= 3;
        Boolean equality = 'same' == 'same' && true != false && null == null;
        Boolean skippedAnd = false && ++mutations > 0;
        Boolean skippedOr = true || ++mutations > 0;
        System.debug(comparisons);
        System.debug(equality);
        System.debug(!skippedAnd);
        System.debug(skippedOr);
        System.debug(mutations);
    "#;
    assert_eq!(
        execute(source).unwrap(),
        ["true", "true", "true", "true", "0"]
    );
}

#[test]
fn concatenates_strings_with_supported_primitive_and_null_values() {
    let source = r#"
        String missing = null;
        String message = 'count=' + 2 + ', enabled=' + true + ', missing=' + null;
        String typedNull = missing + 1;
        System.debug(message);
        System.debug(typedNull);
    "#;
    assert_eq!(
        execute(source).unwrap(),
        ["count=2, enabled=true, missing=null", "null1"]
    );
}

#[test]
fn implements_prefix_and_postfix_increment_and_decrement() {
    let source = r#"
        Integer value = 1;
        System.debug(value++);
        System.debug(++value);
        System.debug(value--);
        System.debug(--value);
    "#;
    assert_eq!(execute(source).unwrap(), ["1", "3", "3", "1"]);
}

#[test]
fn blocks_create_nested_case_insensitive_scopes() {
    let source = r#"
        Integer value = 1;
        {
            Integer VALUE = 2;
            value++;
            System.debug(Value);
        }
        System.debug(VALUE);
    "#;
    assert_eq!(execute(source).unwrap(), ["3", "1"]);

    let error = check("{ Integer local = 1; } System.debug(local);").unwrap_err();
    assert_eq!(error.message, "unknown variable `local`");
}

#[test]
fn executes_if_else_while_and_do_while() {
    let source = r#"
        Integer counter = 0;
        Integer total = 0;
        while (counter < 3) {
            total = total + counter;
            counter++;
        }
        do {
            total++;
        } while (false);
        if (total == 4) {
            System.debug('matched');
        } else {
            System.debug('wrong');
        }
    "#;
    assert_eq!(execute(source).unwrap(), ["matched"]);
}

#[test]
fn break_and_continue_target_the_nearest_loop() {
    let source = r#"
        Integer total = 0;
        for (Integer i = 0; i < 10; i++) {
            if (i == 3) continue;
            if (i == 6) break;
            total = total + i;
        }
        System.debug(total);
    "#;
    assert_eq!(execute(source).unwrap(), ["12"]);
}

#[test]
fn supports_for_loops_with_omitted_clauses() {
    let source = r#"
        Integer count = 0;
        for (;;) {
            count++;
            if (count == 2) break;
        }
        System.debug(count);
    "#;
    assert_eq!(execute(source).unwrap(), ["2"]);
}

#[test]
fn return_stops_anonymous_execution() {
    let source = r#"
        System.debug('before');
        if (true) {
            return;
        }
        System.debug('after');
    "#;
    assert_eq!(execute(source).unwrap(), ["before"]);

    let error = check("return 1;").unwrap_err();
    assert_eq!(
        error.message,
        "anonymous execution does not support returning a value"
    );
}

#[test]
fn null_is_assignable_to_all_supported_primitive_types() {
    let source = r#"
        String text = null;
        Boolean flag = null;
        Integer number = null;
        System.debug(text);
        System.debug(flag);
        System.debug(number);
    "#;
    assert_eq!(execute(source).unwrap(), ["null", "null", "null"]);
}

#[test]
fn assignment_is_right_associative_and_handles_type_named_variables() {
    let source = r#"
        Integer left = 0;
        Integer Integer = 1;
        left = integer = 4;
        System.debug(left);
        System.debug(INTEGER);
    "#;
    assert_eq!(execute(source).unwrap(), ["4", "4"]);
}

#[test]
fn rejects_invalid_operand_and_condition_types() {
    let error = check("Boolean value = 1 && true;").unwrap_err();
    assert_eq!(
        error.message,
        "operator `&&` cannot be applied to Integer and Boolean"
    );

    let error = check("if (1) { System.debug('no'); }").unwrap_err();
    assert_eq!(error.message, "expected Boolean, found Integer");

    let error = check("Integer value = true + false;").unwrap_err();
    assert_eq!(
        error.message,
        "operator `+` cannot be applied to Boolean and Boolean"
    );
}

#[test]
fn rejects_invalid_increment_and_statement_expressions() {
    let error = check("String value = 'x'; value++;").unwrap_err();
    assert_eq!(
        error.message,
        "increment/decrement requires Integer, found String"
    );

    let error = check("Integer value = 1; (value + 1)++;").unwrap_err();
    assert_eq!(
        error.message,
        "increment/decrement operand must be a variable"
    );

    let error = check("1 + 2;").unwrap_err();
    assert_eq!(
        error.message,
        "only assignment and increment/decrement expressions may be statements"
    );
}

#[test]
fn rejects_loop_control_outside_a_loop() {
    let break_error = check("break;").unwrap_err();
    assert_eq!(break_error.message, "`break` is only valid inside a loop");

    let continue_error = check("continue;").unwrap_err();
    assert_eq!(
        continue_error.message,
        "`continue` is only valid inside a loop"
    );
}

#[test]
fn reports_runtime_arithmetic_failures() {
    let division_error = execute("Integer value = 1 / 0;").unwrap_err();
    assert_eq!(division_error.message, "division by zero");

    let remainder_error = execute("Integer value = 1 % 0;").unwrap_err();
    assert_eq!(remainder_error.message, "remainder by zero");

    let null_error = execute("Integer value = null; Integer result = value + 1;").unwrap_err();
    assert_eq!(
        null_error.message,
        "operator cannot be applied to null at runtime"
    );
}

#[test]
fn loop_variables_do_not_escape_the_for_scope() {
    let error =
        check("for (Integer index = 0; index < 1; index++) {} System.debug(index);").unwrap_err();
    assert_eq!(error.message, "unknown variable `index`");
}

#[test]
fn nested_loop_control_is_consumed_by_the_nearest_loop() {
    let source = r#"
        Integer outer = 0;
        Integer visits = 0;
        while (outer < 3) {
            outer++;
            Integer inner = 0;
            while (true) {
                inner++;
                if (inner == 2) break;
                visits++;
            }
        }
        System.debug(visits);
    "#;
    assert_eq!(execute(source).unwrap(), ["3"]);
}

#[test]
fn rejects_invalid_assignment_targets_during_parsing() {
    let error = parse("Integer value = 1; (value + 1) = 2;").unwrap_err();
    assert_eq!(error.message, "invalid assignment target");
}
