use super::*;
use crate::ast::{
    AssignmentTarget, BinaryOperator, CollectionInitializer, Expression, Program, ReturnType,
    Statement, TypeName, UnaryOperator,
};
use crate::lexer::Lexer;
use crate::span::SourceId;

fn parse(source: &str) -> Program {
    Parser::new(Lexer::new(source).tokenize().unwrap())
        .unwrap()
        .parse_program()
        .unwrap()
}

#[test]
fn preserves_source_identity_through_constructed_spans() {
    let source_id = SourceId::new(9);
    let program = Parser::new(
        Lexer::with_source("@IsTest public class Example {}", source_id)
            .tokenize()
            .unwrap(),
    )
    .unwrap()
    .parse_program()
    .unwrap();

    let class = &program.classes[0];
    assert_eq!(class.span.source_id, source_id);
    assert_eq!(class.annotations[0].span.source_id, source_id);
}

#[test]
fn multiplication_binds_more_tightly_than_addition() {
    let program = parse("Integer result = 1 + 2 * 3;");
    let Statement::VariableDeclaration { initializer, .. } = &program.statements[0] else {
        panic!("expected variable declaration");
    };
    let Expression::Binary {
        operator: BinaryOperator::Add,
        left,
        right,
        ..
    } = initializer
    else {
        panic!("expected addition at the expression root");
    };

    assert!(matches!(left.as_ref(), Expression::IntegerLiteral(1, _)));
    assert!(matches!(
        right.as_ref(),
        Expression::Binary {
            operator: BinaryOperator::Multiply,
            ..
        }
    ));
}

#[test]
fn assignment_parses_right_associatively() {
    let program = parse("Integer left = 0; Integer right = 0; left = right = 7;");
    let Statement::Expression { expression, .. } = &program.statements[2] else {
        panic!("expected assignment statement");
    };
    let Expression::Assignment { target, value, .. } = expression else {
        panic!("expected outer assignment");
    };
    let AssignmentTarget::Variable(target) = target else {
        panic!("expected variable assignment target");
    };

    assert_eq!(target.canonical, "left");
    assert!(matches!(
        value.as_ref(),
        Expression::Assignment {
            target: AssignmentTarget::Variable(target),
            ..
        } if target.canonical == "right"
    ));
}

#[test]
fn conditional_is_right_associative_and_binds_between_or_and_assignment() {
    let program = parse(
        "Boolean first = true; Boolean second = false; Integer value = 0; \
         value = first || second ? 1 : second ? 2 : 3;",
    );
    let Statement::Expression { expression, .. } = &program.statements[3] else {
        panic!("expected assignment statement");
    };
    let Expression::Assignment { value, .. } = expression else {
        panic!("expected assignment at the expression root");
    };
    let Expression::Conditional {
        condition,
        when_false,
        ..
    } = value.as_ref()
    else {
        panic!("expected conditional below assignment");
    };
    assert!(matches!(
        condition.as_ref(),
        Expression::Binary {
            operator: BinaryOperator::Or,
            ..
        }
    ));
    assert!(matches!(
        when_false.as_ref(),
        Expression::Conditional { .. }
    ));
}

#[test]
fn instanceof_binds_as_a_comparison_and_preserves_generic_target() {
    let program = parse(
        "Object value = new List<String>(); \
         Boolean result = value instanceof List<String> == true;",
    );
    let Statement::VariableDeclaration { initializer, .. } = &program.statements[1] else {
        panic!("expected declaration");
    };
    let Expression::Binary {
        left,
        operator: BinaryOperator::Equal,
        ..
    } = initializer
    else {
        panic!("expected equality at the expression root");
    };
    assert!(matches!(
        left.as_ref(),
        Expression::Instanceof {
            target: TypeName::List(element),
            ..
        } if element.as_ref() == &TypeName::String
    ));
}

#[test]
fn conditional_requires_a_false_branch_in_the_parser() {
    let error = Parser::new(Lexer::new("Integer value = true ? 1;").tokenize().unwrap())
        .unwrap()
        .parse_program()
        .unwrap_err();

    assert_eq!(
        error.message,
        "expected `:` after the true branch of conditional expression"
    );
}

#[test]
fn else_binds_to_the_nearest_if() {
    let program = parse("if (true) if (false) System.debug('inner'); else System.debug('else');");
    let Statement::If {
        then_branch,
        else_branch,
        ..
    } = &program.statements[0]
    else {
        panic!("expected outer if");
    };

    assert!(else_branch.is_none());
    assert!(matches!(
        then_branch.as_ref(),
        Statement::If {
            else_branch: Some(_),
            ..
        }
    ));
}

#[test]
fn for_statement_records_all_optional_clauses() {
    let program = parse("for (;;) { break; }");
    let Statement::For {
        initializer,
        condition,
        update,
        body,
        ..
    } = &program.statements[0]
    else {
        panic!("expected for statement");
    };

    assert!(initializer.is_none());
    assert!(condition.is_none());
    assert!(update.is_none());
    assert!(matches!(body.as_ref(), Statement::Block { .. }));
}

#[test]
fn parses_nested_generic_types_and_canonicalizes_array_syntax() {
    let program = parse(
        "Map<String, List<Set<Integer>>> grouped = new Map<String, List<Set<Integer>>>(); \
         Integer[] numbers = new Integer[3];",
    );

    let Statement::VariableDeclaration {
        ty, initializer, ..
    } = &program.statements[0]
    else {
        panic!("expected map declaration");
    };
    assert_eq!(
        ty,
        &TypeName::Map(
            Box::new(TypeName::String),
            Box::new(TypeName::List(Box::new(TypeName::Set(Box::new(
                TypeName::Integer
            )))))
        )
    );
    assert!(matches!(
        initializer,
        Expression::NewCollection {
            initializer: CollectionInitializer::Arguments(arguments),
            ..
        } if arguments.is_empty()
    ));

    let Statement::VariableDeclaration {
        ty, initializer, ..
    } = &program.statements[1]
    else {
        panic!("expected array declaration");
    };
    assert_eq!(ty, &TypeName::List(Box::new(TypeName::Integer)));
    assert!(matches!(
        initializer,
        Expression::NewCollection {
            ty: TypeName::List(element),
            initializer: CollectionInitializer::SizedArray(size),
            ..
        } if element.as_ref() == &TypeName::Integer
            && matches!(size.as_ref(), Expression::IntegerLiteral(3, _))
    ));
}

#[test]
fn parses_element_and_map_collection_initializers() {
    let program = parse(
        "List<String> names = new List<String>{'Ada', 'Grace'}; \
         Map<String, Integer> counts = new Map<String, Integer>{'one' => 1, 'two' => 2}; \
         Set<String> copied = new Set<String>(names); \
         String[] aliases = new String[]{'one', 'two'};",
    );

    let Statement::VariableDeclaration { initializer, .. } = &program.statements[0] else {
        panic!("expected list declaration");
    };
    assert!(matches!(
        initializer,
        Expression::NewCollection {
            initializer: CollectionInitializer::Elements(elements),
            ..
        } if elements.len() == 2
    ));

    let Statement::VariableDeclaration { initializer, .. } = &program.statements[1] else {
        panic!("expected map declaration");
    };
    let Expression::NewCollection {
        initializer: CollectionInitializer::MapEntries(entries),
        ..
    } = initializer
    else {
        panic!("expected map entries");
    };
    assert_eq!(entries.len(), 2);
    assert!(matches!(
        &entries[0].key,
        Expression::StringLiteral(value, _) if value == "one"
    ));
    assert!(matches!(
        &entries[0].value,
        Expression::IntegerLiteral(1, _)
    ));

    let Statement::VariableDeclaration { initializer, .. } = &program.statements[2] else {
        panic!("expected set declaration");
    };
    assert!(matches!(
        initializer,
        Expression::NewCollection {
            initializer: CollectionInitializer::Arguments(arguments),
            ..
        } if matches!(arguments.as_slice(), [Expression::Variable(name)] if name.canonical == "names")
    ));

    let Statement::VariableDeclaration { initializer, .. } = &program.statements[3] else {
        panic!("expected array literal declaration");
    };
    assert!(matches!(
        initializer,
        Expression::NewCollection {
            ty: TypeName::List(element),
            initializer: CollectionInitializer::Elements(elements),
            ..
        } if element.as_ref() == &TypeName::String && elements.len() == 2
    ));
}

#[test]
fn parses_index_assignment_and_chained_method_calls() {
    let program = parse(
        "List<String> values = new List<String>{'zero'}; \
         values[0] = String.VaLuEOf(1); \
         values.add(values[0]); \
         System.debug(String.join(values, ''));",
    );

    let Statement::Expression { expression, .. } = &program.statements[1] else {
        panic!("expected index assignment");
    };
    assert!(matches!(
        expression,
        Expression::Assignment {
            target: AssignmentTarget::Index { .. },
            value,
            ..
        } if matches!(
            value.as_ref(),
            Expression::MethodCall { method, arguments, .. }
                if method.spelling == "VaLuEOf"
                    && method.canonical == "valueof"
                    && arguments.len() == 1
        )
    ));

    let Statement::Expression { expression, .. } = &program.statements[2] else {
        panic!("expected add call");
    };
    assert!(matches!(
        expression,
        Expression::MethodCall { method, arguments, .. }
            if method.canonical == "add"
                && matches!(arguments.as_slice(), [Expression::Index { .. }])
    ));

    let Statement::Expression { expression, .. } = &program.statements[3] else {
        panic!("System.debug should be an ordinary expression statement");
    };
    assert!(matches!(
        expression,
        Expression::MethodCall { method, arguments, .. }
            if method.canonical == "debug"
                && matches!(arguments.as_slice(), [Expression::MethodCall { method, .. }] if method.canonical == "join")
    ));
}

#[test]
fn distinguishes_enhanced_and_traditional_for_statements() {
    let program = parse(
        "List<String> values = new List<String>(); \
         for (String value : values) System.debug(value); \
         for (Integer index = 0; index < 1; index++) {}",
    );

    assert!(matches!(
        &program.statements[1],
        Statement::ForEach {
            element_type: TypeName::String,
            name,
            iterable: Expression::Variable(iterable),
            ..
        } if name.canonical == "value" && iterable.canonical == "values"
    ));
    assert!(matches!(&program.statements[2], Statement::For { .. }));
}

#[test]
fn collection_postfix_nodes_preserve_full_source_spans() {
    let source = "List<String> values = new List<String>{'zero'}; values[0].toUpperCase();";
    let program = parse(source);
    let Statement::VariableDeclaration { initializer, .. } = &program.statements[0] else {
        panic!("expected declaration");
    };
    assert_eq!(
        &source[initializer.span().start..initializer.span().end],
        "new List<String>{'zero'}"
    );

    let Statement::Expression { expression, .. } = &program.statements[1] else {
        panic!("expected method call");
    };
    let Expression::MethodCall {
        receiver, method, ..
    } = expression
    else {
        panic!("expected method call expression");
    };
    assert_eq!(
        &source[expression.span().start..expression.span().end],
        "values[0].toUpperCase()"
    );
    assert_eq!(
        &source[receiver.span().start..receiver.span().end],
        "values[0]"
    );
    assert_eq!(&source[method.span.start..method.span.end], "toUpperCase");
    assert_eq!(method.canonical, "touppercase");
}

#[test]
fn rejects_more_than_one_array_suffix_explicitly() {
    let error = Parser::new(
        Lexer::new("Integer[][] values = new Integer[1];")
            .tokenize()
            .unwrap(),
    )
    .unwrap()
    .parse_program()
    .unwrap_err();

    assert_eq!(error.message, "only one array suffix is supported");
}

#[test]
fn parses_methods_separately_from_executable_statements() {
    let source = "Integer add(Integer left, Integer right) { return left + right; } \
                  void report(String value) { System.debug(value); } \
                  Integer total = add(1, 2);";
    let program = parse(source);

    assert_eq!(program.methods.len(), 2);
    assert_eq!(program.statements.len(), 1);

    let add = &program.methods[0];
    assert_eq!(add.return_type, ReturnType::Value(TypeName::Integer));
    assert_eq!(add.name.canonical, "add");
    assert_eq!(add.parameters.len(), 2);
    assert_eq!(add.parameters[0].ty, TypeName::Integer);
    assert_eq!(add.parameters[0].name.canonical, "left");
    assert!(matches!(
        add.body,
        Some(Statement::Block {
            ref statements,
            ..
        }) if matches!(statements.as_slice(), [Statement::Return { value: Some(_), .. }])
    ));

    let report = &program.methods[1];
    assert_eq!(report.return_type, ReturnType::Void);
    assert_eq!(report.parameters[0].ty, TypeName::String);
    assert_eq!(
        &source[report.span.start..report.span.end],
        "void report(String value) { System.debug(value); }"
    );

    let Statement::VariableDeclaration { initializer, .. } = &program.statements[0] else {
        panic!("expected executable declaration");
    };
    assert!(matches!(
        initializer,
        Expression::FunctionCall {
            name,
            arguments,
            ..
        } if name.canonical == "add"
            && arguments.len() == 2
    ));
}

#[test]
fn parses_function_calls_as_postfix_receivers() {
    let program = parse("List<String> make() { return new List<String>(); } make().add('value');");
    let Statement::Expression { expression, .. } = &program.statements[0] else {
        panic!("expected call statement");
    };
    assert!(matches!(
        expression,
        Expression::MethodCall { receiver, method, .. }
            if method.canonical == "add"
                && matches!(receiver.as_ref(), Expression::FunctionCall { name, .. } if name.canonical == "make")
    ));
}

#[test]
fn parses_exception_construction_throw_and_handlers() {
    let source = "try { throw new IllegalArgumentException('bad input'); } \
                  catch (IllegalArgumentException problem) { throw problem; } \
                  catch (Exception ignored) {} \
                  finally { System.debug('cleanup'); }";
    let program = parse(source);
    let Statement::Try {
        try_block,
        catches,
        finally_block,
        ..
    } = &program.statements[0]
    else {
        panic!("expected try statement");
    };

    assert!(matches!(
        try_block.as_ref(),
        Statement::Block { statements, .. }
            if matches!(
                statements.as_slice(),
                [Statement::Throw {
                    value: Expression::NewException {
                        exception_type: TypeName::IllegalArgumentException,
                        arguments,
                        ..
                    },
                    ..
                }] if matches!(arguments.as_slice(), [Expression::StringLiteral(value, _)] if value == "bad input")
            )
    ));
    assert_eq!(catches.len(), 2);
    assert_eq!(
        catches[0].exception_type,
        TypeName::IllegalArgumentException
    );
    assert_eq!(catches[0].name.canonical, "problem");
    assert_eq!(catches[1].exception_type, TypeName::Exception);
    assert!(finally_block.is_some());
}

#[test]
fn parses_try_finally_without_a_catch() {
    let program = parse("try { System.debug('work'); } finally { System.debug('done'); }");
    assert!(matches!(
        &program.statements[0],
        Statement::Try {
            catches,
            finally_block: Some(_),
            ..
        } if catches.is_empty()
    ));
}

#[test]
fn requires_a_catch_or_finally_after_try() {
    let error = Parser::new(Lexer::new("try {}").tokenize().unwrap())
        .unwrap()
        .parse_program()
        .unwrap_err();

    assert_eq!(
        error.message,
        "expected at least one `catch` or a `finally` after try block"
    );
}

#[test]
fn preserves_non_exception_catch_types_for_semantic_validation() {
    let program = parse("try {} catch (String problem) {}");
    assert!(matches!(
        &program.statements[0],
        Statement::Try { catches, .. }
            if catches[0].exception_type == TypeName::String
    ));
}

#[test]
fn preserves_exception_constructor_arguments_for_semantic_validation() {
    let program = parse("throw new Exception('first', 'second');");
    assert!(matches!(
        &program.statements[0],
        Statement::Throw {
            value: Expression::NewException { arguments, .. },
            ..
        } if arguments.len() == 2
    ));
}

#[test]
fn distinguishes_casts_from_grouped_expressions() {
    let program = parse(
        "Object boxed = 1; Integer casted = (Integer) boxed; \
         Integer grouped = (1 + 2) * 3;",
    );

    let Statement::VariableDeclaration { initializer, .. } = &program.statements[1] else {
        panic!("expected cast declaration");
    };
    assert!(matches!(
        initializer,
        Expression::Cast {
            ty: TypeName::Integer,
            expression,
            ..
        } if matches!(expression.as_ref(), Expression::Variable(name) if name.canonical == "boxed")
    ));

    let Statement::VariableDeclaration { initializer, .. } = &program.statements[2] else {
        panic!("expected grouped declaration");
    };
    assert!(matches!(
        initializer,
        Expression::Binary {
            left,
            operator: BinaryOperator::Multiply,
            ..
        } if matches!(left.as_ref(), Expression::Binary { operator: BinaryOperator::Add, .. })
    ));
}

#[test]
fn grouped_postfix_continuations_and_signed_operators_do_not_become_casts() {
    let program = parse(
        "public class Box { public Integer value; } \
         Box box = new Box(); \
         Integer other = 2; \
         Integer[] values = new Integer[] { 1 }; \
         Integer member = (box.value) + other; \
         Integer indexed = (values)[0]; \
         (box.value)++; \
         Integer plus = (other) + -other; \
         Integer minus = (other) - +other; \
         Object boxed = box; \
         Box customCast = (Box) boxed; \
         Integer signedCast = (Integer) -other;",
    );

    let Statement::VariableDeclaration { initializer, .. } = &program.statements[3] else {
        panic!("expected grouped member declaration");
    };
    assert!(matches!(
        initializer,
        Expression::Binary {
            left,
            operator: BinaryOperator::Add,
            ..
        } if matches!(left.as_ref(), Expression::MemberAccess { .. })
    ));

    let Statement::VariableDeclaration { initializer, .. } = &program.statements[4] else {
        panic!("expected grouped index declaration");
    };
    assert!(matches!(initializer, Expression::Index { .. }));

    let Statement::Expression { expression, .. } = &program.statements[5] else {
        panic!("expected grouped member postfix expression");
    };
    assert!(matches!(
        expression,
        Expression::Postfix { operand, .. }
            if matches!(operand.as_ref(), Expression::MemberAccess { .. })
    ));

    for (index, operator, unary) in [
        (6, BinaryOperator::Add, UnaryOperator::Negate),
        (7, BinaryOperator::Subtract, UnaryOperator::Positive),
    ] {
        let Statement::VariableDeclaration { initializer, .. } = &program.statements[index] else {
            panic!("expected signed grouped expression");
        };
        assert!(matches!(
            initializer,
            Expression::Binary {
                left,
                operator: actual_operator,
                right,
                ..
            } if *actual_operator == operator
                && matches!(left.as_ref(), Expression::Variable(_))
                && matches!(
                    right.as_ref(),
                    Expression::Unary {
                        operator: actual_unary,
                        ..
                    } if *actual_unary == unary
                )
        ));
    }

    let Statement::VariableDeclaration { initializer, .. } = &program.statements[9] else {
        panic!("expected genuine custom cast");
    };
    assert!(matches!(
        initializer,
        Expression::Cast {
            ty: TypeName::Custom(name),
            ..
        } if name.canonical == "box"
    ));

    let Statement::VariableDeclaration { initializer, .. } = &program.statements[10] else {
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
}

#[test]
fn supports_object_and_core_exception_type_names_case_insensitively() {
    let program = parse(
        "oBjEcT identity(oBjEcT value) { return value; } \
         throw new nUlLpOiNtErExCePtIoN();",
    );

    assert_eq!(
        program.methods[0].return_type,
        ReturnType::Value(TypeName::Object)
    );
    assert_eq!(program.methods[0].parameters[0].ty, TypeName::Object);
    assert!(matches!(
        &program.statements[0],
        Statement::Throw {
            value: Expression::NewException {
                exception_type: TypeName::NullPointerException,
                arguments,
                ..
            },
            ..
        } if arguments.is_empty()
    ));
}
