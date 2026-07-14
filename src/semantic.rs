use crate::{
    ast::{
        BinaryOperator, Expression, PostfixOperator, Program, Statement, TypeName, UnaryOperator,
    },
    diagnostic::Diagnostic,
    span::Span,
};
use std::collections::HashMap;

pub fn check(program: &Program) -> Result<(), Diagnostic> {
    Checker::new().check_program(program)
}

struct Checker {
    scopes: Vec<HashMap<String, TypeName>>,
    loop_depth: usize,
}

impl Checker {
    fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            loop_depth: 0,
        }
    }

    fn check_program(&mut self, program: &Program) -> Result<(), Diagnostic> {
        for statement in &program.statements {
            self.check_statement(statement)?;
        }
        Ok(())
    }

    fn check_statement(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
        match statement {
            Statement::VariableDeclaration {
                ty,
                name,
                initializer,
                ..
            } => {
                if self.current_scope().contains_key(&name.canonical) {
                    return Err(Diagnostic::new(
                        format!("duplicate variable `{}`", name.spelling),
                        name.span,
                    ));
                }
                let initializer_type = self.expression_type(initializer)?;
                self.require_assignable(*ty, initializer_type, initializer.span())?;
                self.current_scope_mut().insert(name.canonical.clone(), *ty);
                Ok(())
            }
            Statement::Expression { expression, .. } => {
                if !is_statement_expression(expression) {
                    return Err(Diagnostic::new(
                        "only assignment and increment/decrement expressions may be statements",
                        expression.span(),
                    ));
                }
                self.expression_type(expression)?;
                Ok(())
            }
            Statement::Debug { expression, .. } => {
                self.expression_type(expression)?;
                Ok(())
            }
            Statement::Block { statements, .. } => self.with_scope(|checker| {
                for statement in statements {
                    checker.check_statement(statement)?;
                }
                Ok(())
            }),
            Statement::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.require_boolean(condition)?;
                self.check_statement(then_branch)?;
                if let Some(else_branch) = else_branch {
                    self.check_statement(else_branch)?;
                }
                Ok(())
            }
            Statement::While {
                condition, body, ..
            } => {
                self.require_boolean(condition)?;
                self.with_loop(|checker| checker.check_statement(body))
            }
            Statement::DoWhile {
                body, condition, ..
            } => {
                self.with_loop(|checker| checker.check_statement(body))?;
                self.require_boolean(condition)
            }
            Statement::For {
                initializer,
                condition,
                update,
                body,
                ..
            } => self.with_scope(|checker| {
                if let Some(initializer) = initializer {
                    checker.check_statement(initializer)?;
                }
                if let Some(condition) = condition {
                    checker.require_boolean(condition)?;
                }
                checker.with_loop(|checker| {
                    checker.check_statement(body)?;
                    if let Some(update) = update {
                        checker.check_statement(update)?;
                    }
                    Ok(())
                })
            }),
            Statement::Break { span } => {
                if self.loop_depth == 0 {
                    Err(Diagnostic::new(
                        "`break` is only valid inside a loop",
                        *span,
                    ))
                } else {
                    Ok(())
                }
            }
            Statement::Continue { span } => {
                if self.loop_depth == 0 {
                    Err(Diagnostic::new(
                        "`continue` is only valid inside a loop",
                        *span,
                    ))
                } else {
                    Ok(())
                }
            }
            Statement::Return { value, .. } => {
                if let Some(value) = value {
                    Err(Diagnostic::new(
                        "anonymous execution does not support returning a value",
                        value.span(),
                    ))
                } else {
                    Ok(())
                }
            }
        }
    }

    fn expression_type(&mut self, expression: &Expression) -> Result<ExpressionType, Diagnostic> {
        match expression {
            Expression::StringLiteral(..) => Ok(ExpressionType::Primitive(TypeName::String)),
            Expression::BooleanLiteral(..) => Ok(ExpressionType::Primitive(TypeName::Boolean)),
            Expression::IntegerLiteral(..) => Ok(ExpressionType::Primitive(TypeName::Integer)),
            Expression::NullLiteral(..) => Ok(ExpressionType::Null),
            Expression::Variable(identifier) => self
                .lookup(&identifier.canonical)
                .map(ExpressionType::Primitive)
                .ok_or_else(|| {
                    Diagnostic::new(
                        format!("unknown variable `{}`", identifier.spelling),
                        identifier.span,
                    )
                }),
            Expression::Assignment {
                target,
                value,
                span: _,
            } => {
                let expected = self.lookup(&target.canonical).ok_or_else(|| {
                    Diagnostic::new(
                        format!("unknown variable `{}`", target.spelling),
                        target.span,
                    )
                })?;
                let actual = self.expression_type(value)?;
                self.require_assignable(expected, actual, value.span())?;
                Ok(ExpressionType::Primitive(expected))
            }
            Expression::Unary {
                operator,
                operand,
                operator_span,
                ..
            } => self.unary_type(*operator, operand, *operator_span),
            Expression::Postfix {
                operand,
                operator,
                operator_span,
                ..
            } => self.postfix_type(operand, *operator, *operator_span),
            Expression::Binary {
                left,
                operator,
                right,
                operator_span,
                ..
            } => self.binary_type(left, *operator, right, *operator_span),
        }
    }

    fn unary_type(
        &mut self,
        operator: UnaryOperator,
        operand: &Expression,
        operator_span: Span,
    ) -> Result<ExpressionType, Diagnostic> {
        match operator {
            UnaryOperator::Positive | UnaryOperator::Negate => {
                self.require_operand(operand, TypeName::Integer, operator_span)?;
                Ok(ExpressionType::Primitive(TypeName::Integer))
            }
            UnaryOperator::Not => {
                self.require_operand(operand, TypeName::Boolean, operator_span)?;
                Ok(ExpressionType::Primitive(TypeName::Boolean))
            }
            UnaryOperator::PrefixIncrement | UnaryOperator::PrefixDecrement => {
                self.require_mutable_integer(operand, operator_span)
            }
        }
    }

    fn postfix_type(
        &mut self,
        operand: &Expression,
        _operator: PostfixOperator,
        operator_span: Span,
    ) -> Result<ExpressionType, Diagnostic> {
        self.require_mutable_integer(operand, operator_span)
    }

    fn binary_type(
        &mut self,
        left: &Expression,
        operator: BinaryOperator,
        right: &Expression,
        operator_span: Span,
    ) -> Result<ExpressionType, Diagnostic> {
        let left_type = self.expression_type(left)?;
        let right_type = self.expression_type(right)?;
        match operator {
            BinaryOperator::Add => {
                if left_type == ExpressionType::Primitive(TypeName::Integer)
                    && right_type == ExpressionType::Primitive(TypeName::Integer)
                {
                    Ok(ExpressionType::Primitive(TypeName::Integer))
                } else if left_type == ExpressionType::Primitive(TypeName::String)
                    || right_type == ExpressionType::Primitive(TypeName::String)
                {
                    Ok(ExpressionType::Primitive(TypeName::String))
                } else {
                    Err(invalid_binary_operands(
                        operator,
                        left_type,
                        right_type,
                        operator_span,
                    ))
                }
            }
            BinaryOperator::Subtract
            | BinaryOperator::Multiply
            | BinaryOperator::Divide
            | BinaryOperator::Remainder
            | BinaryOperator::Less
            | BinaryOperator::LessEqual
            | BinaryOperator::Greater
            | BinaryOperator::GreaterEqual => {
                if left_type == ExpressionType::Primitive(TypeName::Integer)
                    && right_type == ExpressionType::Primitive(TypeName::Integer)
                {
                    if matches!(
                        operator,
                        BinaryOperator::Less
                            | BinaryOperator::LessEqual
                            | BinaryOperator::Greater
                            | BinaryOperator::GreaterEqual
                    ) {
                        Ok(ExpressionType::Primitive(TypeName::Boolean))
                    } else {
                        Ok(ExpressionType::Primitive(TypeName::Integer))
                    }
                } else {
                    Err(invalid_binary_operands(
                        operator,
                        left_type,
                        right_type,
                        operator_span,
                    ))
                }
            }
            BinaryOperator::Equal | BinaryOperator::NotEqual => {
                if left_type == right_type
                    || left_type == ExpressionType::Null
                    || right_type == ExpressionType::Null
                {
                    Ok(ExpressionType::Primitive(TypeName::Boolean))
                } else {
                    Err(invalid_binary_operands(
                        operator,
                        left_type,
                        right_type,
                        operator_span,
                    ))
                }
            }
            BinaryOperator::And | BinaryOperator::Or => {
                if left_type == ExpressionType::Primitive(TypeName::Boolean)
                    && right_type == ExpressionType::Primitive(TypeName::Boolean)
                {
                    Ok(ExpressionType::Primitive(TypeName::Boolean))
                } else {
                    Err(invalid_binary_operands(
                        operator,
                        left_type,
                        right_type,
                        operator_span,
                    ))
                }
            }
        }
    }

    fn require_boolean(&mut self, expression: &Expression) -> Result<(), Diagnostic> {
        self.require_operand(expression, TypeName::Boolean, expression.span())
    }

    fn require_operand(
        &mut self,
        expression: &Expression,
        expected: TypeName,
        error_span: Span,
    ) -> Result<(), Diagnostic> {
        let actual = self.expression_type(expression)?;
        if actual == ExpressionType::Primitive(expected) {
            Ok(())
        } else {
            Err(Diagnostic::new(
                format!("expected {}, found {}", expected.apex_name(), actual.name()),
                error_span,
            ))
        }
    }

    fn require_mutable_integer(
        &mut self,
        operand: &Expression,
        operator_span: Span,
    ) -> Result<ExpressionType, Diagnostic> {
        let Expression::Variable(identifier) = operand else {
            return Err(Diagnostic::new(
                "increment/decrement operand must be a variable",
                operator_span,
            ));
        };
        let actual = self.lookup(&identifier.canonical).ok_or_else(|| {
            Diagnostic::new(
                format!("unknown variable `{}`", identifier.spelling),
                identifier.span,
            )
        })?;
        if actual != TypeName::Integer {
            return Err(Diagnostic::new(
                format!(
                    "increment/decrement requires Integer, found {}",
                    actual.apex_name()
                ),
                operator_span,
            ));
        }
        Ok(ExpressionType::Primitive(TypeName::Integer))
    }

    fn require_assignable(
        &self,
        expected: TypeName,
        actual: ExpressionType,
        span: Span,
    ) -> Result<(), Diagnostic> {
        if actual == ExpressionType::Primitive(expected) || actual == ExpressionType::Null {
            Ok(())
        } else {
            Err(Diagnostic::new(
                format!(
                    "cannot assign {} to {}",
                    actual.name(),
                    expected.apex_name()
                ),
                span,
            ))
        }
    }

    fn lookup(&self, canonical: &str) -> Option<TypeName> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(canonical).copied())
    }

    fn current_scope(&self) -> &HashMap<String, TypeName> {
        self.scopes.last().expect("checker always has a scope")
    }

    fn current_scope_mut(&mut self) -> &mut HashMap<String, TypeName> {
        self.scopes.last_mut().expect("checker always has a scope")
    }

    fn with_scope<T>(
        &mut self,
        operation: impl FnOnce(&mut Self) -> Result<T, Diagnostic>,
    ) -> Result<T, Diagnostic> {
        self.scopes.push(HashMap::new());
        let result = operation(self);
        self.scopes.pop();
        result
    }

    fn with_loop<T>(
        &mut self,
        operation: impl FnOnce(&mut Self) -> Result<T, Diagnostic>,
    ) -> Result<T, Diagnostic> {
        self.loop_depth += 1;
        let result = operation(self);
        self.loop_depth -= 1;
        result
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExpressionType {
    Primitive(TypeName),
    Null,
}

impl ExpressionType {
    const fn name(self) -> &'static str {
        match self {
            Self::Primitive(ty) => ty.apex_name(),
            Self::Null => "null",
        }
    }
}

fn is_statement_expression(expression: &Expression) -> bool {
    matches!(
        expression,
        Expression::Assignment { .. }
            | Expression::Unary {
                operator: UnaryOperator::PrefixIncrement | UnaryOperator::PrefixDecrement,
                ..
            }
            | Expression::Postfix { .. }
    )
}

fn invalid_binary_operands(
    operator: BinaryOperator,
    left: ExpressionType,
    right: ExpressionType,
    span: Span,
) -> Diagnostic {
    Diagnostic::new(
        format!(
            "operator `{}` cannot be applied to {} and {}",
            binary_operator_spelling(operator),
            left.name(),
            right.name()
        ),
        span,
    )
}

fn binary_operator_spelling(operator: BinaryOperator) -> &'static str {
    match operator {
        BinaryOperator::Add => "+",
        BinaryOperator::Subtract => "-",
        BinaryOperator::Multiply => "*",
        BinaryOperator::Divide => "/",
        BinaryOperator::Remainder => "%",
        BinaryOperator::Less => "<",
        BinaryOperator::LessEqual => "<=",
        BinaryOperator::Greater => ">",
        BinaryOperator::GreaterEqual => ">=",
        BinaryOperator::Equal => "==",
        BinaryOperator::NotEqual => "!=",
        BinaryOperator::And => "&&",
        BinaryOperator::Or => "||",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_source(source: &str) -> Result<(), Diagnostic> {
        let program = crate::parse(source)?;
        check(&program)
    }

    #[test]
    fn permits_nested_shadowing_but_rejects_same_scope_duplicates() {
        check_source("Integer value = 1; { Integer VALUE = 2; }").unwrap();

        let error = check_source("Integer value = 1; Integer VALUE = 2;").unwrap_err();
        assert_eq!(error.message, "duplicate variable `VALUE`");
    }

    #[test]
    fn short_circuit_rhs_is_still_checked_statically() {
        let error = check_source("Boolean result = true || missing;").unwrap_err();
        assert_eq!(error.message, "unknown variable `missing`");
    }

    #[test]
    fn null_assignment_does_not_permit_cross_type_equality() {
        check_source("Integer number = null; number = null;").unwrap();

        let error = check_source("Integer number = 1; Boolean same = number == true;").unwrap_err();
        assert_eq!(
            error.message,
            "operator `==` cannot be applied to Integer and Boolean"
        );
    }

    #[test]
    fn loop_control_is_validated_against_lexical_loop_depth() {
        check_source("while (true) { if (false) continue; break; }").unwrap();

        let error = check_source("{ break; }").unwrap_err();
        assert_eq!(error.message, "`break` is only valid inside a loop");
    }
}
