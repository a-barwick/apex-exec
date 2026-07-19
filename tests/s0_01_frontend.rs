use apex_exec::{
    ast::{
        BinaryOperator, CollectionInitializer, Expression, NamedType, Statement, TypeName,
        UnaryOperator,
    },
    check, execute, parse,
    parser::{Parser, TokenStreamErrorKind},
    semantic,
    span::{SourceId, Span},
    token::TokenKind,
    tokenize,
};
use std::{fs, path::PathBuf, process::Command};

#[test]
fn public_parser_rejects_empty_and_missing_eof_streams() {
    let empty = Parser::new(Vec::new()).unwrap_err();
    assert_eq!(empty.kind, TokenStreamErrorKind::Empty);
    assert_eq!(empty.offending_span, None);

    let mut missing_eof = tokenize("Integer value = 1;").unwrap();
    let last_source_span = missing_eof[missing_eof.len() - 2].span;
    missing_eof.pop();
    let error = Parser::new(missing_eof).unwrap_err();
    assert_eq!(error.kind, TokenStreamErrorKind::MissingEof);
    assert_eq!(error.offending_span, Some(last_source_span));
}

#[test]
fn public_parser_rejects_interior_or_trailing_eof_tokens() {
    let mut interior_eof = tokenize("Integer kept = 1; Integer ignored = 2;").unwrap();
    let semicolon = interior_eof
        .iter()
        .position(|token| token.kind == TokenKind::Semicolon)
        .unwrap();
    let interior_span = interior_eof[semicolon + 1].span;
    interior_eof[semicolon + 1].kind = TokenKind::Eof;
    let error = Parser::new(interior_eof).unwrap_err();
    assert_eq!(error.kind, TokenStreamErrorKind::InteriorEof);
    assert_eq!(error.offending_span, Some(interior_span));

    let mut trailing_after_eof = tokenize("Integer value = 1;").unwrap();
    let eof_span = trailing_after_eof.last().unwrap().span;
    let mut trailing = trailing_after_eof[0].clone();
    trailing.span = eof_span;
    trailing_after_eof.push(trailing);
    let error = Parser::new(trailing_after_eof).unwrap_err();
    assert_eq!(error.kind, TokenStreamErrorKind::InteriorEof);
}

#[test]
fn public_parser_rejects_mixed_source_and_invalid_span_streams() {
    let mut mixed_source = tokenize("Integer value = 1 + 2;").unwrap();
    mixed_source[1].span = Span::new_in(SourceId::new(99), 8, 13);
    let error = Parser::new(mixed_source).unwrap_err();
    assert_eq!(error.kind, TokenStreamErrorKind::MixedSource);
    assert_eq!(
        error.offending_span,
        Some(Span::new_in(SourceId::new(99), 8, 13))
    );

    let mut reversed = tokenize("Integer value = 1;").unwrap();
    reversed[1].span = Span::new(13, 8);
    let error = Parser::new(reversed).unwrap_err();
    assert_eq!(error.kind, TokenStreamErrorKind::ReversedSpan);
    assert_eq!(error.offending_span, Some(Span::new(13, 8)));

    let mut non_monotonic = tokenize("Integer value = 1;").unwrap();
    non_monotonic[2].span = Span::new(7, 7);
    let error = Parser::new(non_monotonic).unwrap_err();
    assert_eq!(error.kind, TokenStreamErrorKind::NonMonotonicSpan);

    let mut overlapping = tokenize("Integer value = 1;").unwrap();
    overlapping[1].span = Span::new(6, 13);
    let error = Parser::new(overlapping).unwrap_err();
    assert_eq!(error.kind, TokenStreamErrorKind::OverlappingSpan);
}

#[test]
fn public_parser_accepts_valid_lexer_output() {
    let parser = Parser::new(tokenize("Integer value = 1;").unwrap()).unwrap();
    let program = parser.parse_program().unwrap();
    assert_eq!(program.statements.len(), 1);
}

#[test]
fn parser_groups_parenthesized_identifiers_and_preserves_interface_arguments() {
    let grouped = parse("Integer foo = 1; Integer bar = 2; Integer result = (foo) + bar;").unwrap();
    let Statement::VariableDeclaration { initializer, .. } = &grouped.statements[2] else {
        panic!("expected result declaration");
    };
    assert!(matches!(
        initializer,
        Expression::Binary {
            left,
            operator: BinaryOperator::Add,
            ..
        } if matches!(left.as_ref(), Expression::Variable(name) if name.canonical == "foo")
    ));

    let source = "\
        public class BatchWork implements Database.Batchable<Integer> {
            public List<Integer> start(Database.BatchableContext context) {
                return new List<Integer>();
            }
            public void execute(Database.BatchableContext context, List<Integer> scope) {}
            public void finish(Database.BatchableContext context) {}
        }";
    let program = parse(source).unwrap();
    let interface = &program.classes[0].interfaces[0];
    assert_eq!(interface.spelling, "Database.Batchable");
    let [argument] = interface.type_arguments.as_slice() else {
        panic!("expected one preserved Batchable type argument");
    };
    assert_eq!(argument.ty, TypeName::Integer);
    assert_eq!(&source[argument.span.start..argument.span.end], "Integer");
    assert_eq!(
        &source[interface.span.start..interface.span.end],
        "Database.Batchable<Integer>"
    );
}

#[test]
fn cast_group_disambiguation_survives_parse_check_execute_and_postfix_shapes() {
    let source = "\
        public class Box { public Integer value; }
        Box box = new Box();
        box.value = 1;
        Integer other = 2;
        Integer[] values = new Integer[] { 4 };
        Integer member = (box.value) + other;
        Integer indexed = (values)[0];
        (box.value)++;
        Integer plus = (other) + -other;
        Integer minus = (other) - +other;
        Object boxed = box;
        Box customCast = (Box) boxed;
        Integer signedCast = (Integer) -other;
        System.debug(member);
        System.debug(indexed);
        System.debug(box.value);
        System.debug(plus);
        System.debug(minus);
        System.debug(customCast.value);
        System.debug(signedCast);";

    let program = parse(source).unwrap();
    let Statement::VariableDeclaration { initializer, .. } = &program.statements[4] else {
        panic!("expected grouped member expression");
    };
    assert!(matches!(
        initializer,
        Expression::Binary {
            left,
            operator: BinaryOperator::Add,
            ..
        } if matches!(left.as_ref(), Expression::MemberAccess { .. })
    ));
    let Statement::VariableDeclaration { initializer, .. } = &program.statements[5] else {
        panic!("expected grouped index expression");
    };
    assert!(matches!(initializer, Expression::Index { .. }));
    let Statement::Expression { expression, .. } = &program.statements[6] else {
        panic!("expected grouped member postfix expression");
    };
    assert!(matches!(
        expression,
        Expression::Postfix { operand, .. }
            if matches!(operand.as_ref(), Expression::MemberAccess { .. })
    ));
    let Statement::VariableDeclaration { initializer, .. } = &program.statements[11] else {
        panic!("expected genuine signed core cast");
    };
    assert!(matches!(
        initializer,
        Expression::Cast {
            ty: TypeName::Integer,
            expression,
            ..
        } if matches!(
            expression.as_ref(),
            Expression::Unary {
                operator: UnaryOperator::Negate,
                ..
            }
        )
    ));

    check(source).unwrap();
    assert_eq!(
        execute(source).unwrap(),
        ["3", "4", "2", "0", "0", "2", "-2"]
    );
}

#[test]
fn parser_and_checker_accept_sized_arrays_across_supported_element_categories() {
    let source = "\
        public class Foo {}
        Foo[] customValues = new Foo[3];
        Object[] objectValues = new Object[3];
        Exception[] exceptionValues = new Exception[3];
        TypeException[] concreteExceptionValues = new TypeException[3];
        Schema.SObjectType objectType = null;
        Schema.DescribeSObjectResult describe = null;";

    let program = parse(source).unwrap();
    for statement in &program.statements[..4] {
        assert!(matches!(
            statement,
            Statement::VariableDeclaration {
                initializer: Expression::NewCollection {
                    initializer: CollectionInitializer::SizedArray(_),
                    ..
                },
                ..
            }
        ));
    }
    let Statement::VariableDeclaration { ty, .. } = &program.statements[4] else {
        panic!("expected qualified Schema.SObjectType declaration");
    };
    assert_eq!(ty, &TypeName::SObjectType);
    let Statement::VariableDeclaration { ty, .. } = &program.statements[5] else {
        panic!("expected qualified Schema.DescribeSObjectResult declaration");
    };
    assert_eq!(ty, &TypeName::DescribeSObjectResult);

    check(source).unwrap();
    let output = execute(
        "\
        public class Foo {}
        Foo[] customValues = new Foo[3];
        Object[] objectValues = new Object[2];
        Exception[] exceptionValues = new Exception[1];
        System.debug(customValues.size());
        System.debug(objectValues.size());
        System.debug(exceptionValues.size());",
    )
    .unwrap();
    assert_eq!(output, ["3", "2", "1"]);

    for invalid in [
        "Object values = new Missing[3];",
        "Object values = new List<Missing>();",
    ] {
        let error = check(invalid).unwrap_err();
        assert!(error.message.contains("unknown type `Missing`"));
    }
}

#[test]
fn parser_rejects_interface_implements_syntax_before_semantic_collection() {
    let source = "\
        public interface A implements B { void a(); }
        public interface B implements A { void b(); }";
    let error = parse(source).unwrap_err();
    assert!(error.message.contains("`implements` is invalid"));
    assert_eq!(&source[error.span.start..error.span.end], "implements");
}

#[test]
fn hierarchy_validation_covers_superclass_and_interface_edges_without_recursion() {
    let valid_cycle = "\
        public interface A extends B { void a(); }
        public interface B extends A { void b(); }
        public class C implements A {
            public void a() {}
            public void b() {}
        }";
    let error = check(valid_cycle).unwrap_err();
    assert!(error.message.contains("cyclic inheritance"));

    let mut raw_interface_edges =
        parse("public interface A { void a(); } public interface B { void b(); }").unwrap();
    let a_span = raw_interface_edges.classes[0].name.span;
    let b_span = raw_interface_edges.classes[1].name.span;
    raw_interface_edges.classes[0]
        .interfaces
        .push(NamedType::new("B".to_owned(), b_span));
    raw_interface_edges.classes[1]
        .interfaces
        .push(NamedType::new("A".to_owned(), a_span));
    let error = semantic::check(&raw_interface_edges).unwrap_err();
    assert!(error.message.contains("cyclic inheritance"));

    let mut deep = String::from("public interface I0 {}");
    for index in 1..1024 {
        deep.push_str(&format!(
            " public interface I{index} extends I{} {{}}",
            index - 1
        ));
    }
    deep.push_str(
        " public class DeepImplementation implements I1023 {} \
         I0 root = new DeepImplementation();",
    );
    check(&deep).unwrap();
}

#[test]
fn subtype_queries_cover_mixed_edges_case_insensitively_and_negative_paths() {
    let hierarchy = "\
        public interface Root {}
        public interface Branch extends Root {}
        public virtual class Base implements Branch {}
        public class Leaf extends Base {}
        public interface Unrelated {}";
    check(&format!("{hierarchy} rOoT accepted = new lEaF();")).unwrap();

    let error = check(&format!("{hierarchy} Unrelated rejected = new Leaf();")).unwrap_err();
    assert!(error.message.contains("cannot assign Leaf to Unrelated"));
}

#[test]
fn checker_binds_batchable_methods_to_the_declared_generic_argument() {
    let correct = "\
        public class CorrectBatch implements Database.Batchable<Integer> {
            public List<Integer> start(Database.BatchableContext context) {
                return new List<Integer>();
            }
            public void execute(Database.BatchableContext context, List<Integer> scope) {}
            public void finish(Database.BatchableContext context) {}
        }";
    check(correct).unwrap();

    let string_start = correct
        .replace("CorrectBatch", "StringStart")
        .replace("List<Integer> start", "List<String> start")
        .replace("return new List<Integer>();", "return new List<String>();");
    let error = check(&string_start).unwrap_err();
    assert!(error.message.contains("List<Integer>"));
    assert!(error.message.contains("declared Database.Batchable"));

    let string_execute = correct
        .replace("CorrectBatch", "StringExecute")
        .replace("List<Integer> scope", "List<String> scope");
    let error = check(&string_execute).unwrap_err();
    assert!(error.message.contains("execute"));
    assert!(error.message.contains("List<Integer>"));
}

#[test]
fn checker_rejects_unchecked_or_unsupported_interface_arguments() {
    let missing = "\
        public class MissingArgument implements Database.Batchable {
            public List<Integer> start(Database.BatchableContext context) {
                return new List<Integer>();
            }
            public void execute(Database.BatchableContext context, List<Integer> scope) {}
            public void finish(Database.BatchableContext context) {}
        }";
    assert!(
        check(missing)
            .unwrap_err()
            .message
            .contains("exactly one type argument")
    );

    let excess = missing.replace(
        "implements Database.Batchable {",
        "implements Database.Batchable<Integer, String> {",
    );
    assert!(
        check(&excess)
            .unwrap_err()
            .message
            .contains("exactly one type argument")
    );

    let queueable = "\
        public class GenericQueue implements Queueable<Integer> {
            public void execute(QueueableContext context) {}
        }";
    assert!(
        check(queueable)
            .unwrap_err()
            .message
            .contains("does not accept generic arguments")
    );

    let schedulable = "\
        public class GenericSchedule implements Schedulable<String> {
            public void execute(SchedulableContext context) {}
        }";
    assert!(
        check(schedulable)
            .unwrap_err()
            .message
            .contains("does not accept generic arguments")
    );

    let user_defined = "\
        public interface Work {}
        public class GenericWork implements Work<Integer> {}";
    assert!(
        check(user_defined)
            .unwrap_err()
            .message
            .contains("user-defined interface")
    );

    let duplicate = "\
        public class DuplicateBatch implements
            Database.Batchable<Integer>, database.Batchable<String> {
            public List<Integer> start(Database.BatchableContext context) {
                return new List<Integer>();
            }
            public void execute(Database.BatchableContext context, List<Integer> scope) {}
            public void finish(Database.BatchableContext context) {}
        }";
    assert!(
        check(duplicate)
            .unwrap_err()
            .message
            .contains("more than once")
    );
}

#[test]
fn cli_reproductions_return_bounded_success_or_diagnostics() {
    let supported = run_cli_script(
        "supported",
        "\
        public class Foo { public Integer value; }
        Integer foo = 1;
        Integer bar = 2;
        Integer result = (foo) + bar;
        Foo box = new Foo();
        box.value = 1;
        Integer member = (box.value) + bar;
        Integer[] indexedValues = new Integer[] { 1 };
        Integer indexed = (indexedValues)[0];
        (box.value)++;
        Integer plus = (bar) + -bar;
        Integer minus = (bar) - +bar;
        Object boxed = box;
        Foo customCast = (Foo) boxed;
        Integer signedCast = (Integer) -bar;
        Foo[] customValues = new Foo[3];
        Object[] objectValues = new Object[2];
        Exception[] errors = new Exception[1];
        Schema.SObjectType objectType = null;
        Schema.DescribeSObjectResult describe = null;",
    );
    assert!(supported.status.success());
    assert_eq!(String::from_utf8(supported.stdout).unwrap(), "OK\n");

    let invalid_edge = run_cli_script(
        "invalid-interface-edge",
        "public interface A implements B {} public interface B implements A {}",
    );
    assert!(!invalid_edge.status.success());
    let stderr = String::from_utf8(invalid_edge.stderr).unwrap();
    assert!(stderr.contains("`implements` is invalid"));
    assert!(!stderr.contains("stack overflow"));

    let cyclic = run_cli_script(
        "cyclic-interface",
        "public interface A extends B {} public interface B extends A {}",
    );
    assert!(!cyclic.status.success());
    let stderr = String::from_utf8(cyclic.stderr).unwrap();
    assert!(stderr.contains("cyclic inheritance"));
    assert!(!stderr.contains("stack overflow"));

    let generic_mismatch = run_cli_script(
        "generic-mismatch",
        "\
        public class BadBatch implements Database.Batchable<Integer> {
            public List<String> start(Database.BatchableContext context) {
                return new List<String>();
            }
            public void execute(Database.BatchableContext context, List<String> scope) {}
            public void finish(Database.BatchableContext context) {}
        }",
    );
    assert!(!generic_mismatch.status.success());
    assert!(
        String::from_utf8(generic_mismatch.stderr)
            .unwrap()
            .contains("List<Integer>")
    );

    let missing_array = run_cli_script("missing-array-element", "Object values = new Missing[3];");
    assert!(!missing_array.status.success());
    assert!(
        String::from_utf8(missing_array.stderr)
            .unwrap()
            .contains("unknown type `Missing`")
    );
}

fn run_cli_script(label: &str, source: &str) -> std::process::Output {
    let path = temporary_script(label);
    fs::write(&path, source).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_apex-exec"))
        .args(["check", path.to_str().unwrap()])
        .output()
        .unwrap();
    fs::remove_file(path).unwrap();
    output
}

fn temporary_script(label: &str) -> PathBuf {
    let unique = format!(
        "apex-exec-s0-01-{label}-{}-{}.apex",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    std::env::temp_dir().join(unique)
}
