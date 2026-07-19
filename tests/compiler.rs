use apex_exec::{check, execute, parse, tokenize};

#[test]
fn executes_string_variable_and_debug() {
    let output = execute("String message = 'Hello, world!'; System.debug(message);").unwrap();
    assert_eq!(output, ["Hello, world!"]);
}

#[test]
fn supports_boolean_integer_assignment_and_case_insensitivity() {
    let source = "bOoLeAn FLAG = true; INTEGER Count = 41; count = 42; SYSTEM.DEBUG(flag); System.Debug(COUNT);";
    let output = execute(source).unwrap();
    assert_eq!(output, ["true", "42"]);
}

#[test]
fn rejects_double_quoted_strings() {
    let error = tokenize("String value = \"no\";").unwrap_err();
    assert_eq!(error.message, "Apex string literals must use single quotes");
}

#[test]
fn uninitialized_locals_receive_typed_null() {
    let source = "String value; System.debug(value);";
    assert_eq!(parse(source).unwrap().statements.len(), 2);
    assert_eq!(execute(source).unwrap(), ["null"]);
}

#[test]
fn rejects_unknown_variables_at_compile_time() {
    let error = check("String value = 'yes'; System.debug(vlaue);").unwrap_err();
    assert_eq!(error.message, "unknown variable `vlaue`");
}

#[test]
fn rejects_type_mismatch_at_compile_time() {
    let error = check("Integer value = 'wrong';").unwrap_err();
    assert_eq!(error.message, "cannot assign String to Integer");
}

#[test]
fn supports_comments_and_apex_string_escapes() {
    let source = "/* setup */ String value = 'it\\'s valid'; // output\nSystem.debug(value);";
    assert_eq!(execute(source).unwrap(), ["it's valid"]);
}

#[test]
fn all_public_compiler_stages_accept_the_milestone_program() {
    let source = "String message = 'Hello!'; System.debug(message);";
    assert!(!tokenize(source).unwrap().is_empty());
    assert_eq!(parse(source).unwrap().statements.len(), 2);
    assert_eq!(check(source).unwrap().statements.len(), 2);
}
