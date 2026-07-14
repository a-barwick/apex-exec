use crate::{
    ast::{
        BinaryOperator, Expression, Identifier, PostfixOperator, Program, Statement, TypeName,
        UnaryOperator,
    },
    diagnostic::Diagnostic,
    span::Span,
};
use std::collections::HashMap;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Value {
    String(String),
    Boolean(bool),
    Integer(i64),
    Null(Option<TypeName>),
}

impl Value {
    fn display(&self) -> String {
        match self {
            Self::String(value) => value.clone(),
            Self::Boolean(value) => value.to_string(),
            Self::Integer(value) => value.to_string(),
            Self::Null(_) => "null".to_owned(),
        }
    }

    fn has_string_type(&self) -> bool {
        matches!(self, Self::String(_) | Self::Null(Some(TypeName::String)))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Slot {
    ty: TypeName,
    value: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Flow {
    Normal,
    Break,
    Continue,
    Return,
}

pub struct Interpreter {
    scopes: Vec<HashMap<String, Slot>>,
    output: Vec<String>,
}

impl Interpreter {
    pub fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            output: Vec::new(),
        }
    }

    pub fn execute(mut self, program: &Program) -> Result<Vec<String>, Diagnostic> {
        for statement in &program.statements {
            match self.execute_statement(statement)? {
                Flow::Normal => {}
                Flow::Return => break,
                Flow::Break => {
                    return Err(Diagnostic::new(
                        "`break` escaped semantic validation",
                        statement.span(),
                    ));
                }
                Flow::Continue => {
                    return Err(Diagnostic::new(
                        "`continue` escaped semantic validation",
                        statement.span(),
                    ));
                }
            }
        }
        Ok(self.output)
    }

    fn execute_statement(&mut self, statement: &Statement) -> Result<Flow, Diagnostic> {
        match statement {
            Statement::VariableDeclaration {
                ty,
                name,
                initializer,
                ..
            } => {
                let value = typed_value(self.evaluate(initializer)?, *ty);
                self.current_scope_mut()
                    .insert(name.canonical.clone(), Slot { ty: *ty, value });
                Ok(Flow::Normal)
            }
            Statement::Expression { expression, .. } => {
                self.evaluate(expression)?;
                Ok(Flow::Normal)
            }
            Statement::Debug { expression, .. } => {
                let value = self.evaluate(expression)?;
                self.output.push(value.display());
                Ok(Flow::Normal)
            }
            Statement::Block { statements, .. } => self.execute_block(statements),
            Statement::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                if self.evaluate_boolean(condition)? {
                    self.execute_statement(then_branch)
                } else if let Some(else_branch) = else_branch {
                    self.execute_statement(else_branch)
                } else {
                    Ok(Flow::Normal)
                }
            }
            Statement::While {
                condition, body, ..
            } => {
                while self.evaluate_boolean(condition)? {
                    match self.execute_statement(body)? {
                        Flow::Normal | Flow::Continue => {}
                        Flow::Break => break,
                        Flow::Return => return Ok(Flow::Return),
                    }
                }
                Ok(Flow::Normal)
            }
            Statement::DoWhile {
                body, condition, ..
            } => {
                loop {
                    match self.execute_statement(body)? {
                        Flow::Normal | Flow::Continue => {}
                        Flow::Break => break,
                        Flow::Return => return Ok(Flow::Return),
                    }
                    if !self.evaluate_boolean(condition)? {
                        break;
                    }
                }
                Ok(Flow::Normal)
            }
            Statement::For {
                initializer,
                condition,
                update,
                body,
                ..
            } => self.execute_for(
                initializer.as_deref(),
                condition.as_ref(),
                update.as_deref(),
                body,
            ),
            Statement::Break { .. } => Ok(Flow::Break),
            Statement::Continue { .. } => Ok(Flow::Continue),
            Statement::Return { .. } => Ok(Flow::Return),
        }
    }

    fn execute_block(&mut self, statements: &[Statement]) -> Result<Flow, Diagnostic> {
        self.scopes.push(HashMap::new());
        let result = (|| {
            for statement in statements {
                let flow = self.execute_statement(statement)?;
                if flow != Flow::Normal {
                    return Ok(flow);
                }
            }
            Ok(Flow::Normal)
        })();
        self.scopes.pop();
        result
    }

    fn execute_for(
        &mut self,
        initializer: Option<&Statement>,
        condition: Option<&Expression>,
        update: Option<&Statement>,
        body: &Statement,
    ) -> Result<Flow, Diagnostic> {
        self.scopes.push(HashMap::new());
        let result = (|| {
            if let Some(initializer) = initializer {
                let flow = self.execute_statement(initializer)?;
                if flow != Flow::Normal {
                    return Ok(flow);
                }
            }
            loop {
                if let Some(condition) = condition
                    && !self.evaluate_boolean(condition)?
                {
                    break;
                }
                match self.execute_statement(body)? {
                    Flow::Normal | Flow::Continue => {}
                    Flow::Break => break,
                    Flow::Return => return Ok(Flow::Return),
                }
                if let Some(update) = update {
                    let flow = self.execute_statement(update)?;
                    if flow != Flow::Normal {
                        return Ok(flow);
                    }
                }
            }
            Ok(Flow::Normal)
        })();
        self.scopes.pop();
        result
    }

    fn evaluate(&mut self, expression: &Expression) -> Result<Value, Diagnostic> {
        match expression {
            Expression::StringLiteral(value, _) => Ok(Value::String(value.clone())),
            Expression::BooleanLiteral(value, _) => Ok(Value::Boolean(*value)),
            Expression::IntegerLiteral(value, _) => Ok(Value::Integer(*value)),
            Expression::NullLiteral(_) => Ok(Value::Null(None)),
            Expression::Variable(identifier) => {
                self.lookup(identifier).map(|slot| slot.value.clone())
            }
            Expression::Assignment { target, value, .. } => {
                let value = self.evaluate(value)?;
                self.assign(target, value)
            }
            Expression::Unary {
                operator,
                operand,
                operator_span,
                ..
            } => self.evaluate_unary(*operator, operand, *operator_span),
            Expression::Postfix {
                operand,
                operator,
                operator_span,
                ..
            } => self.evaluate_postfix(operand, *operator, *operator_span),
            Expression::Binary {
                left,
                operator,
                right,
                operator_span,
                ..
            } => self.evaluate_binary(left, *operator, right, *operator_span),
        }
    }

    fn evaluate_unary(
        &mut self,
        operator: UnaryOperator,
        operand: &Expression,
        operator_span: Span,
    ) -> Result<Value, Diagnostic> {
        match operator {
            UnaryOperator::Positive => {
                let value = self.evaluate_integer(operand)?;
                Ok(Value::Integer(value))
            }
            UnaryOperator::Negate => {
                let value = self.evaluate_integer(operand)?;
                value
                    .checked_neg()
                    .map(Value::Integer)
                    .ok_or_else(|| integer_overflow(operator_span))
            }
            UnaryOperator::Not => {
                let value = self.evaluate_boolean(operand)?;
                Ok(Value::Boolean(!value))
            }
            UnaryOperator::PrefixIncrement => {
                let identifier = mutable_identifier(operand, operator_span)?;
                self.mutate_integer(identifier, 1, false, operator_span)
            }
            UnaryOperator::PrefixDecrement => {
                let identifier = mutable_identifier(operand, operator_span)?;
                self.mutate_integer(identifier, -1, false, operator_span)
            }
        }
    }

    fn evaluate_postfix(
        &mut self,
        operand: &Expression,
        operator: PostfixOperator,
        operator_span: Span,
    ) -> Result<Value, Diagnostic> {
        let identifier = mutable_identifier(operand, operator_span)?;
        let delta = match operator {
            PostfixOperator::Increment => 1,
            PostfixOperator::Decrement => -1,
        };
        self.mutate_integer(identifier, delta, true, operator_span)
    }

    fn evaluate_binary(
        &mut self,
        left: &Expression,
        operator: BinaryOperator,
        right: &Expression,
        operator_span: Span,
    ) -> Result<Value, Diagnostic> {
        if operator == BinaryOperator::And {
            let left = self.evaluate_boolean(left)?;
            return if left {
                Ok(Value::Boolean(self.evaluate_boolean(right)?))
            } else {
                Ok(Value::Boolean(false))
            };
        }
        if operator == BinaryOperator::Or {
            let left = self.evaluate_boolean(left)?;
            return if left {
                Ok(Value::Boolean(true))
            } else {
                Ok(Value::Boolean(self.evaluate_boolean(right)?))
            };
        }

        let left = self.evaluate(left)?;
        let right = self.evaluate(right)?;
        match operator {
            BinaryOperator::Add => {
                if left.has_string_type() || right.has_string_type() {
                    Ok(Value::String(left.display() + &right.display()))
                } else {
                    match (&left, &right) {
                        (Value::Integer(left), Value::Integer(right)) => left
                            .checked_add(*right)
                            .map(Value::Integer)
                            .ok_or_else(|| integer_overflow(operator_span)),
                        (Value::Null(_), _) | (_, Value::Null(_)) => Err(Diagnostic::new(
                            "operator cannot be applied to null at runtime",
                            operator_span,
                        )),
                        _ => Err(invalid_runtime_operands(operator_span)),
                    }
                }
            }
            BinaryOperator::Subtract => {
                checked_integer_binary(left, right, operator_span, i64::checked_sub)
            }
            BinaryOperator::Multiply => {
                checked_integer_binary(left, right, operator_span, i64::checked_mul)
            }
            BinaryOperator::Divide => {
                let (left, right) = integer_pair(left, right, operator_span)?;
                if right == 0 {
                    return Err(Diagnostic::new("division by zero", operator_span));
                }
                left.checked_div(right)
                    .map(Value::Integer)
                    .ok_or_else(|| integer_overflow(operator_span))
            }
            BinaryOperator::Remainder => {
                let (left, right) = integer_pair(left, right, operator_span)?;
                if right == 0 {
                    return Err(Diagnostic::new("remainder by zero", operator_span));
                }
                left.checked_rem(right)
                    .map(Value::Integer)
                    .ok_or_else(|| integer_overflow(operator_span))
            }
            BinaryOperator::Less => compare_integers(left, right, operator_span, |a, b| a < b),
            BinaryOperator::LessEqual => {
                compare_integers(left, right, operator_span, |a, b| a <= b)
            }
            BinaryOperator::Greater => compare_integers(left, right, operator_span, |a, b| a > b),
            BinaryOperator::GreaterEqual => {
                compare_integers(left, right, operator_span, |a, b| a >= b)
            }
            BinaryOperator::Equal => Ok(Value::Boolean(values_equal(&left, &right))),
            BinaryOperator::NotEqual => Ok(Value::Boolean(!values_equal(&left, &right))),
            BinaryOperator::And | BinaryOperator::Or => unreachable!("handled above"),
        }
    }

    fn evaluate_boolean(&mut self, expression: &Expression) -> Result<bool, Diagnostic> {
        match self.evaluate(expression)? {
            Value::Boolean(value) => Ok(value),
            _ => Err(Diagnostic::new(
                "expected Boolean value at runtime",
                expression.span(),
            )),
        }
    }

    fn evaluate_integer(&mut self, expression: &Expression) -> Result<i64, Diagnostic> {
        match self.evaluate(expression)? {
            Value::Integer(value) => Ok(value),
            _ => Err(Diagnostic::new(
                "expected Integer value at runtime",
                expression.span(),
            )),
        }
    }

    fn mutate_integer(
        &mut self,
        identifier: &Identifier,
        delta: i64,
        return_old: bool,
        operator_span: Span,
    ) -> Result<Value, Diagnostic> {
        let slot = self.lookup_mut(identifier)?;
        let Value::Integer(old) = &mut slot.value else {
            return Err(Diagnostic::new(
                "increment/decrement requires a non-null Integer value",
                operator_span,
            ));
        };
        let previous = *old;
        *old = old
            .checked_add(delta)
            .ok_or_else(|| integer_overflow(operator_span))?;
        Ok(Value::Integer(if return_old { previous } else { *old }))
    }

    fn lookup(&self, identifier: &Identifier) -> Result<&Slot, Diagnostic> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(&identifier.canonical))
            .ok_or_else(|| unknown_variable(identifier))
    }

    fn lookup_mut(&mut self, identifier: &Identifier) -> Result<&mut Slot, Diagnostic> {
        self.scopes
            .iter_mut()
            .rev()
            .find_map(|scope| scope.get_mut(&identifier.canonical))
            .ok_or_else(|| unknown_variable(identifier))
    }

    fn assign(&mut self, identifier: &Identifier, value: Value) -> Result<Value, Diagnostic> {
        let slot = self.lookup_mut(identifier)?;
        let value = typed_value(value, slot.ty);
        slot.value = value.clone();
        Ok(value)
    }

    fn current_scope_mut(&mut self) -> &mut HashMap<String, Slot> {
        self.scopes
            .last_mut()
            .expect("interpreter always has a scope")
    }
}

impl Default for Interpreter {
    fn default() -> Self {
        Self::new()
    }
}

fn mutable_identifier(
    expression: &Expression,
    operator_span: Span,
) -> Result<&Identifier, Diagnostic> {
    if let Expression::Variable(identifier) = expression {
        Ok(identifier)
    } else {
        Err(Diagnostic::new(
            "increment/decrement operand must be a variable",
            operator_span,
        ))
    }
}

fn checked_integer_binary(
    left: Value,
    right: Value,
    span: Span,
    operation: fn(i64, i64) -> Option<i64>,
) -> Result<Value, Diagnostic> {
    let (left, right) = integer_pair(left, right, span)?;
    operation(left, right)
        .map(Value::Integer)
        .ok_or_else(|| integer_overflow(span))
}

fn compare_integers(
    left: Value,
    right: Value,
    span: Span,
    comparison: impl FnOnce(i64, i64) -> bool,
) -> Result<Value, Diagnostic> {
    let (left, right) = integer_pair(left, right, span)?;
    Ok(Value::Boolean(comparison(left, right)))
}

fn integer_pair(left: Value, right: Value, span: Span) -> Result<(i64, i64), Diagnostic> {
    match (left, right) {
        (Value::Integer(left), Value::Integer(right)) => Ok((left, right)),
        (Value::Null(_), _) | (_, Value::Null(_)) => Err(Diagnostic::new(
            "operator cannot be applied to null at runtime",
            span,
        )),
        _ => Err(invalid_runtime_operands(span)),
    }
}

fn typed_value(value: Value, ty: TypeName) -> Value {
    match value {
        Value::Null(_) => Value::Null(Some(ty)),
        value => value,
    }
}

fn values_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Null(_), Value::Null(_)) => true,
        _ => left == right,
    }
}

fn unknown_variable(identifier: &Identifier) -> Diagnostic {
    Diagnostic::new(
        format!("unknown variable `{}`", identifier.spelling),
        identifier.span,
    )
}

fn invalid_runtime_operands(span: Span) -> Diagnostic {
    Diagnostic::new("invalid operands escaped semantic validation", span)
}

fn integer_overflow(span: Span) -> Diagnostic {
    Diagnostic::new("integer overflow", span)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn execute_source(source: &str) -> Result<Vec<String>, Diagnostic> {
        let program = crate::check(source)?;
        Interpreter::new().execute(&program)
    }

    #[test]
    fn typed_nulls_retain_static_string_behavior_and_compare_as_null() {
        let string_null = typed_value(Value::Null(None), TypeName::String);
        let integer_null = typed_value(Value::Null(None), TypeName::Integer);

        assert!(string_null.has_string_type());
        assert!(!integer_null.has_string_type());
        assert!(values_equal(&string_null, &Value::Null(None)));
    }

    #[test]
    fn continue_in_a_for_loop_still_executes_the_update_clause() {
        let output = execute_source(
            "Integer i = 0; Integer total = 0; \
             for (; i < 4; i++) { if (i < 2) continue; total = total + i; } \
             System.debug(i); System.debug(total);",
        )
        .unwrap();

        assert_eq!(output, ["4", "5"]);
    }

    #[test]
    fn return_unwinds_nested_blocks_and_loops() {
        let output = execute_source(
            "System.debug('before'); while (true) { { return; } } System.debug('after');",
        )
        .unwrap();

        assert_eq!(output, ["before"]);
    }
}
